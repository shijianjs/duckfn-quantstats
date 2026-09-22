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
// ============================================================================

use duckfn::{DuckDate, DuckResult, duck_error};
use quantstats_rs::ReturnSeries;

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
    /// 自 1970-01-01 起的天数（换算成 `NaiveDate` 可能失败，所以留到 `build_series` 里交给 duckfn 的
    /// chrono 桥做）。
    ///
    /// Days since 1970-01-01 (converting to `NaiveDate` can fail, so it is left to `build_series`, which
    /// hands it to duckfn's chrono bridge).
    pub(super) days_since_epoch: i32,
    /// 该点的值：收益率，或价格/净值。
    ///
    /// The value of the point: a return, or a price/NAV.
    pub(super) value: f64,
}

/// 一组点 → `ReturnSeries`。排序由 `ReturnSeries::new` 内部完成，所以 SQL 侧不需要 `ORDER BY`，
/// `combine` 的拼接顺序也不影响结果。
///
/// 天数 → `NaiveDate` 这一步交给 duckfn 的 chrono 桥（`DuckDate::to_naive_date`，靠 Cargo 里的
/// `chrono` feature 打开）：纪元常数、单位换算与溢出检查都是它的活，`DATE 'infinity'`
/// （DuckDB 用 `i32::MAX` 表示）这类越界值由它转成查询错误，不会 panic。这里只把内部表示补回
/// `DuckDate` —— 它产出的正是 `ReturnSeries::new` 要的 `chrono::NaiveDate`。
///
/// A list of points → `ReturnSeries`. Sorting happens inside `ReturnSeries::new`, so SQL does not need an
/// `ORDER BY` and the concatenation order inside `combine` does not matter.
///
/// The day count → `NaiveDate` step belongs to duckfn's chrono bridge (`DuckDate::to_naive_date`, enabled
/// by the `chrono` feature in Cargo.toml): the epoch constant, the unit conversion and the overflow check
/// are its job, and out-of-range values such as `DATE 'infinity'` (DuckDB represents it as `i32::MAX`)
/// come back as a query error rather than a panic. All this code does is put the internal representation
/// back into a `DuckDate` — and what comes out is exactly the `chrono::NaiveDate` `ReturnSeries::new`
/// wants.
pub(super) fn build_series(points: &[SeriesPoint], name: Option<String>) -> DuckResult<ReturnSeries> {
    let mut dates = Vec::with_capacity(points.len());
    let mut values = Vec::with_capacity(points.len());
    for point in points {
        let date = DuckDate {
            days_since_epoch: point.days_since_epoch,
        };
        dates.push(date.to_naive_date()?);
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
