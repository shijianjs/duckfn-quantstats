// ============================================================================
// 内置翻译数据：装配成一张完整的表，并在装配时自检
//
// # 六种语言，两种写法
//
// `en` 只写**说明**：它的 `label` 就是 `keys.rs` 里那个位置的英文原文。这样做有两个理由 ——
// 抄一份英文原文进来等于多一份会过期的真相，而且抄错了会让**英文报告的文字变样**（`en` 的语义是「文字
// 不变，只加说明」，那是最不该出错的一档）。
//
// 另外五种语言写 `(key, 译文, 说明)` 三元组，说明会成为报告里那个元素的 `title`。
//
// # 加载期自检
//
// [`table`] 在装配时做强校验，任何一条不成立都让**扩展加载失败**：
//
//   1. 每个 key 都必须在 `keys.rs` 的目录里 —— 抓的是内置数据里拼错的 key；写错的 key 永远不会生效
//      （报告里没有对应 DOM 位置），如果放过去，用户只会看到「某个指标没被翻译」，无从排查；
//   2. 每个语言都必须覆盖目录里的**全部** key —— 少一条就是「这个报告里有一处英文没被翻译」；
//   3. 同一个语言里同一个 key 不能出现两次 —— 后者覆盖前者，多出来那条是没写清楚的意图。
//
// 这些是**编译期抓不到、只有跑起来才看得见**的那类问题（字符串是数据），所以放在加载期而不是测试里：
// 一旦出现，`LOAD` 会直接把原因说出来，而不是让用户去比对两份报告。
//
// Built-in translation data: assembled into one complete table, with self-checks during assembly.
//
// Six languages, two shapes. `en` only carries **notes**: its `label` values are the English sources from
// `keys.rs`. Two reasons — copying the English text in would be a second source of truth that goes stale, and
// a typo in it would visibly **change the English report** (the `en` contract is "same text, notes added",
// which is the one tier that must not drift). The other five carry `(key, translation, note)` triples, and the
// note becomes that element's `title`.
//
// Load-time self-checks: `table()` validates strictly and any failure makes the **extension fail to load**:
// (1) every key must exist in the `keys.rs` catalog — this catches a mistyped key in the built-in data, and a
// key that does not exist never has any effect (there is no DOM position for it), so letting it through would
// only show up as "some metric is not translated", with nothing to go on; (2) every language must cover
// **all** keys in the catalog — a missing one means one visible English string in every report; (3) a key may
// not appear twice inside one language — the later would overwrite the earlier and the extra entry would be an
// unclear intent. These are the problems the compiler cannot see (the strings are data), hence load time rather
// than a test: if one ever appears, `LOAD` names the cause instead of leaving the user to diff two reports.
// ============================================================================

mod de;
mod en;
mod es;
mod fr;
mod ja;
mod zh_cn;

use std::collections::{BTreeMap, HashSet};

use duckfn::{DuckResult, duck_error};

use super::keys;
use super::table::{Entry, LanguageTable, Tables};

/// 一个语言的整张表：`(key, 译文, 说明)` 三元组的数组。
///
/// 写成一个别名而不是把类型摊在 [`LANGUAGES`] 上：那是个三层嵌套的元组类型，摊开会盖住「这里装的是一张
/// 表」这个重点。
///
/// One language's whole table: an array of `(key, translation, note)` triples. Spelling it as an alias rather
/// than inlining it on [`LANGUAGES`] keeps a three-level nested tuple type from burying the point that this is
/// a table.
type Entries = &'static [(&'static str, &'static str, &'static str)];

/// `en` 之外的五种内置语言：语言标签 + 它的整张表。
///
/// 标签就是 SQL 里要写的那个字符串（`{'language': 'zh-CN'}`），也是 `qs_list_translations()` 的第一列。
///
/// The five built-in languages besides `en`: the tag plus that language's whole table. The tag is exactly the
/// string SQL writes (`{'language': 'zh-CN'}`) and the first column of `qs_list_translations()`.
const LANGUAGES: &[(&str, Entries)] = &[
    ("zh-CN", zh_cn::ENTRIES),
    ("ja", ja::ENTRIES),
    ("de", de::ENTRIES),
    ("fr", fr::ENTRIES),
    ("es", es::ENTRIES),
];

/// 装配整张内置表；任何不一致都返回错误（见模块头）。
///
/// Assemble the whole built-in table; any inconsistency is an error (see the module header).
pub(super) fn table() -> DuckResult<Tables> {
    // key -> 英文原文。方向要写清楚：`keys::all()` 给的是 `(原文, key)`，而后面两处校验都是「这个 key 存在
    // 吗」，所以这里翻成 key 在前 —— 反过来会让 `contains_key` 永远为假（拿 key 去查原文），而那正是这条
    // 自检要抓的错误。
    //
    // key -> English source. The direction matters: `keys::all()` yields `(source, key)` while both checks below
    // ask "does this key exist", so it is flipped to put the key first — the other way round would make
    // `contains_key` permanently false (looking a key up among the sources), which is precisely the mistake this
    // self-check exists to catch.
    let catalog: BTreeMap<&str, &str> = keys::all().map(|(source, key)| (key, source)).collect();

    let mut tables = Tables::new();
    tables.insert("en".to_string(), english(&catalog)?);
    for (language, entries) in LANGUAGES {
        tables.insert((*language).to_string(), check(language, entries, &catalog)?);
    }

    Ok(tables)
}

/// `en` 的那张表：`label` 取目录里的英文原文，说明取 `en::DESCRIPTIONS`。
///
/// The `en` table: `label` comes from the catalog's English source and the note from `en::DESCRIPTIONS`.
fn english(catalog: &BTreeMap<&str, &str>) -> DuckResult<LanguageTable> {
    // 说明先收成表，再逐 key 取 —— 反过来的话每个 key 都要扫一遍说明数组。
    //
    // The notes are collected into a map first and then looked up per key; the other way round would scan the
    // note array once per key.
    let notes: BTreeMap<&str, &str> = en::DESCRIPTIONS.iter().copied().collect();
    if notes.len() != en::DESCRIPTIONS.len() {
        return Err(duck_error(
            "built-in en: DESCRIPTIONS lists the same key twice",
        ));
    }

    let mut table = LanguageTable::new();
    for (key, source) in catalog {
        let Some(note) = notes.get(key) else {
            return Err(duck_error(format!(
                "built-in en: no description for key '{key}'"
            )));
        };
        table.insert(
            (*key).to_string(),
            Entry {
                label: (*source).to_string(),
                description: (*note).to_string(),
            },
        );
    }

    Ok(table)
}

/// 把某个语言的 `(key, 译文, 说明)` 三元组装成一张表，顺带做模块头里那三条校验。
///
/// A language's `(key, translation, note)` triples turned into a table, along with the three checks from the
/// module header.
fn check(
    language: &str,
    entries: &[(&str, &str, &str)],
    catalog: &BTreeMap<&str, &str>,
) -> DuckResult<LanguageTable> {
    let mut table = LanguageTable::new();
    for (key, label, description) in entries {
        if !catalog.contains_key(key) {
            return Err(duck_error(format!(
                "built-in {language}: unknown key '{key}' — keys.rs does not know it"
            )));
        }
        if table.contains_key(*key) {
            return Err(duck_error(format!(
                "built-in {language}: key '{key}' appears twice"
            )));
        }
        table.insert(
            (*key).to_string(),
            Entry {
                label: (*label).to_string(),
                description: (*description).to_string(),
            },
        );
    }

    // 覆盖性：缺的 key 一次全报出来，省得一遍遍补、一遍遍重新加载。
    //
    // Coverage: every missing key is reported at once, instead of one fix-and-reload cycle per key.
    let missing: HashSet<&str> = catalog
        .keys()
        .filter(|key| !table.contains_key(**key))
        .copied()
        .collect();
    if !missing.is_empty() {
        let mut missing: Vec<&str> = missing.into_iter().collect();
        missing.sort_unstable();
        return Err(duck_error(format!(
            "built-in {language}: {} key(s) have no translation: {}",
            missing.len(),
            missing.join(", ")
        )));
    }

    Ok(table)
}
