// ============================================================================
// 收尾：点 → ReturnSeries → 渲染 HTML → 落盘 / 打开浏览器
//
// 四条路径的最后一步都一样，差别只在「单序列还是带基准」，所以收敛成这两个函数。
//
// 配置直接从状态里的配置槽取（`DuckLazySlot<QuantstatsHtmlOptions>`）：整列配置是 `NULL`、或这一组一行
// 都没有过时，槽里没有解析结果，退化成 quantstats-rs 自己的全默认。
//
// 报告的去处由 `ReportTarget` 定下：配置里写了 `output` 就落盘（经 duckfn 的便捷层走 DuckDB 的 VFS ——
// 本地磁盘 / 内存文件系统 / wasm 上的文件系统是同一条通路 —— 而不是 `std::fs`，细节见 `write_report`）；
// 另外要了 `open_in_browser` 就在落盘之后用系统默认浏览器打开它（没写 `output` 则先落一个临时文件，
// 见 browser.rs）。
//
// 空输入（一行都没有，或价格差分后没有有效点）返回 `Ok(None)`，即 SQL `NULL` —— 不要交给 `html()`，
// 那边会报 `EmptySeries` 错误。
//
// The tail: points → ReturnSeries → rendered HTML → written to disk / opened in a browser.
//
// The last step is the same on all four paths and only differs in "single series or with a benchmark",
// hence these two functions.
//
// The options come straight out of the state's options slot (`DuckLazySlot<QuantstatsHtmlOptions>`): when
// the whole column was NULL, or the group never had a row, the slot holds no parse result and the report
// falls back to quantstats-rs' own all-defaults.
//
// Where the report goes is decided by `ReportTarget`: a configured `output` is written through duckfn's
// convenience layer on DuckDB's VFS (local disk / in-memory file systems / the wasm build's file system all
// take the same path) rather than `std::fs`, see `write_report`; and when `open_in_browser` was asked for,
// the report is opened with the system default browser afterwards (via a temporary file when no `output` was
// given, see browser.rs).
//
// Empty input (no row at all, or no valid point after differencing prices) yields `Ok(None)`, i.e. SQL
// `NULL` — do not hand it to `html()`, which would fail with `EmptySeries`.
// ============================================================================

use std::path::PathBuf;

use duckfn::{DuckLazySlot, DuckOptionResult, DuckResult, duck_error, duck_vfs};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::browser;
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
    // 先把报告的去处定下来（顺带校验 `output` 与 `open_in_browser`），免得白渲染一份报告才失败。
    //
    // Settle where the report goes first (validating `output` and `open_in_browser` on the way) instead of
    // rendering a report for nothing and only then reporting the bad configuration.
    let target = ReportTarget::new(&options)?;
    let report = render(kind, &series, options.to_report_options()?)?;
    target.deliver(&report)?;

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
    let target = ReportTarget::new(&options)?;
    let report_options = options
        .to_report_options()?
        .with_benchmark(&benchmark_series);
    let report = render(kind, &strategy_series, report_options)?;
    target.deliver(&report)?;

    Ok(Some(report))
}

/// 报告的去处：写到哪个路径（可能不写），以及要不要在浏览器里打开。
///
/// 「写」与「打开」是两套路径，因此分开存：
///
/// - `write_to` 是配置里的 `output` 原样（相对路径就还是相对路径），由 DuckDB 的 VFS 去解释；
/// - `open_with_browser` 是**本地绝对路径**，只给系统浏览器用 —— VFS 路径里可能有 `s3://` 这种浏览器
///   打不开的东西，相对路径也得先补成绝对路径。
///
/// 两者在 [`ReportTarget::new`] 里一次定下来，配置错误因此发生在渲染**之前**，不会白渲染一份几百 KB
/// 的报告。`output` 没写而又要在浏览器里打开时，落盘路径退化成 `browser::temporary_file` 建好的那个临时
/// 文件（浏览器需要一个真实存在的文件）。
///
/// Where a report goes: the path to write it to (possibly none) and whether to open it in a browser.
///
/// Writing and opening are two different paths and are kept apart:
///
/// - `write_to` is the configured `output`, verbatim (a relative path stays relative), interpreted by
///   DuckDB's VFS;
/// - `open_with_browser` is an **absolute local path** for the system browser only — a VFS path may be
///   something like `s3://…` that no browser can open, and a relative path has to be made absolute first.
///
/// Both are settled in [`ReportTarget::new`], so a bad configuration is reported **before** anything is
/// rendered rather than after a few hundred KB of work. When `output` is not set but the browser was asked
/// for, the write target falls back to the temporary file `browser::temporary_file` created (a browser needs
/// a file that actually exists).
pub(super) struct ReportTarget {
    /// 落盘路径，配置原样；`None` 表示不落盘。
    ///
    /// The path to write to, verbatim from the configuration; `None` means nothing is written.
    write_to: Option<String>,
    /// 要交给浏览器的本地绝对路径；`None` 表示不打开。
    ///
    /// The absolute local path handed to the browser; `None` means nothing is opened.
    open_with_browser: Option<PathBuf>,
}

impl ReportTarget {
    /// 按配置定下报告的去处，顺带校验与它相关的取值（`output` 是不是空串、非本地路径能不能打开）。
    ///
    /// Resolve where the report goes from the configuration, validating the related option values on the way
    /// (whether `output` is an empty string, whether a non-local path could be opened at all).
    pub(super) fn new(options: &QuantstatsHtmlOptions) -> DuckResult<Self> {
        // 先看要不要打开（wasm 下恒为 false）：它同时决定「要不要校验 `output` 能不能打开」与
        // 「没有 `output` 时要不要先落一个临时文件」。
        //
        // Is the browser wanted at all (always false on wasm)? That one answer decides both whether `output`
        // has to be openable and whether a temporary file is needed when it is unset.
        let open_in_browser = browser::is_requested(options);

        // 空字符串的 `output` 在这里就报掉（`output_path`）。
        //
        // An empty `output` is reported right here, by `output_path`.
        let write_to = match options.output_path()? {
            Some(path) => Some(path.to_owned()),
            // 要在浏览器里打开却没有落盘路径：让 browser 那边先把临时文件建好，报告写进去就是。
            //
            // The browser was asked for but no path was configured: let the browser side create the
            // temporary file first, then write the report into it.
            None if open_in_browser => browser::temporary_file(options)?,
            None => None,
        };

        let open_with_browser = match write_to.as_deref() {
            Some(path) if open_in_browser => Some(browser::local_path(path)?),
            _ => None,
        };

        Ok(Self {
            write_to,
            open_with_browser,
        })
    }

    /// 落盘（配置里写了路径时）并按需用系统默认浏览器打开 —— 先写后开，浏览器打开时文件一定已经在了。
    ///
    /// Write the report (when a path was configured) and open it in the system default browser when asked —
    /// the write comes first, so the file is always there by the time the browser looks at it.
    pub(super) fn deliver(&self, report: &str) -> DuckResult<()> {
        if let Some(path) = &self.write_to {
            write_report(path, report)?;
        }
        if let Some(path) = &self.open_with_browser {
            browser::open_report(path)?;
        }

        Ok(())
    }
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
