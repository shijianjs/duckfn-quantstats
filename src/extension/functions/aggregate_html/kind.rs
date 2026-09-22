// ============================================================================
// 一条路径在 SQL 侧的名字
//
// 注册名不在这里手写：属性宏为每个签名生成了一个 `SQL_NAME` 常量（这里是 Rust 函数名本身，因为两个
// 函数都是单签名、没有用 `overloads_name` 合并重载），下面两个常量直接指过去，于是那个字面量只存在于
// `#[duck_aggregate_function]` 一处。错误信息前缀（`{function}: …`）与其它要提到注册名的地方都读它。
//
// 代价是那些注册函数得写成 `pub(super)`：生成的模块沿用函数的可见性，私有函数的名字常量只有它自己
// 那个文件读得到。
//
// 这个结构体曾经还要顺带记住「值列在 SQL 里叫什么键」（基准是列表参数的年代，报错提示里要举例该写哪个
// 键）；基准改成表里的一个 symbol 之后，两条路径的差别只剩算法本身（收益率 vs 先差分），常量也就只剩
// 名字。之所以仍然是一个结构体而不是裸 `&'static str`：它是「宏生成的常量只在一个地方被读」的那层
// 包装，两条路径的常量形状一样，透传给下面各层时也不必逐个参数解释「这是函数名」。
//
// The SQL-side name of one branch.
//
// The registered name is not written out here: the attribute macro generates a `SQL_NAME` constant per
// signature (the Rust function name itself here, since both functions are single-signature and neither
// uses `overloads_name` to merge overloads) and the two constants below point straight at it, so that
// literal lives in exactly one place. Error prefixes (`{function}: …`) and anything else that mentions
// the registered name read it.
//
// The price is that the registered functions have to be `pub(super)` — the generated module inherits the
// function's visibility, so a private function's name constant is readable only inside its own file.
//
// This struct used to also carry "what the value column is called in SQL" (back when the benchmark was a
// list argument and the error hints had to name the key to write). Since the benchmark became a symbol of
// the same table, the two branches differ only in the algorithm itself (returns vs difference first), so
// the constant is down to the name. It is still a struct rather than a bare `&'static str`: it is the one
// wrapper over the macro-generated constant, the two branches share its shape, and threading it down to
// the lower layers does not require explaining "this is the function name" at every parameter.
// ============================================================================

/// 一条路径在 SQL 侧的名字（同时用作错误信息前缀）。
///
/// The SQL-side name of one branch (also used as the error-message prefix).
#[derive(Clone, Copy)]
pub(super) struct SeriesKind {
    /// 注册名，同时也用作错误信息前缀。
    ///
    /// The registered name, also used as the error-message prefix.
    pub(super) function: &'static str,
}

/// 收益率序列路径。函数名取自 `html_returns.rs` 里属性宏生成的 `SQL_NAME`。
///
/// The return-series branch. Its name comes from the `SQL_NAME` the attribute macro generates in
/// `html_returns.rs`.
pub(super) const RETURNS: SeriesKind = SeriesKind {
    function: super::html_returns::qs_html_reports::SQL_NAME,
};

/// 价格/净值序列路径。函数名取自 `html_prices.rs` 里属性宏生成的 `SQL_NAME`。
///
/// 值列叫 `price` 是照搬 Python quantstats 的词汇：它把「收益率或价格」这类输入统称 prices，
/// 内部对看起来像价格的序列自动做 pct_change。净值（NAV）严格说不是 price，但 quantstats 也不区分，
/// 净值序列照样当 prices 喂 —— 所以那里同样用 `price` 这一个键收下这类水平值。
///
/// The price/NAV branch. Its name comes from the `SQL_NAME` the attribute macro generates in
/// `html_prices.rs`.
///
/// The value column is called `price`, borrowing Python quantstats' vocabulary: it lumps this kind of
/// input under "prices" and runs pct_change on anything that looks like a price series. A NAV is not
/// strictly a price, but quantstats does not distinguish either — a NAV series goes in as prices just the
/// same — so a single `price` key takes in all of these level values.
pub(super) const PRICES: SeriesKind = SeriesKind {
    function: super::html_prices::qs_html_reports_by_prices::SQL_NAME,
};
