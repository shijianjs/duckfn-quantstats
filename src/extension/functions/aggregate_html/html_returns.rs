// ============================================================================
// 路径 1/2：收益率序列
//
// 一次调用出整套报告：整张长表（`symbol` + `date` + `period_return` + 配置）进来，函数内部按 symbol
// 分组，每个 symbol 一份完整报告。SQL 里**不需要 GROUP BY**（分组是函数的事），也**不需要 ORDER BY**
// （排序是报告自己的事）。
//
// 返回值是 `list<struct{symbol, benchmark, strategy_title, benchmark_title, html, file_path}>`，见
// types/html_report.rs。
//
// 基准是同一张表里的一个 symbol（配置里的 `benchmark` 键），它只作输入、不出报告；配对与渲染都在
// report.rs 的收尾里统一做。价格路径在 html_prices.rs，除了「收进来的是价格、要先差分」以外完全一样。
//
// Path 1/2: the return series.
//
// One call produces the whole set of reports: a long table (`symbol` + `date` + `period_return` +
// options) goes in, the function groups by symbol internally, one full report per symbol. SQL needs
// **no GROUP BY** (grouping is the function's job) and **no ORDER BY** (ordering is the report's).
//
// The return value is `list<struct{symbol, benchmark, strategy_title, benchmark_title, html, file_path}>`, see
// types/html_report.rs.
//
// The benchmark is a symbol of that same table (the `benchmark` key in the options); it is input only
// and gets no report. Pairing and rendering happen in one place, the tail in report.rs. The price
// branch lives in html_prices.rs and differs only in "the value that comes in is a price and has to be
// differenced first".
// ============================================================================

use duckfn::{
    DuckAggregateState, DuckDate, DuckLazy, DuckOptionResult, DuckResult, duck_aggregate_function,
};

use crate::extension::types::html_report::QuantstatsHtmlReport;
use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::kind::RETURNS;
use super::report::render_return_reports;
use super::series::SeriesPoint;
use super::slots::SymbolTable;

/// 整套报告的聚合状态（收益率路径）：一张按 symbol 分好的表。
///
/// The aggregate state of the whole set of reports (return branch): one table split by symbol.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlReportsState {
    symbols: SymbolTable,
}

/// 收益率序列的整套报告：按 `symbol` 分组，每个 symbol 一份完整报告。
///
/// ```sql
/// -- 整张表、不带基准
/// SELECT qs_html_reports(symbol, trade_date, daily_return, NULL) FROM daily_returns;
///
/// -- 整张表、带基准（SPX 是表里一个普通的 symbol；每个标的各自的标题与落盘路径由 symbol 列拼出来）
/// SELECT qs_html_reports(
///            symbol, trade_date, daily_return,
///            {'benchmark': 'SPX',
///             'title': symbol,
///             'output': 'reports/' || symbol || '.html'}::qs_html_report_options)
/// FROM daily_returns;
/// ```
///
/// `symbol`、`date` 或 `period_return` 为 NULL 的行整行跳过（duckfn 对非 `Option` 入参的既有语义，
/// 与 SQL 聚合惯例一致）；`symbol` 是空串的行同样跳过 —— 它当不了文件名，也当不了缺省显示名。
///
/// The whole set of reports over a return series: grouped by `symbol`, one full report per symbol.
///
/// ```sql
/// -- the whole table, no benchmark
/// SELECT qs_html_reports(symbol, trade_date, daily_return, NULL) FROM daily_returns;
///
/// -- the whole table with a benchmark (SPX is an ordinary symbol in it; every instrument's own title
/// -- and output path are built out of the symbol column)
/// SELECT qs_html_reports(
///            symbol, trade_date, daily_return,
///            {'benchmark': 'SPX',
///             'title': symbol,
///             'output': 'reports/' || symbol || '.html'}::qs_html_report_options)
/// FROM daily_returns;
/// ```
///
/// A row whose `symbol`, `date` or `period_return` is NULL is skipped entirely (duckfn's existing
/// semantics for non-`Option` arguments, and the usual SQL aggregate behaviour); an empty-string
/// `symbol` is skipped as well — it can be neither a file name nor a default display name.
// `pub(super)` 不是要把它当公开 API：属性宏会把函数包进同名模块，而模块沿用函数的可见性，
// kind.rs 要从那儿读生成的 `SQL_NAME`（见 kind.rs 的说明）。
//
// `pub(super)` is not about making this public API: the attribute macro wraps the function in a
// same-named module that inherits the function's visibility, and kind.rs reads the generated `SQL_NAME`
// from there (see kind.rs). The registered name is the Rust function name, so no `overloads_name`.
#[duck_aggregate_function]
pub(super) fn qs_html_reports(
    symbol: String,
    date: DuckDate,
    period_return: f64,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlReportsState,
) -> DuckResult<()> {
    state.symbols.push(
        &symbol,
        options.as_ref(),
        SeriesPoint {
            days_since_epoch: date.days_since_epoch,
            value: period_return,
        },
    )
}

impl DuckAggregateState for HtmlReportsState {
    /// 一次调用返回整套报告：`list<struct{symbol, benchmark, strategy_title, benchmark_title, html,
    /// file_path}>`。
    ///
    /// One call returns the whole set of reports: `list<struct{symbol, benchmark, strategy_title,
    /// benchmark_title, html, file_path}>`.
    type Output = Vec<QuantstatsHtmlReport>;

    fn simple_combine(&mut self, other: &Self) {
        self.symbols.combine(&other.symbols);
    }

    fn result(&self) -> DuckOptionResult<Vec<QuantstatsHtmlReport>> {
        render_return_reports(RETURNS, &self.symbols)
    }
}
