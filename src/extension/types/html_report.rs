use duckfn::DuckStruct;

// ============================================================================
// 一份报告的结果行
//
// 一次调用要出很多份报告（表里每个 symbol × 每个基准各一份），所以返回值是**一个数组**，元素就是这里的
// 结构体：`STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]`。
//
// 为什么把「报告给的谁、对哪个基准、叫什么、写到哪」也带出来：一份报告是几百 KB 的 HTML，落盘之后调用方
// 真正关心的是「文件在哪、这份是跟谁比的、这个标的显示成什么名字」，把这些当成数据行返回，SQL 侧才能继续
// `unnest` / `list_transform` / `COPY`，而不是回终端里一份份翻 —— 同一 symbol 的多行（一个基准一行）也
// 只有靠 `benchmark` 字段才分得清谁是谁。
//
// 两个显示名都回显（`strategy_title` / `benchmark_title`）：它们就是报告图例与文件名里实际用的那两份
// （缺省时各自退回自己的 symbol），回显出来才不必为了看一眼显示名去 HTML 里抠。
//
// 字段名就是 SQL 里的键名（duckfn 的 `DuckStruct` 派生不支持字段级改名）。`benchmark` 与
// `benchmark_title` 同生同灭、都可空 —— 没配基准时那份报告是单序列的，既没有基准可写、也没有基准显示名；
// `file_path` 也可空 —— 没配 `output_dir`（也没开浏览器）时报告没有落过盘，没有路径可回填。
//
// **不注册命名类型**（`create_type` 默认关闭）：返回值本身就带着完整的匿名
// `STRUCT(...)[]`，SQL 里按字段名取用即可，再注册一个类型名只是多一份要维护的表面。
//
// One row of the report result.
//
// A single call produces many reports (one per symbol × benchmark in the table), so the return value is a
// **list** whose elements are this struct: `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html,
// file_path)[]`.
//
// The row also carries "whose report, against which benchmark, what it is called, where it was written": a
// report is a few hundred KB of HTML, and after it has been persisted the caller really cares about the file,
// about what it was compared against and about the display name — returning them as ordinary data is what
// lets SQL keep processing the reports (`unnest`, `list_transform`, `COPY`) instead of scrolling through a
// terminal, and it is also the only way to tell the several rows of one symbol (one per benchmark) apart.
//
// Both display names are echoed back (`strategy_title` / `benchmark_title`): they are exactly the ones the
// report legend and the file name used (each falling back to its own symbol), so echoing them saves digging
// through the HTML just to see what a report ended up calling itself.
//
// The field names are the keys in SQL (duckfn's `DuckStruct` derive has no field-level renaming). `benchmark`
// and `benchmark_title` are born and gone together and both are nullable — with no benchmark configured that
// report is a single-series one and has neither a benchmark nor a benchmark name; `file_path` is nullable
// too — without `output_dir` (and without the browser) the report never leaves memory, so there is no path to
// report back.
//
// **No named type is registered** (`create_type` defaults to off): the return value already carries
// the full anonymous `STRUCT(...)[]` and SQL can read it by field name. A type name on top would
// only add another surface to maintain.
// ============================================================================

/// 一份报告的结果行：`(标的, 基准, 策略显示名, 基准显示名, 报告 HTML, 落盘路径)`。
///
/// One row of the report result: `(symbol, benchmark, strategy name, benchmark name, report HTML, written
/// path)`.
#[derive(Clone, Debug, Default, DuckStruct)]
pub(crate) struct QuantstatsHtmlReport {
    /// 这个 symbol 的名字，也就是它在输入列里的取值。
    ///
    /// The name of this symbol, i.e. the value it had in the input column.
    pub symbol: String,

    /// 这一份报告用的基准 symbol；没配基准（单序列报告）时为 NULL。
    ///
    /// 配置里可以给多个基准，那时同一个 `symbol` 会出现多行，靠这个字段区分。列表顺序就是同标的下各行的
    /// 顺序。
    ///
    /// The benchmark symbol this report used; NULL when no benchmark was configured (a single-series
    /// report).
    ///
    /// Several benchmarks may be configured, in which case one `symbol` shows up in several rows, told
    /// apart by this field. The order of the configured list is the order of the rows inside one symbol.
    pub benchmark: Option<String>,

    /// 报告里用的显示名：配置写了 `strategy_title` 就是它，没写则退回 `symbol`。
    ///
    /// The display name used in the report: the configured `strategy_title` when there is one,
    /// otherwise the `symbol` itself.
    pub strategy_title: String,

    /// 报告里用的基准显示名：取了 `benchmark_title` 按下标与 `benchmark` 对应的那一项，缺项（列表短了 /
    /// NULL / 空串）则退回 `benchmark` 本身。
    ///
    /// 与 `benchmark` 同生同灭：没配基准（单序列报告）时为 NULL。
    ///
    /// The benchmark display name used in the report: the `benchmark_title` entry paired with `benchmark` by
    /// index, falling back to `benchmark` itself when that entry was not given (shorter list / NULL / empty
    /// string).
    ///
    /// Born and gone with `benchmark`: NULL for a single-series report (no benchmark configured).
    pub benchmark_title: Option<String>,

    /// 渲染好的完整 HTML 报告。
    ///
    /// The rendered HTML report.
    pub html: String,

    /// 报告实际落盘的路径；没落盘（没配 `output_dir`）时为 NULL。
    ///
    /// The path the report was actually written to; NULL when nothing was written (no `output_dir`).
    pub file_path: Option<String>,
}
