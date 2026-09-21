// ============================================================================
// 两个「槽」：只缓存**解析结果**，绝不缓存 DuckLazy 凭证
//
// `DuckLazy<T>` 的契约是「凭证只在本行回调内有效」，把它存进聚合状态、跨 chunk / 跨线程再解析会被
// 运行时守卫拦下（报 "DuckLazy<T> is stale"）。所以这里存的是解析出来的普通数据，combine 时也只是把
// 这份数据搬过去 —— 同一列在各分组/各分片里解析出来的内容一样，不需要（也不能）重新解析。
//
// # 为什么基准是「列表参数」而不是「同组里的标签行」
//
// 聚合函数只看得到自己组内的行。若把基准写成同组内的行（长表 + 一个「哪个标签是基准」的配置项），
// 那么为了让每个分组都拿得到基准，基准行就必须在**每个分组里各出现一遍**：100 个标的 × 1000 天
// = 10 万行被物化/扫描，而基准本身只有 1000 行。改成一次性传入的列表后，基准只写一次、只求值一次，
// 策略侧仍然靠 GROUP BY 自然分组，也不再需要「哪个标签是基准」这类配置。
//
// 顺带消失的还有两条护栏：分组内不会再混进别的标的，也不会出现「只有基准行、没有策略行」的组。
//
// # 为什么基准与配置参数都要裹 DuckLazy
//
// `duckfn` 的适配层是**逐行**读参数的（`aggregate_function_adapter.rs` 的 `read_columns` 在行循环里），
// 而 `Vec<T>::read_valid` 每次调用都会新建 Vec 并逐元素复制整个列表。裸写 `Vec<...Point>`
// 就是每行复制一遍整条基准序列，退化成 O(行数 × 基准长度)。裹上 `DuckLazy` 后每行只构造一个 O(1) 的
// 凭证，真正的解析在分组首行做一次。配置参数同理（它同样是「多行不变、比被聚合的值贵」的东西），
// 只是它本来就很小。
//
// Two slots: they cache the *parsed value* and never the DuckLazy token.
//
// A `DuckLazy<T>` token is only valid inside the callback that produced it, so storing it in the aggregate
// state and consuming it past the chunk or on another thread is rejected by the runtime guard
// ("DuckLazy<T> is stale"). These slots therefore keep the parsed value, and combine merely moves that
// value across — the same column parses to the same content everywhere, so re-parsing is neither needed
// nor allowed.
//
// # Why the benchmark is a list argument rather than labelled rows in the same group
//
// An aggregate only sees the rows of its own group. Representing the benchmark as rows in the same
// group (a long table plus a config key saying which label is the benchmark) forces the benchmark rows
// to appear **once per group**: 100 instruments × 1000 days = 100k rows materialised and scanned, while
// the benchmark itself is only 1000 rows. With a list passed in once, the benchmark is written once and
// evaluated once, the strategy side still gets its grouping from GROUP BY, and the "which label is the
// benchmark" config key disappears.
//
// Two guards disappear with it: a group can no longer mix several instruments, and a group can no longer
// hold only benchmark rows.
//
// # Why both the benchmark and the options argument are wrapped in DuckLazy
//
// duckfn's adapter reads arguments **per row** (`read_columns` sits inside the row loop of
// `aggregate_function_adapter.rs`), and `Vec<T>::read_valid` allocates a fresh Vec and copies every
// element on each call. A bare `Vec<...Point>` would therefore copy the whole benchmark series once per
// row, degrading to O(rows × benchmark length). Wrapped in `DuckLazy`, every row only builds an O(1)
// token and the single real parse happens on the group's first row. The options argument works the same
// way (it is likewise "constant across rows and more expensive than the aggregated values"), it is just
// much smaller.
// ============================================================================

use std::sync::Arc;

use duckfn::{DuckLazy, DuckResult, DuckValueType, duck_error};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::kind::SeriesKind;
use super::series::{IntoSeriesPoint, SeriesPoint};

/// 报告配置的三种状态。
///
/// The three states of the options slot.
#[derive(Default, Debug, Clone)]
pub(super) enum OptionsSlot {
    /// 还没有在任何 update 回调里解析过。
    ///
    /// Not parsed inside an update callback yet.
    #[default]
    Unresolved,
    /// 已解析：`None` 表示整列配置都是 NULL，SQL 侧等价于全默认。
    ///
    /// Parsed: `None` means the whole options column was NULL, i.e. all defaults.
    Resolved(Option<QuantstatsHtmlOptions>),
}

impl OptionsSlot {
    /// 只在第一行解析一次；之后所有行只付 O(1) 的凭证构造成本。
    ///
    /// 用 `try_get()` 而不是 `get()`：后者把失败做成 panic（再由适配层的 unwind 包成查询错误），
    /// 而这里已经是 `DuckResult` 语境，让错误正常传播更清楚。
    ///
    /// Parses once, on the first row; every later row only pays the O(1) token construction.
    ///
    /// `try_get()` rather than `get()`: the latter turns failures into panics (which the adapter's unwind
    /// wrapper converts back into a query error), while here we are already in a `DuckResult` context and
    /// can propagate the error directly.
    pub(super) fn resolve(
        &mut self,
        options: Option<&DuckLazy<QuantstatsHtmlOptions>>,
    ) -> DuckResult<()> {
        if matches!(self, OptionsSlot::Unresolved) {
            *self = OptionsSlot::Resolved(match options {
                Some(lazy) => Some(lazy.try_get()?),
                None => None,
            });
        }
        Ok(())
    }

    /// 合并两个状态：配置不重新解析，直接把对面解析好的结果搬过来。
    ///
    /// Merge two states: the options are not re-parsed, the already-parsed value is moved over.
    pub(super) fn merge(&mut self, other: &Self) {
        if matches!(self, OptionsSlot::Unresolved) {
            *self = other.clone();
        }
    }

    /// 取配置。整列配置是 NULL、或这一组一行都没有过（`Unresolved`）时，退化成 `default()` 的逐字段
    /// 默认值 —— 也就是 quantstats-rs 自己的全默认。
    ///
    /// Read the options. When the options column was NULL, or the group never had a row at all
    /// (`Unresolved`), this falls back to `default()` field-by-field — which equals quantstats-rs' own
    /// defaults.
    pub(super) fn get(&self) -> QuantstatsHtmlOptions {
        match self {
            OptionsSlot::Resolved(Some(options)) => options.clone(),
            OptionsSlot::Resolved(None) | OptionsSlot::Unresolved => QuantstatsHtmlOptions::default(),
        }
    }
}

/// 基准序列的三种状态。
///
/// 与 [`OptionsSlot`] 同构：只在首行解析一次，之后所有行复用；combine 只搬运已解析的数据。
/// 存 `Arc` 是为了让 combine 变成 O(1) 的引用计数复制，而不是把整条基准序列深拷一遍。
///
/// The three states of the benchmark slot.
///
/// Isomorphic to [`OptionsSlot`]: parsed once on the first row and reused afterwards, with combine only
/// moving the parsed data. The `Arc` keeps combine at an O(1) refcount bump instead of deep-copying the
/// whole benchmark series.
#[derive(Default, Debug, Clone)]
pub(super) enum BenchmarkSlot {
    /// 还没有在任何 update 回调里解析过。
    ///
    /// Not parsed inside an update callback yet.
    #[default]
    Unresolved,
    /// 已解析且非空（空列表在解析时就已经报错）。
    ///
    /// Parsed and non-empty (an empty list is rejected while parsing).
    ///
    /// 注意：价格路径下这里存的还是**价格/净值**，差分留到 `result()` 里做（差分可能让结果变空）。
    ///
    /// Note: on the price branch this still holds *prices*; the differencing happens in `result()`
    /// (because it may end up empty).
    Resolved(Arc<Vec<SeriesPoint>>),
}

impl BenchmarkSlot {
    /// 只在第一行解析一次：校验非 NULL、非空，并把列表转成内部的点数组。
    ///
    /// 错误都带函数名前缀（`kind.function`），并且会指出替代方案 —— 只想要单序列报告的话，用三参数
    /// 那次重载即可。
    ///
    /// Parses once, on the first row: it rejects NULL and empty lists and converts the list into the
    /// internal point array.
    ///
    /// Every error carries the function name (`kind.function`) and points at the alternative — for a
    /// single-series report, use the three-argument overload.
    // `T` 除了能归一化成 `SeriesPoint`，还必须满足 `DuckValueType` —— `DuckLazy<Vec<T>>::try_get`
    // 要求 `Vec<T>: DuckValueType`，而它由 `T: DuckValueType` 推出。
    //
    // Besides normalising into a `SeriesPoint`, `T` must satisfy `DuckValueType`: `try_get` on
    // `DuckLazy<Vec<T>>` needs `Vec<T>: DuckValueType`, which follows from `T: DuckValueType`.
    pub(super) fn resolve<T: IntoSeriesPoint + DuckValueType>(
        &mut self,
        benchmark: Option<&DuckLazy<Vec<T>>>,
        kind: SeriesKind,
    ) -> DuckResult<()> {
        if !matches!(self, BenchmarkSlot::Unresolved) {
            return Ok(());
        }

        let function = kind.function;
        let value_field = kind.value_field;

        let Some(lazy) = benchmark else {
            return Err(duck_error(format!(
                "{function}: the benchmark list must not be NULL — pass the benchmark series as \
                 list({{'date': ..., '{value_field}': ...}}), or omit the benchmark argument for a \
                 single-series report"
            )));
        };

        // 上游的错误信息是写给 duckfn 使用者的（会提到 `try_get()`），这里换成对 SQL 调用方有意义的说法：
        // 列表里出现了整体为 NULL 的元素（例如字面量 `[NULL, ...]`）时就会走到这里。
        //
        // The upstream message is written for a duckfn user (it mentions `try_get()`), so it is replaced
        // with something meaningful to a SQL caller: this is reached when the list contains a whole-NULL
        // element (e.g. a literal `[NULL, ...]`).
        let parsed = lazy.try_get().map_err(|err| {
            duck_error(format!(
                "{function}: cannot read the benchmark list — every element must be a \
                 STRUCT(date DATE, {value_field} DOUBLE) and must not be NULL ({err})"
            ))
        })?;

        let points: Vec<SeriesPoint> = parsed
            .into_iter()
            .filter_map(IntoSeriesPoint::into_series_point)
            .collect();

        if points.is_empty() {
            return Err(duck_error(format!(
                "{function}: the benchmark list is empty (or every point is missing its date or its \
                 value) — omit the benchmark argument for a single-series report"
            )));
        }

        *self = BenchmarkSlot::Resolved(Arc::new(points));
        Ok(())
    }

    /// 合并两个状态：不重新解析，只搬运对面解析好的结果。
    ///
    /// Merge two states: no re-parsing, the already-parsed value of the other side is moved over.
    pub(super) fn merge(&mut self, other: &Self) {
        if matches!(self, BenchmarkSlot::Unresolved) {
            *self = other.clone();
        }
    }

    /// 取已解析的基准点。
    ///
    /// `Unresolved` 在这里不可达：一组一行都没有时 `result()` 已经提前返回 `NULL` 了。留这条分支
    /// 只是不想在理论上不可达的路径上 unwrap。
    ///
    /// Read the parsed benchmark points.
    ///
    /// `Unresolved` is unreachable here: `result()` already returns `NULL` early when a group has no row at
    /// all. The branch exists only so that no theoretically unreachable path unwraps.
    pub(super) fn get(&self, kind: SeriesKind) -> DuckResult<Arc<Vec<SeriesPoint>>> {
        match self {
            BenchmarkSlot::Resolved(points) => Ok(Arc::clone(points)),
            BenchmarkSlot::Unresolved => Err(duck_error(format!(
                "{}: the benchmark list was never read",
                kind.function
            ))),
        }
    }
}
