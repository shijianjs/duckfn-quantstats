use duckfn::{DuckDate, DuckStruct};

// ============================================================================
// 基准序列里的一个点
//
// 这个类型**刻意不注册**命名类型（`create_type` 默认关闭）：`list({'date': ..., 'period_return': ...})`
// 产出的匿名 `STRUCT(date DATE, period_return DOUBLE)[]` 与它的字段名、顺序、类型完全一致，可以直接匹配，
// 调用方不必再写一次 cast，SQL 名字面上也就只多一个类型（配置那个）。实测确认：不写 `create_type` 时
// `#[derive(DuckStruct)]` 不提交任何注册项。
//
// 字段名固定是 `date` / `period_return`，这是硬约束：duckfn 的 `DuckStruct` 派生不支持字段级改名
// （字段的 SQL 名就是 Rust 字段名原样）。也刻意避开 SQL 关键字：实测 `return` 渲染正常但 `returns`、
// `value` 会被 DuckDB 加上引号，用 `period_return` 则到处都不用引号。
//
// 字段写成 `Option<...>`：列表里某一项缺日期或缺收益时读成 `None`，聚合函数跳过它 —— 与策略侧
// 「`date` 或 `period_return` 为 NULL 的行整行跳过」保持同一语义。`Option<T>` 的逻辑类型与 `T` 相同，
// 所以类型形状不变。
//
// One point of the benchmark series.
//
// This type deliberately registers **no** named type (`create_type` defaults to off): the anonymous
// `STRUCT(date DATE, period_return DOUBLE)[]` produced by
// `list({'date': ..., 'period_return': ...})` has exactly the same field names, order and types, so it
// matches directly — the caller writes no cast and the SQL surface only gains one type (the options
// one). Verified: without `create_type`, `#[derive(DuckStruct)]` submits no registration item.
//
// The field names are fixed to `date` / `period_return` — duckfn's `DuckStruct` derive has no
// field-level renaming (the SQL field name is the Rust field name verbatim). They also avoid SQL
// keywords: `return` renders fine but `returns` and `value` get quoted by DuckDB, while
// `period_return` never needs quotes.
//
// The fields are `Option<...>` so that an element missing its date or its return reads as `None` and
// the aggregate skips it — the same semantics as the strategy side, where a row with a NULL `date` or
// `period_return` is skipped entirely. `Option<T>` has the same logical type as `T`, so the type shape
// is unchanged.
// ============================================================================

/// 基准序列的一个点：`(日期, 该周期的收益率)`。
///
/// One point of the benchmark series: `(date, return per period)`.
#[derive(Clone, Debug, Default, DuckStruct)]
pub(crate) struct QuantstatsReturnPoint {
    /// 该点的日期。缺省（NULL）时这个点会在聚合里被跳过。
    ///
    /// The date of this point. When missing (NULL) the point is skipped by the aggregate.
    pub date: Option<DuckDate>,

    /// 该点在它所在周期内的收益率。缺省（NULL）时这个点会在聚合里被跳过。
    ///
    /// The return of this point within its period. When missing (NULL) the point is skipped by the
    /// aggregate.
    pub period_return: Option<f64>,
}
