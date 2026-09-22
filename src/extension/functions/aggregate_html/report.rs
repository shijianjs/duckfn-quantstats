// ============================================================================
// 收尾：按 (标的, 基准) 逐份渲染 → 落盘 / 打开浏览器 → 回填清单
//
// 两条路径的最后一步只差「点要不要先差分」，所以共用 `render_reports` 一个实现：
//
//   render_return_reports  点本来就是收益率
//   render_price_reports   点先过 `prices_to_returns`（策略与基准走同一套规则）
//
// # 一份报告 = 一个 (标的, 基准) 对
//
// 报告只能有一个基准（quantstats-rs 的 `HtmlReportOptions` 就是一个 `Option<&ReturnSeries>`），所以
// 「一个标的对多个基准」落成**多份报告**：`benchmark: ['SPX', 'NDX']` + 3 个标的 = 6 行。顺序是
// 「标的按 symbol 升序，同一标的内按基准列表给出的顺序」，所以基准列表的顺序就是报告的顺序。
//
// 基准的序列每个只转换一次（价格路径上是差分），所有标的共用；被指为基准的 symbol 只作输入、不出报告。
//
// # 顺序与落盘
//
// 顺序是刻意定的：
//
//   1. 先把每个标的的序列、以及每个 (标的, 基准) 对的落盘目标（[`ReportTarget`]）都定下来，顺带校验
//      `output_dir` 非空、`open_in_browser` 指的路径能不能交给浏览器 —— 配置错误因此发生在渲染之前，
//      不会白渲染几十份几百 KB 的报告；
//   2. 再逐个渲染、落盘、按需开浏览器；
//   3. 最后把「实际写到哪」回填进返回行（没落盘就是 NULL）。
//
// 文件名不给用户填（见 [`ReportTarget`]）：`output_dir` 只给目录，名字由 naming.rs 按「时间 + 策略名 +
// 基准名 + 随机尾缀」生成，`report_path` 再确认它没被占用 —— 于是不管一个标的对几个基准、一次调用写多少
// 文件，都不存在互相覆盖这回事。
//
// 配置直接从每个 symbol 的槽位取（`SymbolSlot::options_or_default`）：整列配置是 `NULL`、或那一行
// 都没轮到解析时，槽里没有解析结果，退化成 quantstats-rs 自己的全默认。
//
// 没有任何可出的报告（一行都没有，或每个标的转换后都没有有效点）返回 `Ok(None)`，即 SQL `NULL` ——
// 不要交给 `html()`，那边会报 `EmptySeries` 错误。
//
// The tail: render one report per (symbol, benchmark) pair → write / open in a browser → fill the list in.
//
// The last step is the same on both paths and differs only in "do the points have to be differenced first",
// hence a single `render_reports` implementation:
//
//   render_return_reports  the points already are returns
//   render_price_reports   the points go through `prices_to_returns` (same rules for strategy and benchmark)
//
// # One report = one (symbol, benchmark) pair
//
// A report can only carry one benchmark (quantstats-rs' `HtmlReportOptions` holds a single
// `Option<&ReturnSeries>`), so "one instrument against several benchmarks" becomes **several reports**:
// `benchmark: ['SPX', 'NDX']` with 3 instruments yields 6 rows. The order is "symbols ascending, and within
// one symbol the order of the configured benchmark list", which makes that list the order of the reports.
//
// Each benchmark's series is converted exactly once (differenced first, on the price branch) and shared by
// every instrument; a symbol named as a benchmark is input only and gets no report.
//
// # Order and persistence
//
// The order is deliberate:
//
//   1. settle every instrument's series and every (symbol, benchmark) pair's destination ([`ReportTarget`]),
//      validating `output_dir` and `open_in_browser` on the way — a bad configuration is therefore reported
//      before anything is rendered, rather than after dozens of few-hundred-KB reports;
//   2. render, persist and open each one;
//   3. fill the actual path into the returned row (NULL when nothing was written).
//
// The file name is not the caller's to type (see [`ReportTarget`]): `output_dir` only takes a directory and
// naming.rs builds the name from "time + strategy + benchmark + random suffix", which `report_path` then
// checks is free — so no matter how many benchmarks an instrument has or how many files one call writes,
// they cannot overwrite each other.
//
// The options come straight out of each symbol's slot (`SymbolSlot::options_or_default`): when the whole
// column was NULL, or no row of that symbol has been parsed yet, the slot holds no parse result and the
// report falls back to quantstats-rs' own all-defaults.
//
// Nothing to report at all (no row, or no instrument with a valid series after conversion) yields
// `Ok(None)`, i.e. SQL `NULL` — do not hand it to `html()`, which would fail with `EmptySeries`.
// ============================================================================

use std::path::PathBuf;
use std::sync::Arc;

use duckfn::{DuckOptionResult, DuckResult, duck_error, duck_vfs};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::types::html_report::QuantstatsHtmlReport;
use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::browser;
use super::kind::SeriesKind;
use super::naming;
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

/// 一个标的的「一份报告」所需的一切：序列、配置、落盘目标（基准由外部按列表顺序给出）。
///
/// Everything one report of one instrument needs: its series, its options and its destination (the
/// benchmark comes from the outer list order).
struct StrategyReport<'a> {
    /// 报告对应的 symbol。
    ///
    /// The symbol this report belongs to.
    symbol: &'a str,
    /// 该 symbol 的配置；没解析到就是全默认。
    ///
    /// This symbol's options; all defaults when nothing was parsed.
    options: Arc<QuantstatsHtmlOptions>,
    /// 已经换算成收益率的策略序列。同一个标的的所有基准共用它。
    ///
    /// The strategy series, already converted into returns. Every benchmark of this instrument shares it.
    series: ReturnSeries,
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
    let benchmarks = benchmark_names(kind, &configured)?;

    // 基准序列每个只转换一次，所有标的共用 —— 别在下面的循环里对每个标的重算一遍。
    //
    // Each benchmark series is converted once and shared by every instrument — do not recompute it per
    // instrument in the loop below.
    let mut benchmark_series = Vec::with_capacity(benchmarks.len());
    for benchmark in &benchmarks {
        let points = symbols
            .get(benchmark)
            .map(|slot| slot.points())
            .ok_or_else(|| {
                duck_error(format!(
                    "{function}: no row for the benchmark symbol '{benchmark}' — the benchmark must be \
                     a symbol present in the input"
                ))
            })?;

        let returns = to_returns(points);
        if returns.is_empty() {
            return Err(duck_error(format!(
                "{function}: the benchmark symbol '{benchmark}' produced no returns — it needs at least \
                 two points, and on the price branch its predecessors must not be zero"
            )));
        }

        benchmark_series.push(build_series(&returns, None)?);
    }

    // 第一遍：把要出报告的标的挑出来（基准只作输入）并建好序列。换算后没有有效点的标的略过 ——
    // 「没有报告可出」与「一行都没有 → NULL」是同一套语义，不是错误。
    //
    // First pass: pick the instruments that get a report (the benchmark is input only) and build their
    // series. An instrument with no valid point after conversion is left out — the same semantics as
    // "no row at all → NULL", not an error.
    let mut strategies: Vec<StrategyReport<'_>> = Vec::with_capacity(configured.len());
    for (symbol, options) in &configured {
        if benchmarks.contains(symbol) {
            continue;
        }

        let points = symbols.get(symbol).map(|slot| slot.points()).unwrap_or(&[]);
        let returns = to_returns(points);
        if returns.is_empty() {
            continue;
        }

        strategies.push(StrategyReport {
            symbol,
            options: Arc::clone(options),
            series: build_series(&returns, None)?,
        });
    }

    // 每个标的的落盘目标：没配基准时一个（单序列报告），否则每个基准一个。全部在这里定下来，所以
    // `output_dir` / `open_in_browser` 的配置错误发生在渲染之前。
    //
    // Every instrument's destinations: one when no benchmark is configured (a single-series report),
    // otherwise one per benchmark. They are all settled here, so `output_dir` / `open_in_browser`
    // misconfiguration surfaces before anything is rendered.
    let targets: Vec<Vec<ReportTarget>> = strategies
        .iter()
        .map(|strategy| -> DuckResult<Vec<ReportTarget>> {
            if benchmarks.is_empty() {
                return Ok(vec![ReportTarget::new(
                    &strategy.options,
                    strategy.symbol,
                    None,
                )?]);
            }

            benchmarks
                .iter()
                .map(|benchmark| {
                    ReportTarget::new(&strategy.options, strategy.symbol, Some(benchmark))
                })
                .collect()
        })
        .collect::<DuckResult<_>>()?;

    // 第二遍：渲染 → 落盘 / 开浏览器 → 回填路径。
    //
    // Second pass: render → write / open in a browser → fill the path in.
    let mut reports = Vec::with_capacity(
        strategies
            .len()
            .saturating_mul(benchmarks.len().max(1)),
    );
    for (strategy, targets) in strategies.iter().zip(&targets) {
        for (index, target) in targets.iter().enumerate() {
            // 没配基准时 `benchmarks` 是空的，`get(0)` 就是 `None` —— 与只有一个目标的那一支对上。
            //
            // With no benchmark configured `benchmarks` is empty and `get(0)` is `None`, which is what
            // the single-destination branch means.
            let benchmark = benchmarks.get(index).copied();
            let mut report_options = strategy.options.to_report_options(strategy.symbol, benchmark)?;
            if benchmark.is_some() {
                report_options = report_options.with_benchmark(&benchmark_series[index]);
            }

            let report = render(kind, &strategy.series, report_options)?;
            target.deliver(&report)?;

            reports.push(QuantstatsHtmlReport {
                symbol: strategy.symbol.to_owned(),
                benchmark: benchmark.map(str::to_owned),
                strategy_title: strategy.options.strategy_title_or(strategy.symbol),
                html: report,
                // 回填的就是这一次真正写出去的路径：`output_dir` 下那个自动命名的文件，或只在浏览器里
                // 打开时的临时文件，两者都没配则为 NULL。
                //
                // This is the path this very call wrote to: the auto-named file under `output_dir`, the
                // temporary file when the browser was the only one asking for a file, and NULL when there
                // is neither.
                file_path: target.write_to().map(str::to_owned),
            });
        }
    }

    if reports.is_empty() {
        Ok(None)
    } else {
        Ok(Some(reports))
    }
}

/// 配置里给出的基准 symbol 列表；没配基准时是空数组。
///
/// 一次调用里所有标的必须给**同一个列表**（元素与顺序都一致），否则「谁把谁当基准」没有单一答案，
/// 被指的 symbol 该不该出报告也就说不清了 —— 所以直接报错，而不是挑某一个用。列表本身的问题（空串、
/// NULL 元素、重复、多基准时还写了 `benchmark_title`）由 [`QuantstatsHtmlOptions::benchmark_names`] 拦。
///
/// The benchmark symbols configured; empty when no benchmark was configured.
///
/// Every instrument in one call has to supply the **same list** (same entries, same order), otherwise "which
/// one is the benchmark" has no single answer and whether the named symbol should also get a report becomes
/// unanswerable — so this is an error instead of picking one. Problems inside the list itself (an empty
/// string, a NULL element, a duplicate, a `benchmark_title` set alongside several benchmarks) are caught by
/// [`QuantstatsHtmlOptions::benchmark_names`].
fn benchmark_names<'b>(
    kind: SeriesKind,
    configured: &'b [(&str, Arc<QuantstatsHtmlOptions>)],
) -> DuckResult<Vec<&'b str>> {
    let mut agreed: Option<Vec<&'b str>> = None;

    for (symbol, options) in configured {
        let names = options.benchmark_names()?;

        match &agreed {
            None => agreed = Some(names),
            Some(existing) if *existing != names => {
                return Err(duck_error(format!(
                    "{}: every symbol must use the same benchmark list — found [{}] and [{}] (symbol \
                     '{symbol}')",
                    kind.function,
                    existing.join(", "),
                    names.join(", ")
                )));
            }
            Some(_) => {}
        }
    }

    Ok(agreed.unwrap_or_default())
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
    html(series, options).map_err(|err| duck_error(format!("{function}: cannot render the report: {err}")))
}

/// 报告的去处：写到哪个文件（可能不写），以及要不要在浏览器里打开。
///
/// 「写」与「打开」是两套路径，因此分开存：
///
/// - `write_to` 是**函数自己定的完整路径** —— `output_dir` 下一个自动命名的文件（见 `report_path`），
///   或者没有 `output_dir` 时要打开浏览器而新建的临时文件；它同时也是返回行里 `file_path` 的来源；
/// - `open_with_browser` 是**本地绝对路径**，只给系统浏览器用 —— VFS 路径里可能有 `s3://` 这种浏览器
///   打不开的东西，相对路径也得先补成绝对路径。
///
/// 每个 (标的, 基准) 对各定一份：两者在 [`ReportTarget::new`] 里一次定下来，配置错误因此发生在渲染
/// **之前**，不会白渲染几十份几百 KB 的报告。
///
/// Where a report goes: the file to write it to (possibly none) and whether to open it in a browser.
///
/// Writing and opening are two different paths and are kept apart:
///
/// - `write_to` is the **full path the function picked for itself** — an auto-named file under
///   `output_dir` (see `report_path`), or a freshly created temporary file when there is no `output_dir`
///   but the browser was asked for; it is also where the returned `file_path` comes from;
/// - `open_with_browser` is an **absolute local path** for the system browser only — a VFS path may be
///   something like `s3://…` that no browser can open, and a relative path has to be made absolute first.
///
/// There is one of these per (symbol, benchmark) pair: both are settled in [`ReportTarget::new`], so a bad
/// configuration is reported **before** anything is rendered rather than after dozens of few-hundred-KB
/// reports.
pub(super) struct ReportTarget {
    /// 落盘路径，函数自己命名的；`None` 表示不落盘。
    ///
    /// The path to write to, named by the function itself; `None` means nothing is written.
    write_to: Option<String>,
    /// 要交给浏览器的本地绝对路径；`None` 表示不打开。
    ///
    /// The absolute local path handed to the browser; `None` means nothing is opened.
    open_with_browser: Option<PathBuf>,
}

impl ReportTarget {
    /// 按配置定下这一份报告的去处，顺带校验与它相关的取值（`output_dir` 是不是空串、非本地路径能不能
    /// 打开）。
    ///
    /// Resolve where this one report goes from the options, validating the related values on the way
    /// (whether `output_dir` is an empty string, whether a non-local path could be opened at all).
    pub(super) fn new(
        options: &QuantstatsHtmlOptions,
        symbol: &str,
        benchmark: Option<&str>,
    ) -> DuckResult<Self> {
        // 先看要不要打开（wasm 下恒为 false）：它同时决定「要不要校验落盘路径能不能打开」与
        // 「没有 `output_dir` 时要不要先落一个临时文件」。
        //
        // Is the browser wanted at all (always false on wasm)? That one answer decides both whether the
        // write path has to be openable and whether a temporary file is needed when `output_dir` is unset.
        let open_in_browser = browser::is_requested(options);

        // 空字符串的 `output_dir` 在这里就报掉（`output_dir_path`）。
        //
        // An empty `output_dir` is reported right here, by `output_dir_path`.
        let write_to = match options.output_dir_path()? {
            Some(dir) => Some(report_path(dir, options, symbol, benchmark)?),
            // 要在浏览器里打开却没有落盘目录：让 browser 那边先把临时文件建好，报告写进去就是。
            //
            // The browser was asked for but no directory was configured: let the browser side create the
            // temporary file first, then write the report into it.
            None if open_in_browser => browser::temporary_file(options, symbol, benchmark)?,
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

    /// 落盘（有路径时）并按需用系统默认浏览器打开 —— 先写后开，浏览器打开时文件一定已经在了。
    ///
    /// Write the report (when there is a path) and open it in the system default browser when asked —
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

/// 名字撞上上限时重试几次：随机尾缀是 32 位，撞上几乎不可能，但「不覆盖已有文件」要是保证而不是概率。
///
/// How many times to retry when a name is taken: the random suffix is 32 bits wide, so a collision is
/// practically impossible, but "nothing existing is overwritten" should be a guarantee, not a probability.
const MAX_NAME_ATTEMPTS: usize = 8;

/// 在 `output_dir` 下挑一个没被占用的文件名，返回完整路径。
///
/// 名字由 naming.rs 按「时间 + 策略名 + 基准名 + 随机尾缀」生成，这里只解决「万一撞上已存在的文件」：
/// 换一个尾缀重试；重试到上限仍然撞上就报错（先查 `duck_vfs::exists`，所以不会拿别人的文件去覆盖）。
///
/// Pick a free file name under `output_dir` and return the full path.
///
/// naming.rs builds the name from "time + strategy + benchmark + random suffix"; all this does is handle
/// "what if that file already exists": it retries with another suffix, and errors out if it keeps colliding
/// (`duck_vfs::exists` is checked first, so somebody else's file is never overwritten).
fn report_path(
    dir: &str,
    options: &QuantstatsHtmlOptions,
    symbol: &str,
    benchmark: Option<&str>,
) -> DuckResult<String> {
    for _ in 0..MAX_NAME_ATTEMPTS {
        let path = join(dir, &naming::file_name(options, symbol, benchmark));
        if !duck_vfs::exists(&path) {
            return Ok(path);
        }
    }

    Err(duck_error(format!(
        "qs_html_report_options.output_dir={dir}: could not find a free report file name in \
         {MAX_NAME_ATTEMPTS} attempts"
    )))
}

/// 目录 + 文件名。
///
/// 刻意不用 `Path::join`：它按**平台**的分隔符拼，而这里的目录可能是 `s3://bucket/reports` 这种 VFS
/// 路径 —— 在 Windows 上会被拼成 `s3://bucket/reports\name.html`。DuckDB 的本地文件系统两边都认，所以
/// 统一用 `/`，只把用户写在末尾的分隔符去掉（`/` 与 `\` 都算）。
///
/// Directory + file name.
///
/// `Path::join` is deliberately not used: it joins with the **platform** separator, while the directory here
/// may be a VFS path like `s3://bucket/reports` — on Windows that would come out as
/// `s3://bucket/reports\name.html`. DuckDB's local file system accepts either, so `/` is used throughout and
/// only a trailing separator the user wrote (either `/` or `\`) is trimmed.
fn join(dir: &str, file_name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches(['/', '\\']), file_name)
}

/// 把渲染好的报告写到 [`ReportTarget`] 定下的路径。
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
/// Persists the rendered report to the path [`ReportTarget`] settled on.
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
