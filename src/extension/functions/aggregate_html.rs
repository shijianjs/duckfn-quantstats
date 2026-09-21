use std::sync::Arc;

use chrono::{NaiveDate, TimeDelta};
use duckfn::{
    DuckAggregateState, DuckDate, DuckLazy, DuckOptionResult, DuckResult, duck_aggregate_function,
    duck_error,
};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;
use crate::extension::types::return_point::QuantstatsReturnPoint;

// ============================================================================
// 一个 SQL 名字，两个聚合重载
//
//   duckfn_quantstats_html(date, period_return, options)                单序列报告
//   duckfn_quantstats_html(date, period_return, benchmark, options)     带基准报告
//
// 两个签名只差一个 `benchmark` 参数，所以用 `overloads_name` 把两者注册成**同一个函数集**：
// SQL 侧只有一个名字，按参数个数分派（`register_all_aggregate_overload` 会按名字分组，
// 每个重载各自带参数表与返回类型）。参数顺序固定为「数据列在前、配置在后」。
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
// 而 `Vec<T>::read_valid` 每次调用都会新建 Vec 并逐元素复制整个列表。裸写 `Vec<QuantstatsReturnPoint>`
// 就是每行复制一遍整条基准序列，退化成 O(行数 × 基准长度)。裹上 `DuckLazy` 后每行只构造一个 O(1) 的
// 凭证，真正的解析在分组首行做一次。配置参数同理（它同样是「多行不变、比被聚合的值贵」的东西），
// 只是它本来就很小。
//
// 报告只在 result() 里生成一次 —— 即每个分组一次。GROUP BY 100 个标的就会渲染 100 份完整报告
// （每份内含十几张 SVG），这是预期行为，不是性能 bug。同理，每个分组仍会各自持有一份解析好的基准点：
// 聚合状态不跨分组共享，这部分开销消不掉，能省掉的是 DuckDB 层的行展开与扫描。
//
// 排序：DuckDB 并行/分块执行时 combine 的调用顺序不保证，所以这里只做「拼接」，
// 真正的按日期排序交给 ReturnSeries::new（它内部会 sort）。因此 SQL 侧**不需要 ORDER BY**。
//
// One SQL name, two aggregate overloads.
//
//   duckfn_quantstats_html(date, period_return, options)                single series
//   duckfn_quantstats_html(date, period_return, benchmark, options)     with a benchmark
//
// The two signatures differ only by the `benchmark` argument, so `overloads_name` registers them as
// **one function set**: a single SQL name dispatched by argument count
// (`register_all_aggregate_overload` groups by name and every overload keeps its own parameter list
// and return type). The argument order is always "data columns first, options last".
//
// # Why the benchmark is a list argument rather than labelled rows in the same group
//
// An aggregate only sees the rows of its own group. Representing the benchmark as rows in the same
// group (a long table plus a config key saying which label is the benchmark) forces the benchmark
// rows to appear **once per group**: 100 instruments × 1000 days = 100k rows materialised and
// scanned, while the benchmark itself is only 1000 rows. With a list passed in once, the benchmark is
// written once and evaluated once, the strategy side still gets its grouping from GROUP BY, and the
// "which label is the benchmark" config key disappears.
//
// Two guards disappear with it: a group can no longer mix several instruments, and a group can no
// longer hold only benchmark rows.
//
// # Why both the benchmark and the options argument are wrapped in DuckLazy
//
// duckfn's adapter reads arguments **per row** (`read_columns` sits inside the row loop of
// `aggregate_function_adapter.rs`), and `Vec<T>::read_valid` allocates a fresh Vec and copies every
// element on each call. A bare `Vec<QuantstatsReturnPoint>` would therefore copy the whole benchmark
// series once per row, degrading to O(rows × benchmark length). Wrapped in `DuckLazy`, every row only
// builds an O(1) token and the single real parse happens on the group's first row. The options
// argument works the same way (it is likewise "constant across rows and more expensive than the
// aggregated values"), it is just much smaller.
//
// The report is rendered once in result(), i.e. once per group. GROUP BY over 100 instruments renders
// 100 full reports (each with a dozen inline SVGs); that is expected, not a performance bug. For the
// same reason every group holds its own parsed copy of the benchmark points: aggregate states are not
// shared across groups, so that part cannot be avoided — what this design removes is DuckDB's row
// expansion and scanning.
//
// Ordering: DuckDB may call combine in any order when running in parallel or in chunks, so combine
// only concatenates; the real date ordering is done by ReturnSeries::new (which sorts internally).
// SQL therefore does **not** need an ORDER BY.
// ============================================================================

/// 注册名（两个重载共用），同时也用作错误信息前缀。
///
/// 注意：函数集名字只写在下面两个 `#[duck_aggregate_function(overloads_name = "...")]` 上，
/// 宏属性只能吃字面量，所以这两处字符串必须手动保持一致。
///
/// The registered name shared by both overloads, also used as the error-message prefix.
///
/// Note: the function-set name lives only on the two
/// `#[duck_aggregate_function(overloads_name = "...")]` attributes below — macro attributes accept
/// literals only, so those two strings must be kept in sync by hand.
const HTML_FUNCTION: &str = "duckfn_quantstats_html";

// ============================================================================
// 内部用的「一个点」
// ============================================================================

/// 序列里的一个点：**推迟**日期换算，只留自 1970-01-01 起的天数。
///
/// 用具名字段而不是 `(i32, f64)` 元组：两个字段类型不同、含义也不同，元组在调用点很容易写反
/// （策略侧和基准侧都要用到它）。
///
/// One point of a series: the conversion to a calendar date is deferred, only the day count since
/// 1970-01-01 is kept.
///
/// A named struct rather than an `(i32, f64)` tuple: the two fields differ in type and meaning, and a
/// tuple is easy to get backwards at a call site (both the strategy and the benchmark side use it).
#[derive(Clone, Copy, Debug)]
struct ReturnPoint {
    /// 自 1970-01-01 起的天数（换算成 `NaiveDate` 可能失败，所以留到 `build_series` 里做）。
    ///
    /// Days since 1970-01-01 (converting to `NaiveDate` can fail, so it is left to `build_series`).
    days_since_epoch: i32,
    /// 该周期的收益率。
    ///
    /// The return of that period.
    period_return: f64,
}

/// 把 `DuckDate`（自 1970-01-01 起的天数）换成 chrono 的 `NaiveDate`。
///
/// `ReturnSeries` 要的是 `NaiveDate`，而 duckfn 的 `DuckDate` 只存天数，换算只能自己做。
/// 越界（例如 `DATE 'infinity'` 之类的极端值）返回查询错误，不用会 panic 的运算符。
///
/// Convert a `DuckDate` (days since 1970-01-01) into a chrono `NaiveDate`.
///
/// `ReturnSeries` wants `NaiveDate`, while duckfn's `DuckDate` only stores days, so the conversion is
/// on us. Out-of-range values (e.g. `DATE 'infinity'`) become a query error instead of a panic.
fn naive_date(days_since_epoch: i32) -> DuckResult<NaiveDate> {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|epoch| epoch.checked_add_signed(TimeDelta::days(i64::from(days_since_epoch))))
        .ok_or_else(|| {
            duck_error(format!(
                "cannot convert DuckDate {{ days_since_epoch: {days_since_epoch} }} into a calendar date"
            ))
        })
}

/// 一组点 → `ReturnSeries`。排序由 `ReturnSeries::new` 内部完成，所以 SQL 侧不需要 `ORDER BY`，
/// `combine` 的拼接顺序也不影响结果。
///
/// A list of points → `ReturnSeries`. Sorting happens inside `ReturnSeries::new`, so SQL does not need
/// an `ORDER BY` and the concatenation order inside `combine` does not matter.
fn build_series(points: &[ReturnPoint], name: Option<String>) -> DuckResult<ReturnSeries> {
    let mut dates = Vec::with_capacity(points.len());
    let mut values = Vec::with_capacity(points.len());
    for point in points {
        dates.push(naive_date(point.days_since_epoch)?);
        values.push(point.period_return);
    }

    ReturnSeries::new(dates, values, name)
        .map_err(|err| duck_error(format!("cannot build the returns series: {err}")))
}

/// 渲染报告，并把 quantstats-rs 的错误转成 DuckDB 查询错误。
///
/// Render the report, turning quantstats-rs errors into DuckDB query errors.
fn render(series: &ReturnSeries, options: HtmlReportOptions<'_>) -> DuckResult<String> {
    html(series, options)
        .map_err(|err| duck_error(format!("{HTML_FUNCTION}: cannot render the report: {err}")))
}

// ============================================================================
// 两个「槽」：只缓存**解析结果**，绝不缓存 DuckLazy 凭证
//
// `DuckLazy<T>` 的契约是「凭证只在本行回调内有效」，把它存进聚合状态、跨 chunk / 跨线程再解析会被
// 运行时守卫拦下（报 "DuckLazy<T> is stale"）。所以这里存的是解析出来的普通数据，combine 时也只是把
// 这份数据搬过去 —— 同一列在各分组/各分片里解析出来的内容一样，不需要（也不能）重新解析。
//
// Two slots: they cache the *parsed value* and never the DuckLazy token.
//
// A `DuckLazy<T>` token is only valid inside the callback that produced it, so storing it in the
// aggregate state and consuming it past the chunk or on another thread is rejected by the runtime
// guard ("DuckLazy<T> is stale"). These slots therefore keep the parsed value, and combine merely
// moves that value across — the same column parses to the same content everywhere, so re-parsing is
// neither needed nor allowed.
// ============================================================================

/// 报告配置的三种状态。
///
/// The three states of the options slot.
#[derive(Default, Debug, Clone)]
enum OptionsSlot {
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
    /// `try_get()` rather than `get()`: the latter turns failures into panics (which the adapter's
    /// unwind wrapper converts back into a query error), while here we are already in a `DuckResult`
    /// context and can propagate the error directly.
    fn resolve(&mut self, options: Option<&DuckLazy<QuantstatsHtmlOptions>>) -> DuckResult<()> {
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
    fn merge(&mut self, other: &Self) {
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
    fn get(&self) -> QuantstatsHtmlOptions {
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
enum BenchmarkSlot {
    /// 还没有在任何 update 回调里解析过。
    ///
    /// Not parsed inside an update callback yet.
    #[default]
    Unresolved,
    /// 已解析且非空（空列表在解析时就已经报错）。
    ///
    /// Parsed and non-empty (an empty list is rejected while parsing).
    Resolved(Arc<Vec<ReturnPoint>>),
}

impl BenchmarkSlot {
    /// 只在第一行解析一次：校验非 NULL、非空，并把列表转成内部的点数组。
    ///
    /// 错误都带函数名前缀，并且会指出替代方案 —— 只想要单序列报告的话，用三参数那次重载即可。
    ///
    /// Parses once, on the first row: it rejects NULL and empty lists and converts the list into the
    /// internal point array.
    ///
    /// Every error carries the function name and points at the alternative — for a single-series
    /// report, use the three-argument overload.
    fn resolve(
        &mut self,
        benchmark: Option<&DuckLazy<Vec<QuantstatsReturnPoint>>>,
    ) -> DuckResult<()> {
        if !matches!(self, BenchmarkSlot::Unresolved) {
            return Ok(());
        }

        let Some(lazy) = benchmark else {
            return Err(duck_error(format!(
                "{HTML_FUNCTION}: the benchmark list must not be NULL — pass the benchmark series as \
                 list({{'date': ..., 'period_return': ...}}), or omit the benchmark argument for a \
                 single-series report"
            )));
        };

        // 上游的错误信息是写给 duckfn 使用者的（会提到 `try_get()`），这里换成对 SQL 调用方有意义的说法：
        // 列表里出现了整体为 NULL 的元素（例如字面量 `[NULL, ...]`）时就会走到这里。
        //
        // The upstream message is written for a duckfn user (it mentions `try_get()`), so it is
        // replaced with something meaningful to a SQL caller: this is reached when the list contains a
        // whole-NULL element (e.g. a literal `[NULL, ...]`).
        let parsed = lazy.try_get().map_err(|err| {
            duck_error(format!(
                "{HTML_FUNCTION}: cannot read the benchmark list — every element must be a \
                 STRUCT(date DATE, period_return DOUBLE) and must not be NULL ({err})"
            ))
        })?;

        let mut points = Vec::with_capacity(parsed.len());
        for point in parsed {
            // 与策略侧一致：缺日期或缺收益的点直接跳过，而不是让整条查询失败。
            //
            // Same as the strategy side: a point missing its date or its return is skipped instead of
            // failing the whole query.
            let (Some(date), Some(period_return)) = (point.date, point.period_return) else {
                continue;
            };
            points.push(ReturnPoint {
                days_since_epoch: date.days_since_epoch,
                period_return,
            });
        }

        if points.is_empty() {
            return Err(duck_error(format!(
                "{HTML_FUNCTION}: the benchmark list is empty (or every point is missing its date or \
                 its return) — omit the benchmark argument for a single-series report"
            )));
        }

        *self = BenchmarkSlot::Resolved(Arc::new(points));
        Ok(())
    }

    /// 合并两个状态：不重新解析，只搬运对面解析好的结果。
    ///
    /// Merge two states: no re-parsing, the already-parsed value of the other side is moved over.
    fn merge(&mut self, other: &Self) {
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
    /// `Unresolved` is unreachable here: `result()` already returns `NULL` early when a group has no
    /// row at all. The branch exists only so that no theoretically unreachable path unwraps.
    fn get(&self) -> DuckResult<Arc<Vec<ReturnPoint>>> {
        match self {
            BenchmarkSlot::Resolved(points) => Ok(Arc::clone(points)),
            BenchmarkSlot::Unresolved => Err(duck_error(format!(
                "{HTML_FUNCTION}: the benchmark list was never read"
            ))),
        }
    }
}

// ============================================================================
// 重载 1/2：单序列报告
// ============================================================================

/// 单序列报告聚合状态：累积策略的点。
///
/// Aggregate state for the single-series report: accumulates the points of the series.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlReportState {
    options: OptionsSlot,
    points: Vec<ReturnPoint>,
}

/// 单序列报告：对 `date` 与 `period_return` 两列做聚合，输出该组的完整 HTML 报告字符串。
///
/// 三参数那次重载。两列任一为 NULL 的行会被整行跳过（duckfn 对非 `Option` 入参的既有语义，
/// 与 SQL 聚合惯例一致）。
///
/// ```sql
/// SELECT duckfn_quantstats_html(date, period_return, NULL) FROM daily_returns;
/// SELECT symbol,
///        duckfn_quantstats_html(date, period_return, {'title': 'My Fund'}::duckfn_quantstats_html_options)
/// FROM daily_returns GROUP BY symbol;
/// ```
///
/// Single-series report: aggregates the `date` and `period_return` columns into the full HTML report
/// string of that group.
///
/// This is the three-argument overload. A row whose `date` or `period_return` is NULL is skipped
/// entirely — duckfn's existing semantics for non-`Option` arguments, and the usual SQL aggregate
/// behaviour.
#[duck_aggregate_function(overloads_name = "duckfn_quantstats_html")]
fn duckfn_quantstats_html(
    date: DuckDate,
    period_return: f64,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlReportState,
) -> DuckResult<()> {
    state.options.resolve(options.as_ref())?;
    state.points.push(ReturnPoint {
        days_since_epoch: date.days_since_epoch,
        period_return,
    });
    Ok(())
}

impl DuckAggregateState for HtmlReportState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.merge(&other.options);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        // 一组没有任何有效行 → SQL NULL。不要交给 html()，那边会报 EmptySeries 错误，
        // 而「空输入给 NULL」才是聚合函数该有的行为。
        //
        // A group without any valid row → SQL NULL. Do not hand it to html(), which would fail with
        // EmptySeries; returning NULL is the behaviour SQL users expect from an aggregate.
        if self.points.is_empty() {
            return Ok(None);
        }

        let series = build_series(&self.points, None)?;
        let options = self.options.get().to_report_options()?;

        Ok(Some(render(&series, options)?))
    }
}

// ============================================================================
// 重载 2/2：带基准报告
// ============================================================================

/// 带基准报告聚合状态：策略点逐行累积，基准点整体解析一次。
///
/// Aggregate state for the benchmark report: strategy points accumulate row by row, while the
/// benchmark points are parsed as a whole, once.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlBenchmarkState {
    options: OptionsSlot,
    benchmark: BenchmarkSlot,
    points: Vec<ReturnPoint>,
}

/// 带基准报告：策略侧逐行聚合，基准侧是**一次性传入的列表**。
///
/// 四参数那次重载。`benchmark` 是 `STRUCT(date DATE, period_return DOUBLE)[]`，通常由 `list(...)`
/// 在一张单行结果里构造出来；它只写一次、只求值一次，不会随分组的数量重复出现。两侧的起始日期对齐
/// 由 quantstats-rs 按 `match_dates`（默认 true）完成，这里不做额外处理。
///
/// `benchmark` 为 NULL 或为空（或每个点都缺日期/收益）时**直接报错**：这一支重载就是为带基准的场景
/// 存在的，只想要单序列报告就该少传这个参数。报错比静默出一份没有基准的报告更不容易让人误判。
///
/// ```sql
/// WITH benchmark AS (
///     SELECT list({'date': date, 'period_return': period_return}) AS series
///     FROM benchmark_returns
/// )
/// SELECT fund,
///        duckfn_quantstats_html(
///            date, period_return, benchmark.series,
///            {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::duckfn_quantstats_html_options)
/// FROM fund_returns, benchmark
/// GROUP BY fund;
/// ```
///
/// Benchmark report: the strategy side is aggregated row by row and the benchmark is a **list passed
/// in once**.
///
/// This is the four-argument overload. `benchmark` is
/// `STRUCT(date DATE, period_return DOUBLE)[]`, normally built by `list(...)` over a single-row
/// result; it is written once and evaluated once, and never repeats with the number of groups.
/// Start-date alignment is done inside quantstats-rs according to `match_dates` (true by default) and
/// is not repeated here.
///
/// A NULL or empty `benchmark` (or one whose points all miss their date/return) is an **error**: this
/// overload exists for the benchmark case, so a single-series report should simply omit the argument.
/// An error is harder to misread than a silently benchmark-less report.
#[duck_aggregate_function(overloads_name = "duckfn_quantstats_html")]
fn duckfn_quantstats_html_with_benchmark(
    date: DuckDate,
    period_return: f64,
    benchmark: Option<DuckLazy<Vec<QuantstatsReturnPoint>>>,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlBenchmarkState,
) -> DuckResult<()> {
    state.options.resolve(options.as_ref())?;
    state.benchmark.resolve(benchmark.as_ref())?;
    state.points.push(ReturnPoint {
        days_since_epoch: date.days_since_epoch,
        period_return,
    });
    Ok(())
}

impl DuckAggregateState for HtmlBenchmarkState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.merge(&other.options);
        self.benchmark.merge(&other.benchmark);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        // 空输入 → SQL NULL，与单序列重载一致。
        //
        // Empty input → SQL NULL, matching the single-series overload.
        if self.points.is_empty() {
            return Ok(None);
        }

        let benchmark_points = self.benchmark.get()?;
        let strategy_series = build_series(&self.points, None)?;
        let benchmark_series = build_series(&benchmark_points, None)?;

        let options = self
            .options
            .get()
            .to_report_options()?
            .with_benchmark(&benchmark_series);

        Ok(Some(render(&strategy_series, options)?))
    }
}
