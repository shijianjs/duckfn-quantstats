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
// # 两个显示名：解析一次，三个地方用
//
// 每份报告要两个名字：策略显示名与**那一份报告所用基准**的显示名（`benchmark_title` 按下标对齐
// `benchmark`，缺项退回基准 symbol）。它们在同一个地方解析一次，然后同时交给报告、文件名
// （`ReportTarget` → naming.rs）与返回行（`strategy_title` / `benchmark_title`），所以目录里的文件名、
// 报告里的图例与结果行不会各说各话 —— 显示名回显在结果里，看的人也就不必去 HTML 里抠。
//
// # 顺序与落盘
//
// 顺序是刻意定的：
//
//   1. 先建好每个标的的序列，再把每个 (标的, 基准) 对的显示名与落盘目标（[`ReportTarget`]）都定下来，
//      顺带校验 `output_dir` 非空、`open_in_browser` 指的路径能不能交给浏览器、以及 `language` 在翻译表里
//      有没有条目 —— 配置错误因此发生在渲染之前，不会白渲染几十份几百 KB 的报告；
//   2. 再逐个渲染、**按需翻译**（渲染之后、落盘与开浏览器之前，见 `translation::translate_report`）、落盘、
//      按需开浏览器；
//   3. 最后把「实际写到哪」与两个显示名落进返回行（没落盘时 `file_path` 是 NULL）。
//
// 文件名不给用户填（见 [`ReportTarget`]）：`output_dir` 只给目录，名字由 naming.rs 按「时间 + 策略名 +
// 基准名 + 随机尾缀」生成，`storage::report_path` 再确认它没被占用 —— 于是不管一个标的对几个基准、一次
// 调用写多少文件，都不存在互相覆盖这回事。
//
// 落盘只发生在原生构建里：wasm 构建整个跳过文件操作（`output_dir` 不落盘、`file_path` 为 NULL，报告
// 照常渲染并返回），见 storage.rs。**打开浏览器**同理，也只在原生构建里发生，见 browser.rs。
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
// # The two display names: resolved once, used three times
//
// Every report needs two names: the strategy's and **that very report's benchmark's** (`benchmark_title` pairs
// up with `benchmark` by index and falls back to the benchmark symbol). They are resolved in one place and
// then handed to the report, to the file name (`ReportTarget` → naming.rs) and to the returned row
// (`strategy_title` / `benchmark_title`), so the name on disk, the legend inside the report and the result
// row cannot drift apart — and with the names echoed back nobody has to dig through the HTML to read them.
//
// # Order and persistence
//
// The order is deliberate:
//
//   1. build every instrument's series, then settle every (symbol, benchmark) pair's display names and
//      destination ([`ReportTarget`]), validating `output_dir`, `open_in_browser` and "does this `language` have
//      entries" on the way — a bad configuration is therefore reported before anything is rendered, rather than
//      after dozens of few-hundred-KB reports;
//   2. render, **translate when asked** (after rendering and before persistence and the browser, see
//      `translation::translate_report`), persist and open each one;
//   3. put the actual path and the two display names into the returned row (`file_path` is NULL when nothing
//      was written).
//
// The file name is not the caller's to type (see [`ReportTarget`]): `output_dir` only takes a directory and
// naming.rs builds the name from "time + strategy + benchmark + random suffix", which
// `storage::report_path` then checks is free — so no matter how many benchmarks an instrument has or how
// many files one call writes, they cannot overwrite each other.
//
// Persisting only ever happens in a native build: a wasm build skips the file operation altogether
// (`output_dir` writes nothing, `file_path` is NULL, and the report is rendered and returned as usual) —
// see storage.rs. **Opening a browser** works the same way and is native-only too, see browser.rs.
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

use duckfn::{DuckOptionResult, DuckResult, duck_error};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::functions::translation;
use crate::extension::types::html_report::QuantstatsHtmlReport;
use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::browser;
use super::kind::SeriesKind;
use super::series::{SeriesPoint, build_series, prices_to_returns};
use super::slots::SymbolTable;
use super::storage;

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

/// 一个标的：配置与序列。同一个标的的每一份报告（每个基准一份）共用这两样。
///
/// One instrument: its options and its series. Every report of that instrument (one per benchmark) shares
/// both.
struct Strategy<'a> {
    /// 这个标的的名字，也就是它在输入列里的取值。
    ///
    /// The name of this instrument, i.e. the value it had in the input column.
    symbol: &'a str,
    /// 该 symbol 的配置；没解析到就是全默认。
    ///
    /// This symbol's options; all defaults when nothing was parsed.
    options: Arc<QuantstatsHtmlOptions>,
    /// 已经换算成收益率的序列。
    ///
    /// The series, already converted into returns.
    series: ReturnSeries,
}

/// 一份待渲染的报告：谁、对哪个基准、用哪条序列、写到哪。
///
/// 两个显示名在这里就已经解析完（见模块头「解析一次，两个地方用」），所以渲染那一步不需要再算一遍。
///
/// One report waiting to be rendered: which instrument, against which benchmark, from which series, written
/// where.
///
/// Both display names are already resolved here (see "resolved once, used twice" in the module header), so
/// the rendering step does not compute them again.
struct PlannedReport<'a> {
    /// 报告对应的 symbol。
    ///
    /// The symbol this report belongs to.
    symbol: &'a str,
    /// 这一份报告对上的基准 symbol；没配基准时是 `None`（单序列报告）。
    ///
    /// The benchmark symbol this report is against; `None` when no benchmark was configured (a
    /// single-series report).
    benchmark: Option<&'a str>,
    /// 那条基准序列（与 `benchmark` 同生同灭），渲染时挂到报告上。
    ///
    /// That benchmark's series (born and gone with `benchmark`), attached to the report when it is rendered.
    benchmark_series: Option<&'a ReturnSeries>,
    /// 报告、文件名与返回行里用的策略显示名。
    ///
    /// The strategy display name used in the report, the file name and the returned row.
    strategy_title: String,
    /// 报告、文件名与返回行里用的基准显示名；没有基准时是 `None`。
    ///
    /// The benchmark display name used in the report, the file name and the returned row; `None` without a
    /// benchmark.
    benchmark_title: Option<String>,
    /// 这个标的的序列。
    ///
    /// This instrument's series.
    series: &'a ReturnSeries,
    /// 这个标的的配置（渲染时要读 `title` / `rf` / `periods_per_year` / `match_dates`）。
    ///
    /// This instrument's options (rendering reads `title` / `rf` / `periods_per_year` / `match_dates`).
    options: Arc<QuantstatsHtmlOptions>,
    /// 报告的去处（`output_dir` / `open_in_browser` 已经在这里校验过）。
    ///
    /// Where the report goes (`output_dir` / `open_in_browser` were validated here).
    target: ReportTarget,
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

    // 第一遍：建好每个标的的序列（基准只作输入；换算后没有有效点的标的略过 —— 「没有报告可出」与
    // 「一行都没有 → NULL」是同一套语义，不是错误）。
    //
    // First pass: build every instrument's series (the benchmark is input only; an instrument with no valid
    // point after conversion is left out — the same semantics as "no row at all → NULL", not an error).
    let mut strategies: Vec<Strategy<'_>> = Vec::with_capacity(configured.len());
    for (symbol, options) in &configured {
        if benchmarks.contains(symbol) {
            continue;
        }

        let points = symbols.get(symbol).map(|slot| slot.points()).unwrap_or(&[]);
        let returns = to_returns(points);
        if returns.is_empty() {
            continue;
        }

        strategies.push(Strategy {
            symbol,
            options: Arc::clone(options),
            series: build_series(&returns, None)?,
        });
    }

    // 第二遍：每个 (标的, 基准) 对的显示名与落盘目标都定下来，所以 `output_dir` / `open_in_browser` 的
    // 配置错误发生在渲染之前。
    //
    // 没配基准时只有一个「槽」（那份单序列报告），否则每个基准一个槽 —— 这样下面渲染那一趟不必再分情况。
    //
    // Second pass: settle every (symbol, benchmark) pair's display names and destination, so
    // `output_dir` / `open_in_browser` misconfiguration surfaces before anything is rendered.
    //
    // With no benchmark there is exactly one slot (the single-series report), otherwise one slot per
    // benchmark — which is what keeps the rendering pass below branch-free.
    let slots: Vec<Option<(&str, usize)>> = if benchmarks.is_empty() {
        vec![None]
    } else {
        benchmarks
            .iter()
            .enumerate()
            .map(|(index, benchmark)| Some((*benchmark, index)))
            .collect()
    };

    let mut planned = Vec::with_capacity(strategies.len() * slots.len());
    for strategy in &strategies {
        // 语言写错要在这里就报出来（而不是渲染完一份报告才发现）：翻译表里没有这个语言时，
        // `require_language` 会带着「用 qs_list_translations() 查」的提示失败。
        //
        // A mistyped language is reported right here rather than after a report has been rendered: when the
        // translation table has no such language, `require_language` fails and points at
        // `qs_list_translations()`.
        if let Some(language) = strategy.options.language.as_deref() {
            translation::require_language(language)?;
        }

        for slot in &slots {
            let strategy_title = strategy.options.strategy_title_or(strategy.symbol);
            let (benchmark, benchmark_title) = match slot {
                // `benchmark_title` 按下标对齐 `benchmark`，缺项退回基准 symbol。
                //
                // `benchmark_title` pairs up with `benchmark` by index, falling back to the benchmark
                // symbol when the entry is missing.
                Some((benchmark, index)) => (
                    Some(*benchmark),
                    Some(strategy.options.benchmark_title_or(*index, benchmark)),
                ),
                None => (None, None),
            };

            planned.push(PlannedReport {
                symbol: strategy.symbol,
                benchmark,
                benchmark_series: slot.as_ref().map(|(_, index)| &benchmark_series[*index]),
                target: ReportTarget::new(
                    &strategy.options,
                    &strategy_title,
                    benchmark_title.as_deref(),
                )?,
                strategy_title,
                benchmark_title,
                series: &strategy.series,
                options: Arc::clone(&strategy.options),
            });
        }
    }

    // 第三遍：渲染 → 落盘 / 开浏览器 → 回填路径。
    //
    // Third pass: render → write / open in a browser → fill the path in.
    let mut reports = Vec::with_capacity(planned.len());
    for planned in &planned {
        let mut report_options = planned
            .options
            .to_report_options(&planned.strategy_title, planned.benchmark_title.as_deref())?;
        if let Some(benchmark_series) = planned.benchmark_series {
            report_options = report_options.with_benchmark(benchmark_series);
        }

        let report = render(kind, planned.series, report_options)?;

        // 翻译发生在**渲染之后、落盘与开浏览器之前**：磁盘上的文件、返回行里的 `html` 与浏览器里打开的页面
        // 因此是同一份内容，不存在「文件是英文、返回值是中文」这种错位。
        //
        // 没配 `language`（或整列是 NULL）时整段跳过 —— 缺省行为就是「一个字都不动」。
        //
        // The translation happens **after rendering and before persistence and the browser**: the file on disk,
        // the `html` in the returned row and the page the browser opens are therefore the same content, with no
        // "the file is English, the return value is Chinese" mismatch. Without a `language` (or with the whole
        // column NULL) this is skipped entirely — the default is "not one character changed".
        let report = match planned.options.language.as_deref() {
            Some(language) => translation::translate_report(&report, language)?,
            None => report,
        };

        planned.target.deliver(&report)?;

        reports.push(QuantstatsHtmlReport {
            symbol: planned.symbol.to_owned(),
            benchmark: planned.benchmark.map(str::to_owned),
            strategy_title: planned.strategy_title.clone(),
            benchmark_title: planned.benchmark_title.clone(),
            html: report,
            // 回填的就是这一次真正写出去的路径：`output_dir` 下那个自动命名的文件，或只在浏览器里
            // 打开时的临时文件，两者都没配则为 NULL。
            //
            // This is the path this very call wrote to: the auto-named file under `output_dir`, the
            // temporary file when the browser was the only one asking for a file, and NULL when there
            // is neither.
            file_path: planned.target.write_to().map(str::to_owned),
        });
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
/// NULL 元素、重复）由 [`QuantstatsHtmlOptions::benchmark_names`] 拦。
///
/// The benchmark symbols configured; empty when no benchmark was configured.
///
/// Every instrument in one call has to supply the **same list** (same entries, same order), otherwise "which
/// one is the benchmark" has no single answer and whether the named symbol should also get a report becomes
/// unanswerable — so this is an error instead of picking one. Problems inside the list itself (an empty
/// string, a NULL element, a duplicate) are caught by [`QuantstatsHtmlOptions::benchmark_names`].
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
/// - `write_to` 是**函数自己定的完整路径** —— `output_dir` 下一个自动命名的文件（见
///   [`storage::report_path`]），或者没有 `output_dir` 时要打开浏览器而新建的临时文件；它同时也是
///   返回行里 `file_path` 的来源。wasm 构建下恒为 `None`：那边整个跳过文件操作，不给路径也就不会去写；
/// - `open_with_browser` 是**本地绝对路径**，只给系统浏览器用 —— `output_dir` 万一写成 `s3://…` 这种
///   非本地路径，浏览器打不开，所以那种取值在这里就报错；相对路径也得先补成绝对路径。
///
/// 每个 (标的, 基准) 对各定一份：两者在 [`ReportTarget::new`] 里一次定下来，配置错误因此发生在渲染
/// **之前**，不会白渲染几十份几百 KB 的报告。两个显示名由调用方传进来（它同时也交给报告），所以文件名
/// 与报告图例用的是同一份名字。
///
/// Where a report goes: the file to write it to (possibly none) and whether to open it in a browser.
///
/// Writing and opening are two different paths and are kept apart:
///
/// - `write_to` is the **full path the function picked for itself** — an auto-named file under
///   `output_dir` (see [`storage::report_path`]), or a freshly created temporary file when there is no
///   `output_dir` but the browser was asked for; it is also where the returned `file_path` comes from. It
///   is always `None` in a wasm build: the file operation is skipped there, and with no path there is
///   nothing to write;
/// - `open_with_browser` is an **absolute local path** for the system browser only — an `output_dir` that
///   says `s3://…` could never be opened by a browser, so such a value is an error right here, and a
///   relative path has to be made absolute first.
///
/// There is one of these per (symbol, benchmark) pair: both are settled in [`ReportTarget::new`], so a bad
/// configuration is reported **before** anything is rendered rather than after dozens of few-hundred-KB
/// reports. The two display names come from the caller (which also hands them to the report), so the file
/// name and the report legend use one and the same name.
pub(super) struct ReportTarget {
    /// 落盘路径，函数自己命名的；`None` 表示不落盘（wasm 构建恒为此）。
    ///
    /// The path to write to, named by the function itself; `None` means nothing is written (always so in
    /// a wasm build).
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
        strategy_title: &str,
        benchmark_title: Option<&str>,
    ) -> DuckResult<Self> {
        // 先看要不要打开（wasm 下恒为 false）：它同时决定「要不要校验落盘路径能不能打开」与
        // 「没有 `output_dir` 时要不要先落一个临时文件」。
        //
        // Is the browser wanted at all (always false on wasm)? That one answer decides both whether the
        // write path has to be openable and whether a temporary file is needed when `output_dir` is unset.
        let open_in_browser = browser::is_requested(options);

        // 空字符串、带 NUL 字节的 `output_dir` 在这里就报掉（`output_dir_path`）。
        //
        // An empty `output_dir`, or one carrying a NUL byte, is reported right here, by `output_dir_path`.
        let write_to = match options.output_dir_path()? {
            // 挑一个落盘路径。wasm 构建下整个跳过文件操作，拿回来的就是 `None`（见 storage.rs）。
            //
            // Pick a path to write to. A wasm build skips the file operation altogether and answers `None`
            // instead (see storage.rs).
            Some(dir) => storage::report_path(dir, strategy_title, benchmark_title)?,
            // 要在浏览器里打开却没有落盘目录：让 browser 那边先把临时文件建好，报告写进去就是。
            //
            // The browser was asked for but no directory was configured: let the browser side create the
            // temporary file first, then write the report into it.
            None if open_in_browser => Some(browser::temporary_file(strategy_title, benchmark_title)?),
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
            storage::write_report(path, report)?;
        }
        if let Some(path) = &self.open_with_browser {
            browser::open_report(path)?;
        }

        Ok(())
    }
}


