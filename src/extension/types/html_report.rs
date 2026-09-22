use duckfn::DuckStruct;

// ============================================================================
// 一份报告的结果行
//
// 一次调用要出很多份报告（表里每个 symbol 一份），所以返回值是**一个数组**，元素就是这里的
// 结构体：`STRUCT(symbol, strategy_title, html, file_path)[]`。
//
// 为什么把「报告给的谁、叫什么、写到哪」也带出来：一份报告是几百 KB 的 HTML，落盘之后调用方
// 真正关心的是「文件在哪、这个标的显示成什么名字」，把这些当成数据行返回，SQL 侧才能继续
// `unnest` / `list_transform` / `COPY`，而不是回终端里一份份翻。
//
// 字段名就是 SQL 里的键名（duckfn 的 `DuckStruct` 派生不支持字段级改名）。`symbol` /
// `strategy_title` / `html` 一定存在；只有 `file_path` 可空 —— 没配 `output`（也没开浏览器）
// 时报告没有落过盘，没有路径可回填。
//
// **不注册命名类型**（`create_type` 默认关闭）：返回值本身就带着完整的匿名
// `STRUCT(...)[]`，SQL 里按字段名取用即可，再注册一个类型名只是多一份要维护的表面。
//
// One row of the report result.
//
// A single call produces many reports (one per symbol in the table), so the return value is a
// **list** whose elements are this struct: `STRUCT(symbol, strategy_title, html, file_path)[]`.
//
// The row also carries "whose report, what it is called, where it was written": a report is a few
// hundred KB of HTML, and after it has been persisted the caller really cares about the file and
// the display name — returning them as ordinary data is what lets SQL keep processing the reports
// (`unnest`, `list_transform`, `COPY`) instead of scrolling through a terminal.
//
// The field names are the keys in SQL (duckfn's `DuckStruct` derive has no field-level renaming).
// `symbol` / `strategy_title` / `html` are always there; only `file_path` is nullable — without
// `output` (and without the browser) the report never leaves memory, so there is no path to
// report back.
//
// **No named type is registered** (`create_type` defaults to off): the return value already carries
// the full anonymous `STRUCT(...)[]` and SQL can read it by field name. A type name on top would
// only add another surface to maintain.
// ============================================================================

/// 一份报告的结果行：`(标的, 显示名, 报告 HTML, 落盘路径)`。
///
/// One row of the report result: `(symbol, display name, report HTML, written path)`.
#[derive(Clone, Debug, Default, DuckStruct)]
pub(crate) struct QuantstatsHtmlReport {
    /// 这个 symbol 的名字，也就是它在输入列里的取值。
    ///
    /// The name of this symbol, i.e. the value it had in the input column.
    pub symbol: String,

    /// 报告里用的显示名：配置写了 `strategy_title` 就是它，没写则退回 `symbol`。
    ///
    /// The display name used in the report: the configured `strategy_title` when there is one,
    /// otherwise the `symbol` itself.
    pub strategy_title: String,

    /// 渲染好的完整 HTML 报告。
    ///
    /// The rendered HTML report.
    pub html: String,

    /// 报告实际落盘的路径；没落盘（没配 `output`）时为 NULL。
    ///
    /// The path the report was actually written to; NULL when nothing was written (no `output`).
    pub file_path: Option<String>,
}
