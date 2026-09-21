// ============================================================================
// 两个参数槽：配置与基准
//
// 它们的共同点是「多行不变，却比被聚合的值贵」—— 配置是一个结构体，基准是一整条序列。duckfn 的适配层是
// **逐行**读参数的（`aggregate_function_adapter.rs` 的 `read_columns` 坐在行循环里），而
// `Vec<T>::read_valid` 每次调用都会新建 Vec 并逐元素复制整个列表，所以两者都写成 `DuckLazy<T>`：
// 每行只构造一个 O(1) 的凭证，把真正的解析推迟到我们自己选定的时刻。
//
// 推迟到哪一刻？只在本行回调里。`DuckLazy<T>` 的契约是「凭证只在产生它的那次回调内有效」，把它存进聚合
// 状态、跨 chunk / 跨线程再解析会被运行时守卫拦下（报 "DuckLazy<T> is stale"，是查询报错而不是 UB），
// 能活下来的只有**解析结果**。而「每行都读、只真正解析第一行、combine 时搬运、result 里取值」正是这条
// 路径的固定形状，0.0.6 起由 duckfn 的 `DuckLazySlot<T>` 直接提供，本文件不再自己写三态枚举：
//
//   update 回调    slot.resolve_optional(arg.as_ref())?   首行解析一次，其余行只加一次引用计数
//   simple_combine slot.combine(&other.slot)              搬运已解析的值，不重新解析
//   result         slot.get()                            `Option<Arc<T>>`；未解析与「解析为 NULL」都是 None
//
// 留在这里的是 duckfn 管不到的两件事：
//
//   1. 基准列表的**报错**：`try_get` 的原始错误是写给 duckfn 使用者看的，SQL 调用方需要的是「哪个函数、
//      该写哪个键、不想带基准该怎么办」；
//   2. 基准列表的**归一化**：把 `Vec<点>` 折叠成内部的 [`SeriesPoint`]，顺便丢掉缺日期 / 缺值的点。
//
// 配置侧没有这两件事（它的类型是具名 STRUCT，读不出来就是用户写错了），所以配置槽只用到上面那三行调用，
// 没有别的适配代码。`NULL` 配置退化成全默认的处理在 report.rs 里（那儿才知道要交给哪个渲染函数）。
//
// # 为什么基准是「列表参数」而不是「同组里的标签行」
//
// 聚合函数只看得到自己组内的行。若把基准写成同组内的行（长表 + 一个「哪个标签是基准」的配置项），
// 那么为了让每个分组都拿得到基准，基准行就必须在**每个分组里各出现一遍**：100 个标的 × 1000 天
// = 10 万行被物化/扫描，而基准本身只有 1000 行。改成一次性传入的列表后，基准只写一次、只求值一次，
// 策略侧仍然靠 GROUP BY 自然分组，也不再需要「哪个标签是基准」这类配置。
//
// 顺带消失的还有两条护栏：分组内不会再混进别的标的，也不会出现「只有基准行、没有策略行」的组。
//
// Two argument slots: the configuration and the benchmark.
//
// Both of them are "constant across rows and more expensive than the aggregated values" — one is a struct,
// the other a whole series. duckfn's adapter reads arguments **per row** (`read_columns` sits inside the
// row loop of `aggregate_function_adapter.rs`) and `Vec<T>::read_valid` allocates a fresh Vec and copies
// every element on each call, so both are written as `DuckLazy<T>`: every row only builds an O(1) token,
// and the real parse is deferred to the moment we choose.
//
// That moment is inside the row callback, and only there: a `DuckLazy<T>` token is only valid inside the
// callback that produced it, so storing it in the aggregate state and consuming it past the chunk or on
// another thread is rejected by the runtime guard ("DuckLazy<T> is stale" — a query error, not UB). Only
// the *parsed value* can survive. "Read every row, really parse only the first one, carry the result over
// in `combine`, read it back in `result`" is exactly the shape duckfn's `DuckLazySlot<T>` provides since
// 0.0.6, so this file no longer hand-rolls the three-state enum.
//
// What is left here are the two things duckfn cannot know about:
//
//   1. the benchmark list **errors** (the raw `try_get` message is written for a duckfn user, while a SQL
//      caller needs "which function, which key, and what to do instead");
//   2. the benchmark list **normalisation** — folding `Vec<point>` into the internal [`SeriesPoint`] and
//      dropping the points that miss their date or their value.
//
// The options side needs neither (it is a named STRUCT: if it cannot be read, the user wrote it wrong), so
// the options slot is used through nothing but the three calls above. Falling back to all defaults when
// the options column is NULL happens in report.rs, where the target renderer is known.
//
// # Why the benchmark is a list argument rather than labelled rows in the same group
//
// An aggregate only sees the rows of its own group. Representing the benchmark as rows in the same group
// (a long table plus a config key saying which label is the benchmark) forces the benchmark rows to appear
// **once per group**: 100 instruments × 1000 days = 100k rows materialised and scanned, while the
// benchmark itself is only 1000 rows. With a list passed in once, the benchmark is written once and
// evaluated once, the strategy side still gets its grouping from GROUP BY, and the "which label is the
// benchmark" config key disappears.
//
// Two guards disappear with it: a group can no longer mix several instruments, and a group can no longer
// hold only benchmark rows.
// ============================================================================

use duckfn::{DuckLazy, DuckLazySlot, DuckResult, DuckValueType, duck_error};

use super::kind::SeriesKind;
use super::series::{IntoSeriesPoint, SeriesPoint};

/// 行回调里解析基准列表：首行解析一次并缓存，其余行只付 O(1) 的成本。
///
/// 解析失败时把错误换成对 SQL 调用方有意义的说法，并**保持槽原样**（`DuckLazySlot` 的语义），
/// 于是下一行还会再试一次；当然，这里返回的错误会立刻让整条查询失败。
///
/// Parses the benchmark list in the row callback: once on the first row, O(1) afterwards.
///
/// A failed parse is turned into a message that means something to a SQL caller and leaves the slot
/// untouched (that is `DuckLazySlot`'s own semantics), so the next row would try again — although the
/// error returned here fails the whole query straight away.
pub(super) fn resolve_benchmark<T: DuckValueType>(
    slot: &mut DuckLazySlot<Vec<T>>,
    benchmark: Option<&DuckLazy<Vec<T>>>,
    kind: SeriesKind,
) -> DuckResult<()> {
    let SeriesKind {
        function,
        value_field,
    } = kind;

    // 上游的错误信息是写给 duckfn 使用者的（会提到 `try_get()`），这里换成对 SQL 调用方有意义的说法：
    // 列表里出现了整体为 NULL 的元素（例如字面量 `[NULL, ...]`）时就会走到这里。
    //
    // The upstream message is written for a duckfn user (it mentions `try_get()`), so it is replaced with
    // something meaningful to a SQL caller: this is reached when the list contains a whole-NULL element
    // (e.g. a literal `[NULL, ...]`).
    slot.resolve_optional(benchmark).map_err(|err| {
        duck_error(format!(
            "{function}: cannot read the benchmark list — every element must be a \
             STRUCT(date DATE, {value_field} DOUBLE) and must not be NULL ({err})"
        ))
    })?;
    Ok(())
}

/// 把基准列表从槽里取出来，归一化成内部的点数组。
///
/// 只在 `result()` 里调用（每组一次）：归一化要遍历整条列表，放在逐行的 `resolve_benchmark` 里就成了
/// O(行数 × 基准长度)，正是 `DuckLazy` / `DuckLazySlot` 要消掉的那笔开销。
///
/// Reads the benchmark list out of the slot and normalises it into the internal point array.
///
/// Called from `result()` only (once per group): normalising walks the whole list, and doing that in the
/// per-row `resolve_benchmark` would be O(rows × benchmark length) — exactly the cost `DuckLazy` and
/// `DuckLazySlot` exist to remove.
pub(super) fn benchmark_points<T: IntoSeriesPoint>(
    slot: &DuckLazySlot<Vec<T>>,
    kind: SeriesKind,
) -> DuckResult<Vec<SeriesPoint>> {
    let SeriesKind {
        function,
        value_field,
    } = kind;

    // `get()` 的 `None` 同时覆盖「没解析过」与「解析成 NULL」。前者在这儿不可达：一组一行都没有时
    // `result()` 已经提前返回 NULL，而只要 update 跑过一次，槽里就一定有解析结果。所以走到这里的就是
    // 「整列基准都是 NULL」那种情况。
    //
    // `get()` yields `None` for both "never parsed" and "parsed as NULL". The former is unreachable here:
    // `result()` already returns NULL when a group has no row at all, and a single update call is enough to
    // put a parse result in the slot. So what lands here is a benchmark column that is NULL throughout.
    let Some(parsed) = slot.get() else {
        return Err(duck_error(format!(
            "{function}: the benchmark list must not be NULL — pass the benchmark series as \
             list({{'date': ..., '{value_field}': ...}}), or omit the benchmark argument for a \
             single-series report"
        )));
    };

    let points: Vec<SeriesPoint> = parsed
        .iter()
        .filter_map(IntoSeriesPoint::to_series_point)
        .collect();

    // 空列表，或列表里每个点都缺日期/缺值 —— 两者对报告来说一样：没有基准可用。这一支重载就是为带基准
    // 的场景存在的，所以报错而不是静默出一份没有基准的报告。
    //
    // An empty list, or a list whose every point misses its date or its value — both mean the same to the
    // report: there is no benchmark to use. This overload exists for the benchmark case, so it errors out
    // instead of silently producing a benchmark-less report.
    if points.is_empty() {
        return Err(duck_error(format!(
            "{function}: the benchmark list is empty (or every point is missing its date or its \
             value) — omit the benchmark argument for a single-series report"
        )));
    }

    Ok(points)
}
