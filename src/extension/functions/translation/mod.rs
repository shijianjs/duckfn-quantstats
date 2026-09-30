// ============================================================================
// 翻译：把 QuantStats 报告里的固定文本换成别的语言，并给每个位置挂一句说明
//
// # 一次调用是怎么走完的
//
//   1. 报告选项里写 `{'language': 'zh-CN'}`（见 types/html_report_options.rs）；
//   2. 出报告之前先校验这个语言在当前翻译表里有条目（`table::require_language`）—— 写错语言要在渲染之前
//      报错，而不是先渲染几十份几百 KB 的报告再发现；
//   3. 报告渲染完之后、落盘/开浏览器之前，`report::translate_report` 用 lol_html 静态改写一次 HTML；
//   4. 改写只认目录（`keys.rs`）里列出的 DOM 位置，命中才改。
//
// # 状态
//
// 翻译表是**进程内**状态：内置数据在扩展加载时装入（`table::install_builtin`，由 extension/mod.rs 的
// `#[duck_custom_register]` 调用），`qs_set_translation()` 就地改写，`qs_list_translations()` 把它摊成一张
// 表。不做持久化，也没有「删了之后回退到内置」—— 重新加载扩展或重开进程就恢复内置数据。完整的状态图见
// `table.rs` 的头注释。
//
// # 文件分工（按「数据 → 逻辑」的顺序读）
//
//   keys.rs       目录：每个可翻译位置的 key、CSS 选择器、英文原文（`qs_set_translation` 拿它校验 key）
//   builtin/      六种语言的内置数据（`en` 只写说明，其余五种写译文 + 说明）
//   table.rs      进程级状态：装载、读取、改写，以及「语言不存在」的统一报错
//   set.rs        qs_set_translation(language, entries) -> BOOLEAN
//   list.rs       qs_list_translations()
//   report.rs     把译法贴回报告的那次 lol_html 改写（含日期区间模板与月份大小写）
//
// Translation: turn the fixed texts of a QuantStats report into another language and hang a short note on every
// position.
//
// How one call walks through it: the report options carry `{'language': 'zh-CN'}` (see
// types/html_report_options.rs); before anything is rendered the language is checked against the current table
// (`table::require_language`), because a mistyped language has to fail before dozens of few-hundred-KB reports
// are rendered rather than after; once a report has been rendered and before it is persisted or opened,
// `report::translate_report` rewrites the HTML statically with lol_html; and the rewrite only recognises the DOM
// positions listed in the catalog (`keys.rs`), changing nothing it does not recognise.
//
// State: the table is **in-process**. The built-in data is installed when the extension loads
// (`table::install_builtin`, called by the `#[duck_custom_register]` in extension/mod.rs),
// `qs_set_translation()` rewrites it in place and `qs_list_translations()` flattens it into a table. Nothing is
// persisted and there is no "fall back to built-in after a delete" — reloading the extension or restarting the
// process restores the built-in data. The full state diagram lives in the header of `table.rs`.
//
// File layout (read "data" first, then "logic"): `keys.rs` is the catalog — every translatable position's key,
// CSS selector and English source text (`qs_set_translation` validates keys against it); `builtin/` holds the
// built-in data of six languages (`en` carries notes only, the other five carry a translation plus a note);
// `table.rs` is the process-level state — install, read, rewrite and the single wording for "no such language";
// `set.rs` is `qs_set_translation(language, entries) -> BOOLEAN`; `list.rs` is `qs_list_translations()`;
// `report.rs` is the one lol_html rewrite that sticks the translations back onto the report (date-range template
// and month casing included).
// ============================================================================

// `table` 比同层的兄弟宽一档：`extension/mod.rs` 的加载期初始化要从那儿调 `install_builtin`，而它是
// `functions` 的**父**模块，`pub(super)` 到不了。
//
// `table` is one notch more visible than its siblings: the load-time initialisation in `extension/mod.rs` calls
// `install_builtin` from there, and that module is `functions`' **parent**, which `pub(super)` cannot reach.
pub(crate) mod table;

// 其余兄弟只在本子树里用，保持私有就够：`set`/`list` 甚至不必被按名字引用（属性宏已经把函数注册好了），
// 而 `keys`/`builtin`/`report` 的条目都是 `pub(super)`，在 `translation` 及其子树里看得见。
//
// The remaining siblings are used only inside this subtree and stay private: `set`/`list` do not even have to be
// referenced by name (the attribute macros already registered the functions), while `keys`/`builtin`/`report`
// expose `pub(super)` items, which are visible within `translation` and its own subtree.
mod builtin;
mod keys;
mod list;
mod report;
mod set;

// 报告那一侧（`aggregate_html`）只认这两个名字，不必知道翻译子树内部的文件划分。
//
// The report side (`aggregate_html`) only needs these two names and does not have to know how the translation
// subtree splits across files.
pub(crate) use report::translate_report;
pub(crate) use table::require_language;
