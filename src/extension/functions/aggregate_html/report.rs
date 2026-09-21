// ============================================================================
// 收尾：点 → ReturnSeries → 渲染 HTML
//
// 四条路径的最后一步都一样，差别只在「单序列还是带基准」，所以收敛成这两个函数。
//
// 配置直接从状态里的配置槽取（`DuckLazySlot<QuantstatsHtmlOptions>`）：整列配置是 `NULL`、或这一组一行
// 都没有过时，槽里没有解析结果，退化成 quantstats-rs 自己的全默认。
//
// 配置里写了 `output` 的话，落盘也在这里收尾 —— 走 DuckDB 的 VFS 而不是 `std::fs`，见 `write_report`。
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
// DuckDB's VFS rather than `std::fs`, see `write_report`.
//
// Empty input (no row at all, or no valid point after differencing prices) yields `Ok(None)`, i.e. SQL
// `NULL` — do not hand it to `html()`, which would fail with `EmptySeries`.
// ============================================================================

use std::ffi::CString;

use duckfn::{
    DuckLazySlot, DuckOptionResult, DuckResult, ErrorData, FileOpenOptions, duck_error,
    with_file_system,
};
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
        write_report(kind, path, &report)?;
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
        write_report(kind, path, &report)?;
    }

    Ok(Some(report))
}

/// 把渲染好的报告写到配置里的路径：经 **DuckDB 的 VFS**，而不是 `std::fs`。
///
/// # 为什么不用 std::fs
///
/// `std::fs` 只看得到本地磁盘，而且在 `wasm32-unknown-emscripten` 下根本没有可写的文件系统（这正是
/// 之前该字段在 wasm 上被直接忽略的原因）。DuckDB 的 VFS 覆盖本地磁盘、内存文件系统、wasm 构建里
/// 宿主真正的那个文件系统，装了 httpfs 还覆盖 `s3://` / `http(s)://` —— 一条通路，一套语义。
///
/// 聚合函数在 C API 里拿不到客户端上下文（没有 bind 回调，也没有
/// `duckdb_aggregate_function_get_client_context`），所以 duckfn 在注册期留了一条自有长连接，
/// `with_file_system` 就是在它上面现取 `ClientContext` → `FileSystem`；这里只用一次、句柄不逸出。
///
/// # Writes the rendered report to the configured path, through **DuckDB's VFS**, not `std::fs`.
///
/// # Why not std::fs
///
/// `std::fs` only ever sees local disk, and under `wasm32-unknown-emscripten` there is no writable file
/// system at all (which is exactly why this field used to be ignored on wasm). DuckDB's VFS covers
/// local disk, in-memory file systems, the file system the wasm build actually exposes, and
/// `s3://` / `http(s)://` once httpfs is loaded — one path, one set of semantics.
///
/// An aggregate gets no client context from the C API (no bind callback, no
/// `duckdb_aggregate_function_get_client_context`), so duckfn keeps an owned long-lived connection from
/// registration time; `with_file_system` takes a fresh `ClientContext` → `FileSystem` from it and
/// nothing escapes.
///
/// # 已知限制：不能 truncate
///
/// DuckDB 的 C API 只有「需要时新建」（`DUCKDB_FILE_FLAG_CREATE` → `FILE_FLAGS_FILE_CREATE`，在
/// 本地文件系统上是 `O_CREAT` / `OPEN_ALWAYS`），没有映射到 `O_TRUNC` / `CREATE_ALWAYS` 的那个标志
/// （C++ 侧的 `FILE_FLAGS_FILE_CREATE_NEW`，C API 里拿不到），所以覆盖一个**更长**的旧文件会在尾部
/// 留下残渣。这里选择**报错**而不是静默留下一份「报告 + 垃圾」：让用户删掉文件或换个路径，
/// 比让他以为落盘成功了要好。
///
/// # Known limitation: no truncate
///
/// DuckDB's C API only has "create if needed" (`DUCKDB_FILE_FLAG_CREATE` → `FILE_FLAGS_FILE_CREATE`,
/// which is `O_CREAT` / `OPEN_ALWAYS` on the local file system); the flag that maps to `O_TRUNC` /
/// `CREATE_ALWAYS` is the C++-side `FILE_FLAGS_FILE_CREATE_NEW`, which the C API does not expose.
/// Overwriting a **longer** existing file therefore leaves a tail behind, and that is reported as an
/// error rather than silently producing "report plus garbage": failing loudly beats letting the user
/// believe the write succeeded.
fn write_report(kind: SeriesKind, path: &str, report: &str) -> DuckResult<()> {
    let SeriesKind { function, .. } = kind;

    // DuckDB 的 C API 收 C 字符串；含 NUL 字节的路径在这里就挡掉。
    //
    // The C API takes C strings, so a path with a NUL byte is rejected here.
    let c_path = CString::new(path).map_err(|_| {
        duck_error(format!(
            "{function}: cannot write the report: the path contains a NUL byte"
        ))
    })?;

    with_file_system(|file_system| {
        let options = FileOpenOptions::write_create();
        let handle = file_system
            .open(&c_path, &options)
            .map_err(|error| file_error(kind, path, error))?;

        handle
            .write_all(report.as_bytes())
            .map_err(|error| file_error(kind, path, error))?;

        let size = handle
            .size()
            .map_err(|error| file_error(kind, path, error))?;
        let written = report.len() as u64;
        if size > written {
            return Err(duck_error(format!(
                "{function}: cannot write the report to '{path}': the file already exists and is \
                 longer ({size} bytes) than the report ({written} bytes), and DuckDB's C API cannot \
                 truncate it — delete the file or write to a new path"
            )));
        }

        Ok(())
    })
}

/// 把文件系统的结构化错误转成查询错误，带上函数名与路径。
///
/// Turns the file system's structured error into a query error, tagging the function name and the path.
fn file_error(kind: SeriesKind, path: &str, error: ErrorData) -> quack_rs::error::ExtensionError {
    duck_error(format!(
        "{}: cannot write the report to '{path}': {}",
        kind.function,
        error
            .message()
            .unwrap_or_else(|| String::from("unknown file system error"))
    ))
}
