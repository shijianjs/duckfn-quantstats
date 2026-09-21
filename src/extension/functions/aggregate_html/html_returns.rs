// ============================================================================
// 路径 1/4：收益率序列，单序列（3 参）
// 路径 2/4：收益率序列，带基准（4 参）
//
// 两个重载共用 SQL 名字 `duckfn_quantstats_html`，按参数个数分派；两侧的点都是**收益率**，
// 不需要任何换算（价格路径在 html_prices.rs）。
//
// Paths 1/4 and 2/4: the return series, single (3 arguments) and with a benchmark (4 arguments).
//
// Both overloads share the SQL name `duckfn_quantstats_html` and are dispatched by argument count; the
// points on both sides are **returns**, so no conversion is needed here (the price branch lives in
// html_prices.rs).
// ============================================================================

use duckfn::{
    DuckAggregateState, DuckDate, DuckLazy, DuckLazySlot, DuckOptionResult, DuckResult,
    duck_aggregate_function,
};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;
use crate::extension::types::return_point::QuantstatsReturnPoint;

use super::kind::RETURNS;
use super::report::{render_single, render_with_benchmark};
use super::series::SeriesPoint;
use super::slots::{benchmark_points, resolve_benchmark};

/// 单序列报告聚合状态（收益率路径）。
///
/// Aggregate state for the single-series return report.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlReportState {
    /// 配置槽：首行解析一次，之后每行只加一次引用计数。
    ///
    /// The options slot: parsed on the first row, a refcount bump on every later one.
    options: DuckLazySlot<QuantstatsHtmlOptions>,
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
    state.options.resolve_optional(options.as_ref())?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: period_return,
    });
    Ok(())
}

impl DuckAggregateState for HtmlReportState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.combine(&other.options);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        render_single(RETURNS, &self.options, &self.points)
    }
}

/// 带基准报告聚合状态（收益率路径）。
///
/// Aggregate state for the benchmark report over a return series.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlBenchmarkState {
    options: DuckLazySlot<QuantstatsHtmlOptions>,
    /// 基准列表的槽：同样是首行解析一次 —— `Vec<T>` 的逐元素复制只发生在那一行。
    ///
    /// The benchmark list slot: likewise parsed on the first row only, which is where the element-by-
    /// element copy of the `Vec<T>` happens.
    benchmark: DuckLazySlot<Vec<QuantstatsReturnPoint>>,
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
    state.options.resolve_optional(options.as_ref())?;
    resolve_benchmark(&mut state.benchmark, benchmark.as_ref(), RETURNS)?;
    state.points.push(SeriesPoint {
        days_since_epoch: date.days_since_epoch,
        value: period_return,
    });
    Ok(())
}

impl DuckAggregateState for HtmlBenchmarkState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.options.combine(&other.options);
        self.benchmark.combine(&other.benchmark);
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        // 一行都没有 → SQL NULL。这个提前返回是必需的，不只是省事：一次 update 都没跑过时基准槽还是
        // 未解析的，先取它就会把「整列都是 NULL」这个含义误当成「没读过基准」。
        //
        // No row at all → SQL NULL. This early return is required, not just an optimisation: with zero
        // update calls nothing has parsed the benchmark slot yet, and reading it first would mistake
        // "the whole column is NULL" for something else.
        if self.points.is_empty() {
            return Ok(None);
        }

        let benchmark_points = benchmark_points(&self.benchmark, RETURNS)?;
        render_with_benchmark(RETURNS, &self.options, &self.points, &benchmark_points)
    }
}
