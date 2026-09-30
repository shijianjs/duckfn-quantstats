use duckfn::DuckStruct;

// ============================================================================
// 翻译表：输入项与输出行
//
// 翻译表是**进程内运行时状态**（见 functions/translation/table.rs），SQL 侧只需要两个形状：
//
//   qs_translation_entry   写入口：一条「key + 译文 + 说明」（`qs_set_translation` 的 list 元素）
//   TranslationRow         读出口：`qs_list_translations()` 的一行（language + key + show + description）
//
// 写入口注册成命名类型（`create_type = true`），于是 SQL 里既能写字面量列表，也能 cast：
//
//   SELECT qs_set_translation('zh-CN', [
//       {'key': 'metric.sharpe', 'show': '夏普比率', 'description': '…'}
//   ]);
//
// 字段**全部**是 `Option<T>`，理由与 `html_report_options.rs` 相同：DuckDB 给 struct 字面量缺的键补
// NULL，而 duckfn 见到「非 Option 字段是 NULL」会把**整个 struct** 变成 NULL —— 那样只写 `key` 的项会被
// 静默丢掉，而「只写 key、不写 show」恰恰是删除一项的写法。
//
// Translation table: the input entry and the output row.
//
// The table is **in-process runtime state** (see functions/translation/table.rs) and SQL only needs two
// shapes: the write-side entry (one "key + show + description", the list element of
// `qs_set_translation`) and the read-side row of `qs_list_translations()`.
//
// The write-side entry is registered as a named type (`create_type = true`) so SQL can pass a literal
// list or cast to it. Every field is `Option<T>` for the same reason as in `html_report_options.rs`:
// DuckDB fills a struct literal's missing keys with NULL and duckfn turns the **whole struct** into NULL
// when a non-Option field reads NULL — which would silently drop an entry that only carries `key`, and
// "only `key`" is exactly how a deletion is written.
// ============================================================================

/// `qs_set_translation` 列表里的一项：改写或删除**一个 key** 的译法与说明。
///
/// 三项的含义（`NULL` 与空串在这里不是一回事，逐项写死在 `table.rs` 的 `apply` 里）：
///
/// - `key`：**必填**，且必须是内置 key 之一（`qs_list_translations()` 列得出来）。写错的 key 会报错 ——
///   它对应报告里一个具体的 DOM 位置，凭空造的 key 永远不会生效；
/// - `show`：**NULL 或空串表示删除这一项**（说明一并删掉，不做任何回退）；
/// - `description`：`NULL` 表示**保留原有说明**（只改译文时不必把说明重抄一遍），空串表示把说明清掉
///   （报告里就不再挂 `title`）。
///
/// One entry of the `qs_set_translation` list: rewrite or delete **one key**.
///
/// What the three fields mean (`NULL` and an empty string are not the same thing here; the exact rules
/// live in `table.rs::apply`): `key` is required and must be one of the built-in keys; `show` being
/// NULL or empty **deletes the entry** (description included, with no fallback); `description` being
/// NULL **keeps the existing description** (so rewriting just the `show` needs no retyping) while an
/// empty string clears it (no `title` is attached in the report).
#[derive(Clone, Debug, Default, DuckStruct)]
#[duck(
    sql_name = "qs_translation_entry",
    create_type = true
)]
pub(crate) struct TranslationEntry {
    /// 要改写或删除的 key。
    ///
    /// The key to rewrite or delete.
    pub key: Option<String>,

    /// 该 key 在这个语言里的显示文本；NULL / 空串表示删除这一项。
    ///
    /// The display text of that key in this language; NULL / empty deletes the entry.
    pub show: Option<String>,

    /// 该 key 的简要说明，渲染成所在元素的 `title`（原生浮出提示）；NULL 表示保留原有说明。
    ///
    /// A short note for that key, rendered as the `title` of the element it sits in (the browser's
    /// native tooltip); NULL keeps the existing description.
    pub description: Option<String>,
}

/// `qs_list_translations()` 的一行：按 (language, key) 升序排列。
///
/// 字段名即结果列名；`description` 不可空（没有说明时是空串而不是 NULL），所以 SQL 侧不必先做空值判断
/// 就能直接 `concat`。
///
/// 这个结构体是 `pub` 而 [`TranslationEntry`] 不是：`#[duck_table_function]` 生成的是一个**公开**的
/// `TableFunctionImpl`（它的 `type Output` 直接被写进公开接口），而 `pub(crate)` 的输出类型会在那里触发
/// E0446；标量函数的入参不经过公开接口（生成的参数结构体字段是私有的），所以不必跟着放宽。模块本身仍然是
/// 私有的，`pub` 只是「在这条路径上不再是 crate 私有」。
///
/// One row of `qs_list_translations()`, ordered by (language, key). The field names are the column names,
/// and `description` is non-nullable (an empty string rather than NULL when there is no note), so SQL can
/// concatenate it without a null check.
///
/// This struct is `pub` while [`TranslationEntry`] is not: `#[duck_table_function]` generates a **public**
/// `TableFunctionImpl` whose `type Output` lands in a public interface, where a `pub(crate)` output type trips
/// E0446. A scalar function's parameters do not go through a public interface (the generated argument struct's
/// fields are private), so they need no widening. The module itself stays private — `pub` here only means "no
/// longer crate-private along that one path".
#[derive(Clone, Debug, Default, DuckStruct)]
pub struct TranslationRow {
    /// 语言标签，如 `zh-CN`。
    ///
    /// The language tag, e.g. `zh-CN`.
    pub language: String,

    /// 报告里一个可翻译位置的名字。
    ///
    /// The name of one translatable position in the report.
    pub key: String,

    /// 该 key 在这个语言里的显示文本。
    ///
    /// The display text of that key in this language.
    pub show: String,

    /// 该 key 的简要说明，会成为报告里那个元素的 `title`。
    ///
    /// The short note for that key, which becomes that element's `title` in the report.
    pub description: String,
}
