// ============================================================================
// 一个分支在 SQL 侧的两个名字
//
// 收益率路径与价格路径要做的事完全一样，差别只在「注册名」与「值列的键名」这两处，而这两处又都要
// 出现在错误信息里（提示用户该用哪个函数、该写哪个键）。用一个 `Copy` 的小结构体把它们绑在一起，
// 四条路径就只需要各自写一遍常量，而把常量透传给下面各层。
//
// 注册名不在这里手写：属性宏给每个签名生成了一个 `SQL_NAME` 常量（设了 `overloads_name` 时是函数集名，
// 否则是函数名），下面两个常量直接指过去，于是那个字面量只存在于
// `#[duck_aggregate_function(overloads_name = "...")]` 一处。代价是那些注册函数得写成 `pub(super)`：
// 生成的模块沿用函数的可见性，私有函数的名字常量只有它自己那个文件读得到。
//
// The two SQL-side names of one branch.
//
// The return branch and the price branch do exactly the same thing; they differ only in the registered
// name and the value-column key, and both of those must appear in error messages (to point the user at
// the right function and the right key). A small `Copy` struct binds the two together, so each of the
// four paths only declares one constant and threads it down.
//
// The registered name is not written out here: the attribute macro generates a `SQL_NAME` constant per
// signature (the function-set name when `overloads_name` is set, the function name otherwise) and the two
// constants below point straight at it, so that literal lives in exactly one place. The price is that the
// registered functions have to be `pub(super)` — the generated module inherits the function's visibility,
// so a private function's name constant is readable only inside its own file.
// ============================================================================

/// 一条路径在 SQL 侧的两个名字：函数名（兼错误信息前缀）与值列名（错误提示里要举例的键）。
///
/// The two SQL-side names of one branch: the function name (also the error-message prefix) and the value
/// column name (the key to show in error hints).
#[derive(Clone, Copy)]
pub(super) struct SeriesKind {
    /// 注册名，同时也用作错误信息前缀。
    ///
    /// The registered name, also used as the error-message prefix.
    pub(super) function: &'static str,
    /// 值列在 SQL 里的键名。
    ///
    /// The key of the value column in SQL.
    pub(super) value_field: &'static str,
}

/// 收益率序列路径。函数名取自 `html_returns.rs` 里属性宏生成的 `SQL_NAME`。
///
/// The return-series branch. Its name comes from the `SQL_NAME` the attribute macro generates in
/// `html_returns.rs`.
pub(super) const RETURNS: SeriesKind = SeriesKind {
    function: super::html_returns::qs_html_report::SQL_NAME,
    value_field: "period_return",
};

/// 价格/净值序列路径。函数名取自 `html_prices.rs` 里属性宏生成的 `SQL_NAME`。
///
/// 值列叫 `price` 是照搬 Python quantstats 的词汇：它把「收益率或价格」这类输入统称 prices，
/// 内部对看起来像价格的序列自动做 pct_change。净值（NAV）严格说不是 price，但 quantstats 也不区分，
/// 净值序列照样当 prices 喂 —— 所以这里同样用 `price` 这一个键收下这类水平值。
///
/// The price/NAV branch. Its name comes from the `SQL_NAME` the attribute macro generates in
/// `html_prices.rs`.
///
/// The value column is called `price`, borrowing Python quantstats' vocabulary: it lumps this kind of
/// input under "prices" and runs pct_change on anything that looks like a price series. A NAV is not
/// strictly a price, but quantstats does not distinguish either — a NAV series goes in as prices just
/// the same — so a single `price` key takes in all of these level values.
pub(super) const PRICES: SeriesKind = SeriesKind {
    function: super::html_prices::qs_html_report_by_prices::SQL_NAME,
    value_field: "price",
};
