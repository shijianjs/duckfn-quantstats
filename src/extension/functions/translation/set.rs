// ============================================================================
// qs_set_translation(language, entries) -> BOOLEAN
//
// 一次调用改写一个语言的一批 key；规则的正文在 `types/translation.rs`（`TranslationEntry` 的三个字段各
// 自意味着什么）与 `table.rs::apply`（怎么落进那张表）。这个文件只负责把它接成 SQL：
//
//   SELECT qs_set_translation('zh-CN', [
//       {'key': 'metric.sharpe', 'show': '夏普比率', 'description': '每单位波动换来的超额收益 ⚖️'},
//       {'key': 'month.jan',    'show': '1月',     'description': '一月 ❄️'}
//   ]);
//   -- true
//
//   SELECT qs_set_translation('zh-CN', NULL);   -- 整个语言删掉
//   -- true
//
// 两个参数各有讲究：
//
//   - `language` 是**不可空**的 `String`：写 NULL 时 duckfn 的读取层会把整行短路成 NULL（函数体不执行），
//     于是「语言是 NULL」不会变成「把谁删了」这种意外；空串由 `table::apply` 报错。
//   - `entries` 是 `Option<Vec<Option<TranslationEntry>>>`：外层 `Option` 收下 SQL 的 `NULL`（= 删掉整个
//     语言），内层 `Option` 收下列表里的 `NULL` 元素，由 `table::apply` 判定那是配置错误。
//
// 返回值是「表是否真的变了」，不是恒真的 `true` —— 见 `table.rs::apply`。
//
// One call rewrites a batch of keys of one language. The rules live in `types/translation.rs` (what each of
// `TranslationEntry`'s three fields means) and `table.rs::apply` (how it lands in the table); this file only
// wires them up to SQL. Both arguments are deliberate: `language` is a non-nullable `String`, so a NULL
// short-circuits the whole row to NULL (the body never runs) rather than turning "the language is NULL" into
// "something was deleted", while an empty string is an error from `table::apply`; `entries` is an
// `Option<Vec<Option<TranslationEntry>>>`, the outer `Option` taking SQL `NULL` (delete the whole language) and
// the inner one taking a NULL element inside the list, which `table::apply` judges to be a configuration error.
// The return value is "did the table change" rather than a constant `true` — see `table.rs::apply`.
// ============================================================================

use duckfn::{DuckOptionResult, duck_scalar_function};

use crate::extension::types::translation::TranslationEntry;

use super::table;

/// 改写或删除一个语言的一批翻译项；返回翻译表是否真的变了。
///
/// `special_null_handling = true` 是**要删整个语言**这一步的前提：DuckDB 默认的 null 处理会在参数是 NULL 时
/// 直接把结果置成 NULL、根本不调用函数，那样 `qs_set_translation('zh-CN', NULL)` 只会得到 NULL 而不是「删掉
/// 这个语言」。开了这个开关之后 NULL 才会真的进入函数体；而 `language` 写成不可空的 `String`，所以它仍然是
/// NULL 就整行短路成 NULL（读取层的既有语义），不会被误当成「删了谁」。
///
/// Rewrite or delete a batch of translations of one language; returns whether the table really changed.
///
/// `special_null_handling = true` is what makes "delete the whole language" possible: DuckDB's default null
/// handling turns the result into NULL whenever an argument is NULL and never calls the function at all, so
/// `qs_set_translation('zh-CN', NULL)` would just be NULL instead of "delete that language". With the switch on,
/// a NULL really reaches the body — while `language` being a non-nullable `String` still short-circuits the whole
/// row to NULL (the argument reader's existing semantics), so it can never be mistaken for "something got
/// deleted".
#[duck_scalar_function(
    special_null_handling = true,
    description = "Overwrites or deletes translations of one language in this DuckDB process, entry by entry",
    comment = "Setting 'show' to NULL or an empty string deletes that key and its note; a NULL entries list deletes the whole language; a NULL 'description' keeps the existing note. Nothing is persisted: reloading the extension or restarting the process restores the built-in data",
    examples = [
        "SELECT qs_set_translation('de', [{'key': 'month.jan', 'show': 'Januar', 'description': 'January'}])",
        "SELECT qs_set_translation('zh-CN', NULL)"
    ]
)]
fn qs_set_translation(
    language: String,
    entries: Option<Vec<Option<TranslationEntry>>>,
) -> DuckOptionResult<bool> {
    // `entries.as_deref()` 把 `Option<Vec<..>>` 变成 `Option<&[..]>`：表那边只需要看，不必持有列表。
    //
    // `entries.as_deref()` turns `Option<Vec<..>>` into `Option<&[..]>`: the table side only has to look at
    // the list, never own it.
    Ok(Some(table::apply(&language, entries.as_deref())?))
}
