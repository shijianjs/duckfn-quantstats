// ============================================================================
// 收尾：点 → ReturnSeries → 渲染 HTML
//
// 四条路径的最后一步都一样，差别只在「单序列还是带基准」，所以收敛成这两个函数。
//
// 配置直接从状态里的配置槽取（`DuckLazySlot<QuantstatsHtmlOptions>`）：整列配置是 `NULL`、或这一组一行
// 都没有过时，槽里没有解析结果，退化成 quantstats-rs 自己的全默认。
//
// 空输入（一行都没有，或价格差分后没有有效点）返回 `Ok(None)`，即 SQL `NULL` —— 不要交给 `html()`，
// 那边会报 `EmptySeries` 错误。
//
// The tail: points → ReturnSeries → rendered HTML.
//
// The last step is the same on all four paths and only differs in "single series or with a benchmark",
// hence these two functions.
//
// The options come straight out of the state's options slot (`DuckLazySlot<QuantstatsHtmlOptions>`): when
// the whole column was NULL, or the group never had a row, the slot holds no parse result and the report
// falls back to quantstats-rs' own all-defaults.
//
// Empty input (no row at all, or no valid point after differencing prices) yields `Ok(None)`, i.e. SQL
// `NULL` — do not hand it to `html()`, which would fail with `EmptySeries`.
// ============================================================================

use duckfn::{DuckLazySlot, DuckOptionResult, DuckResult, duck_error};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::kind::SeriesKind;
use super::series::{SeriesPoint, build_series};

/// 渲染报告，并把 quantstats-rs 的错误转成 DuckDB 查询错误。
///
/// Render the report, turning quantstats-rs errors into DuckDB query errors.
fn render(kind: SeriesKind, series: &ReturnSeries, options: HtmlReportOptions<'_>) -> DuckResult<String> {
    let function = kind.function;
    html(series, options)
        .map_err(|err| duck_error(format!("{function}: cannot render the report: {err}")))
}

/// 单序列收尾：点（已经是收益率）→ `ReturnSeries` → 渲染。
///
/// 空输入返回 `Ok(None)`，见模块头。
///
/// The single-series tail: points (already returns) → `ReturnSeries` → render.
///
/// Empty input yields `Ok(None)`, see the module header.
pub(super) fn render_single(
    kind: SeriesKind,
    options: &DuckLazySlot<QuantstatsHtmlOptions>,
    points: &[SeriesPoint],
) -> DuckOptionResult<String> {
    if points.is_empty() {
        return Ok(None);
    }

    let series = build_series(points, None)?;
    let options = options.get().unwrap_or_default();

    Ok(Some(render(kind, &series, options.to_report_options()?)?))
}

/// 带基准收尾：两侧的点都已经是收益率。
///
/// The benchmark tail: the points of both sides are already returns.
pub(super) fn render_with_benchmark(
    kind: SeriesKind,
    options: &DuckLazySlot<QuantstatsHtmlOptions>,
    strategy_points: &[SeriesPoint],
    benchmark_points: &[SeriesPoint],
) -> DuckOptionResult<String> {
    if strategy_points.is_empty() {
        return Ok(None);
    }

    let strategy_series = build_series(strategy_points, None)?;
    let benchmark_series = build_series(benchmark_points, None)?;
    let options = options.get().unwrap_or_default();
    let report_options = options
        .to_report_options()?
        .with_benchmark(&benchmark_series);

    Ok(Some(render(kind, &strategy_series, report_options)?))
}
