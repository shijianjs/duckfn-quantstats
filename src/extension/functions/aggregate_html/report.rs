// ============================================================================
// 收尾：点 → ReturnSeries → 渲染 HTML
//
// 四条路径的最后一步都一样，差别只在「单序列还是带基准」，所以收敛成这两个函数。
//
// 配置直接从状态里的配置槽取（`DuckLazySlot<QuantstatsHtmlOptions>`）：整列配置是 `NULL`、或这一组一行
// 都没有过时，槽里没有解析结果，退化成 quantstats-rs 自己的全默认。
//
// 配置里写了 `output` 就顺带落盘：经 duckfn 的便捷层走 DuckDB 的 VFS（本地磁盘 / 内存文件系统 /
// wasm 上的文件系统是同一条通路），而不是 `std::fs`；细节见 `write_report`。
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
// When the configuration asks for `output`, persisting the report is part of this tail too — through
// duckfn's convenience layer on DuckDB's VFS (local disk / in-memory file systems / the wasm build's
// file system all take the same path) rather than `std::fs`, see `write_report`.
//
// Empty input (no row at all, or no valid point after differencing prices) yields `Ok(None)`, i.e. SQL
// `NULL` — do not hand it to `html()`, which would fail with `EmptySeries`.
// ============================================================================

use duckfn::{DuckLazySlot, DuckOptionResult, DuckResult, duck_error, duck_vfs};
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
    // 先校验 `output`（空路径直接报错），免得白渲染一份报告才失败。
    //
    // Validate `output` first (an empty path fails right away) instead of rendering a report for
    // nothing and only then reporting the bad configuration.
    let output_path = options.output_path()?;
    let report = render(kind, &series, options.to_report_options()?)?;

    if let Some(path) = output_path {
        write_report(path, &report)?;
    }

    Ok(Some(report))
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
    let output_path = options.output_path()?;
    let report_options = options
        .to_report_options()?
        .with_benchmark(&benchmark_series);
    let report = render(kind, &strategy_series, report_options)?;

    if let Some(path) = output_path {
        write_report(path, &report)?;
    }

    Ok(Some(report))
}

/// 把渲染好的报告按配置里的 `output` 落盘。
///
/// 走 duckfn 的便捷层 [`duck_vfs::write_string`]，它内部经 **DuckDB 的 VFS** 写入，而不是 `std::fs`：
/// 本地磁盘、内存文件系统、wasm 构建里的文件系统、装了 httpfs 的 `s3://` / `http(s)://` 是同一条通路、
/// 同一套语义（`std::fs` 只看得到本地磁盘，wasm 下更是没有可写的地方）。聚合函数能写文件也不靠额外
/// 机制：duckfn 在注册期留了一条自有长连接，便捷层每次调用都在它上面现取句柄。
///
/// 「C API 没有 truncate」这个坑由便捷层处理掉了 —— 覆盖写就是覆盖写（旧文件更长时它内部先清零再写），
/// 所以这里只是两件事的收尾：写、把错误原样抛出。错误里已经带了操作名与路径
/// （`duckfn::duck_vfs::write: '<path>': ...`），不必再包一层。
///
/// Persists the rendered report to the configured `output` path.
///
/// It goes through duckfn's convenience layer [`duck_vfs::write_string`], which writes via **DuckDB's
/// VFS** rather than `std::fs`: local disk, in-memory file systems, the wasm build's file system and
/// `s3://` / `http(s)://` once httpfs is loaded all take the same path with the same semantics
/// (`std::fs` only ever sees local disk, and on wasm there is nowhere writable at all). An aggregate
/// needs no extra machinery to write either: duckfn keeps an owned long-lived connection from
/// registration time, and the convenience layer takes fresh handles from it on every call.
///
/// The "C API has no truncate" pitfall is dealt with inside that layer — replace really replaces (a
/// longer existing file is zeroed first), so all that is left here is writing and propagating the
/// error. The error already carries the operation and the path (`duckfn::duck_vfs::write: '<path>':
/// ...`), so there is nothing to wrap it in.
fn write_report(path: &str, report: &str) -> DuckResult<()> {
    duck_vfs::write_string(path, report)
}
