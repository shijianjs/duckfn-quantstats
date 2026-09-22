// ============================================================================
// 路径 2/2：价格/净值序列
//
// 与收益率路径（html_returns.rs）唯一的区别是：收进来的 `price` 是**价格/净值**，要在收尾里先差分成
// 收益率再渲染，差分规则见 series.rs 的 `prices_to_returns`（策略与基准两侧都走同一套规则）。
//
// 它必须与收益率那一支分开命名：`(symbol, date, price, options)` 与
// `(symbol, date, period_return, options)` 的类型序列完全一样（`VARCHAR, DATE, DOUBLE, STRUCT`），
// 同一个名字下无法按类型分派。
//
// 其余一切 —— 一次调用处理整张长表、按 symbol 分组、基准是同一张表里的一个 symbol、
// `list<struct{symbol, strategy_title, html, file_path}>` 的返回形状 —— 都与收益率路径相同。
//
// Path 2/2: the price/NAV series.
//
// The only difference from the return branch (html_returns.rs) is that `price` here is a **price/NAV**
// and has to be differenced into returns in the tail before rendering; the rules live in
// `prices_to_returns` in series.rs and apply to both the strategy and the benchmark side.
//
// It needs its own name because `(symbol, date, price, options)` and
// `(symbol, date, period_return, options)` have exactly the same type sequence
// (`VARCHAR, DATE, DOUBLE, STRUCT`), so one name could not dispatch them.
//
// Everything else — a whole long table per call, grouping by symbol, the benchmark being a symbol of
// that same table, the `list<struct{symbol, strategy_title, html, file_path}>` shape — matches the
// return branch.
// ============================================================================

use duckfn::{
    DuckAggregateState, DuckDate, DuckLazy, DuckOptionResult, DuckResult, duck_aggregate_function,
};

use crate::extension::types::html_report::QuantstatsHtmlReport;
use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::kind::PRICES;
use super::report::render_price_reports;
use super::series::SeriesPoint;
use super::slots::SymbolTable;

/// 整套报告的聚合状态（价格路径）。
///
/// The aggregate state of the whole set of reports (price branch).
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlPriceReportsState {
    /// 这里存的是**价格/净值**，差分留到收尾里做。
    ///
    /// This holds **prices/NAVs**; the differencing happens in the tail.
    symbols: SymbolTable,
}

/// 价格/净值序列的整套报告：按 `symbol` 分组，函数内部把每个 symbol（以及基准）换算成收益率。
///
/// ```sql
/// -- 整张净值表、带基准（SPX 是表里一个普通的 symbol）
/// SELECT qs_html_reports_by_prices(
///            symbol, trade_date, nav,
///            {'benchmark': 'SPX',
///             'title': symbol,
///             'output': 'reports/' || symbol || '.html'}::qs_html_report_options)
/// FROM nav_table;
/// ```
///
/// 差分规则见 `prices_to_returns`：按日期排序、逐点算 `price_t / price_{t-1} - 1`，首个点丢弃，
/// 前值缺失/为 0 时跳过该点；某个 symbol 差分后没有有效点（例如整组只有一个点）时，该 symbol 从
/// 结果里略过，而基准差分后没有有效点则是报错（基准是配置里明确要的）。
///
/// The whole set of reports over a price/NAV series: grouped by `symbol`, with every symbol (and the
/// benchmark) converted into returns internally.
///
/// ```sql
/// -- a whole NAV table with a benchmark (SPX is an ordinary symbol in it)
/// SELECT qs_html_reports_by_prices(
///            symbol, trade_date, nav,
///            {'benchmark': 'SPX',
///             'title': symbol,
///             'output': 'reports/' || symbol || '.html'}::qs_html_report_options)
/// FROM nav_table;
/// ```
///
/// See `prices_to_returns` for the conversion: sort by date, then `price_t / price_{t-1} - 1` per point,
/// dropping the first and skipping a point whose predecessor is missing or zero. A symbol left with no
/// valid point (a single-point series, say) is left out of the result, while a benchmark left with none
/// is an error (the benchmark was explicitly asked for).
// `pub(super)` 不是要把它当公开 API：属性宏会把函数包进同名模块，而模块沿用函数的可见性，
// kind.rs 要从那儿读生成的 `SQL_NAME`（见 kind.rs 的说明）。
//
// `pub(super)` is not about making this public API: the attribute macro wraps the function in a
// same-named module that inherits the function's visibility, and kind.rs reads the generated `SQL_NAME`
// from there (see kind.rs). The registered name is the Rust function name, so no `overloads_name`.
#[duck_aggregate_function]
pub(super) fn qs_html_reports_by_prices(
    symbol: String,
    date: DuckDate,
    price: f64,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlPriceReportsState,
) -> DuckResult<()> {
    state.symbols.push(
        &symbol,
        options.as_ref(),
        SeriesPoint {
            days_since_epoch: date.days_since_epoch,
            value: price,
        },
    )
}

impl DuckAggregateState for HtmlPriceReportsState {
    /// 与收益率路径同一个返回类型：`list<struct{symbol, strategy_title, html, file_path}>`。
    ///
    /// The same return type as the return branch: `list<struct{symbol, strategy_title, html,
    /// file_path}>`.
    type Output = Vec<QuantstatsHtmlReport>;

    fn simple_combine(&mut self, other: &Self) {
        self.symbols.combine(&other.symbols);
    }

    fn result(&self) -> DuckOptionResult<Vec<QuantstatsHtmlReport>> {
        render_price_reports(PRICES, &self.symbols)
    }
}
