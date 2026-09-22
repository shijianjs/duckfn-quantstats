// ============================================================================
// 收尾：按 symbol 逐份渲染 → 落盘 / 打开浏览器 → 回填清单
//
// 两条路径的最后一步只差「点要不要先差分」，所以共用 `render_reports` 一个实现：
//
//   render_return_reports  点本来就是收益率
//   render_price_reports   点先过 `prices_to_returns`（策略与基准走同一套规则）
//
// 顺序是刻意定的：
//
//   1. 先把每个标的的配置与「报告的去处」（[`ReportTarget`]）都定下来，顺带校验 `output` 非空、
//      `open_in_browser` 指的路径能不能交给浏览器，并**查重落盘路径** —— 两个标的写进同一个文件
//      等于静默丢报告，在渲染之前拦住比渲染之后才发现划算得多；
//   2. 再逐个渲染、落盘、按需开浏览器；
//   3. 最后把「实际写到哪」回填进返回行（没落盘就是 NULL）。
//
// 基准：配置里 `benchmark` 指的 symbol 只作输入、不出报告。它的序列**只转换一次**（价格路径上是
// 差分），所有标的共用；它不在表里、或转换后没有有效点时**报错** —— 这个键是调用方明确写下的，
// 静默出一份没有基准的报告比报错更容易让人误判。
//
// 配置直接从每个 symbol 的槽位取（`SymbolSlot::options_or_default`）：整列配置是 `NULL`、或那一行
// 都没轮到解析时，槽里没有解析结果，退化成 quantstats-rs 自己的全默认。
//
// 报告的去处由 [`ReportTarget`] 定下：配置里写了 `output` 就落盘（经 duckfn 的便捷层走 DuckDB 的
// VFS —— 本地磁盘 / 内存文件系统 / wasm 上的文件系统是同一条通路 —— 而不是 `std::fs`，细节见
// `write_report`）；另外要了 `open_in_browser` 就在落盘之后用系统默认浏览器打开它（没写 `output`
// 则先落一个临时文件，见 browser.rs；多个标的开多个标签页，这是它的自然语义）。
//
// 没有任何可出的报告（一行都没有，或每个标的转换后都没有有效点）返回 `Ok(None)`，即 SQL `NULL` ——
// 不要交给 `html()`，那边会报 `EmptySeries` 错误。
//
// The tail: render one report per symbol → write / open in a browser → fill the list in.
//
// The last step is the same on both paths and differs only in "do the points have to be differenced
// first", hence a single `render_reports` implementation:
//
//   render_return_reports  the points already are returns
//   render_price_reports   the points go through `prices_to_returns` (same rules for strategy and
//                          benchmark)
//
// The order is deliberate:
//
//   1. settle every instrument's options and where its report goes ([`ReportTarget`]), validating
//      `output` and `open_in_browser` on the way, and **check for duplicate output paths** — two
//      instruments writing into one file silently loses a report, and catching that before anything is
//      rendered is much cheaper than catching it after;
//   2. render, persist and open each one;
//   3. fill the actual path into the returned row (NULL when nothing was written).
//
// The benchmark: the symbol named by `benchmark` is input only and gets no report. Its series is
// converted **once** (differenced, on the price branch) and shared by every instrument; when it is not
// in the table, or has no valid point left after conversion, that is an **error** — the caller wrote
// that key explicitly, and a silently benchmark-less report would be easier to misread than a failure.
//
// The options come straight out of each symbol's slot (`SymbolSlot::options_or_default`): when the whole
// column was NULL, or no row of that symbol has been parsed yet, the slot holds no parse result and the
// report falls back to quantstats-rs' own all-defaults.
//
// Where a report goes is decided by [`ReportTarget`]: a configured `output` is written through duckfn's
// convenience layer on DuckDB's VFS (local disk / in-memory file systems / the wasm build's file system
// all take the same path) rather than `std::fs`, see `write_report`; and when `open_in_browser` was
// asked for, the report is opened with the system default browser afterwards (via a temporary file when
// no `output` was given, see browser.rs; several instruments open several tabs, which is what the option
// naturally means).
//
// Nothing to report at all (no row, or no instrument with a valid series after conversion) yields
// `Ok(None)`, i.e. SQL `NULL` — do not hand it to `html()`, which would fail with `EmptySeries`.
// ============================================================================

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use duckfn::{DuckOptionResult, DuckResult, duck_error, duck_vfs};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::types::html_report::QuantstatsHtmlReport;
use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::browser;
use super::kind::SeriesKind;
use super::series::{SeriesPoint, build_series, prices_to_returns};
use super::slots::SymbolTable;

/// 收益率路径的收尾：点已经是收益率，直接渲染。
///
/// The return branch's tail: the points already are returns, so they go straight to rendering.
pub(super) fn render_return_reports(
    kind: SeriesKind,
    symbols: &SymbolTable,
) -> DuckOptionResult<Vec<QuantstatsHtmlReport>> {
    render_reports(kind, symbols, |points| points.to_vec())
}

/// 价格路径的收尾：每个标的的点（包括基准）先按日期差分。
///
/// The price branch's tail: every instrument's points (the benchmark's included) are differenced by
/// date first.
pub(super) fn render_price_reports(
    kind: SeriesKind,
    symbols: &SymbolTable,
) -> DuckOptionResult<Vec<QuantstatsHtmlReport>> {
    render_reports(kind, symbols, prices_to_returns)
}

/// 一份待渲染的报告：谁、用什么配置、写到哪、用哪条序列。
///
/// One report waiting to be rendered: which symbol, with which options, written where, from which
/// series.
struct PlannedReport<'a> {
    /// 报告对应的 symbol（取自输入列，不是配置）。
    ///
    /// The symbol this report belongs to (from the input column, not from the options).
    symbol: &'a str,
    /// 该 symbol 的配置；没解析到就是全默认。
    ///
    /// This symbol's options; all defaults when nothing was parsed.
    options: Arc<QuantstatsHtmlOptions>,
    /// 报告的去处（`output` / `open_in_browser` 已经在这里校验过）。
    ///
    /// Where the report goes (`output` / `open_in_browser` were validated here).
    target: ReportTarget,
    /// 已经换算成收益率的点。
    ///
    /// The points, already converted into returns.
    returns: Vec<SeriesPoint>,
}

/// 收尾的实现：`to_returns` 是两条路径唯一的差别（收益率路径是恒等，价格路径是差分）。
///
/// The implementation behind the tail: `to_returns` is the only difference between the two paths (the
/// identity on the return branch, the differencing on the price branch).
fn render_reports(
    kind: SeriesKind,
    symbols: &SymbolTable,
    to_returns: impl Fn(&[SeriesPoint]) -> Vec<SeriesPoint>,
) -> DuckOptionResult<Vec<QuantstatsHtmlReport>> {
    // 一行都没有 → SQL NULL。这个提前返回同时也是「没有任何可出的报告 → NULL」的一侧。
    //
    // No row at all → SQL NULL. This early return is one half of "nothing to report → NULL".
    if symbols.is_empty() {
        return Ok(None);
    }

    // 每个 symbol 的配置（没解析到 = 全默认）。顺序由 `iter_sorted` 定下：结果 LIST 的顺序因此只
    // 取决于 symbol 名，不取决于 HashMap 的迭代顺序，也不取决于 DuckDB 的 merge 顺序。
    //
    // Every symbol's options (nothing parsed → all defaults). `iter_sorted` fixes the order, so the
    // returned LIST is ordered by symbol name instead of by HashMap iteration or DuckDB's merge order.
    let configured: Vec<(&str, Arc<QuantstatsHtmlOptions>)> = symbols
        .iter_sorted()
        .map(|(symbol, slot)| (symbol, slot.options_or_default()))
        .collect();

    let function = kind.function;
    let benchmark = benchmark_symbol(kind, &configured)?;

    // 基准序列只换算一次，所有标的共用 —— 别在下面的循环里对每个标的重算一遍。
    //
    // The benchmark series is converted once and shared by every instrument — do not recompute it per
    // instrument in the loop below.
    let benchmark_returns = match benchmark {
        Some(benchmark) => {
            let points = symbols
                .get(benchmark)
                .map(|slot| slot.points())
                .ok_or_else(|| {
                    duck_error(format!(
                        "{function}: no row for the benchmark symbol '{benchmark}' — the benchmark must \
                         be a symbol present in the input"
                    ))
                })?;

            let returns = to_returns(points);
            if returns.is_empty() {
                return Err(duck_error(format!(
                    "{function}: the benchmark symbol '{benchmark}' produced no returns — it needs at \
                     least two points, and on the price branch its predecessors must not be zero"
                )));
            }

            Some(returns)
        }
        None => None,
    };

    // 第一遍：把每个标的的去处定下来（顺带校验配置），并确认没有两个标的写同一个文件。
    //
    // First pass: settle where every instrument's report goes (validating its options on the way) and
    // make sure no two of them write into the same file.
    let mut planned = Vec::with_capacity(configured.len());
    for (symbol, options) in &configured {
        // 基准只作输入，不出报告。
        //
        // The benchmark is input only and gets no report.
        if Some(*symbol) == benchmark {
            continue;
        }

        let points = symbols.get(symbol).map(|slot| slot.points()).unwrap_or(&[]);
        let returns = to_returns(points);
        // 这个标的换算后没有有效点（整组只有一个点，或所有点都缺前值）→ 它没有报告可出，略过。
        // 与「一行都没有 → NULL」是同一套语义，不是错误。
        //
        // After conversion this instrument has no valid point (a single-point series, or every point
        // missing its predecessor) → there is no report for it, so it is left out. The same semantics
        // as "no row at all → NULL", not an error.
        if returns.is_empty() {
            continue;
        }

        planned.push(PlannedReport {
            symbol,
            options: Arc::clone(options),
            target: ReportTarget::new(options)?,
            returns,
        });
    }
    check_output_paths(kind, &planned)?;

    // 基准序列在这里建一次：下面每个标的的报告都要挂到同一个 `ReturnSeries` 上（`with_benchmark`
    // 借它）。
    //
    // The benchmark series is built once here: every report below attaches to that same `ReturnSeries`
    // (`with_benchmark` borrows it).
    let benchmark_series = match &benchmark_returns {
        Some(returns) => Some(build_series(returns, None)?),
        None => None,
    };

    // 第二遍：渲染 → 落盘 / 开浏览器 → 回填路径。
    //
    // Second pass: render → write / open in a browser → fill the path in.
    let mut reports = Vec::with_capacity(planned.len());
    for planned in planned {
        let series = build_series(&planned.returns, None)?;
        let mut report_options = planned.options.to_report_options(planned.symbol, benchmark)?;
        if let Some(benchmark_series) = &benchmark_series {
            report_options = report_options.with_benchmark(benchmark_series);
        }

        let report = render(kind, &series, report_options)?;
        planned.target.deliver(&report)?;

        reports.push(QuantstatsHtmlReport {
            symbol: planned.symbol.to_owned(),
            strategy_title: planned.options.strategy_title_or(planned.symbol),
            html: report,
            // 回填的就是这一次真正写出去的路径：`output` 原样，只在浏览器里打开时会退化成那个临时
            // 文件，两者都没配则为 NULL。
            //
            // This is the path this very call wrote to: `output` verbatim, the temporary file when the
            // browser was the only one asking for a file, and NULL when there is neither.
            file_path: planned.target.write_to().map(str::to_owned),
        });
    }

    if reports.is_empty() {
        Ok(None)
    } else {
        Ok(Some(reports))
    }
}

/// 配置里 `benchmark` 指出的那个 symbol；没有配置基准则是 `None`。
///
/// 一次调用里非 NULL 的 `benchmark` 取值必须一致，否则「谁把谁当基准」没有单一答案，被指的 symbol
/// 该不该出报告也就说不清了 —— 所以直接报错，而不是挑某一个用。
///
/// The symbol named by `benchmark` in the options, or `None` when no benchmark was configured.
///
/// Every non-NULL `benchmark` written in one call must agree, otherwise "which one is the benchmark" has
/// no single answer and whether the named symbol should also get a report becomes unanswerable — so this
/// is an error instead of picking one.
fn benchmark_symbol<'b>(
    kind: SeriesKind,
    configured: &'b [(&str, Arc<QuantstatsHtmlOptions>)],
) -> DuckResult<Option<&'b str>> {
    let mut found: Option<&'b str> = None;

    for (symbol, options) in configured {
        let Some(benchmark) = options.benchmark_name()? else {
            continue;
        };

        match found {
            Some(existing) if existing != benchmark => {
                return Err(duck_error(format!(
                    "{}: every symbol must use the same benchmark — found '{existing}' and \
                     '{benchmark}' (symbol '{symbol}')",
                    kind.function
                )));
            }
            _ => found = Some(benchmark),
        }
    }

    Ok(found)
}

/// 两个标的的 `output` 落到同一个路径就报错。
///
/// 覆盖是静默的：先写的报告会被后来的替换掉，调用方拿到两份「成功」的结果却只有一份文件。这属于配置
/// 写错（每个标的都该有自己的路径），所以直接说清是哪两个标的撞了。
///
/// Two instruments writing to the same `output` path is an error.
///
/// The overwrite is silent: the earlier report is replaced by the later one, and the caller gets two
/// "successful" rows but only one file. That is a misconfiguration (each instrument needs its own path),
/// so the error names both symbols.
fn check_output_paths(kind: SeriesKind, planned: &[PlannedReport<'_>]) -> DuckResult<()> {
    let mut seen: HashMap<&str, &str> = HashMap::new();

    for planned in planned {
        let Some(path) = planned.target.write_to() else {
            continue;
        };
        if let Some(first) = seen.insert(path, planned.symbol) {
            return Err(duck_error(format!(
                "{}: symbols '{first}' and '{symbol}' both write to '{path}' — give every symbol its own \
                 output path (for example 'reports/' || symbol || '.html')",
                kind.function,
                symbol = planned.symbol
            )));
        }
    }

    Ok(())
}

/// 渲染报告，并把 quantstats-rs 的错误转成 DuckDB 查询错误。
///
/// Render the report, turning quantstats-rs errors into DuckDB query errors.
fn render(kind: SeriesKind, series: &ReturnSeries, options: HtmlReportOptions<'_>) -> DuckResult<String> {
    let function = kind.function;
    html(series, options)
        .map_err(|err| duck_error(format!("{function}: cannot render the report: {err}")))
}

/// 报告的去处：写到哪个路径（可能不写），以及要不要在浏览器里打开。
///
/// 「写」与「打开」是两套路径，因此分开存：
///
/// - `write_to` 是配置里的 `output` 原样（相对路径就还是相对路径），由 DuckDB 的 VFS 去解释；它同时
///   也是返回行里 `file_path` 的来源；
/// - `open_with_browser` 是**本地绝对路径**，只给系统浏览器用 —— VFS 路径里可能有 `s3://` 这种浏览器
///   打不开的东西，相对路径也得先补成绝对路径。
///
/// 每个 symbol 各定一份（配置是逐行的，见 html_report_options.rs）：两者在 [`ReportTarget::new`] 里
/// 一次定下来，配置错误因此发生在渲染**之前**，不会白渲染几十份几百 KB 的报告。`output` 没写而又要在
/// 浏览器里打开时，落盘路径退化成 `browser::temporary_file` 建好的那个临时文件（浏览器需要一个真实
/// 存在的文件）。
///
/// Where a report goes: the path to write it to (possibly none) and whether to open it in a browser.
///
/// Writing and opening are two different paths and are kept apart:
///
/// - `write_to` is the configured `output`, verbatim (a relative path stays relative), interpreted by
///   DuckDB's VFS; it is also where the returned `file_path` comes from;
/// - `open_with_browser` is an **absolute local path** for the system browser only — a VFS path may be
///   something like `s3://…` that no browser can open, and a relative path has to be made absolute first.
///
/// There is one of these per symbol (the options are per row, see html_report_options.rs): both are
/// settled in [`ReportTarget::new`], so a bad configuration is reported **before** anything is rendered
/// rather than after dozens of few-hundred-KB reports. When `output` is not set but the browser was asked
/// for, the write target falls back to the temporary file `browser::temporary_file` created (a browser
/// needs a file that actually exists).
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

    /// 这次真正会写出去的路径；不落盘则为 `None`（也就是返回行里 `file_path` 的值）。
    ///
    /// The path this target will actually write to; `None` when nothing is written (which is the value
    /// that ends up in the returned `file_path`).
    pub(super) fn write_to(&self) -> Option<&str> {
        self.write_to.as_deref()
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
