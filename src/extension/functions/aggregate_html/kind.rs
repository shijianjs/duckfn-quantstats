// ============================================================================
// 一个分支在 SQL 侧的两个名字
//
// 收益率路径与价格路径要做的事完全一样，差别只在「注册名」与「值列的键名」这两处，而这两处又都要
// 出现在错误信息里（提示用户该用哪个函数、该写哪个键）。用一个 `Copy` 的小结构体把它们绑在一起，
// 四条路径就只需要各自写一遍常量，而把常量透传给下面各层。
//
// The two SQL-side names of one branch.
//
// The return branch and the price branch do exactly the same thing; they differ only in the registered
// name and the value-column key, and both of those must appear in error messages (to point the user at
// the right function and the right key). A small `Copy` struct binds the two together, so each of the
// four paths only declares one constant and threads it down.
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

/// 收益率序列路径。
///
/// 注意：函数集名字只写在 `#[duck_aggregate_function(overloads_name = "...")]` 上，宏属性只能吃字面量，
/// 所以这里的 `function` 与那两处字符串必须手动保持一致。
///
/// The return-series branch.
///
/// Note: the function-set name lives only on the `#[duck_aggregate_function(overloads_name = "...")]`
/// attributes — macro attributes accept literals only, so `function` here and those two strings must be
/// kept in sync by hand.
pub(super) const RETURNS: SeriesKind = SeriesKind {
    function: "qs_html_report",
    value_field: "period_return",
};

/// 价格/净值序列路径。
///
/// 值列叫 `price` 是照搬 Python quantstats 的词汇：它把「收益率或价格」这类输入统称 prices，
/// 内部对看起来像价格的序列自动做 pct_change。净值（NAV）严格说不是 price，但 quantstats 也不区分，
/// 净值序列照样当 prices 喂 —— 所以这里同样用 `price` 这一个键收下这类水平值。
///
/// The price/NAV branch.
///
/// The value column is called `price`, borrowing Python quantstats' vocabulary: it lumps this kind of
/// input under "prices" and runs pct_change on anything that looks like a price series. A NAV is not
/// strictly a price, but quantstats does not distinguish either — a NAV series goes in as prices just
/// the same — so a single `price` key takes in all of these level values.
pub(super) const PRICES: SeriesKind = SeriesKind {
    function: "qs_html_report_by_prices",
    value_field: "price",
};
