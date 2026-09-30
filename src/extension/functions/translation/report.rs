// ============================================================================
// 把译法贴回报告：一次 lol_html 的静态改写
//
// 输入是 `quantstats_rs::html()` 渲染出来的完整 HTML，输出还是完整 HTML —— 没有运行时 i18n、没有注入任何
// 脚本，报告依然是自包含的静态文件。翻译只发生在**渲染之后、落盘/开浏览器之前**（report.rs 里的调用点），
// 所以磁盘上的文件、`html` 列里的字符串、浏览器里打开的页面三者永远一致。
//
// # 为什么按「选择器 + 原文」定位，而不是全文替换
//
// `Strategy` 既是 EOY 表头又是 SVG 图例，`Benchmark` 出现在更多地方；全文替换会让它们互相串味，而且会把
// 用户自己的数据（策略名、基准名、symbol）一起改掉。目录（`keys.rs`）给每个位置配一条选择器与一张原文表，
// 这里只做「这段文本是不是这张表里的某一项」，命中才改。
//
// # 为什么改写的是**文本节点**而不是元素
//
// lol_html 的流式约束：元素处理器在起始标签处就被调用，那时元素里的文本还没解析出来（读不到，就没法查表）；
// 而元素属性只能在起始标签处设置。两者一叠加，结论是「先读文本、再改元素」这条路走不通。所以：
//
//   - 读与改都在文本处理器里完成；
//   - 浮出说明跟着**文本**走：HTML 用 `<span title="…">`，SVG 用标准的 `<title>` 子元素。
//
// 代价是报告里多一层行内 `<span>`（对布局无影响），换来的是「只改我们认得的那些文本」这条保证。
//
// # 文本节点会被切成好几片
//
// lol_html 给的是**分片**，不是完整文本节点：`<h3>Key Performance Metrics</h3>` 会来两片（实测 `Key
// Performance` 与 ` Metrics`）。所以处理器按「攒到本节点的最后一片再决定」来做：前面的片先撤出输出，最后
// 一片把整段拼出来 —— 命中就换成译文，没命中就把原字节写回，输出与没动过完全一致（见 `text_handler`）。
//
// 这也要求「不命中就一个字节都不动」：同一个文本可能被两条选择器同时命中（热量图的月份就同时落在
// `.qs-plot svg text` 与 `#monthly_heatmap text` 里），不命中的那条要是写回点什么，就会把另一条刚写下的
// 译文盖掉。所以目录里那两条 SVG 选择器已经用 `:not()` 切开（见 keys.rs），而单片段命中失败时不产生任何
// 改写动作。
//
// # 月份为什么要分「槽位大小写」
//
// 热量图表头原文是全大写的 `JAN`，日期区间原文是 `Jan`。两处共用 `month.*` 这批 key（见 keys.rs），热量图
// 那一处按槽位风格把译文转成大写：`en` 的 `show` 是 `Jan`，渲染回热量图仍是 `JAN` —— 英文那一档一个字符都
// 没变，日期区间拿到的也还是 `Jan`。
//
// Sticking the translations back onto the report: one static lol_html rewrite.
//
// What goes in is the complete HTML `quantstats_rs::html()` rendered and what comes out is still complete HTML —
// no runtime i18n, no injected script, and the report stays a self-contained static file. The translation
// happens **after rendering and before persisting/opening** (the call site in report.rs), so the file on disk,
// the string in the `html` column and the page in the browser are always the same thing.
//
// Why "selector + source text" rather than a global replacement: `Strategy` is both an EOY table header and an
// SVG legend label, `Benchmark` shows up in even more places, a global replacement would let them bleed into
// each other, and it would rewrite the caller's own data (strategy name, benchmark name, symbols) as well. The
// catalog (keys.rs) gives every position its own selector and source table, and all this file does is ask "is
// this text one of the entries in that table" — only a hit changes anything.
//
// Why the **text nodes** are what gets rewritten rather than the elements: lol_html's streaming constraint. An
// element handler runs at the start tag, before the element's text has been parsed (nothing to look up), while
// element attributes can only be set at that same start tag. Together they rule out "read the text, then change
// the element". So both reading and writing happen in the text handler, and the note follows the **text**:
// `<span title="…">` for HTML, the standard `<title>` child for SVG. The price is one extra inline `<span>` in
// the report (no effect on layout); what it buys is the guarantee that only texts we recognise are touched.
//
// Only "a text node with a single fragment" is processed: lol_html splits one text node into several fragments
// (at buffer boundaries, entities, comments) and a fragment is not the whole node, so looking it up would miss
// (or, worse, match the wrong thing). Each handler keeps a tiny state: as soon as the current fragment
// continues the previous one, or is not the last of its text node, nothing at all is touched. The texts this
// table matches are short plain ASCII (the longest metric name is 22 characters) and are not split; treating a
// split as "leave it alone" is the safe answer — better one translation missed than half a text replaced by
// something else.
//
// Why the months need a "slot casing": the heatmap header is uppercase `JAN` in the source while the date range
// is `Jan`. Both use the same `month.*` keys (see keys.rs) and the heatmap slot uppercases the translation:
// `en`'s `show` is `Jan` and the heatmap still renders `JAN` — English is unchanged down to the character,
// while the date range still gets `Jan`.
// ============================================================================

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use duckfn::{DuckResult, duck_error};
use lol_html::html_content::{ContentType, TextChunk};
use lol_html::{
    ElementContentHandlers, RewriteStrSettings, Selector, element, rewrite_str, text,
};

use super::keys;
use super::table::{self, LanguageTable};

// ---------------------------------------------------------------------------
// 目录的运行期形态
// ---------------------------------------------------------------------------

/// 一条处理器在运行期需要的东西：选择器、渲染方式，以及**已编译**的「原文 → key」表。
///
/// 查表用 `HashMap` 而不是线性扫 `pairs`：一份报告有上百个可翻译文本、每条选择器又会被成百上千个元素命中
/// （SVG 的 `<text>` 尤其多），线性扫会让每次命中都过一遍整张表。表是静态的，所以只建一次。
///
/// What one handler needs at run time: the selector, how to render, and the **compiled** "source text → key"
/// table. A `HashMap` rather than a linear scan over `pairs` because one report holds a hundred-odd
/// translatable texts and every selector fires for hundreds or thousands of elements (SVG `<text>` especially),
/// which would re-walk the whole table on every hit. The table is static, so it is built once.
struct RuntimeScope {
    /// CSS 选择器。
    ///
    /// The CSS selector.
    selector: &'static str,

    /// 命中后怎么写回 DOM。
    ///
    /// How a hit is written back into the DOM.
    style: keys::Style,

    /// 是否按槽位风格转大写（热量图月份表头）。
    ///
    /// Whether to uppercase for the slot (the heatmap month header).
    uppercase: bool,

    /// 原文 -> key。
    ///
    /// 键是 `String` 而不是 `&'static str`：槽位大写（见 [`RuntimeScope::uppercase`]）时键得是锚点的大写形态，
    /// 那是**算出来的**，借用不了。表只建一次，这点分配不算什么。
    ///
    /// Source text -> key. The key is a `String` rather than a `&'static str` because a slot that is uppercase
    /// (see [`RuntimeScope::uppercase`]) needs the uppercased anchor, which is computed and cannot be borrowed.
    /// The table is built once, so the allocation is immaterial.
    lookup: HashMap<String, &'static str>,
}

/// `keys::SCOPES` 的运行期形态，进程内只建一次。
///
/// `keys::SCOPES` in its run-time shape, built once per process.
static SCOPES: LazyLock<Vec<RuntimeScope>> = LazyLock::new(|| {
    keys::SCOPES
        .iter()
        .map(|scope| RuntimeScope {
            selector: scope.selector,
            style: scope.style,
            uppercase: scope.uppercase,
            lookup: scope
                .pairs
                .iter()
                .map(|(source, key)| {
                    let source = if scope.uppercase {
                        source.to_uppercase()
                    } else {
                        (*source).to_string()
                    };

                    (source, *key)
                })
                .collect(),
        })
        .collect()
});

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 按某个语言改写一份渲染好的报告；语言没有条目时报错（不静默返回原文）。
///
/// 调用方负责**先**校验语言存在（`table::require_language`，在渲染任何报告之前），这里再查一次是必要的
/// 冗余：改写发生在渲染之后，两者之间没有锁，语言理论上可能刚被 `qs_set_translation(…, NULL)` 删掉。
///
/// Rewrite one rendered report into one language; a language with no entries is an error rather than a silent
/// return of the original. The caller validates the language **first** (`table::require_language`, before any
/// report is rendered); looking it up again here is deliberate redundancy: the rewrite happens after rendering
/// with no lock held in between, so the language could in principle just have been deleted by
/// `qs_set_translation(…, NULL)`.
pub(crate) fn translate_report(report: &str, language: &str) -> DuckResult<String> {
    let rewritten = table::with_language(language, |entries| rewrite(report, entries, language))?;

    rewritten.map_err(|err| {
        duck_error(format!(
            "language '{language}': cannot rewrite the rendered report: {err}"
        ))
    })
}

/// 装好所有处理器，跑一次改写。
///
/// Assemble every handler and run one rewrite.
fn rewrite(
    report: &str,
    entries: &LanguageTable,
    language: &str,
) -> Result<String, lol_html::errors::RewritingError> {
    // `<html lang="…">` 跟着语言走：屏幕阅读器与浏览器挑字体时看的就是它。语言标签本来就是合法的 `lang`
    // 取值，所以不需要校验。
    //
    // `<html lang="…">` follows the language: that is what screen readers and browsers' font selection look at.
    // A language tag is a legal `lang` value by construction, so nothing needs checking.
    let mut handlers = vec![element!("html", move |el| {
        el.set_attribute("lang", language)?;
        Ok(())
    })];

    // 一张表一条处理器。
    //
    // One handler per table entry.
    for scope in SCOPES.iter() {
        handlers.push(scope_handler(scope, entries));
    }

    // 日期区间与 `<h4>` 不是「整段文本等于某一项」，各有自己的处理器。
    //
    // The date range and the `<h4>` are not "this whole node equals one entry" and get handlers of their own.
    handlers.push(text_handler(
        keys::DATE_RANGE_SELECTOR,
        entries,
        apply_date_range,
    ));
    handlers.push(text_handler(
        keys::HEADER_SELECTOR,
        entries,
        apply_header_fragments,
    ));

    // `RewriteStrSettings` 的字段是公开的，所以直接填那个 vec（2.x 上没有 `append_*` 那套建造者）。
    //
    // `RewriteStrSettings`' fields are public, so the vector goes straight in (2.x has no `append_*` builders).
    let settings = RewriteStrSettings {
        element_content_handlers: handlers,
        ..RewriteStrSettings::new()
    };

    rewrite_str(report, settings)
}

// ---------------------------------------------------------------------------
// 处理器
// ---------------------------------------------------------------------------

/// 一条「整段文本命中表里某一项」的处理器。
///
/// A handler for "this whole text node is one entry of the table".
fn scope_handler<'h>(
    scope: &'static RuntimeScope,
    entries: &'h LanguageTable,
) -> (Cow<'static, Selector>, ElementContentHandlers<'h>) {
    text_handler(scope.selector, entries, move |entries, chunk, text| {
        apply_whole_text(scope, entries, chunk, text)
    })
}

/// 把「一个文本节点可能被切成好几片」这件事包起来，见模块头。
///
/// lol_html 给的是**分片**，不是完整文本节点：`<h3>Key Performance Metrics</h3>` 会来两片（实测
/// `Key Performance` 与 ` Metrics`）。所以这里的做法是：
///
///   - 单片就是整节点（最常见）：直接拿它去查表；
///   - 被切开时：前面的片先攒起来、并从输出里撤掉（`remove`），到最后一片再把整段拼出来一次性决定 ——
///     命中就换成译文，没命中就**原样写回**（`ContentType::Html` 写回原字节，因此与不撤掉时完全一致）。
///
/// 状态直接放进闭包捕获的局部变量：`FnMut` 允许改捕获的值，不需要 `Cell`/`RefCell`。
///
/// Wraps "one text node may arrive as several fragments" (see the module header). What lol_html hands over is
/// **fragments**, not whole text nodes — `<h3>Key Performance Metrics</h3>` arrives as two (measured:
/// `Key Performance` and ` Metrics`). So: a single fragment (the common case) is looked up directly, and when the
/// node was split the earlier fragments are buffered and withdrawn from the output (`remove`) until the last one,
/// which decides the whole node at once — a hit becomes the translation and a miss is **written back as is**
/// (raw bytes through `ContentType::Html`, so the output is identical to never having withdrawn anything). The
/// state lives in locals captured by the closure: `FnMut` may mutate what it captures, so no `Cell`/`RefCell` is
/// needed.
fn text_handler<'h, F>(
    selector: &'static str,
    entries: &'h LanguageTable,
    mut apply: F,
) -> (Cow<'static, Selector>, ElementContentHandlers<'h>)
where
    F: FnMut(&LanguageTable, &mut TextChunk, &str) -> bool + 'h,
{
    let mut buffered = String::new();
    let mut continues = false;

    text!(selector, move |chunk: &mut TextChunk| {
        // `continues` 在读之前的值 = 「这一片是不是接着上一片」，写回的值 = 「下一片要不要接着这一片」。
        //
        // The value of `continues` before the read is "does this fragment continue the previous one"; the value
        // written back is "does the next fragment continue this one".
        let was_continuation = continues;
        continues = !chunk.last_in_text_node();

        // 单片就是整节点：不命中时一个字节都不动（这一点很重要 —— 同一个文本可能同时被两条选择器命中，
        // 不命中的那条必须留手，否则会把另一条刚写下的译文覆盖回去）。
        //
        // A single fragment is the whole node; a miss touches nothing (this matters: one text can be matched by
        // two selectors, and the one that finds no entry has to keep its hands off, or it would overwrite the
        // translation the other one just wrote).
        // 先拷一份再传：`apply` 同时要 `&mut chunk`（写回去），而 `chunk.as_str()` 借的是 `&chunk`，
        // 两个参数不能在同一次调用里同时并存。
        //
        // The text is copied out first because `apply` also needs `&mut chunk` (to write back) while
        // `chunk.as_str()` borrows `&chunk`; the two cannot coexist in one call.
        if !was_continuation && !continues {
            let text = chunk.as_str().to_owned();
            apply(entries, chunk, &text);
            return Ok(());
        }

        // 被切开了：本片先攒起来并撤出输出，等最后一片。
        //
        // Split: buffer this fragment and withdraw it from the output; the last one decides.
        buffered.push_str(chunk.as_str());
        if continues {
            chunk.remove();
            return Ok(());
        }

        let full = format!("{}{}", std::mem::take(&mut buffered), chunk.as_str());
        if !apply(entries, chunk, &full) {
            // 撤掉过前面的片，所以无论如何都得写回去；写的是原字节，输出与没撤过完全一样。
            //
            // Earlier fragments were withdrawn, so this has to be written back no matter what; it is written back
            // as the original bytes, leaving the output exactly as if nothing had been withdrawn.
            chunk.replace(&full, ContentType::Html);
        }

        Ok(())
    })
}

/// 「整段文本 = 表里某一项」的落地：命中就换成译文（并按需挂上说明），否则一个字都不动。
///
/// Landing "this whole text node is one table entry": a hit becomes the translation (with the note attached
/// where asked), a miss changes nothing at all.
fn apply_whole_text(
    scope: &RuntimeScope,
    entries: &LanguageTable,
    chunk: &mut TextChunk,
    text: &str,
) -> bool {
    let Some(entry) = scope.lookup.get(text.trim()).and_then(|key| entries.get(*key)) else {
        return false;
    };

    // 热量图表头按槽位风格大写（见模块头）；其余语言/位置原样。
    //
    // The heatmap header uppercases for the slot (see the module header); everything else is left as is.
    let show = if scope.uppercase {
        Cow::Owned(entry.show.to_uppercase())
    } else {
        Cow::Borrowed(entry.show.as_str())
    };

    chunk.replace(
        &render(scope.style, &show, &entry.description),
        ContentType::Html,
    );

    true
}

/// `<h4>` 里两个固定片段的替换（`Benchmark is` / `Generated by`）。
///
/// 整段文本是「固定片段 + 用户数据 + 链接 + 版本号」的拼接（`Benchmark is S&P 500 | Generated by
/// QuantStats-RS (v. 0.1.0)`），所以按片段改；` (v. 0.1.0)` 是数据，不动。
///
/// 做法是**先整体转义、再替换片段**：两个片段都是纯 ASCII，转义不会动它们（`&`、`<`、`>` 都不出现），而
/// 用户数据里可能出现的 `&` 之类会被正确转义 —— 报告在这里把显示名原样拼进了 HTML，我们重写这段文本时得
/// 替它补上这一步。
///
/// Fragments inside `<h4>`. The whole text node is "fixed fragment + user data + link + version" pasted
/// together (`Benchmark is S&P 500 | Generated by QuantStats-RS (v. 0.1.0)`), hence fragment replacement; the
/// trailing ` (v. 0.1.0)` is data and stays. The order is **escape everything first, then replace the
/// fragments**: both fragments are plain ASCII (no `&`, `<`, `>`), so escaping leaves them untouched, while an
/// `&` inside the user's data is escaped properly — the report pasted the display name into HTML as is, and
/// rewriting this text is where we have to do that step for it.
fn apply_header_fragments(entries: &LanguageTable, chunk: &mut TextChunk, text: &str) -> bool {
    let escaped = escape(text, false);
    let mut out = escaped.clone();

    for (fragment, key) in keys::H4_FRAGMENTS {
        let Some(entry) = entries.get(*key) else {
            continue;
        };
        if !out.contains(fragment) {
            continue;
        }
        // 片段本身可能就是原文（`en` 的 `show` 等于原文）：那时替换只是给它套上一层带说明的 `<span>`，
        // 文字没变，但 `out` 与 `escaped` 已经不同了 —— 这正是「要写回去」的信号。
        //
        // The fragment may itself stay as it was (`en`'s `show` equals the source): the replacement then merely
        // wraps it in a `<span>` carrying the note. The text is unchanged, but `out` differs from `escaped` —
        // which is exactly the signal that something has to be written back.
        out = out.replace(
            fragment,
            &render(keys::Style::Html, &entry.show, &entry.description),
        );
    }

    if out == escaped {
        return false;
    }

    chunk.replace(&out, ContentType::Html);

    true
}

/// `<h1><dt>` 的日期区间：`5 Jan, 2021 - 21 Sep, 2026` → 按语言的模板重排。
///
/// 月份名字换成译文后，英文语序（`{日} {月}, {年}`）在中文/日文里读不通，所以整段按模板渲染；模板来自
/// `date.range` 这一项的 `show`（见 keys.rs）。模板被删掉时退回「保留原语序、只换月份名」—— 少翻译一点
/// 好过把日期写成半截。
///
/// The `<h1><dt>` date range: `5 Jan, 2021 - 21 Sep, 2026` reordered through the language's template. Once the
/// month names are translated the English word order (`{day} {month}, {year}`) reads wrong in Chinese or
/// Japanese, so the whole range renders from a template — the `show` of the `date.range` entry (see keys.rs).
/// When that entry has been deleted it falls back to "keep the word order, swap the month names only": one
/// translation fewer beats a half-written date.
fn apply_date_range(entries: &LanguageTable, chunk: &mut TextChunk, text: &str) -> bool {
    let Some(range) = parse_date_range(text) else {
        return false;
    };

    let month = |part: &DatePart<'_>| month_markup(part, entries);

    let rendered = match entries.get(keys::DATE_RANGE_KEY) {
        Some(template) => {
            let values = [
                ("{d1}", escape(range.start.day, false)),
                ("{y1}", escape(range.start.year, false)),
                ("{d2}", escape(range.end.day, false)),
                ("{y2}", escape(range.end.year, false)),
                // 月份放最后替换：译名里万一出现 `{d1}` 这种字面量也不会被再替换一次。
                //
                // The months are substituted last, so a translation that happens to contain something like
                // `{d1}` is not substituted a second time.
                ("{m1}", month(&range.start)),
                ("{m2}", month(&range.end)),
            ];
            let filled = fill_template(&template.show, &values);

            // 外层再套一层说明：整个区间是一个整体，而月份各自还有自己的说明（内层 `<span>` 命中时优先）。
            //
            // One more note around the whole thing: the range as a whole has one, and the months keep theirs
            // (the inner `<span>` wins when it is the one hovered).
            wrap_note(&template.description, &filled)
        }
        None => {
            let start = format!(
                "{} {}, {}",
                escape(range.start.day, false),
                month(&range.start),
                escape(range.start.year, false)
            );
            let end = format!(
                "{} {}, {}",
                escape(range.end.day, false),
                month(&range.end),
                escape(range.end.year, false)
            );
            format!("{start} - {end}")
        }
    };

    chunk.replace(&rendered, ContentType::Html);

    true
}

// ---------------------------------------------------------------------------
// 日期区间的解析与模板
// ---------------------------------------------------------------------------

/// 一个日期区间的一端。
///
/// One end of a date range.
struct DatePart<'a> {
    /// 日，原文那种写法（`5`、`21`）。
    ///
    /// The day as written in the source (`5`, `21`).
    day: &'a str,

    /// 月份缩写原文（`Jan`），月份没有译文时原样用回去。
    ///
    /// The month abbreviation as written (`Jan`), used as is when that month has no translation.
    month: &'a str,

    /// 月份序号，`1`..`12`。
    ///
    /// The month number, `1`..`12`.
    month_number: u32,

    /// 四位年份。
    ///
    /// The four-digit year.
    year: &'a str,
}

/// 日期区间（起点、终点）。
///
/// A date range (start, end).
struct DateRange<'a> {
    start: DatePart<'a>,
    end: DatePart<'a>,
}

/// `quantstats-rs` 写日期区间用的是 `%e %b, %Y` 两端拼 `" - "`，例如 `5 Jan, 2021 - 21 Sep, 2026`。
///
/// 解析失败就返回 `None`（上游换了格式、或者这段文本不是日期），调用方原样放行 —— 这里不做「尽力猜」，
/// 猜错会把日期写坏，比不翻译糟得多。
///
/// quantstats-rs writes the range as two `%e %b, %Y` parts joined by `" - "`, e.g. `5 Jan, 2021 - 21 Sep,
/// 2026`. A parse failure yields `None` (upstream changed the format, or this text is not a date) and the caller
/// passes it through: no best-effort guessing here, since guessing wrong would corrupt the date, which is far
/// worse than not translating it.
fn parse_date_range(text: &str) -> Option<DateRange<'_>> {
    let (start, end) = text.split_once(" - ")?;

    Some(DateRange {
        start: parse_date_part(start)?,
        end: parse_date_part(end)?,
    })
}

/// `5 Jan, 2021` 这一段。
///
/// One `5 Jan, 2021` part.
fn parse_date_part(part: &str) -> Option<DatePart<'_>> {
    let (date, year) = part.split_once(", ")?;
    let (day, month) = date.split_once(' ')?;

    if day.is_empty() || day.len() > 2 || !day.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    Some(DatePart {
        day,
        month,
        month_number: month_number(month)?,
        year,
    })
}

/// 三位月份缩写（chrono 的 `%b`，英文、首字母大写）→ 月份序号。
///
/// The three-letter month abbreviation (chrono's `%b`, English, capitalised) -> month number.
fn month_number(abbreviation: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    MONTHS
        .iter()
        .position(|month| *month == abbreviation)
        .map(|index| index as u32 + 1)
}

/// 月份那一格的 HTML：有译文就带上说明，没有就用原文。
///
/// The month's HTML: the translation with its note when there is one, the source text otherwise.
fn month_markup(part: &DatePart<'_>, entries: &LanguageTable) -> String {
    match entries.get(keys::MONTH_KEYS[(part.month_number - 1) as usize]) {
        Some(entry) => render(keys::Style::Html, &entry.show, &entry.description),
        None => escape(part.month, false),
    }
}

/// 把模板里的 `{…}` 占位符换掉；**字面部分按文本转义**，占位符的值本身已经是标记。
///
/// 认不出来的占位符原样保留（连花括号一起），不认识的 `{` 之后没有 `}` 时剩下的全文当字面量 —— 这两种情况
/// 都只是「这段模板写得怪」，不该让整条查询失败。
///
/// Replaces the `{…}` placeholders of a template; the **literal parts** are escaped as text, while a
/// placeholder's value is already markup. An unrecognised placeholder is kept as is (braces included), and a `{`
/// with no `}` after it makes the rest literal — both are merely "this template is oddly written" and should not
/// fail the whole query.
fn fill_template(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&escape(&rest[..open], false));

        let after = &rest[open..];
        let Some(close) = after.find('}') else {
            out.push_str(&escape(after, false));
            return out;
        };

        let name = &after[..=close];
        match values.iter().find(|(placeholder, _)| *placeholder == name) {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&escape(name, false)),
        }

        rest = &after[close + 1..];
    }
    out.push_str(&escape(rest, false));

    out
}

// ---------------------------------------------------------------------------
// 渲染：译文怎么写回 DOM
// ---------------------------------------------------------------------------

/// 按槽位的方式把一段译法与它的说明渲染成标记。
///
/// 说明为空时不加任何包装 —— 一个空的 `title` 只会在浮出时显示一个空框，不如没有。
///
/// Render one translation and its note as markup in the slot's own way. An empty note adds no wrapper at all: an
/// empty `title` only pops up an empty box, which is worse than none.
fn render(style: keys::Style, show: &str, description: &str) -> String {
    let text = escape(show, false);

    match style {
        keys::Style::Html => wrap_note(description, &text),
        // SVG 的浮出提示按规范是 `<title>` **子元素**，而且它不参与渲染，所以不影响图形。
        //
        // The SVG tooltip is standardised as a `<title>` **child**, and it is not rendered, so the chart is
        // unaffected.
        keys::Style::Svg => {
            if description.is_empty() {
                text
            } else {
                format!("<title>{}</title>{text}", escape(description, false))
            }
        }
        // 文档 `<title>`：标签页没有可浮出的地方，只换文本。
        //
        // The document `<title>`: a browser tab has nothing to hover, so only the text changes.
        keys::Style::Plain => text,
    }
}

/// HTML 里挂说明的那层包装：`<span title="说明">已有内容</span>`。
///
/// The wrapper that carries the note in HTML: `<span title="note">content</span>`.
fn wrap_note(description: &str, inner: &str) -> String {
    if description.is_empty() {
        inner.to_string()
    } else {
        format!(
            "<span title=\"{}\">{inner}</span>",
            escape(description, true)
        )
    }
}

/// HTML 转义；`attribute` 为真时额外转义 `"`（属性值用双引号包围）。
///
/// 我们插入的是**原文标记**（`ContentType::Html`），所以转义得自己做 —— lol_html 不会再替我们做一遍。
///
/// HTML escaping; `attribute` additionally escapes `"` (attribute values are double-quoted). What gets inserted
/// is **raw markup** (`ContentType::Html`), so the escaping is ours to do — lol_html does not do it again.
fn escape(text: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());

    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }

    out
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

// 这一组测试守的是**目录里那批 CSS 选择器与英文原文**：它们只在这里被真正执行，写错一个字符不会有编译错误，
// 只会在报告里静默少翻译一处。所以每个作用域都拿一段小 HTML 走一遍改写，确认译文真的落地了（表格单元格、
// 分节标题、图表标题、SVG 标签、月份、日期区间、`<h4>` 片段），并且确认「不认识的文本一个字都不动」。
//
// These tests guard the **selectors and English sources in the catalog**: this is the only place they are really
// executed, and a single wrong character raises no compile error — it just silently leaves one spot untranslated
// in a report. So every scope runs a small piece of HTML through the rewrite and checks that the translation
// really lands (table cells, section heading, plot title, SVG label, month, date range, `<h4>` fragment), and
// that "unrecognised text is not touched at all".
#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::functions::translation::builtin;

    /// 一份「报告形状」的最小 HTML：每个作用域至少有一处，另外加两段**不该被碰**的文本。
    ///
    /// A minimal "report-shaped" HTML: at least one instance of every scope, plus two texts that must **not** be
    /// touched.
    const REPORT: &str = r##"<!DOCTYPE html>
<html lang="en">
<head><title>Tearsheet (generated by QuantStats-RS)</title></head>
<body onload="save()">
<h1>GOOGL <dt>5 Jan, 2021 - 21 Sep, 2026</dt></h1>
<h4>Benchmark is SPX |  Generated by <a href="https://example.com">QuantStats-RS</a> (v. 0.1.0)</h4>
<div class="container">
<div id="left">
<div class="qs-plot"><div class="qs-plot-title" style="text-align:center">Cumulative Returns vs Benchmark</div>
<svg viewBox="0 0 576 288"><text x="36" y="270" text-anchor="start" fill="#333">Strategy</text><text x="36" y="286" fill="#333">Benchmark</text><text x="100" y="100">2021-01</text></svg></div>
<div id="monthly_heatmap"><div class="qs-plot"><div class="qs-plot-title" style="text-align:center">Strategy - Monthly Returns (%)</div>
<svg viewBox="0 0 576 288"><text x="36" y="270">JAN</text><text x="60" y="270">FEB</text><text x="36" y="100">2021</text></svg></div></div>
</div>
<div id="right">
<h3>Key Performance Metrics</h3>
<table><thead><tr><th>Metric</th><th>SPX</th><th>GOOGL</th></tr></thead><tbody>
<tr><td>Risk-Free Rate</td><td>0.0%</td><td>0.0%</td></tr>
<tr><td>Sharpe</td><td>0.90</td><td>1.20</td></tr>
<tr><td colspan="3"><hr></td></tr>
<tr><td>CAGR﹪</td><td>10.00%</td><td>12.00%</td></tr>
</tbody></table>
<div id="eoy">
<h3>EOY Returns vs Benchmark</h3>
<table><thead><tr><th>Year</th><th>Benchmark</th><th>Strategy</th><th>Multiplier</th><th>Won</th></tr></thead>
<tbody><tr><td>2021</td><td>28.71</td><td>32.15</td><td>1.12</td><td>+</td></tr></tbody></table>
</div>
<div id="ddinfo"><h3>Worst 10 Drawdowns</h3>
<table><thead><tr><th>Started</th><th>Recovered</th><th>Drawdown</th><th>Days</th></tr></thead>
<tbody><tr><td>2022-01-03</td><td>2022-06-01</td><td>-20.00</td><td>120</td></tr></tbody></table>
</div>
</div>
</div>
</body>
</html>"##;

    /// 某个语言的表；直接取自内置数据，测试因此也顺带覆盖了「内置数据能装配出来」。
    ///
    /// One language's table, taken straight from the built-in data, so these tests also cover "the built-in data
    /// assembles at all".
    fn entries(language: &str) -> LanguageTable {
        builtin::table()
            .expect("built-in tables assemble")
            .remove(language)
            .unwrap_or_else(|| panic!("built-in language {language} exists"))
    }

    fn translate(language: &str) -> String {
        rewrite(REPORT, &entries(language), language).expect("rewriting succeeds")
    }

    #[test]
    fn translates_the_whole_report_into_chinese() {
        let out = translate("zh-CN");

        // 文档标题、`<html lang>`、分节标题、指标行、表头、图表标题、SVG 图例、月份、日期区间、`<h4>` 片段。
        //
        // The document title, `<html lang>`, section headings, metric rows, table headers, plot titles, SVG
        // legend, months, the date range and the `<h4>` fragments.
        assert!(out.contains("lang=\"zh-CN\""));
        assert!(out.contains("业绩报告（由 QuantStats-RS 生成）"));
        assert!(out.contains("关键绩效指标"));
        assert!(out.contains("最差 10 次回撤"));
        assert!(out.contains(">夏普比率<") || out.contains(">夏普比率</span>"));
        assert!(!out.contains("CAGR﹪"), "the metric label is gone");
        assert!(out.contains("年化收益"));
        assert!(out.contains(">指标<"), "the Metric header");
        assert!(out.contains(">年份<"), "the EOY header");
        assert!(out.contains(">开始<"), "the drawdown header");
        assert!(out.contains("累计收益 vs 基准"), "the plot title");
        assert!(out.contains(">策略<"), "the SVG legend");
        assert!(out.contains(">1月<"), "the heatmap month");
        assert!(
            visible_text(&out).contains("2021年1月5日 - 2026年9月21日"),
            "the date range, month clauses unwrapped"
        );
        assert!(out.contains("基准</span> SPX"), "the h4 benchmark fragment");
        assert!(out.contains("生成自"), "the h4 generated-by fragment");
        // 版本号是数据，不动。
        //
        // The version number is data and stays.
        assert!(out.contains("(v. 0.1.0)"));
        // 浮出说明真的挂上了（中文说明 + emoji）。
        //
        // The tooltips are really attached (Chinese notes with emoji).
        assert!(out.contains("<span title=\"每单位波动换来的超额收益 ⚖️\">夏普比率</span>"));
        // 用户数据一个字都不动。
        //
        // Not one character of the caller's data changes.
        assert!(out.contains(">GOOGL "), "the strategy name");
        assert!(out.contains(">SPX<"), "the benchmark name in `<th>`");
        assert!(out.contains("2021-01"), "an axis label");
        assert!(out.contains("onload=\"save()\""), "the template's own script hook");
    }

    #[test]
    fn english_keeps_every_word_and_adds_notes() {
        let out = translate("en");

        // `en` 的语义是「文字不变、只加说明」。把标记剥掉之后，可见文本必须与原报告逐字相同。
        //
        // `en` means "same words, notes added": with the markup stripped, the visible text has to be identical to
        // the original, character for character.
        assert_eq!(visible_text(&out), visible_text(REPORT));

        // 文字没变，说明却真的挂上了（这是 `en` 与「什么都不做」的唯一区别）。
        //
        // The words are unchanged, yet the notes really are attached — the only thing that tells `en` apart from
        // doing nothing.
        assert!(out.contains("<span title=\"Excess return per unit of volatility ⚖️\">Sharpe</span>"));
        assert!(out.contains("<title>January ❄️</title>JAN"), "the SVG month");
    }

    #[test]
    fn without_a_language_the_report_is_untouched() {
        // 没配语言时根本不进改写器（调用侧的判断），这里守的是「改写器本身也不该乱动不认识的文本」。
        //
        // Without a language the rewriter is not entered at all (the caller decides); what this guards is that the
        // rewriter itself leaves unrecognised text alone.
        let out = rewrite(REPORT, &entries("zh-CN"), "zh-CN").expect("rewriting succeeds");

        assert!(out.contains(">2021<"), "a year cell");
        assert!(out.contains("text-anchor=\"start\""), "attributes stay put");
    }

    #[test]
    fn an_empty_note_leaves_no_title_attribute() {
        let mut table = entries("zh-CN");
        table.entry("metric.sharpe".to_string()).and_modify(|entry| {
            entry.description.clear();
        });

        let out = rewrite(REPORT, &table, "zh-CN").expect("rewriting succeeds");

        assert!(out.contains(">夏普比率<"), "the translation is still applied");
        assert!(
            !out.contains("<span title=\"\">"),
            "an empty note adds no wrapper at all"
        );
    }

    #[test]
    fn a_deleted_key_stays_in_the_original_language() {
        let mut table = entries("zh-CN");
        table.remove("metric.sharpe");

        let out = rewrite(REPORT, &table, "zh-CN").expect("rewriting succeeds");

        assert!(out.contains(">Sharpe<"), "no entry, no change");
    }

    #[test]
    fn the_date_range_template_can_be_replaced_by_the_caller() {
        let mut table = entries("zh-CN");
        table
            .entry("date.range".to_string())
            .and_modify(|entry| entry.show = "{y1}/{m1}/{d1} - {y2}/{m2}/{d2}".to_string());

        let out = rewrite(REPORT, &table, "zh-CN").expect("rewriting succeeds");

        assert!(
            visible_text(&out).contains("2021/1月/5 - 2026/9月/21"),
            "the caller's template wins"
        );
    }

    #[test]
    fn a_missing_date_range_template_keeps_the_english_word_order() {
        let mut table = entries("zh-CN");
        table.remove("date.range");

        let out = rewrite(REPORT, &table, "zh-CN").expect("rewriting succeeds");

        assert!(
            visible_text(&out).contains("5 1月, 2021 - 21 9月, 2026"),
            "months only"
        );
    }

    #[test]
    fn an_unknown_language_is_an_error_not_a_passthrough() {
        let error = translate_report(REPORT, "no-such-language").expect_err("unknown language fails");

        assert!(
            error.to_string().contains("qs_list_translations()"),
            "the message points at the way to find out: {error}"
        );
    }

    /// 只留下**浏览器看得见**的文本：`<title>`、`<style>`、`<script>` 整块（标签 + 内容）都去掉，其余标签
    /// 只去标签、留内容。
    ///
    /// `<title>` 必须整块去掉：SVG 的浮出说明正是以 `<title>` 子元素的形式插进去的，它的内容属于「说明」而不是
    /// 正文（列表里那个日历 emoji 就是它的尾巴）；只去标签会把说明当成正文，让「英文原文没变」这条断言失去
    /// 意义。
    ///
    /// Only the text a **browser shows** is kept: `<title>`, `<style>` and `<script>` go as whole blocks (tags and
    /// content), every other tag loses just its tags. `<title>` has to go as a block because that is exactly the
    /// shape an SVG note is inserted in — its content is a note, not body text (the trailing calendar emoji in the
    /// list above belongs to it) — and stripping tags alone would count notes as body text, making the "English
    /// words are unchanged" assertion meaningless.
    fn visible_text(html: &str) -> String {
        let mut out = String::with_capacity(html.len());
        let mut rest = html;

        while let Some(open) = rest.find('<') {
            // 整块丢弃的元素：从开标签一路吃到闭标签。
            //
            // Elements dropped as a whole: from the opening tag through to the closing one.
            let dropped = ["title", "style", "script"]
                .iter()
                .find(|name| {
                    rest[open + 1..]
                        .to_ascii_lowercase()
                        .starts_with(*name)
                })
                .map(|name| format!("</{name}>"));

            if let Some(closing) = dropped {
                match rest.to_ascii_lowercase().find(&closing) {
                    Some(at) => {
                        rest = &rest[at + closing.len()..];
                        continue;
                    }
                    None => return out,
                }
            }

            out.push_str(&rest[..open]);
            match rest[open..].find('>') {
                Some(close) => rest = &rest[open + close + 1..],
                None => return out,
            }
        }
        out.push_str(rest);

        out
    }
}
