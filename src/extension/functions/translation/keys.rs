// ============================================================================
// 可翻译目录：报告里每一个可翻译位置叫什么 key、在 DOM 的哪一处、英文原文是什么
//
// 这是整个翻译功能的**锚点表**，三件事都从这里读：
//
//   1. `qs_set_translation()` 用它校验 key（不在这里的 key 直接报错）；
//   2. `qs_list_translations()`（间接）与内置数据的完整性自检（见 table.rs::install_builtin）；
//   3. report.rs 的后处理用它把 DOM 里的英文原文映射成 key。
//
// # 为什么是「一个选择器 + 一张原文表」而不是「全文替换」
//
// 报告里的文本分两类。一类是**位置唯一**的（`<h3>` 分节标题、`.qs-plot-title` 图表标题、文档
// `<title>`），另一类**同一个词出现在好几个地方**（`Strategy` 既是 EOY 表头、又是 SVG 图例）。全文替换
// 会让后一类互相串味，所以每个位置都配一条 CSS 选择器、配自己那张「英文原文 → key」的表：同一个词在不同
// 位置是两个 key（`table.eoy_strategy` 与 `label.strategy`），因此可以分别翻译、分别写说明。
//
// 选择器只允许落到**内容由 quantstats-rs 模板与 plots.rs 决定的元素**上（指标行的第一个 `<td>`、EOY 表头、
// 热量图的月份、SVG 的 `<text>`……），落到「用户数据」上的只有一处也不该有 —— 策略名与基准名是用户给的，
// 报表里它们是 `<h1>` / `<th>` 的内容，不在本表里。
//
// # 月份为什么只有一套 key
//
// 月份出现在两处：热量图表头（原文全大写 `JAN`）与 `<h1><dt>` 的日期区间（原文 `Jan`，chrono 的 `%b`）。
// 两处共用 `month.jan`…`month.dec`，锚点写**散文里的那种写法**（`Jan`），热量图那一处靠
// [`Scope::uppercase`] 把「查找」与「渲染」两端都转成大写 —— 于是：`en` 的 `label` 是 `Jan`，热量图渲染出来
// 仍是 `JAN`（英文那一档一个字符都没变），日期区间拿到的也还是 `Jan`。12 个 key 同时喂两处，不必为大小写各留
// 一份数据，也不必让 `en` 为月份单列一套 `label`。
//
// The translatable catalog: what every translatable position is called, where it lives in the DOM and
// what its English source text is.
//
// This is the **anchor table** of the whole feature, and three things read it: `qs_set_translation()`
// validates keys against it, the built-in data is checked for completeness against it (see
// `table.rs::install_builtin`), and the report post-processing uses it to map English source text in the
// DOM onto keys.
//
// Why "one selector plus one source-text table" rather than a global string replacement: some text is
// unique by position (an `<h3>` section heading, a `.qs-plot-title`, the document `<title>`), while one
// word shows up in several places (`Strategy` is both an EOY table header and an SVG legend label).
// A global replacement would let the latter bleed into each other, so every position gets its own
// selector and its own "source text → key" table: the same word in two positions is two keys
// (`table.eoy_strategy` and `label.strategy`) and can therefore be translated and annotated separately.
//
// Selectors only ever land on elements whose content is decided by the quantstats-rs template and
// plots.rs (the first `<td>` of a metric row, the EOY headers, the heatmap months, SVG `<text>` …) and
// never on user data — the strategy and benchmark names are the caller's, they are the content of
// `<h1>`/`<th>` in the report, and they are not in this table.
//
// Why the months have a single set of keys: months appear twice — in the heatmap header (uppercase `JAN`) and
// in the `<h1><dt>` date range (source `Jan`, chrono's `%b`) — sharing `month.jan` … `month.dec`, with the
// anchor written the **prose** way (`Jan`) and the heatmap slot using [`Scope::uppercase`] to uppercase both
// ends: the lookup and the rendered text. So `en`'s `label` is `Jan` while the heatmap still renders `JAN`
// (English is byte-identical) and the date range still gets `Jan` — twelve keys feed both places, with no
// second set for casing and no month-specific `label` overrides in `en`.
// ============================================================================

/// 一个可翻译位置**怎么被找到**，以及它那批「英文原文 → key」。
///
/// How one translatable position is found, plus its batch of "English source → key" pairs.
pub(super) struct Scope {
    /// CSS 选择器，命中承载文本的元素（文本处理器挂在它上面）。
    ///
    /// The CSS selector matching the element that carries the text (the text handler hangs off it).
    pub(super) selector: &'static str,

    /// 命中的文本节点该按哪种方式渲染回 DOM。
    ///
    /// How a matched text node is rendered back into the DOM.
    pub(super) style: Style,

    /// 该槽位在原文与译文里都是**大写**（热量图表头，见模块头）：查找时把锚点转成大写去匹配，渲染时把
    /// 译文转成大写写回。
    ///
    /// 之所以连查找也要转：同一批 key 的锚点是**散文写法**（`Jan`），而热量图里写的是 `JAN`
    /// （`quantstats-rs` 的 `month_labels` 就是全大写）。
    ///
    /// The slot is **uppercase** in both the source and the translation (the heatmap header, see the module
    /// header): the anchor is uppercased for the lookup and the translation is uppercased for the write-back.
    /// The lookup is uppercased too because these keys' anchors are the **prose** form (`Jan`) while the heatmap
    /// writes `JAN` (quantstats-rs' `month_labels` are all caps).
    pub(super) uppercase: bool,

    /// 该位置允许出现的文本：`(英文原文, key)`。
    ///
    /// The texts allowed at this position: `(English source, key)`.
    pub(super) pairs: &'static [(&'static str, &'static str)],
}

/// 文本写回 DOM 的三种方式。
///
/// How a translated text is written back into the DOM.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Style {
    /// HTML 正文元素：包一层 `<span title="说明">译文</span>`。
    ///
    /// 之所以要包一层而不是给元素本身设 `title`：`lol_html` 的文本处理器拿不到父元素，而元素处理器在
    /// 起始标签处就已经写出、那时还读不到元素里的文本（流式解析的固有约束，见 report.rs 的说明）。
    /// 包一层 `<span>` 不影响任何布局（它是行内元素），而 `title` 挂在 span 上与挂在父元素上同样能浮出。
    ///
    /// An HTML element: wrap it as `<span title="note">translation</span>`.
    ///
    /// The wrapper exists because lol_html's text handler cannot reach the parent element, while an
    /// element handler runs at the start tag — before the element's text is known (an inherent streaming
    /// constraint, see report.rs). An inline `<span>` changes no layout, and a `title` on it pops up just
    /// like one on the parent would.
    Html,

    /// SVG 的 `<text>`：插一个 `<title>说明</title>` 子元素再跟译文。
    ///
    /// SVG 元素的浮出提示按规范是 `<title>` **子元素**（`title` 属性在 SVG 里不是标准做法），而
    /// `<title>` 在 SVG 里不参与渲染，所以插它不会改变图形。
    ///
    /// An SVG `<text>`: insert a `<title>note</title>` child followed by the translation.
    ///
    /// The SVG tooltip is standardised as a `<title>` **child** element (the `title` attribute is not the
    /// SVG way), and an SVG `<title>` is not rendered, so inserting it does not change the chart.
    Svg,

    /// 只替换文本，不挂任何说明 —— 给文档 `<title>` 用：浏览器标签页没法浮出提示。
    ///
    /// Replace the text only, with no note — for the document `<title>`, since a browser tab has nothing
    /// to hover.
    Plain,
}

// ---------------------------------------------------------------------------
// 指标表：`#right > table tbody` 每行第一个 `<td>`
// ---------------------------------------------------------------------------

/// 指标行名，顺序与 `quantstats-rs` 的 `build_metrics_table` 一致（读起来好对照）。
///
/// 索引 0/1 两项由 `format!` 写出（`Risk-Free Rate`）或紧接其后（`Time in Market`），与其余的写法不同，
/// 但匹配的是文本、与写法无关。
///
/// The metric row labels, in the order `build_metrics_table` emits them (so the two read side by side). The
/// first two are written through `format!`/`push_str` differently from the rest, but matching is on the
/// text, not on how it was produced.
pub(super) const METRICS: &[(&str, &str)] = &[
    ("Risk-Free Rate", "metric.risk_free_rate"),
    ("Time in Market", "metric.time_in_market"),
    ("Cumulative Return", "metric.cumulative_return"),
    // `﹪` 是 U+FE6A（小写百分号），不是 ASCII 的 `%`；写成转义免得日后被编辑器「顺手」换成别的字符。
    //
    // `﹪` is U+FE6A (small percent sign), not an ASCII `%`; it is written as an escape so no editor
    // ever "helpfully" replaces it.
    ("CAGR\u{fe6a}", "metric.cagr"),
    ("Sharpe", "metric.sharpe"),
    ("Prob. Sharpe Ratio", "metric.prob_sharpe_ratio"),
    ("Smart Sharpe", "metric.smart_sharpe"),
    ("Sortino", "metric.sortino"),
    ("Smart Sortino", "metric.smart_sortino"),
    ("Sortino/\u{221a}2", "metric.sortino_sqrt2"),
    ("Smart Sortino/\u{221a}2", "metric.smart_sortino_sqrt2"),
    ("Omega", "metric.omega"),
    ("Max Drawdown", "metric.max_drawdown"),
    ("Max DD Date", "metric.max_dd_date"),
    ("Max DD Period Start", "metric.max_dd_period_start"),
    ("Max DD Period End", "metric.max_dd_period_end"),
    ("Longest DD Days", "metric.longest_dd_days"),
    ("Volatility (ann.)", "metric.volatility_ann"),
    ("R^2", "metric.r_squared"),
    ("Information Ratio", "metric.information_ratio"),
    ("Calmar", "metric.calmar"),
    ("Skew", "metric.skew"),
    ("Kurtosis", "metric.kurtosis"),
    ("Expected Daily", "metric.expected_daily"),
    ("Expected Monthly", "metric.expected_monthly"),
    ("Expected Yearly", "metric.expected_yearly"),
    ("Kelly Criterion", "metric.kelly_criterion"),
    ("Risk of Ruin", "metric.risk_of_ruin"),
    ("Daily Value-at-Risk", "metric.daily_value_at_risk"),
    ("Expected Shortfall (cVaR)", "metric.expected_shortfall"),
    ("Max Consecutive Wins", "metric.max_consecutive_wins"),
    ("Max Consecutive Losses", "metric.max_consecutive_losses"),
    ("Gain/Pain Ratio", "metric.gain_pain_ratio"),
    ("Gain/Pain (1M)", "metric.gain_pain_1m"),
    ("Payoff Ratio", "metric.payoff_ratio"),
    ("Profit Factor", "metric.profit_factor"),
    ("Common Sense Ratio", "metric.common_sense_ratio"),
    ("Tail Ratio", "metric.tail_ratio"),
    ("CPC Index", "metric.cpc_index"),
    ("Outlier Win Ratio", "metric.outlier_win_ratio"),
    ("Outlier Loss Ratio", "metric.outlier_loss_ratio"),
    ("MTD", "metric.mtd"),
    ("3M", "metric.3m"),
    ("6M", "metric.6m"),
    ("YTD", "metric.ytd"),
    ("1Y", "metric.1y"),
    ("3Y (ann.)", "metric.3y_ann"),
    ("5Y (ann.)", "metric.5y_ann"),
    ("10Y (ann.)", "metric.10y_ann"),
    ("All-time (ann.)", "metric.all_time_ann"),
    ("Best Day", "metric.best_day"),
    ("Worst Day", "metric.worst_day"),
    ("Best Month", "metric.best_month"),
    ("Worst Month", "metric.worst_month"),
    ("Best Year", "metric.best_year"),
    ("Worst Year", "metric.worst_year"),
    ("Avg. Drawdown", "metric.avg_drawdown"),
    ("Avg. Drawdown Days", "metric.avg_drawdown_days"),
    ("Recovery Factor", "metric.recovery_factor"),
    ("Ulcer Index", "metric.ulcer_index"),
    ("Serenity Index", "metric.serenity_index"),
    ("Avg. Up Month", "metric.avg_up_month"),
    ("Avg. Down Month", "metric.avg_down_month"),
    ("Win Days", "metric.win_days"),
    ("Win Month", "metric.win_month"),
    ("Win Quarter", "metric.win_quarter"),
    ("Win Year", "metric.win_year"),
    ("Beta", "metric.beta"),
    ("Alpha", "metric.alpha"),
    ("Correlation", "metric.correlation"),
    ("Treynor Ratio", "metric.treynor_ratio"),
];

// ---------------------------------------------------------------------------
// 图表标题：`.qs-plot-title`
// ---------------------------------------------------------------------------

/// 图表标题。带基准与不带基准是两句不同的话（`quantstats-rs` 按有没有基准选标题），所以两套都要有 key
/// —— 落到同一个 key 上会让「不带基准」的那些报告显示成「vs 基准」。
///
/// Plot titles. With and without a benchmark are two different sentences (quantstats-rs picks the title by
/// whether there is one), so both forms need their own key — sharing one would label a single-series
/// report as "vs benchmark".
pub(super) const PLOTS: &[(&str, &str)] = &[
    ("Cumulative Returns", "plot.cumulative_returns"),
    (
        "Cumulative Returns vs Benchmark",
        "plot.cumulative_returns_vs_benchmark",
    ),
    (
        "Cumulative Returns (Log Scaled)",
        "plot.cumulative_returns_log_scaled",
    ),
    (
        "Cumulative Returns vs Benchmark (Log Scaled)",
        "plot.cumulative_returns_vs_benchmark_log_scaled",
    ),
    (
        "Cumulative Returns (Volatility Matched)",
        "plot.cumulative_returns_volatility_matched",
    ),
    (
        "Cumulative Returns vs Benchmark (Volatility Matched)",
        "plot.cumulative_returns_vs_benchmark_volatility_matched",
    ),
    ("EOY Returns", "plot.eoy_returns"),
    ("EOY Returns vs Benchmark", "plot.eoy_returns_vs_benchmark"),
    (
        "Daily Returns (Cumulative Sum)",
        "plot.daily_returns_cumulative_sum",
    ),
    ("Return Quantiles", "plot.return_quantiles"),
    (
        "Distribution of Monthly Returns",
        "plot.distribution_of_monthly_returns",
    ),
    (
        "Distribution of Monthly Returns vs Benchmark",
        "plot.distribution_of_monthly_returns_vs_benchmark",
    ),
    (
        "Rolling Volatility (6-Months)",
        "plot.rolling_volatility_6_months",
    ),
    ("Rolling Sharpe (6-Months)", "plot.rolling_sharpe_6_months"),
    ("Rolling Sortino (6-Months)", "plot.rolling_sortino_6_months"),
    ("Rolling Beta to Benchmark", "plot.rolling_beta_to_benchmark"),
    (
        "Strategy - Monthly Returns (%)",
        "plot.strategy_monthly_returns",
    ),
    ("Drawdown (Underwater)", "plot.drawdown_underwater"),
    (
        "Strategy - Worst 5 Drawdown Periods",
        "plot.strategy_worst_5_drawdown_periods",
    ),
];

// ---------------------------------------------------------------------------
// 其余位置
// ---------------------------------------------------------------------------

/// 分节标题（`<h3>`）。模板里只有这三处 `<h3>`：指标表、EOY 表、回撤表。
///
/// Section headings (the `<h3>`s). The template has exactly three: the metrics table, the EOY table and
/// the drawdown table.
pub(super) const SECTIONS: &[(&str, &str)] = &[
    (
        "Key Performance Metrics",
        "section.key_performance_metrics",
    ),
    ("Worst 10 Drawdowns", "section.worst_drawdowns"),
    ("EOY Returns", "section.eoy_returns"),
    (
        "EOY Returns vs Benchmark",
        "section.eoy_returns_vs_benchmark",
    ),
];

/// 指标表表头第一格。
///
/// 只有 `Metric` 一格；其余两格是策略显示名与基准显示名 —— 那是调用方给的数据，不翻译。
///
/// The first cell of the metrics table header. Only `Metric`; the other two cells are the strategy and
/// benchmark display names, i.e. the caller's data, and are not translated.
pub(super) const METRIC_HEADER: &[(&str, &str)] = &[("Metric", "table.metric")];

/// EOY 表表头（`#eoy thead th`）。这几格是**字面量**，不是显示名（显示名只在指标表表头里）。
///
/// The EOY table header (`#eoy thead th`). These cells are **literals**, not display names (display names
/// only appear in the metrics table header).
pub(super) const EOY_HEADERS: &[(&str, &str)] = &[
    ("Year", "table.eoy_year"),
    ("Benchmark", "table.eoy_benchmark"),
    ("Strategy", "table.eoy_strategy"),
    ("Multiplier", "table.eoy_multiplier"),
    ("Won", "table.eoy_won"),
];

/// 最差回撤表表头（`#ddinfo thead th`）。
///
/// The worst-drawdowns table header (`#ddinfo thead th`).
pub(super) const DRAWDOWN_HEADERS: &[(&str, &str)] = &[
    ("Started", "table.dd_started"),
    ("Recovered", "table.dd_recovered"),
    ("Drawdown", "table.dd_drawdown"),
    ("Days", "table.dd_days"),
];

/// SVG 里的零散标签：图例、分布图的五个分组名、以及均值参考线的 `Mean`。
///
/// 这些标签的文本由 `plots.rs` 写死（图例退化值、分组名、参考线名），与调用方的显示名无关 ——
/// 报告里那份 `Strategy` / `Benchmark` 是**图例的兜底文本**，不是策略名。
///
/// The scattered SVG labels: the legend, the distribution plot's five group names and the `Mean` reference
/// line. Their text is hard-coded in plots.rs (legend fallbacks, group names, the guide name) and unrelated
/// to the caller's display names — the `Strategy`/`Benchmark` here are the legend's fallback text, not the
/// strategy's name.
pub(super) const SVG_LABELS: &[(&str, &str)] = &[
    ("Strategy", "label.strategy"),
    ("Benchmark", "label.benchmark"),
    ("Mean", "label.mean"),
    ("Daily", "dist.daily"),
    ("Weekly", "dist.weekly"),
    ("Monthly", "dist.monthly"),
    ("Quarterly", "dist.quarterly"),
    ("Yearly", "dist.yearly"),
];

/// 热量图表头的月份。锚点是**散文写法**（`Jan`），槽位靠 [`Scope::uppercase`] 在查找与渲染两端都转大写；
/// 同一批 key 也喂 `<h1><dt>` 的日期区间，见模块头。
///
/// The heatmap header months. The anchors are the **prose** form (`Jan`) and the slot uppercases both ends
/// through [`Scope::uppercase`]; the same keys feed the `<h1><dt>` date range, see the module header.
pub(super) const MONTHS: &[(&str, &str)] = &[
    ("Jan", "month.jan"),
    ("Feb", "month.feb"),
    ("Mar", "month.mar"),
    ("Apr", "month.apr"),
    ("May", "month.may"),
    ("Jun", "month.jun"),
    ("Jul", "month.jul"),
    ("Aug", "month.aug"),
    ("Sep", "month.sep"),
    ("Oct", "month.oct"),
    ("Nov", "month.nov"),
    ("Dec", "month.dec"),
];

/// `1`..`12` 月对应的 key，下标 = 月份 - 1。日期区间（`%b`，如 `Jan`）靠它取月份译文。
///
/// The key of month `1`..`12`, indexed by month - 1. The date range (chrono's `%b`, e.g. `Jan`) uses it to
/// look a month's translation up.
pub(super) const MONTH_KEYS: [&str; 12] = [
    "month.jan",
    "month.feb",
    "month.mar",
    "month.apr",
    "month.may",
    "month.jun",
    "month.jul",
    "month.aug",
    "month.sep",
    "month.oct",
    "month.nov",
    "month.dec",
];

/// 文档 `<title>`：一句只出现一次的固定文本，替换掉即可（标签页没法浮出提示，所以没有说明）。
///
/// The document `<title>`: a fixed sentence appearing exactly once, replaced as a whole (a browser tab has
/// nothing to hover, hence no note).
pub(super) const DOCUMENT_TITLE: &[(&str, &str)] = &[(
    "Tearsheet (generated by QuantStats-RS)",
    "text.title",
)];

/// EOY 表没有数据时模板里那一段（`<p>No EOY data available.</p>`）。
///
/// The paragraph the template shows when there is no EOY data (`<p>No EOY data available.</p>`).
pub(super) const NO_EOY_DATA: &[(&str, &str)] = &[("No EOY data available.", "text.no_eoy_data")];

/// `<h4>` 里的两个固定片段（中间夹着链接与版本号，所以按片段替换而不是整段替换）。
///
/// 剩下的 ` (v. 0.1.0)` 不动：版本号是数据，不是文本。
///
/// The two fixed fragments inside `<h4>` (a link and a version number sit between them, hence fragment
/// replacement rather than whole-node replacement). The trailing ` (v. 0.1.0)` is left alone: that is data,
/// not text.
pub(super) const H4_FRAGMENTS: &[(&str, &str)] = &[
    ("Benchmark is", "text.benchmark_is"),
    ("Generated by", "text.generated_by"),
];

/// `<h1><dt>` 日期区间的 key，以及 `en` 的那份**模板**。
///
/// 这一项的 `label` 和其它 key 不同：它是一段带占位符的模板，`{d1} {m1} {y1}` 是起点、`{d2} {m2} {y2}` 是终点
/// （`d` 日 / `m` 月名 / `y` 年）。`quantstats-rs` 用 `%e %b, %Y` 写死英文语序，而中文、日文要的是
/// 「2021年1月5日」，靠替换月份名字改不掉语序，所以这一项按模板渲染 —— 顺带也把日期的排版交给了翻译表，
/// 想换成本地写法不必改代码。
///
/// 模板里的 `{m1}`/`{m2}` 会被 [`MONTH_KEYS`] 那批 key 的译文替换（并挂上月份的说明）。
///
/// The key of the `<h1><dt>` date range plus the **template** used for `en`. This entry's `label` differs
/// from the others: it is a template with placeholders, `{d1} {m1} {y1}` for the start and `{d2} {m2}
/// {y2}` for the end (`d` day, `m` month name, `y` year). quantstats-rs hard-codes the English word order
/// through `%e %b, %Y`, and Chinese or Japanese want "2021年1月5日" — replacing month names cannot reorder a
/// sentence, so this entry renders from a template, which also hands date layout to the translation table
/// (a different local convention needs no code change). `{m1}`/`{m2}` are filled from the [`MONTH_KEYS`]
/// entries, notes included.
pub(super) const DATE_RANGE_KEY: &str = "date.range";
pub(super) const DATE_RANGE_EN: &str = "{d1} {m1}, {y1} - {d2} {m2}, {y2}";

/// `<h1><dt>` 的选择器：日期区间是**一句带数据的句子**，不能按整段文本匹配，所以单独一个处理器。
///
/// The `<h1><dt>` selector: the date range is a **sentence with data in it**, so it cannot be matched as a
/// whole text node and gets a handler of its own.
pub(super) const DATE_RANGE_SELECTOR: &str = "h1 dt";

/// `<h4>` 的选择器（两个固定片段在里面）。
///
/// The `<h4>` selector (the two fixed fragments live in it).
pub(super) const HEADER_SELECTOR: &str = "h4";

/// 按「整段文本等于某一项」匹配的处理器清单。
///
/// 顺序无关紧要：选择器之间可能重叠（`#monthly_heatmap text` 也落在 `.qs-plot svg text` 里），但两张表
/// 的 key 集合**不相交**，所以重叠处只有一个处理器会真的改写（另一个找不到 key，原样放行）。
///
/// The handlers that match "this whole text node equals one of these". Order does not matter: selectors may
/// overlap (`#monthly_heatmap text` also falls under `.qs-plot svg text`), but the two tables' key sets are
/// **disjoint**, so only one handler ever rewrites an overlapping node (the other finds no key and passes it
/// through untouched).
pub(super) const SCOPES: &[Scope] = &[
    Scope {
        selector: "title",
        style: Style::Plain,
        uppercase: false,
        pairs: DOCUMENT_TITLE,
    },
    Scope {
        selector: "h3",
        style: Style::Html,
        uppercase: false,
        pairs: SECTIONS,
    },
    Scope {
        selector: "#right > table thead th:first-child",
        style: Style::Html,
        uppercase: false,
        pairs: METRIC_HEADER,
    },
    Scope {
        selector: "#right > table tbody td:first-child",
        style: Style::Html,
        uppercase: false,
        pairs: METRICS,
    },
    Scope {
        selector: "#eoy thead th",
        style: Style::Html,
        uppercase: false,
        pairs: EOY_HEADERS,
    },
    Scope {
        selector: "#ddinfo thead th",
        style: Style::Html,
        uppercase: false,
        pairs: DRAWDOWN_HEADERS,
    },
    Scope {
        selector: "#eoy p",
        style: Style::Html,
        uppercase: false,
        pairs: NO_EOY_DATA,
    },
    Scope {
        selector: ".qs-plot-title",
        style: Style::Html,
        uppercase: false,
        pairs: PLOTS,
    },
    // 刻意**排除**热量图：`.qs-plot svg text` 会把 `#monthly_heatmap` 里的月份标签也框进来，两条选择器
    // 命中同一段文本时，后一条的写回（哪怕是原样写回）会把前一条刚写下的译文盖掉。`div:not(…)` 是
    // `:not()` 允许的简单选择器形式，能干净地切开两边。
    //
    // The heatmap is deliberately **excluded**: `.qs-plot svg text` would also cover the month labels inside
    // `#monthly_heatmap`, and when two selectors match one text the later write-back — even a verbatim one —
    // would clobber the translation the earlier one just wrote. `div:not(…)` is the simple-selector form `:not()`
    // accepts, which cuts the two sides apart cleanly.
    Scope {
        selector: "#left > div:not(#monthly_heatmap) svg text",
        style: Style::Svg,
        uppercase: false,
        pairs: SVG_LABELS,
    },
    Scope {
        selector: "#monthly_heatmap text",
        style: Style::Svg,
        uppercase: true,
        pairs: MONTHS,
    },
];

/// 目录里全部 `(英文原文, key)`；`en` 的 `label` 与内置数据的完整性自检都用它。
///
/// 只在一处消费（扩展加载期），所以直接现算，不做缓存。
///
/// Every `(English source, key)` in the catalog; it backs `en`'s `label` values and the completeness check on
/// the built-in data. Computed on the spot — it is consumed exactly once, at extension load.
pub(super) fn all() -> impl Iterator<Item = (&'static str, &'static str)> {
    SCOPES
        .iter()
        .flat_map(|scope| scope.pairs.iter().copied())
        .chain(std::iter::once((DATE_RANGE_EN, DATE_RANGE_KEY)))
        .chain(H4_FRAGMENTS.iter().copied())
}

/// `key` 是不是目录里的一个 key。
///
/// Whether `key` is one of the catalog's keys.
pub(super) fn is_known(key: &str) -> bool {
    all().any(|(_, known)| known == key)
}
