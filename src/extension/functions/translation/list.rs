// ============================================================================
// qs_list_translations() —— 把当前翻译表摊成一张表
//
//   SELECT * FROM qs_list_translations();
//   -- lang | key              | label     | description
//   -- de   | date.range       | …         | …
//   -- de   | dist.daily       | Täglich   | Tagesrenditen 📆
//   -- …
//
// 它反映的是**当前进程里的表**（内置数据 + `qs_set_translation()` 改过的部分），所以它同时是两件东西：
//
//   - 查 key 的地方 —— `qs_set_translation()` 报「unknown key」时的下一步；
//   - 查语言的地方 —— `{'lang': …}` 报「没有这个语言」时的下一步。
//
// 输出按 (lang, key) 升序。没有参数：一次要看的通常是「某几个 key 译成了什么」，用 WHERE 过滤比再加一个
// 参数更顺手（`… WHERE key LIKE 'metric.%'`）。
//
// 快照是**克隆**出来的：函数返回后读锁就放了，扫描期间不占着翻译表。
//
// Flattens the current translation table into a table. What it reflects is the table **in this process**
// (built-in data plus whatever `qs_set_translation()` changed), which makes it two things at once: where to
// look up a key (the next step after `qs_set_translation()` says "unknown key") and where to look up a language
// (the next step after `{'lang': …}` says there is no such language). The output is ordered by (lang, key). It
// takes no parameters: what one usually wants is "how are these few keys translated", and a `WHERE` clause beats
// an extra argument (`… WHERE key LIKE 'metric.%'`). The snapshot is **cloned**, so the read lock is released
// before the function returns and the scan does not sit on the table.
// ============================================================================

use duckfn::duck_table_function;

use crate::extension::types::translation::TranslationRow;

use super::table;

/// 列出当前进程里翻译表的全部条目，按 (lang, key) 升序。
///
/// List every entry of this process's translation table, ordered by (lang, key).
#[duck_table_function(
    description = "Lists the translations of this DuckDB process, ordered by lang and key",
    comment = "Reflects the built-in data plus every qs_set_translation() call of this process; nothing is persisted, so reloading the extension restores the built-in table",
    example = "SELECT * FROM qs_list_translations() WHERE key LIKE 'metric.%'"
)]
fn qs_list_translations() -> impl Iterator<Item = TranslationRow> {
    table::snapshot().into_iter()
}
