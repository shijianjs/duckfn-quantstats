// ============================================================================
// 报告文件名：`<时间>-<策略名>-<基准名>[-<随机尾缀>].html`
//
// 文件名不给用户填（`output_dir` 只给目录），所以它必须由函数自己命名，而命名这件事有两条外来的规则
// 要守：**合法性**交给 `sanitize-filename`（Node 同名库的移植：非法字符、控制字符、Windows 保留设备名、
// 结尾的点与空格），**随机**交给 `fastrand`。本文件只负责把它们拼成一个「能放在任何目录里、且一眼看得
// 出是哪份报告」的名字。
//
// 主干的三段各有用处：时间在最前，所以按名字排序一个目录排出来正好是时间顺序；后两段只为人看 ——
// 目录里挤着一堆报告时，一眼能认出这是哪个标的对哪个基准。两段都过一遍 [`sanitize_part`]（并各自限长），
// 随机尾缀则保证同一秒里连着出几份报告也不会撞名。
//
// 浏览器那份临时文件用的是同一套主干（见 browser.rs），差别只在「随机尾缀由 tempfile 在系统临时目录里
// 生成，并且它会验证那个名字当时是空的」。
//
// The report file name: `<time>-<strategy>-<benchmark>[-<random>].html`.
//
// The name is not something the user types (`output_dir` only takes a directory), so the function has to name
// the files itself, and naming has two external rules to respect: **legality** belongs to `sanitize-filename`
// (a port of the Node library of the same name: illegal and control characters, Windows reserved device names,
// trailing dots and spaces) and **randomness** to `fastrand`. All this file does is assemble them into a name
// that fits in any directory and says at a glance which report it is.
//
// The three parts of the stem each earn their place: the time comes first, so sorting a directory by name
// sorts it by time; the other two are for humans — with a crowded directory they say which instrument against
// which benchmark this is. Both go through [`sanitize_part`] (with a per-part cap), and the random suffix
// keeps several reports from the same second apart.
//
// The browser's temporary file uses the same stem (see browser.rs), the only difference being that there
// `tempfile` generates the random suffix inside the system temp directory and verifies the name was free.
// ============================================================================

use chrono::Local;

/// 报告文件的后缀。系统据此把它交给浏览器渲染，而不是当成下载。
///
/// The report file's extension: what makes the system render it in a browser instead of downloading it.
pub(super) const REPORT_EXTENSION: &str = ".html";

/// 文件名里每一段（策略名、基准名）最多保留的字符数。
///
/// 名字里有时间、两段显示名与随机尾缀，单段不设上限很容易顶到文件名的 255 字符上限。
///
/// How many characters of one part (strategy name, benchmark name) may end up in a file name.
///
/// A name holds the time, two display names and a random suffix; without a per-part cap it is easy to run
/// into the 255-character limit on file names.
const MAX_PART_CHARS: usize = 32;

/// 报告文件名的主干：`<时间>-<策略名>-<基准名>`，没写的那几段跳过。
///
/// 两段显示名由调用方（report.rs）**解析好再传进来**，用的正是报告里显示的那两个名字
/// （`strategy_title` 缺省退回 symbol、`benchmark_title` 按下标退回基准 symbol），所以目录里的文件名与
/// 报告里的图例永远对得上 —— 一处解析、两处使用。都没有时主干只剩时间戳，功能不受影响。返回的名字里
/// **不含**随机尾缀与后缀，由调用方决定要不要加（浏览器那份交给 `tempfile`）。
///
/// The stem of a report file name: `<time>-<strategy>-<benchmark>`, skipping whichever parts are unset.
///
/// The two display names are **resolved by the caller** (report.rs) and passed in, using exactly the names the
/// report itself shows (`strategy_title` falling back to the symbol, `benchmark_title` falling back to its
/// benchmark symbol), so the file name on disk and the legend inside the report always agree — resolved once,
/// used twice. When neither is set the stem is just the timestamp and nothing else changes. The name returned
/// here carries **no** random suffix and no extension; the caller decides (the browser's copy hands that over
/// to `tempfile`).
pub(super) fn stem(strategy_title: &str, benchmark_title: Option<&str>) -> String {
    let mut parts = vec![Local::now().format("%Y%m%d-%H%M%S").to_string()];

    push_part(&mut parts, strategy_title);
    if let Some(benchmark_title) = benchmark_title {
        push_part(&mut parts, benchmark_title);
    }

    parts.join("-")
}

/// 一个完整的报告文件名：主干 + 随机尾缀 + `.html`。
///
/// 时间戳只到秒，同一秒内跑两次同样的查询、或者一次调用里出现同一个 (标的, 基准) —— 后者不可能 ——
/// 才会用得上随机尾缀；调用方还会再查一遍这个名字在目标目录里是否已存在，所以「不覆盖已有文件」是保证
/// 而不是概率（见 report.rs）。
///
/// A complete report file name: the stem plus a random suffix plus `.html`.
///
/// The timestamp stops at seconds, so the random suffix only earns its place when the same query runs twice
/// within one second (the same (instrument, benchmark) pair twice in one call cannot happen). Callers also
/// check whether the name already exists in the target directory, so "nothing existing is overwritten" is a
/// guarantee rather than a probability (see report.rs).
pub(super) fn file_name(strategy_title: &str, benchmark_title: Option<&str>) -> String {
    format!(
        "{}-{:08x}{REPORT_EXTENSION}",
        stem(strategy_title, benchmark_title),
        fastrand::u32(..)
    )
}

/// 把一段文本变成能安全放进文件名的一段。
///
/// 合法性的规则本身交给 `sanitize-filename`（Node 同名库的移植）：非法字符（`/ \ ? < > : * | "`）、控制
/// 字符、Windows 保留设备名（`CON`、`NUL`、`COM1`…）以及结尾的点与空格都由它处理，我们只说「换成 `_`」。
/// 两个选项是刻意写死的：`windows: true`，因为 Windows 的规则更严，一套规则在三个平台都成立，不必看平台
/// 下菜；`truncate: false`，因为它的截断是按 255 字节来的，对于要拼进同一个文件名的一段前缀太宽松，长度
/// 由 [`MAX_PART_CHARS`] 自己按字符数管。
///
/// 之后是本项目的两条策略 —— 它们不属于「文件名字法」，所以放在库之外：
///
/// - 空格并成 `_`：空格确实合法，但带空格的名字在命令行里每次都得加引号；
/// - 最多 [`MAX_PART_CHARS`] 个字符，并去掉结尾多出来的 `_`。
///
/// 整段都成了 `_` 时返回空串，调用方据此决定要不要这一段。
///
/// Turn a piece of text into something that can safely go into a file name.
///
/// The legality rules themselves are `sanitize-filename`'s (a port of the Node library of the same name):
/// illegal characters (`/ \ ? < > : * | "`), control characters, Windows reserved device names (`CON`, `NUL`,
/// `COM1` …) and trailing dots and spaces are all its business, and all we say is "replace with `_`". Two
/// options are deliberately pinned: `windows: true`, because the Windows rules are the strictest and one set
/// of rules holds on all three platforms; and `truncate: false`, because its truncation is 255 *bytes*, far
/// too generous for one part of a name, so the length is handled by [`MAX_PART_CHARS`] in characters instead.
///
/// Two more steps are this project's own policy — they are not about what makes a file name legal, hence they
/// live outside the crate:
///
/// - spaces become `_`: they are legal, but a name with spaces has to be quoted every time it is typed into a
///   shell;
/// - at most [`MAX_PART_CHARS`] characters, with a trailing `_` left over by the cap trimmed off.
///
/// A part that turns into nothing but `_` comes back empty, which is how the caller decides to leave it out.
pub(super) fn sanitize_part(part: &str) -> String {
    let sanitized = sanitize_filename::sanitize_with_options(
        part,
        sanitize_filename::Options {
            windows: true,
            truncate: false,
            replacement: "_",
        },
    );

    let joined = sanitized.split_whitespace().collect::<Vec<_>>().join("_");
    let capped: String = joined.chars().take(MAX_PART_CHARS).collect();

    capped.trim_end_matches('_').to_string()
}

/// 把一段已经过 [`sanitize_part`] 的文本追加进名字里，整段变成空串时跳过。
///
/// Append an already [`sanitize_part`]-ed piece to the name, skipping it when it came back empty.
fn push_part(parts: &mut Vec<String>, name: &str) {
    let part = sanitize_part(name);
    if !part.is_empty() {
        parts.push(part);
    }
}
