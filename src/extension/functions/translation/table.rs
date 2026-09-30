// ============================================================================
// 翻译表：一个进程级状态，装载、读取、改写
//
// # 状态语义（用户明确要的那套）
//
//   extension load
//         │
//         ▼
//   内置翻译数据          ← install_builtin()，由 #[duck_custom_register] 在加载期调用
//         │
//         ▼
//   当前翻译表            ← qs_set_translation() 覆盖 / 删除
//         │
//         ▼
//   扩展重新加载 / 进程重启 → 重新从内置数据初始化
//
// 不做持久化，也不做「删掉之后回退到内置」：`qs_set_translation('zh-CN', NULL)` 之后那个语言就真的没有条目
// 了，再拿它出报告会报错（而不是悄悄退回内置数据）—— 「我删了」和「还能用」同时成立只会让人困惑。
//
// # 为什么是一个 `LazyLock<RwLock<BTreeMap<..>>>`
//
// 翻译表不属于任何一次查询：`qs_set_translation` 改的是「这个 DuckDB 进程里的表」，之后每一次
// `qs_html_reports(..., {'lang': 'zh-CN'})` 都读同一份。所以它是进程级静态 + 一把读写锁（DuckDB 会
// 在多个工作线程上跑聚合，读要能并发）。`BTreeMap` 而不是 `HashMap`：`qs_list_translations()` 的输出要
// 按 (language, key) 有序，用 BTreeMap 就不必再排一遍。
//
// 初始化放在 `#[duck_custom_register]` 而不是 `LazyLock::new`：那样「扩展加载时初始化、重新加载后恢复」
// 是字面意义上的行为，而不是「第一次用到时才初始化」的近似。
//
// # 锁中毒
//
// 拿锁失败（中毒）时不报错：那意味着某次调用在持锁期间 panic 了，而翻译表本身仍然是一致的（每次改写都是
// 单条插入/删除，没有「改一半」的中间态）。继续用比整条查询失败对用户更有用。
//
// The translation table: one process-level state to install, read and rewrite.
//
// State semantics (exactly what was asked for): install_builtin() runs at extension load, through
// `#[duck_custom_register]`; `qs_set_translation()` then overrides or deletes entries; reloading the extension
// or restarting the process resets everything to the built-in data. Nothing is persisted and there is no
// "fall back to built-in after a delete": once `qs_set_translation('zh-CN', NULL)` has run, that language has
// no entries and using it in a report is an error rather than a silent return to the built-in table — "I
// deleted it" and "it still works" cannot both be true without confusion.
//
// Why a `LazyLock<RwLock<BTreeMap<..>>>`: the table belongs to no single query — `qs_set_translation` changes
// the table of **this DuckDB process** and every later `qs_html_reports(..., {'lang': 'zh-CN'})` reads the
// same one — so it is a process-level static behind a read/write lock (DuckDB runs aggregates on several
// worker threads, so reads have to be concurrent). A `BTreeMap` rather than a `HashMap` because
// `qs_list_translations()` has to be ordered by (language, key), which then needs no extra sort.
//
// Initialisation sits in a `#[duck_custom_register]` rather than in `LazyLock::new` so that "initialised when
// the extension loads, restored when it is reloaded" is literal behaviour rather than "initialised on first
// use".
//
// Lock poisoning is not an error: it means one call panicked while holding the lock, and the table itself is
// still consistent (every rewrite is a single insert or remove with no half-applied state), so carrying on is
// more useful to the caller than failing the query.
// ============================================================================

use std::collections::BTreeMap;
use std::sync::{LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use duckfn::{DuckResult, duck_error};

use crate::extension::types::translation::{TranslationEntry, TranslationRow};

use super::{builtin, keys};

/// 一个 key 在一个语言里的条目：显示文本 + 浮出说明。
///
/// One key's entry in one language: the display text and the tooltip note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// 写进报告里的显示文本（SQL 侧那一列叫 `label`）。
    ///
    /// The display text written into the report (that column is called `label` on the SQL side).
    pub(crate) label: String,

    /// 报告里那个元素的 `title`；空串表示不挂说明。
    ///
    /// That element's `title` in the report; an empty string means no note.
    pub(crate) description: String,
}

/// 一个语言的全部条目：key -> 条目。
///
/// Every entry of one language: key -> entry.
pub(super) type LanguageTable = BTreeMap<String, Entry>;

/// 整张翻译表：language -> 该语言的条目。
///
/// The whole translation table: language -> that language's entries.
pub(super) type Tables = BTreeMap<String, LanguageTable>;

// 进程级状态。`LazyLock` 让「静态 + 延迟初始化」变成一个原子，不必自己写 OnceLock + get_or_init。
//
// The process-level state. `LazyLock` makes "a static initialised lazily" atomic, so no hand-rolled
// OnceLock + get_or_init.
static TABLES: LazyLock<RwLock<Tables>> = LazyLock::new(|| RwLock::new(Tables::new()));

/// 读锁；中毒时直接用里面的值（见模块头）。
///
/// The read guard; poisoning is ignored (see the module header).
fn read() -> RwLockReadGuard<'static, Tables> {
    TABLES.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 写锁；中毒时直接用里面的值。
///
/// The write guard; poisoning is ignored.
fn write() -> RwLockWriteGuard<'static, Tables> {
    TABLES.write().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 把内置翻译数据装进进程级状态；扩展加载时调用一次。
///
/// 内置数据的一致性（key 必须存在、每个语言必须覆盖全部 key）在 [`builtin::table`] 里校验，所以这里
/// 拿到的要么是一张完整的表、要么是一个错误 —— 不会出现「半张表」。
///
/// Install the built-in data into the process-level state; called once when the extension loads. The built-in
/// data's consistency (every key must exist, every language must cover every key) is checked in
/// [`builtin::table`], so what arrives here is either a complete table or an error — never half a table.
pub(crate) fn install_builtin() -> DuckResult<()> {
    let built = builtin::table()?;
    *write() = built;
    Ok(())
}

/// 借用某个语言的整张表；这个语言一条条目都没有时报错。
///
/// 「查不到语言」按配置错误处理（用户选的：宁可报错，也不要静默出一份没翻译的报告），提示直接指向
/// `qs_list_translations()` —— 手滑写错语言标签是最可能的原因，而那张表恰好能证明写错了。
///
/// 空表按「没有这个语言」处理：把一个语言的条目删光等于把它删掉，留着空壳只会让下一次报错来得更晚。
///
/// Lend one language's whole table; a language with no entries at all is an error. "Unknown language" is a
/// configuration error (the user's choice: better to fail than to silently produce an untranslated report) and
/// the message points straight at `qs_list_translations()`, since a mistyped language tag is the likely cause
/// and that table is the proof. An empty table counts as "no such language": deleting every entry of a
/// language is deleting the language, and keeping an empty shell would only delay the next error.
pub(super) fn with_language<R>(
    language: &str,
    f: impl FnOnce(&LanguageTable) -> R,
) -> DuckResult<R> {
    let language = language.trim();
    let tables = read();
    match tables.get(language) {
        Some(entries) => Ok(f(entries)),
        None => unknown_language(language),
    }
}

/// 「语言不存在」的统一说法：两个调用点（出报告前的校验与渲染）读同一份文案。
///
/// 返回 `DuckResult<T>` 而不是错误值本身，是为了让两个调用点各自决定 `T`（校验处要 `()`，渲染处要
/// `String`），同时只留一份文案。
///
/// The single wording for "no such language", read by both call sites (the check before rendering and the
/// rendering itself). It returns a `DuckResult<T>` rather than the error value so each call site can pick its
/// own `T` — `()` for the check, `String` for the rendering — while the wording stays in one place.
fn unknown_language<T>(language: &str) -> DuckResult<T> {
    Err(duck_error(format!(
        "no translations for language '{language}' — run qs_list_translations() to see the languages that \
         have entries"
    )))
}

/// 出报告之前先确认这个语言有条目。
///
/// 单独一个函数（而不是让调用方自己 `with_language(lang, |_| ())`）：报告那一侧要的是「先校验、后渲染」，
/// 而校验的意图值得一个名字 —— 报告会白渲染几十份几百 KB 的 HTML，语言写错应该在那之前就挡住。
///
/// Confirm that a language has entries before any report is rendered. It is a function of its own rather than
/// a caller-side `with_language(lang, |_| ())` because the report side wants "validate first, then render", and
/// that intent deserves a name: rendering dozens of few-hundred-KB reports just to then discover a mistyped
/// language is exactly what this prevents.
pub(crate) fn require_language(language: &str) -> DuckResult<()> {
    with_language(language, |_| ())
}

/// `qs_set_translation` 的实现主体：改写或删除一个语言的条目，返回「表是否真的变了」。
///
/// `entries` 为 `None` 表示整个语言删掉（SQL 里的 `NULL`）；为 `Some(list)` 时逐项处理，规则见
/// [`TranslationEntry`]（`label` 为空 => 删这一项；`description` 为 NULL => 保留原有说明）。
///
/// 返回「是否真的变了」而不是恒真的 `true`：`qs_set_translation('zh-CN', [{'key': …}])` 里写一个已经删掉的
/// key、或者覆盖成完全相同的文本，都不算变化。这个布尔因此是唯一能区分「生效了」与「无事发生」的信号。
///
/// The body of `qs_set_translation`: rewrite or delete one language's entries and report whether the table
/// really changed. `entries` being `None` deletes the whole language (SQL `NULL`); `Some(list)` is processed
/// entry by entry under the rules of [`TranslationEntry`] (an empty `label` deletes that entry; a NULL
/// `description` keeps the existing one). The return value is "did it change" rather than a constant `true`:
/// asking to delete a key that is already gone, or overwriting with identical text, is not a change — and this
/// boolean is the only signal that tells "it took effect" apart from "nothing happened".
pub(super) fn apply(
    language: &str,
    entries: Option<&[Option<TranslationEntry>]>,
) -> DuckResult<bool> {
    let language = language.trim();
    if language.is_empty() {
        return Err(duck_error(
            "qs_set_translation: language must not be an empty string",
        ));
    }

    // NULL 列表 = 删掉整个语言。删一个本来就没有的语言不是错误，只是没有变化。
    //
    // A NULL list deletes the whole language. Deleting one that is not there is not an error, merely no change.
    let Some(entries) = entries else {
        return Ok(write().remove(language).is_some());
    };

    // 先校验、再改写：一项写坏就整批作废，不会出现「前两项生效、第三项报错」这种半成品状态。校验也要能看出
    // 同一次调用里重复的 key（谁覆盖谁取决于顺序，那不是调用方想表达的意思）。
    //
    // Validate first, then rewrite: one bad entry voids the whole batch, so there is no "first two applied, the
    // third errored" half-state. Validation also catches a key repeated inside one call (which of them wins
    // would depend on order, which is not what the caller meant).
    let mut seen: Vec<&str> = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(entry) = entry else {
            return Err(duck_error(
                "qs_set_translation: the list must not contain a NULL element — every element has to name a \
                 key",
            ));
        };

        let key = entry.key.as_deref().map(str::trim).unwrap_or_default();
        if key.is_empty() {
            return Err(duck_error(
                "qs_set_translation: every entry needs a 'key' — run qs_list_translations() to see the keys",
            ));
        }
        if !keys::is_known(key) {
            return Err(duck_error(format!(
                "qs_set_translation: unknown key '{key}' — run qs_list_translations() to see the keys that \
                 exist"
            )));
        }
        if seen.contains(&key) {
            return Err(duck_error(format!(
                "qs_set_translation: key '{key}' appears twice in one call"
            )));
        }
        seen.push(key);
    }

    let mut tables = write();
    let mut changed = false;

    // 语言表按需建：第一次写入这个语言时才插入一个空表（删除不建表，否则删一个不存在的语言会留下空壳）。
    //
    // The language table is created on demand: an empty one is inserted the first time something is written
    // to that language. Deletion never creates one, which would otherwise leave an empty shell behind.
    if !entries.is_empty() && !tables.contains_key(language) {
        tables.insert(language.to_string(), LanguageTable::new());
    }

    if let Some(language_table) = tables.get_mut(language) {
        for entry in entries {
            // 上面已经校验过，这里的 `expect` 不可能触发；用 `expect` 而不是 `unwrap_or_default` 是为了
            // 万一将来校验放松了，问题会在原地炸出来而不是静默写个空 key。
            //
            // Validation above makes this `expect` unreachable; it is an `expect` rather than an
            // `unwrap_or_default` so that if the check is ever relaxed, the problem explodes right here instead
            // of silently writing an empty key.
            let entry = entry.as_ref().expect("validated above");
            let key = entry.key.as_deref().expect("validated above").trim();

            match entry.label.as_deref() {
                // 空译文 = 删这一项（连同说明）。不做任何回退。
                //
                // An empty translation deletes the entry, note included, with no fallback.
                None | Some("") => changed |= language_table.remove(key).is_some(),
                Some(label) => {
                    // `description` 为 NULL 表示「保留原有说明」：只改译文时不必把说明重抄一遍。语言表里
                    // 本来没有这一项时，原说明视为空串。
                    //
                    // A NULL `description` means "keep the existing note", so rewriting just the translation
                    // needs no retyping. When the language table has no such entry yet, the old note is empty.
                    let description = match &entry.description {
                        Some(description) => description.clone(),
                        None => language_table
                            .get(key)
                            .map(|old| old.description.clone())
                            .unwrap_or_default(),
                    };
                    let new = Entry {
                        label: label.to_string(),
                        description,
                    };
                    if language_table.get(key) != Some(&new) {
                        changed = true;
                    }
                    language_table.insert(key.to_string(), new);
                }
            }
        }

        // 条目被删光就等于这个语言不存在（见 `with_language` 的说明）。注意这一步发生在所有条目处理完之后，
        // 所以一次调用里「全删掉」与「删一部分」语义清楚。
        //
        // Deleting every entry is the same as the language not existing (see `with_language`). This happens
        // after the whole batch has been applied, so "delete everything" and "delete some" stay unambiguous.
        if language_table.is_empty() {
            tables.remove(language);
        }
    }

    Ok(changed)
}

/// 当前翻译表的一份快照，按 (lang, key) 升序 —— `qs_list_translations()` 的每一行。
///
/// The current table as a snapshot ordered by (lang, key) — one row per entry for
/// `qs_list_translations()`.
pub(super) fn snapshot() -> Vec<TranslationRow> {
    read()
        .iter()
        .flat_map(|(language, entries)| {
            entries.iter().map(move |(key, entry)| TranslationRow {
                lang: language.clone(),
                key: key.clone(),
                label: entry.label.clone(),
                description: entry.description.clone(),
            })
        })
        .collect()
}
