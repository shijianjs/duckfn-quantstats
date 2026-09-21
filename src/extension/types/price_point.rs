use duckfn::{DuckDate, DuckStruct};

// ============================================================================
// 价格/净值序列里的一个点
//
// 字段名用 `price`，直接照搬 Python quantstats 的词汇：它把「收益率或价格」这类输入统称 prices，
// 内部对看起来像价格的序列自动做 pct_change。净值（NAV）严格说不是 price，但 quantstats 也不区分，
// 净值序列照样当 prices 喂 —— 所以这里同样把它归到这一列，免得用户去想「该填哪个键」。
//
// 与 `return_point.rs` 的 `QuantstatsReturnPoint` 是同一套设计：不注册命名类型
// （`list({'date': ..., 'price': ...})` 产出的匿名 `STRUCT(date DATE, price DOUBLE)[]`
// 与它的字段名、类型完全一致，可以直接匹配），字段名只能是 Rust 字段名原样，
// 字段用 `Option` 是为了让「缺日期/缺值」的点被跳过而不是让整条查询失败。
//
// One point of a price (or NAV) series.
//
// The field is called `price`, borrowing Python quantstats' vocabulary: it lumps this kind of input
// under "prices" and runs pct_change on anything that looks like a price series. A NAV is not strictly a
// price, but quantstats does not distinguish either — a NAV series goes in as prices just the same — so
// this column is named the same way, and users do not have to guess which key to fill in.
//
// The same design as `QuantstatsReturnPoint` in `return_point.rs`: no named type is registered (the
// anonymous `STRUCT(date DATE, price DOUBLE)[]` produced by `list({'date': ..., 'price': ...})` has
// exactly the same field names and types, so it matches directly), the field names can only be the Rust
// field names verbatim, and the fields are `Option`s so that a point missing its date or its value is
// skipped instead of failing the whole query.
// ============================================================================

/// 价格/净值序列的一个点：`(日期, 该日的价格或净值)`。
///
/// One point of a price series: `(date, the price or NAV on that date)`.
#[derive(Clone, Debug, Default, DuckStruct)]
pub(crate) struct QuantstatsPricePoint {
    /// 该点的日期。缺省（NULL）时这个点会在聚合里被跳过。
    ///
    /// The date of this point. When missing (NULL) the point is skipped by the aggregate.
    pub date: Option<DuckDate>,

    /// 该点的价格或净值。缺省（NULL）时这个点会在聚合里被跳过。
    ///
    /// The price or NAV of this point. When missing (NULL) the point is skipped by the aggregate.
    pub price: Option<f64>,
}
