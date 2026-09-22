// ============================================================================
// 用系统默认浏览器打开报告
//
// 这个功能有两半，各交给一个成熟的库：
//
//   - 打开哪个文件：配置里写了 `output` 就用它；没写就让 `tempfile` 在系统临时目录里**新建**一个文件，
//     前缀由我们给（时间 + 策略名 + 基准名），随机尾缀与「保证是新建的」由它负责。前缀里那两段显示名
//     过一遍 `sanitize-filename`，文件名的合法性（非法字符、控制字符、Windows 保留设备名、结尾的点与
//     空格）由它维护。浏览器需要一个真实存在的文件，而报告在这里只是一个字符串。
//   - 怎么打开：`open`。各平台的入口（Windows 的 ShellExecute、macOS 的 `open`、类 Unix 的 `xdg-open`
//     及其后备序列）、参数引用、以及不给进程留下僵尸子进程，都是它的事。
//
// 平台差异集中在 `CAN_LAUNCH_BROWSER`：wasm 构建里没有浏览器进程可以启动，这条功能整个被忽略 ——
// 报告字符串原样返回给宿主，展示是宿主页面的事。`open` 本身在 emscripten 上根本编不过（它没有那个平台
// 的实现），所以依赖与实现都限定在非 wasm：Cargo.toml 里那三个 crate 挂在
// `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` 下，这里对应的项也只在非 wasm 编译。
//
// Opening the report in the system default browser.
//
// The feature has two halves, each given to a crate that already knows the platform:
//
//   - which file to open: the configured `output` when there is one, otherwise a file that `tempfile`
//     **creates** in the system temp directory, with a prefix we supply (time + strategy + benchmark) and
//     its own random suffix and "this name is new" guarantee. The two display names in that prefix go through
//     `sanitize-filename`, which owns the rules for legal file names (illegal and control characters, Windows
//     reserved device names, trailing dots and spaces). A browser needs a file that actually exists, while
//     all we have here is a string.
//   - how to open it: `open`. The per-platform entry points (ShellExecute on Windows, `open` on macOS,
//     `xdg-open` and its fallbacks on Unix), the argument quoting, and not leaving zombie children behind are
//     all its business.
//
// The platform difference is concentrated in `CAN_LAUNCH_BROWSER`: a wasm build has no browser process to
// launch, and the whole feature is ignored there — the report string goes back to the host as it is, and
// displaying it is the host page's business. `open` does not even compile for emscripten (it has no
// implementation for that platform), so both the dependency and the implementation are non-wasm only: those
// three crates live under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` in Cargo.toml, and the
// matching items here only exist off wasm.
// ============================================================================

use std::path::{Path, PathBuf, absolute};

// `Local` 只服务临时文件名的前缀（非 wasm 那一半），所以它本身也要跟着 `cfg`，否则 wasm 下是未使用的
// import。
//
// `Local` only serves the temporary file name prefix (the non-wasm half), so it is gated as well — otherwise it
// is an unused import on wasm.
#[cfg(not(target_arch = "wasm32"))]
use chrono::Local;
use duckfn::{DuckResult, duck_error};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

/// 这个平台能不能启动浏览器。
///
/// wasm 构建恒为 false：那里没有浏览器进程可以启动，报告字符串原样返回给宿主。整个模块的平台差异就集中
/// 在这一处 —— 调用方靠它决定「要不要校验 `output` 能不能打开」以及「没有 `output` 时要不要落临时文件」。
///
/// Whether this platform can launch a browser at all.
///
/// Always false in a wasm build: there is no browser process to launch there, and the report string goes back
/// to the host as it is. This constant is where the module's platform difference lives — callers use it to
/// decide whether `output` has to be openable and whether a temporary file is needed when it is unset.
const CAN_LAUNCH_BROWSER: bool = !cfg!(target_arch = "wasm32");

/// 报告生成后是否要用系统默认浏览器打开。
///
/// 配置里没写就是 false；wasm 构建下**恒为 false**（见 [`CAN_LAUNCH_BROWSER`]），因此那边也不会为了
/// 打开而写临时文件。
///
/// Whether the report should be opened in the system default browser once it has been generated.
///
/// Unset means false; a wasm build always answers false (see [`CAN_LAUNCH_BROWSER`]), so it does not write a
/// temporary file just to open it either.
pub(super) fn is_requested(options: &QuantstatsHtmlOptions) -> bool {
    CAN_LAUNCH_BROWSER && options.open_in_browser.unwrap_or(false)
}

/// 把落盘路径变成「系统浏览器能打开的本地绝对路径」。
///
/// DuckDB 的 VFS 路径不一定是本地文件（`s3://`、`memory://`…），系统浏览器打不开它们，所以这里直接报错，
/// 而不是把那种路径交给启动器去猜。判据就是字面上的 `://`：刻意不引 URL 解析库，因为 Windows 的 `C:\...`
/// 在 URL 语法里同样是一个 scheme，用 `://` 才既挡得住 `s3://` 又不误伤盘符。
///
/// 相对路径按进程的当前目录补成绝对路径（`std::path::absolute`，不碰文件系统、不解析符号链接；Windows 上
/// 走 `GetFullPathNameW`，不会加 `\\?\` 前缀 —— 那是 ShellExecute 认不出的东西）—— 与 DuckDB 的本地文件
/// 系统写它时的解释一致（`duck_vfs` 也以同一个当前目录为准）。
///
/// Turn a write path into a local absolute path a browser can open.
///
/// A DuckDB VFS path is not necessarily a local file (`s3://`, `memory://` …) and no system browser can open
/// those, so this reports an error instead of handing such a path to a launcher to guess at. A relative path
/// is made absolute against the process's current directory by `std::path::absolute` (no filesystem access, no
/// symlink resolution; on Windows it goes through `GetFullPathNameW` and adds no `\\?\` prefix, which is
/// something ShellExecute does not understand) — the same current directory DuckDB's local file system uses
/// when it writes the path, since `duck_vfs` resolves it that way too.
pub(super) fn local_path(path: &str) -> DuckResult<PathBuf> {
    if path.contains("://") {
        return Err(duck_error(format!(
            "qs_html_report_options.open_in_browser cannot open '{path}': \
             only local file paths can be opened in a browser"
        )));
    }

    absolute(path).map_err(|err| {
        duck_error(format!(
            "qs_html_report_options.open_in_browser cannot make '{path}' absolute: {err}"
        ))
    })
}

/// 没有 `output` 而又要在浏览器里打开时，替它**新建**一个临时文件并返回它的路径。
///
/// 名字形如 `<时间>-<策略名>-<基准名>-<随机尾缀>.html`（前缀见 [`temporary_name`]），由 `tempfile` 在系统
/// 临时目录里新建：它保证这个名字当时是空的（撞上就换一个随机尾缀重试），所以既不会覆盖已有文件，同一秒
/// 里连着出几份报告也不会互相踩。
///
/// 返回的是**已经存在的空文件**的路径：报告随后照常由 `write_report` 经 DuckDB 的 VFS 写进去 —— 临时
/// 文件的「名字」和「内容」各归各的库，而写路径仍然只有一条。落盘要 `&str`，临时路径则是我们自己拼出来的
/// （系统临时目录 + 前缀 + 随机尾缀），必定是合法 UTF-8，那次转换只是形状上的。
///
/// When no `output` was configured but the browser was asked for, **create** a temporary file for it and
/// return its path.
///
/// The name looks like `<time>-<strategy>-<benchmark>-<random>.html` (the prefix is [`temporary_name`]) and
/// `tempfile` creates it in the system temp directory: it guarantees the name was free at that moment (a
/// collision means another random suffix is tried), so nothing existing is overwritten and several reports
/// generated within the same second do not step on each other.
///
/// What comes back is the path of an **existing empty file**: the report then goes into it through DuckDB's
/// VFS via `write_report` as usual — the temporary file's name and its content each come from the library
/// that suits them, while there is still only one write path. Writing wants a `&str`, and the temporary path
/// is one we assembled ourselves (system temp directory + prefix + random suffix), so it is valid UTF-8 by
/// construction and that conversion is only about the type.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn temporary_file(options: &QuantstatsHtmlOptions) -> DuckResult<Option<String>> {
    if !is_requested(options) {
        return Ok(None);
    }

    let prefix = format!("{}-", temporary_name(options));
    let path = create_temporary_file(&prefix).map_err(|err| {
        duck_error(format!(
            "qs_html_report_options.open_in_browser cannot create a temporary file: {err}"
        ))
    })?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

/// wasm 构建里的 [`temporary_file`]：没有浏览器可以启动，也就没有「为了打开而建一个临时文件」这回事。
///
/// [`temporary_file`] for a wasm build: there is no browser to launch, hence no "create a temporary file just
/// to open it" either.
#[cfg(target_arch = "wasm32")]
pub(super) fn temporary_file(_options: &QuantstatsHtmlOptions) -> DuckResult<Option<String>> {
    Ok(None)
}

/// 用系统默认浏览器打开 [`local_path`] 给出的路径。
///
/// `open::that_detached` 把启动器叫起来就把控制权还给我们，不等它退出 —— 报告已经落盘了，浏览器要怎么
/// 处理这个文件不关这次查询的事。唯一会报错的情形是启动器本身起不来（比如系统里没有 `xdg-open`）。
///
/// Launch the path produced by [`local_path`] with the system default browser.
///
/// `open::that_detached` hands control back as soon as the launcher is started and does not wait for it to
/// exit — the report is already on disk, and what the browser does with the file is no concern of this query.
/// The only failure it reports is the launcher itself not starting (no `xdg-open` on the system, say).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn open_report(path: &Path) -> DuckResult<()> {
    open::that_detached(path).map_err(|err| {
        duck_error(format!(
            "qs_html_report_options.open_in_browser could not launch a browser for '{}': {err}",
            path.display()
        ))
    })
}

/// wasm 构建里的 [`open_report`]：没有浏览器进程可以启动，空操作。
///
/// [`open_report`] for a wasm build: there is no browser process to launch, so it does nothing.
#[cfg(target_arch = "wasm32")]
pub(super) fn open_report(_path: &Path) -> DuckResult<()> {
    Ok(())
}

/// 在系统临时目录里新建一个不重名的空文件，返回它的路径。
///
/// `tempfile` 一次做完三件事：临时目录、随机尾缀、以及「这个文件名当时一定是空的」（新建失败就换个随机
/// 尾缀重试）。`keep()` 则让它不随句柄一起被删掉 —— 浏览器要等这次查询结束之后才去看它。
///
/// Create a uniquely named empty file in the system temp directory and return its path.
///
/// `tempfile` does three things at once: the temp directory, the random suffix, and "this name was definitely
/// free" (a failed create retries with another random suffix). `keep()` is what stops it from disappearing
/// with the handle — the browser only looks at it once this query is over.
#[cfg(not(target_arch = "wasm32"))]
fn create_temporary_file(prefix: &str) -> std::io::Result<PathBuf> {
    let file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(".html")
        .tempfile()?;

    // `PathPersistError` 里就是底层的 IO 错误，剥出它即可（路径我们自己有）。
    //
    // A `PathPersistError` is just the underlying IO error wrapped up; unwrap it (we already have the path).
    file.into_temp_path().keep().map_err(|err| err.error)
}

/// 临时文件名的前缀：`<时间>-<策略名>-<基准名>`（没写的那几段跳过）。
///
/// 时间在最前，所以按名字排序临时目录，排出来正好是时间顺序；两个显示名只为人看 —— 临时目录里挤着一堆
/// 文件时，一眼能认出这是哪份报告。策略名优先取 `strategy_title`（报告里策略那一列的标题），没写就退回
/// 报告标题 `title`；基准名取 `benchmark_title`。都没写时前缀只剩时间戳，功能不受影响。
///
/// The prefix of the temporary file name: `<time>-<strategy>-<benchmark>`, skipping whichever parts are unset.
///
/// The time comes first so that sorting the temp directory by name sorts it by time; the two display names are
/// there for humans only — with a crowded temp directory they say at a glance which report this is. The
/// strategy part prefers `strategy_title` (the heading of the strategy column) and falls back to the report
/// `title`; the benchmark part is `benchmark_title`. When none of them is set the prefix is just the
/// timestamp, and nothing else changes.
#[cfg(not(target_arch = "wasm32"))]
fn temporary_name(options: &QuantstatsHtmlOptions) -> String {
    let mut parts = vec![Local::now().format("%Y%m%d-%H%M%S").to_string()];

    for name in [
        options.strategy_title.as_deref().or(options.title.as_deref()),
        options.benchmark_title.as_deref(),
    ] {
        if let Some(name) = name.map(sanitize_part).filter(|name| !name.is_empty()) {
            parts.push(name);
        }
    }

    parts.join("-")
}

/// 文件名里每一段（策略名、基准名）最多保留的字符数。
///
/// 名字里有时间、两段显示名与随机尾缀，单段不设上限很容易顶到文件名的 255 字符上限。
///
/// How many characters of one part (strategy name, benchmark name) may end up in a file name.
///
/// A name holds the time, two display names and a random suffix; without a per-part cap it is easy to run into
/// the 255-character limit on file names.
#[cfg(not(target_arch = "wasm32"))]
const MAX_PART_CHARS: usize = 32;

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
#[cfg(not(target_arch = "wasm32"))]
fn sanitize_part(part: &str) -> String {
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
