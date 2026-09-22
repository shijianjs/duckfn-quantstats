// ============================================================================
// 路径 3/4：价格/净值序列，单序列（3 参）
// 路径 4/4：价格/净值序列，带基准（4 参）
//
// 两个重载共用 SQL 名字 `qs_html_report_by_prices`，按参数个数分派。与收益率路径（html_returns.rs）
// 唯一的区别是：这里收进来的 `price` 是**价格/净值**，要在 `result()` 里先差分成收益率再渲染，
// 差分规则见 series.rs 的 `prices_to_returns`。
//
// 它必须与收益率那一支分开命名：`(date, price, options)` 与 `(date, period_return, options)`
// 的类型序列完全一样，同一个名字下无法按类型分派。
//
// Paths 3/4 and 4/4: the price/NAV series, single (3 arguments) and with a benchmark (4 arguments).
//
// Both overloads share the SQL name `qs_html_report_by_prices` and are dispatched by argument count.
// The only difference from the return branch (html_returns.rs) is that `price` here is a **price/NAV** and
// has to be differenced into returns in `result()` before rendering; the rules live in
// `prices_to_returns` in series.rs.
//
// It needs its own name because `(date, price, options)` and `(date, period_return, options)` have exactly
// the same type sequence, so one name could not dispatch them.
// ============================================================================

use duckfn::{
    DuckAggregateState, DuckDate, DuckLazy, DuckLazySlot, DuckOptionResult, DuckResult,
    duck_aggregate_function, duck_error,
};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;
use crate::extension::types::price_point::QuantstatsPricePoint;

use super::kind::PRICES;
use super::report::{render_single, render_with_benchmark};
use super::series::{SeriesPoint, prices_to_returns};
use super::slots::{benchmark_points, resolve_benchmark};

/// 单序列报告聚合状态（价格路径）。
///
/// Aggregate state for the single-series price report.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlPriceState {
    /// 配置槽：首行解析一次，之后每行只加一次引用计数。
    ///
    /// The options slot: parsed on the first row, a refcount bump on every later one.
    options: DuckLazySlot<QuantstatsHtmlOptions>,
    /// 这里存的是**价格/净值**，差分留到 `result()` 里做。
    ///
    /// This holds **prices/NAVs**; the differencing happens in `result()`.
    points: Vec<SeriesPoint>,
}

/// 价格/净值序列的单序列报告：对 `date` 与 `price` 两列做聚合，内部换算成收益率后输出 HTML 报告。
///
/// 三参数那次重载。换算规则见 `prices_to_returns`：按日期排序、逐点算 `price_t / price_{t-1} - 1`，
/// 首个点丢弃，前值缺失/为 0 时跳过该点。点数不足 2 个（或全被跳过）时按「没有有效行」返回 `NULL`。
///
/// ```sql
/// SELECT fund, qs_html_report_by_prices(trade_date, nav, NULL)
/// FROM nav_table GROUP BY fund;
/// ```
///
/// The single-series report over a price/NAV series: aggregates the `date` and `price` columns and
/// converts them into returns internally.
///
/// This is the three-argument overload. See `prices_to_returns` for the conversion rules: sort by date,
/// then `price_t / price_{t-1} - 1` per point, dropping the first and skipping a point whose predecessor
/// is missing or zero. Fewer than two points (or everything skipped) is treated as "no valid row" and
/// returns `NULL`.
// `pub(super)` 不是要把它当公开 API：属性宏会把函数包进同名模块，而模块沿用函数的可见性，
// kind.rs 要从那儿读生成的 `SQL_NAME`（见 kind.rs 的说明）。
//
// `pub(super)` is not about making this public API: the attribute macro wraps the function in a
// same-named module that inherits the function's visibility, and kind.rs reads the generated `SQL_NAME`
// from there (see kind.rs).
#[duck_aggregate_function(overloads_name = "qs_html_report_by_prices")]
pub(super) fn qs_html_report_by_prices(
    date: DuckDate,
    price: f64,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlPriceState,
) -> DuckResult<()> {
    state.options.resolve_optional(options.as_ref())?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: price,
    });
    Ok(())
}

impl DuckAggregateState for HtmlPriceState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.combine(&other.options);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        render_single(PRICES, &self.options, &prices_to_returns(&self.points))
    }
}

/// 带基准报告聚合状态（价格路径）。
///
/// Aggregate state for the benchmark report over a price series.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlPriceBenchmarkState {
    options: DuckLazySlot<QuantstatsHtmlOptions>,
    /// 这里存的也是基准的**价格/净值**，差分留到 `result()` 里做。
    ///
    /// This holds the benchmark's **prices/NAVs** too; the differencing happens in `result()`.
    benchmark: DuckLazySlot<Vec<QuantstatsPricePoint>>,
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
///        qs_html_report_by_prices(
///            date, price, benchmark.series,
///            {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::qs_html_report_options)
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
#[duck_aggregate_function(overloads_name = "qs_html_report_by_prices")]
pub(super) fn qs_html_report_by_prices_with_benchmark(
    date: DuckDate,
    price: f64,
    benchmark: Option<DuckLazy<Vec<QuantstatsPricePoint>>>,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlPriceBenchmarkState,
) -> DuckResult<()> {
    state.options.resolve_optional(options.as_ref())?;
    resolve_benchmark(&mut state.benchmark, benchmark.as_ref(), PRICES)?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: price,
    });
    Ok(())
}

impl DuckAggregateState for HtmlPriceBenchmarkState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.combine(&other.options);
        self.benchmark.combine(&other.benchmark);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        // 策略侧一行都没有 → NULL（与其它三支一致）。
        //
        // No strategy row at all → NULL (same as the other three paths).
        if self.points.is_empty() {
            return Ok(None);
        }

        let benchmark_prices = benchmark_points(&self.benchmark, PRICES)?;
        let benchmark_returns = prices_to_returns(&benchmark_prices);

        // `benchmark_points()` 只保证「列表里有带日期和值的点」，差分之后仍可能什么都不剩（基准只有一个
        // 点）。这时不能交给 build_series（会报 EmptySeries 那种含糊的错误），要给一句能直接看懂的话。
        //
        // `benchmark_points()` only guarantees "the list has points with a date and a value"; differencing
        // may still leave nothing (a benchmark of a single point). Handing that to build_series would fail
        // with a vague EmptySeries error, so it gets an explicit message.
        if benchmark_returns.is_empty() {
            return Err(duck_error(format!(
                "{}: the benchmark prices produced no returns — at least two points are needed",
                PRICES.function
            )));
        }

        let strategy_points = prices_to_returns(&self.points);
        render_with_benchmark(PRICES, &self.options, &strategy_points, &benchmark_returns)
    }
}
