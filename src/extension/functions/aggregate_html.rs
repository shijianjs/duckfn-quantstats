use std::sync::Arc;

use chrono::{NaiveDate, TimeDelta};
use duckfn::{
    DuckAggregateState, DuckDate, DuckLazy, DuckOptionResult, DuckResult, DuckValueType,
    duck_aggregate_function, duck_error,
};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;
use crate::extension::types::price_point::QuantstatsPricePoint;
use crate::extension::types::return_point::QuantstatsReturnPoint;

// ============================================================================
// 两个 SQL 名字、四个聚合重载
//
// 收益率序列（一行 = 一个周期的收益率）：
//
//   duckfn_quantstats_html(date, period_return, options)                  单序列报告
//   duckfn_quantstats_html(date, period_return, benchmark, options)       带基准报告
//
// 价格/净值序列（一行 = 一天的价格或净值，函数内部换算成收益率）：
//
//   duckfn_quantstats_html_prices(date, price, options)                   单序列报告
//   duckfn_quantstats_html_prices(date, price, benchmark, options)        带基准报告
//
// 每个名字下两个签名只差一个 `benchmark` 参数，所以用 `overloads_name` 各注册成一个**函数集**，
// 按参数个数分派（`register_all_aggregate_overload` 会按名字分组，每个重载各自带参数表与返回类型）。
// 参数顺序固定为「数据列在前、配置在后」。
//
// 价格那一支**必须**另起一个名字：`(date, price, options)` 与 `(date, period_return, options)`
// 的类型序列完全一样（都是 `DATE, DOUBLE, STRUCT`），同一个名字下无法按类型分派。
//
// # 价格/净值 → 收益率
//
// 组内先按 `date` 排序，再逐点算 `price_t / price_{t-1} - 1`：每组的第一个点没有前值，丢弃；
// 前值不是有限数、或为 0 时该点也跳过（与「NULL 行跳过」同一语义，而不是报错或往序列里塞 inf/NaN）。
// 同一分组内同一天应当只有一个点，否则差分出来的是那一天内部的变动，没有意义。
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
// 报告只在 result() 里生成一次 —— 即每个分组一次。GROUP BY 100 个标的就会渲染 100 份完整报告
// （每份内含十几张 SVG），这是预期行为，不是性能 bug。同理，每个分组仍会各自持有一份解析好的基准点：
// 聚合状态不跨分组共享，这部分开销消不掉，能省掉的是 DuckDB 层的行展开与扫描。
//
// 排序：DuckDB 并行/分块执行时 combine 的调用顺序不保证，所以这里只做「拼接」，
// 真正的按日期排序交给 ReturnSeries::new（它内部会 sort）。因此 SQL 侧**不需要 ORDER BY**；
// 价格那一支要先把点排好才能差分，见 prices_to_returns。
//
// Two SQL names, four aggregate overloads.
//
// Return series (one row per period's return):
//
//   duckfn_quantstats_html(date, period_return, options)                  single series
//   duckfn_quantstats_html(date, period_return, benchmark, options)       with a benchmark
//
// Price/NAV series (one row per day's price, converted to returns inside the function):
//
//   duckfn_quantstats_html_prices(date, price, options)                   single series
//   duckfn_quantstats_html_prices(date, price, benchmark, options)        with a benchmark
//
// The two signatures under each name differ only by the `benchmark` argument, so `overloads_name`
// registers each pair as **one function set**, dispatched by argument count
// (`register_all_aggregate_overload` groups by name and every overload keeps its own parameter list and
// return type). The argument order is always "data columns first, options last".
//
// The price branch **needs its own name**: `(date, price, options)` and `(date, period_return, options)`
// have exactly the same type sequence (`DATE, DOUBLE, STRUCT`), so one name could not dispatch them.
//
// # Price/NAV → returns
//
// The points of a group are sorted by `date` first, then each becomes `price_t / price_{t-1} - 1`: the
// first point has no predecessor and is dropped, and a point whose predecessor is not finite or is zero
// is skipped as well — the same "skip it" semantics as a NULL row, rather than an error or an inf/NaN
// inside the series. A group should hold one point per date, otherwise the difference describes the
// movement within that date, which is meaningless.
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
//
// The report is rendered once in result(), i.e. once per group. GROUP BY over 100 instruments renders 100
// full reports (each with a dozen inline SVGs); that is expected, not a performance bug. For the same
// reason every group holds its own parsed copy of the benchmark points: aggregate states are not shared
// across groups, so that part cannot be avoided — what this design removes is DuckDB's row expansion and
// scanning.
//
// Ordering: DuckDB may call combine in any order when running in parallel or in chunks, so combine only
// concatenates; the real date ordering is done by ReturnSeries::new (which sorts internally). SQL
// therefore does **not** need an ORDER BY. (The price branch has to sort before differencing, see
// prices_to_returns.)
// ============================================================================

/// 一条路径在 SQL 侧的两个名字：函数名（兼错误信息前缀）与值列名（错误提示里要举例的键）。
///
/// The two SQL-side names of one branch: the function name (also the error-message prefix) and the value
/// column name (the key to show in error hints).
#[derive(Clone, Copy)]
struct SeriesKind {
    /// 注册名，同时也用作错误信息前缀。
    ///
    /// The registered name, also used as the error-message prefix.
    function: &'static str,
    /// 值列在 SQL 里的键名。
    ///
    /// The key of the value column in SQL.
    value_field: &'static str,
}

/// 收益率序列路径。
///
/// 注意：函数集名字只写在 `#[duck_aggregate_function(overloads_name = "...")]` 上，宏属性只能吃字面量，
/// 所以这里的 `function` 与那两处字符串必须手动保持一致。
///
/// The return-series branch.
///
/// Note: the function-set name lives only on the `#[duck_aggregate_function(overloads_name = "...")]`
/// attributes — macro attributes accept literals only, so `function` here and those two strings must be
/// kept in sync by hand.
const RETURNS: SeriesKind = SeriesKind {
    function: "duckfn_quantstats_html",
    value_field: "period_return",
};

/// 价格/净值序列路径。
///
/// 值列叫 `price` 是照搬 Python quantstats 的词汇：它把「收益率或价格」这类输入统称 prices，
/// 内部对看起来像价格的序列自动做 pct_change。净值（NAV）严格说不是 price，但 quantstats 也不区分，
/// 净值序列照样当 prices 喂 —— 所以这里同样用 `price` 这一个键收下这类水平值。
///
/// The price/NAV branch.
///
/// The value column is called `price`, borrowing Python quantstats' vocabulary: it lumps this kind of
/// input under "prices" and runs pct_change on anything that looks like a price series. A NAV is not
/// strictly a price, but quantstats does not distinguish either — a NAV series goes in as prices just
/// the same — so a single `price` key takes in all of these level values.
const PRICES: SeriesKind = SeriesKind {
    function: "duckfn_quantstats_html_prices",
    value_field: "price",
};

// ============================================================================
// 内部用的「一个点」
// ============================================================================

/// 序列里的一个点：**推迟**日期换算，只留自 1970-01-01 起的天数。
///
/// `value` 是通用的：走收益率路径时它是收益率，走价格路径时它是价格/净值（差分之后同样是收益率）。
/// 用具名字段而不是 `(i32, f64)` 元组：两个字段类型不同、含义也不同，元组在调用点很容易写反
/// （四条路径都要用到它）。
///
/// One point of a series: the conversion to a calendar date is deferred, only the day count since
/// 1970-01-01 is kept.
///
/// `value` is generic on purpose: it is a return on the return branch and a price/NAV on the price branch
/// (and again a return once differenced). A named struct rather than an `(i32, f64)` tuple: the two fields
/// differ in type and meaning, and a tuple is easy to get backwards at a call site (all four paths use
/// it).
#[derive(Clone, Copy, Debug)]
struct SeriesPoint {
    /// 自 1970-01-01 起的天数（换算成 `NaiveDate` 可能失败，所以留到 `build_series` 里做）。
    ///
    /// Days since 1970-01-01 (converting to `NaiveDate` can fail, so it is left to `build_series`).
    days_since_epoch: i32,
    /// 该点的值：收益率，或价格/净值。
    ///
    /// The value of the point: a return, or a price/NAV.
    value: f64,
}

/// 把 `DuckDate`（自 1970-01-01 起的天数）换成 chrono 的 `NaiveDate`。
///
/// `ReturnSeries` 要的是 `NaiveDate`，而 duckfn 的 `DuckDate` 只存天数，换算只能自己做。
/// 越界（例如 `DATE 'infinity'` 之类的极端值）返回查询错误，不用会 panic 的运算符。
///
/// Convert a `DuckDate` (days since 1970-01-01) into a chrono `NaiveDate`.
///
/// `ReturnSeries` wants `NaiveDate`, while duckfn's `DuckDate` only stores days, so the conversion is on
/// us. Out-of-range values (e.g. `DATE 'infinity'`) become a query error instead of a panic.
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
/// A list of points → `ReturnSeries`. Sorting happens inside `ReturnSeries::new`, so SQL does not need an
/// `ORDER BY` and the concatenation order inside `combine` does not matter.
fn build_series(points: &[SeriesPoint], name: Option<String>) -> DuckResult<ReturnSeries> {
    let mut dates = Vec::with_capacity(points.len());
    let mut values = Vec::with_capacity(points.len());
    for point in points {
        dates.push(naive_date(point.days_since_epoch)?);
        values.push(point.value);
    }

    ReturnSeries::new(dates, values, name)
        .map_err(|err| duck_error(format!("cannot build the returns series: {err}")))
}

/// 价格/净值序列 → 收益率序列。
///
/// 组内先按日期排序，再逐点算 `price_t / price_{t-1} - 1`：第一个点没有前值，丢弃；前值缺失、为 0
/// 或不是有限数时该点也跳过。跳过只影响它自己 —— 下一个点仍然和它自己的前一个点比，不做链式累乘。
///
/// 点数不足 2 个（或全被跳过）时返回空数组，调用方按「没有有效行」处理。
///
/// Price/NAV series → return series.
///
/// The points of a group are sorted by date first, then each becomes `price_t / price_{t-1} - 1`: the
/// first point has no predecessor and is dropped, and a point whose predecessor is missing, zero or not
/// finite is skipped as well. Skipping affects only that point — the next one is still compared with its
/// own predecessor, there is no chained multiplication.
///
/// Fewer than two points (or everything skipped) yields an empty vector, which callers treat as "no valid
/// row".
fn prices_to_returns(prices: &[SeriesPoint]) -> Vec<SeriesPoint> {
    let mut sorted = prices.to_vec();
    sorted.sort_by_key(|point| point.days_since_epoch);

    let mut returns = Vec::with_capacity(sorted.len().saturating_sub(1));
    for pair in sorted.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        if !previous.value.is_finite() || previous.value == 0.0 {
            continue;
        }
        returns.push(SeriesPoint {
            days_since_epoch: current.days_since_epoch,
            value: current.value / previous.value - 1.0,
        });
    }
    returns
}

/// 渲染报告，并把 quantstats-rs 的错误转成 DuckDB 查询错误。
///
/// Render the report, turning quantstats-rs errors into DuckDB query errors.
fn render(
    kind: SeriesKind,
    series: &ReturnSeries,
    options: HtmlReportOptions<'_>,
) -> DuckResult<String> {
    let function = kind.function;
    html(series, options)
        .map_err(|err| duck_error(format!("{function}: cannot render the report: {err}")))
}

/// 单序列收尾：点（已经是收益率）→ `ReturnSeries` → 渲染。
///
/// 空输入（一行都没有，或价格差分后没有有效点）返回 `Ok(None)`，即 SQL `NULL` —— 不要交给 `html()`，
/// 那边会报 `EmptySeries` 错误。
///
/// The single-series tail: points (already returns) → `ReturnSeries` → render.
///
/// Empty input (no row at all, or no valid point after differencing prices) yields `Ok(None)`, i.e. SQL
/// `NULL` — do not hand it to `html()`, which would fail with `EmptySeries`.
fn render_single(
    kind: SeriesKind,
    options: &OptionsSlot,
    points: &[SeriesPoint],
) -> DuckOptionResult<String> {
    if points.is_empty() {
        return Ok(None);
    }

    let series = build_series(points, None)?;

    Ok(Some(render(kind, &series, options.get().to_report_options()?)?))
}

/// 带基准收尾：两侧的点都已经是收益率。
///
/// The benchmark tail: the points of both sides are already returns.
fn render_with_benchmark(
    kind: SeriesKind,
    options: &OptionsSlot,
    strategy_points: &[SeriesPoint],
    benchmark_points: &[SeriesPoint],
) -> DuckOptionResult<String> {
    if strategy_points.is_empty() {
        return Ok(None);
    }

    let strategy_series = build_series(strategy_points, None)?;
    let benchmark_series = build_series(benchmark_points, None)?;
    let report_options = options
        .get()
        .to_report_options()?
        .with_benchmark(&benchmark_series);

    Ok(Some(render(kind, &strategy_series, report_options)?))
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
// A `DuckLazy<T>` token is only valid inside the callback that produced it, so storing it in the aggregate
// state and consuming it past the chunk or on another thread is rejected by the runtime guard
// ("DuckLazy<T> is stale"). These slots therefore keep the parsed value, and combine merely moves that
// value across — the same column parses to the same content everywhere, so re-parsing is neither needed
// nor allowed.
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
    /// `try_get()` rather than `get()`: the latter turns failures into panics (which the adapter's unwind
    /// wrapper converts back into a query error), while here we are already in a `DuckResult` context and
    /// can propagate the error directly.
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

/// 基准点「归一化」成内部表示。
///
/// 收益率路径与价格路径的点结构体只差一个字段名（`period_return` / `price`），聚合对它们要做的事完全
/// 一样，所以抽一层把它们统一成 [`SeriesPoint`]。
///
/// Normalising a benchmark point into the internal representation.
///
/// The point structs of the two branches differ only in one field name (`period_return` / `price`) and the
/// aggregate does exactly the same with both, so this trait normalises them into [`SeriesPoint`].
trait IntoSeriesPoint {
    /// 缺日期或缺失值的点返回 `None` —— 跳过该点，而不是让整条查询失败。
    ///
    /// A point missing its date or its value yields `None` — the point is skipped rather than failing the
    /// whole query.
    fn into_series_point(self) -> Option<SeriesPoint>;
}

impl IntoSeriesPoint for QuantstatsReturnPoint {
    fn into_series_point(self) -> Option<SeriesPoint> {
        let (Some(date), Some(value)) = (self.date, self.period_return) else {
            return None;
        };
        Some(SeriesPoint {
            days_since_epoch: date.days_since_epoch,
            value,
        })
    }
}

impl IntoSeriesPoint for QuantstatsPricePoint {
    fn into_series_point(self) -> Option<SeriesPoint> {
        let (Some(date), Some(value)) = (self.date, self.price) else {
            return None;
        };
        Some(SeriesPoint {
            days_since_epoch: date.days_since_epoch,
            value,
        })
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
    fn resolve<T: IntoSeriesPoint + DuckValueType>(
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
    /// `Unresolved` is unreachable here: `result()` already returns `NULL` early when a group has no row at
    /// all. The branch exists only so that no theoretically unreachable path unwraps.
    fn get(&self, kind: SeriesKind) -> DuckResult<Arc<Vec<SeriesPoint>>> {
        match self {
            BenchmarkSlot::Resolved(points) => Ok(Arc::clone(points)),
            BenchmarkSlot::Unresolved => Err(duck_error(format!(
                "{}: the benchmark list was never read",
                kind.function
            ))),
        }
    }
}

// ============================================================================
// 路径 1/4：收益率序列，单序列（3 参）
// ============================================================================

/// 单序列报告聚合状态（收益率路径）。
///
/// Aggregate state for the single-series return report.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlReportState {
    options: OptionsSlot,
    points: Vec<SeriesPoint>,
}

/// 收益率序列的单序列报告：对 `date` 与 `period_return` 两列做聚合，输出该组的完整 HTML 报告字符串。
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
/// The single-series report over a return series: aggregates the `date` and `period_return` columns into
/// the full HTML report string of that group.
///
/// This is the three-argument overload. A row whose `date` or `period_return` is NULL is skipped entirely
/// — duckfn's existing semantics for non-`Option` arguments, and the usual SQL aggregate behaviour.
#[duck_aggregate_function(overloads_name = "duckfn_quantstats_html")]
fn duckfn_quantstats_html(
    date: DuckDate,
    period_return: f64,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlReportState,
) -> DuckResult<()> {
    state.options.resolve(options.as_ref())?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: period_return,
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
        render_single(RETURNS, &self.options, &self.points)
    }
}

// ============================================================================
// 路径 2/4：收益率序列，带基准（4 参）
// ============================================================================

/// 带基准报告聚合状态（收益率路径）。
///
/// Aggregate state for the benchmark report over a return series.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlBenchmarkState {
    options: OptionsSlot,
    benchmark: BenchmarkSlot,
    points: Vec<SeriesPoint>,
}

/// 收益率序列的带基准报告：策略侧逐行聚合，基准侧是**一次性传入的列表**。
///
/// 四参数那次重载。`benchmark` 是 `STRUCT(date DATE, period_return DOUBLE)[]`，通常由 `list(...)`
/// 在一张单行结果里构造出来；它只写一次、只求值一次，不会随分组的数量重复出现。两侧的起始日期对齐
/// 由 quantstats-rs 按 `match_dates`（默认 true）完成，这里不做额外处理。
///
/// `benchmark` 为 NULL 或为空（或每个点都缺日期/值）时**直接报错**：这一支重载就是为带基准的场景
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
/// The benchmark report over a return series: the strategy side is aggregated row by row and the
/// benchmark is a **list passed in once**.
///
/// This is the four-argument overload. `benchmark` is `STRUCT(date DATE, period_return DOUBLE)[]`,
/// normally built by `list(...)` over a single-row result; it is written once and evaluated once, and
/// never repeats with the number of groups. Start-date alignment is done inside quantstats-rs according
/// to `match_dates` (true by default) and is not repeated here.
///
/// A NULL or empty `benchmark` (or one whose points all miss their date/value) is an **error**: this
/// overload exists for the benchmark case, so a single-series report should simply omit the argument. An
/// error is harder to misread than a silently benchmark-less report.
#[duck_aggregate_function(overloads_name = "duckfn_quantstats_html")]
fn duckfn_quantstats_html_with_benchmark(
    date: DuckDate,
    period_return: f64,
    benchmark: Option<DuckLazy<Vec<QuantstatsReturnPoint>>>,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlBenchmarkState,
) -> DuckResult<()> {
    state.options.resolve(options.as_ref())?;
    state.benchmark.resolve(benchmark.as_ref(), RETURNS)?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: period_return,
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
        // 一行都没有 → SQL NULL。这个提前返回是必需的，不只是省事：一次 update 都没跑过时基准槽还是
        // `Unresolved`，先取它就会撞上「从未解析」那条兜底错误。
        //
        // No row at all → SQL NULL. This early return is required, not just an optimisation: with zero
        // update calls the benchmark slot is still `Unresolved`, and reading it first would hit the
        // "never read" fallback error.
        if self.points.is_empty() {
            return Ok(None);
        }

        let benchmark_points = self.benchmark.get(RETURNS)?;
        render_with_benchmark(RETURNS, &self.options, &self.points, &benchmark_points)
    }
}

// ============================================================================
// 路径 3/4：价格/净值序列，单序列（3 参）
// ============================================================================

/// 单序列报告聚合状态（价格路径）。
///
/// Aggregate state for the single-series price report.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlPriceState {
    options: OptionsSlot,
    /// 这里存的是**价格/净值**，差分留到 `result()` 里做。
    ///
    /// This holds **prices/NAVs**; the differencing happens in `result()`.
    points: Vec<SeriesPoint>,
}

/// 价格/净值序列的单序列报告：对 `date` 与 `price` 两列做聚合，内部换算成收益率后输出 HTML 报告。
///
/// 三参数那次重载。换算规则见本模块顶部「价格/净值 → 收益率」：按日期排序、逐点算
/// `price_t / price_{t-1} - 1`，首个点丢弃，前值缺失/为 0 时跳过该点。点数不足 2 个（或全被跳过）
/// 时按「没有有效行」返回 `NULL`。
///
/// ```sql
/// SELECT fund, duckfn_quantstats_html_prices(trade_date, nav, NULL)
/// FROM nav_table GROUP BY fund;
/// ```
///
/// The single-series report over a price/NAV series: aggregates the `date` and `price` columns and
/// converts them into returns internally.
///
/// This is the three-argument overload. See "Price/NAV → returns" at the top of the module for the
/// conversion rules: sort by date, then `price_t / price_{t-1} - 1` per point, dropping the first and
/// skipping a point whose predecessor is missing or zero. Fewer than two points (or everything skipped)
/// is treated as "no valid row" and returns `NULL`.
#[duck_aggregate_function(overloads_name = "duckfn_quantstats_html_prices")]
fn duckfn_quantstats_html_prices(
    date: DuckDate,
    price: f64,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlPriceState,
) -> DuckResult<()> {
    state.options.resolve(options.as_ref())?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: price,
    });
    Ok(())
}

impl DuckAggregateState for HtmlPriceState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.merge(&other.options);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        render_single(PRICES, &self.options, &prices_to_returns(&self.points))
    }
}

// ============================================================================
// 路径 4/4：价格/净值序列，带基准（4 参）
// ============================================================================

/// 带基准报告聚合状态（价格路径）。
///
/// Aggregate state for the benchmark report over a price series.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlPriceBenchmarkState {
    options: OptionsSlot,
    /// 这里存的是基准的**价格/净值**，差分留到 `result()` 里做。
    ///
    /// This holds the benchmark's **prices/NAVs**; the differencing happens in `result()`.
    benchmark: BenchmarkSlot,
    points: Vec<SeriesPoint>,
}

/// 价格/净值序列的带基准报告：策略与基准都是价格/净值序列，函数内部各自换算成收益率。
///
/// 四参数那次重载。`benchmark` 是 `STRUCT(date DATE, price DOUBLE)[]`，同样由 `list(...)` 在一张
/// 单行结果里构造、只求值一次。两侧都按上面同一套规则差分，之后再走 `align_start_dates` 对齐。
///
/// ```sql
/// WITH benchmark AS (
///     SELECT list({'date': date, 'price': price}) AS series FROM benchmark_nav
/// )
/// SELECT fund,
///        duckfn_quantstats_html_prices(
///            date, price, benchmark.series,
///            {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::duckfn_quantstats_html_options)
/// FROM fund_nav, benchmark
/// GROUP BY fund;
/// ```
///
/// The benchmark report over price/NAV series: both the strategy and the benchmark are prices, and the
/// function converts each side into returns itself.
///
/// This is the four-argument overload. `benchmark` is `STRUCT(date DATE, price DOUBLE)[]`, likewise built
/// by `list(...)` over a single-row result and evaluated once. Both sides are differenced with the same
/// rules as above, and only then go through `align_start_dates`.
#[duck_aggregate_function(overloads_name = "duckfn_quantstats_html_prices")]
fn duckfn_quantstats_html_prices_with_benchmark(
    date: DuckDate,
    price: f64,
    benchmark: Option<DuckLazy<Vec<QuantstatsPricePoint>>>,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlPriceBenchmarkState,
) -> DuckResult<()> {
    state.options.resolve(options.as_ref())?;
    state.benchmark.resolve(benchmark.as_ref(), PRICES)?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: price,
    });
    Ok(())
}

impl DuckAggregateState for HtmlPriceBenchmarkState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.merge(&other.options);
        self.benchmark.merge(&other.benchmark);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        // 策略侧一行都没有 → NULL（与其它三支一致）。
        //
        // No strategy row at all → NULL (same as the other three paths).
        if self.points.is_empty() {
            return Ok(None);
        }

        let benchmark_prices = self.benchmark.get(PRICES)?;
        let benchmark_points = prices_to_returns(&benchmark_prices);

        // 基准的价格点至少要两个才能差出收益率；`resolve()` 只保证了「点非空」，差分之后可能为空。
        // 这时不能交给 build_series（会报 EmptySeries 那种含糊的错误），要给一句能直接看懂的话。
        //
        // At least two benchmark prices are needed to produce a single return; `resolve()` only guarantees
        // "some points", which may still difference down to nothing. Handing that to build_series would
        // fail with a vague EmptySeries error, so it gets an explicit message.
        if benchmark_points.is_empty() {
            return Err(duck_error(format!(
                "{}: the benchmark prices produced no returns — at least two points are needed",
                PRICES.function
            )));
        }

        let strategy_points = prices_to_returns(&self.points);
        render_with_benchmark(PRICES, &self.options, &strategy_points, &benchmark_points)
    }
}
