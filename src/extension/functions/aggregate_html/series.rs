// ============================================================================
// 内部表示：一个点、一组点 → 序列
//
// # 价格/净值 → 收益率
//
// 组内先按 `date` 排序，再逐点算 `price_t / price_{t-1} - 1`：每组的第一个点没有前值，丢弃；
// 前值不是有限数、或为 0 时该点也跳过（与「NULL 行跳过」同一语义，而不是报错或往序列里塞 inf/NaN）。
// 同一分组内同一天应当只有一个点，否则差分出来的是那一天内部的变动，没有意义。
//
// # 排序
//
// DuckDB 并行/分块执行时 combine 的调用顺序不保证，所以聚合状态里只做「拼接」，
// 真正的按日期排序交给 `ReturnSeries::new`（它内部会 sort）。因此 SQL 侧**不需要 ORDER BY**；
// 只有价格那一支要先按日期排好才能差分，见 `prices_to_returns`。
//
// # 两个分支的点结构体
//
// `QuantstatsReturnPoint`（字段 `period_return`）与 `QuantstatsPricePoint`（字段 `price`）只差一个
// 字段名，聚合对它们做的事完全一样，所以用 [`IntoSeriesPoint`] 把它们归一成 [`SeriesPoint`]。
//
// The internal representation: one point, and a list of points → a series.
//
// # Price/NAV → returns
//
// The points of a group are sorted by `date` first, then each becomes `price_t / price_{t-1} - 1`: the
// first point has no predecessor and is dropped, and a point whose predecessor is not finite or is zero
// is skipped as well — the same "skip it" semantics as a NULL row, rather than an error or an inf/NaN
// inside the series. A group should hold one point per date, otherwise the difference describes the
// movement within that date, which is meaningless.
//
// # Ordering
//
// DuckDB may call combine in any order when running in parallel or in chunks, so the aggregate state only
// concatenates; the real date ordering is done by `ReturnSeries::new` (which sorts internally). SQL
// therefore does **not** need an ORDER BY. Only the price branch has to sort before differencing, see
// `prices_to_returns`.
//
// # The point structs of the two branches
//
// `QuantstatsReturnPoint` (field `period_return`) and `QuantstatsPricePoint` (field `price`) differ only
// in one field name and the aggregate does exactly the same with both, so [`IntoSeriesPoint`] normalises
// them into [`SeriesPoint`].
// ============================================================================

use chrono::{NaiveDate, TimeDelta};
use duckfn::{DuckResult, duck_error};
use quantstats_rs::ReturnSeries;

use crate::extension::types::price_point::QuantstatsPricePoint;
use crate::extension::types::return_point::QuantstatsReturnPoint;

/// 序列里的一个点：**推迟**日期换算，只留自 1970-01-01 起的天数。
///
/// `value` 是通用的：走收益率路径时它是收益率，走价格路径时它是价格/净值（差分之后同样是收益率）。
/// 用具名字段而不是 `(i32, f64)` 元组：两个字段类型不同、含义也不同，元组在调用点很容易写反
/// （四条路径都要用到它）。
///
/// One point of a series: the conversion to a calendar date is deferred, only the day count since
/// 1970-01-01 is kept.
///
/// `value` is generic on purpose: it is a return on the return branch and a price/NAV on the price branch
/// (and again a return once differenced). A named struct rather than an `(i32, f64)` tuple: the two fields
/// differ in type and meaning, and a tuple is easy to get backwards at a call site (all four paths use
/// it).
#[derive(Clone, Copy, Debug)]
pub(super) struct SeriesPoint {
    /// 自 1970-01-01 起的天数（换算成 `NaiveDate` 可能失败，所以留到 `build_series` 里做）。
    ///
    /// Days since 1970-01-01 (converting to `NaiveDate` can fail, so it is left to `build_series`).
    pub(super) days_since_epoch: i32,
    /// 该点的值：收益率，或价格/净值。
    ///
    /// The value of the point: a return, or a price/NAV.
    pub(super) value: f64,
}

/// 把 `DuckDate`（自 1970-01-01 起的天数）换成 chrono 的 `NaiveDate`。
///
/// `ReturnSeries` 要的是 `NaiveDate`，而 duckfn 的 `DuckDate` 只存天数，换算只能自己做。
/// 越界（例如 `DATE 'infinity'` 之类的极端值）返回查询错误，不用会 panic 的运算符。
///
/// Convert a `DuckDate` (days since 1970-01-01) into a chrono `NaiveDate`.
///
/// `ReturnSeries` wants `NaiveDate`, while duckfn's `DuckDate` only stores days, so the conversion is on
/// us. Out-of-range values (e.g. `DATE 'infinity'`) become a query error instead of a panic.
fn naive_date(days_since_epoch: i32) -> DuckResult<NaiveDate> {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|epoch| epoch.checked_add_signed(TimeDelta::days(i64::from(days_since_epoch))))
        .ok_or_else(|| {
            duck_error(format!(
                "cannot convert DuckDate {{ days_since_epoch: {days_since_epoch} }} into a calendar date"
            ))
        })
}

/// 一组点 → `ReturnSeries`。排序由 `ReturnSeries::new` 内部完成，所以 SQL 侧不需要 `ORDER BY`，
/// `combine` 的拼接顺序也不影响结果。
///
/// A list of points → `ReturnSeries`. Sorting happens inside `ReturnSeries::new`, so SQL does not need an
/// `ORDER BY` and the concatenation order inside `combine` does not matter.
pub(super) fn build_series(points: &[SeriesPoint], name: Option<String>) -> DuckResult<ReturnSeries> {
    let mut dates = Vec::with_capacity(points.len());
    let mut values = Vec::with_capacity(points.len());
    for point in points {
        dates.push(naive_date(point.days_since_epoch)?);
        values.push(point.value);
    }

    ReturnSeries::new(dates, values, name)
        .map_err(|err| duck_error(format!("cannot build the returns series: {err}")))
}

/// 价格/净值序列 → 收益率序列。
///
/// 组内先按日期排序，再逐点算 `price_t / price_{t-1} - 1`：第一个点没有前值，丢弃；前值缺失、为 0
/// 或不是有限数时该点也跳过。跳过只影响它自己 —— 下一个点仍然和它自己的前一个点比，不做链式累乘。
///
/// 点数不足 2 个（或全被跳过）时返回空数组，调用方按「没有有效行」处理。
///
/// Price/NAV series → return series.
///
/// The points of a group are sorted by date first, then each becomes `price_t / price_{t-1} - 1`: the
/// first point has no predecessor and is dropped, and a point whose predecessor is missing, zero or not
/// finite is skipped as well. Skipping affects only that point — the next one is still compared with its
/// own predecessor, there is no chained multiplication.
///
/// Fewer than two points (or everything skipped) yields an empty vector, which callers treat as "no valid
/// row".
pub(super) fn prices_to_returns(prices: &[SeriesPoint]) -> Vec<SeriesPoint> {
    let mut sorted = prices.to_vec();
    sorted.sort_by_key(|point| point.days_since_epoch);

    let mut returns = Vec::with_capacity(sorted.len().saturating_sub(1));
    for pair in sorted.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        if !previous.value.is_finite() || previous.value == 0.0 {
            continue;
        }
        returns.push(SeriesPoint {
            days_since_epoch: current.days_since_epoch,
            value: current.value / previous.value - 1.0,
        });
    }
    returns
}

/// 基准点「归一化」成内部表示。
///
/// 收益率路径与价格路径的点结构体只差一个字段名（`period_return` / `price`），聚合对它们要做的事完全
/// 一样，所以抽一层把它们统一成 [`SeriesPoint`]。
///
/// Normalising a benchmark point into the internal representation.
///
/// The point structs of the two branches differ only in one field name (`period_return` / `price`) and the
/// aggregate does exactly the same with both, so this trait normalises them into [`SeriesPoint`].
pub(super) trait IntoSeriesPoint {
    /// 缺日期或缺失值的点返回 `None` —— 跳过该点，而不是让整条查询失败。
    ///
    /// A point missing its date or its value yields `None` — the point is skipped rather than failing the
    /// whole query.
    fn into_series_point(self) -> Option<SeriesPoint>;
}

impl IntoSeriesPoint for QuantstatsReturnPoint {
    fn into_series_point(self) -> Option<SeriesPoint> {
        let (Some(date), Some(value)) = (self.date, self.period_return) else {
            return None;
        };
        Some(SeriesPoint {
            days_since_epoch: date.days_since_epoch,
            value,
        })
    }
}

impl IntoSeriesPoint for QuantstatsPricePoint {
    fn into_series_point(self) -> Option<SeriesPoint> {
        let (Some(date), Some(value)) = (self.date, self.price) else {
            return None;
        };
        Some(SeriesPoint {
            days_since_epoch: date.days_since_epoch,
            value,
        })
    }
}
