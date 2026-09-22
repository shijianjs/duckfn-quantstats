// ============================================================================
// 用系统默认浏览器打开报告
//
// 这个功能有两半：
//
//   - 打开哪个文件：配置里写了 `output` 就用它；没写就落到系统临时目录里一个带时间与显示名的随机文件名
//     （浏览器需要一个真实存在的文件，而报告在这里只是一个字符串）；
//   - 怎么打开：各操作系统的「用默认程序打开」入口（Windows 的 `start`、macOS 的 `open`、其余类 Unix 的
//     `xdg-open`）。
//
// `std::process::Command` 在 wasm 目标下不保证可用（emscripten 里也没有浏览器进程可以启动），所以整块
// 实现都限定在非 wasm 平台。wasm 构建下 `is_requested` 恒为 false，报告字符串原样返回给宿主 —— 展示是
// 宿主页面的事，扩展不去碰 DOM。这是本扩展唯一需要区分平台的代码。
//
// Opening the report in the system default browser.
//
// The feature has two halves:
//
//   - which file to open: the configured `output` when there is one, otherwise a randomly named file in the
//     temp directory carrying the time and the display names (a browser needs a file that actually exists,
//     while all we have here is a string);
//   - how to open it: each OS's "open with the default program" entry point (`start` on Windows, `open` on
//     macOS, `xdg-open` elsewhere on Unix).
//
// `std::process::Command` is not guaranteed to be available for wasm targets (and emscripten has no browser
// process to launch anyway), so the whole implementation is non-wasm only. On wasm `is_requested` is always
// false and the report string goes back to the host untouched — displaying it is the host page's business,
// and an extension has no business touching a DOM. This is the only platform-specific code in the extension.
// ============================================================================

use std::env;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

use chrono::Local;
use duckfn::{DuckResult, duck_error};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

/// 这个平台能不能启动浏览器。
///
/// wasm 构建恒为 false：那里没有可以启动的浏览器进程，报告字符串原样返回给宿主。
///
/// Whether this platform can launch a browser at all.
///
/// Always false in a wasm build: there is no browser process to launch there, and the report string goes
/// back to the host as it is.
const CAN_LAUNCH_BROWSER: bool = !cfg!(target_arch = "wasm32");

/// 文件名里每一段（策略名、基准名）最多保留的字符数。
///
/// How many characters of one part (strategy name, benchmark name) may end up in a file name.
const MAX_PART_CHARS: usize = 32;

/// 报告生成后是否要用系统默认浏览器打开。
///
/// 配置里没写就是 false；wasm 构建下**恒为 false**（见 [`CAN_LAUNCH_BROWSER`]），因此那边也不会为了打开
/// 而写临时文件。
///
/// Whether the report should be opened in the system default browser once it has been generated.
///
/// Unset means false; a wasm build always answers false (see [`CAN_LAUNCH_BROWSER`]), so it does not write a
/// temporary file just to open it either.
pub(super) fn is_requested(options: &QuantstatsHtmlOptions) -> bool {
    CAN_LAUNCH_BROWSER && options.open_in_browser.unwrap_or(false)
}

/// 没有 `output` 时给浏览器用的临时文件路径：
/// `<系统临时目录>/<时间>-<策略名>-<基准名>-<随机尾缀>.html`。
///
/// 前缀里的时间与两个显示名只为人看：临时目录里挤着一堆文件时，一眼能认出这是哪份报告，按名字排序也刚好
/// 是时间顺序。名字里出现不了的字符由 [`sanitize_part`] 换成 `_`，所以拼出来的一定是合法文件名；`.html`
/// 后缀也不能省 —— 系统据此把文件交给浏览器而不是别的程序。随机尾缀则保证同一秒里连着出几份报告也不会
/// 互相覆盖。
///
/// 策略名优先取 `strategy_title`（报告里策略那一列的标题），没写就退回报告标题 `title`；基准名取
/// `benchmark_title`。都没写时前缀只剩时间戳，功能不受影响。
///
/// The temporary path used for the browser when there is no `output`:
/// `<temp dir>/<time>-<strategy>-<benchmark>-<random>.html`.
///
/// The time and the two display names are there for humans only: with a crowded temp directory they say at a
/// glance which report this is, and sorting by name happens to sort by time as well. Characters that cannot
/// appear in a file name are replaced by `_` in [`sanitize_part`], so what comes out is always a legal file
/// name; the `.html` suffix matters too — it is what makes the system hand the file to a browser rather than
/// to something else. The random suffix keeps several reports generated within the same second from
/// overwriting each other.
///
/// The strategy part prefers `strategy_title` (the heading of the strategy column) and falls back to the
/// report `title`; the benchmark part is `benchmark_title`. When none of them is set the prefix is just the
/// timestamp, and nothing else changes.
pub(super) fn temporary_report_path(options: &QuantstatsHtmlOptions) -> String {
    let mut parts = vec![Local::now().format("%Y%m%d-%H%M%S").to_string()];

    for name in [
        options.strategy_title.as_deref().or(options.title.as_deref()),
        options.benchmark_title.as_deref(),
    ] {
        if let Some(name) = name.map(sanitize_part).filter(|name| !name.is_empty()) {
            parts.push(name);
        }
    }

    parts.push(random_suffix());

    env::temp_dir()
        .join(format!("{}.html", parts.join("-")))
        .to_string_lossy()
        .into_owned()
}

/// 把配置里的落盘路径变成「系统浏览器能打开的本地绝对路径」。
///
/// DuckDB 的 VFS 路径不一定是本地文件（`s3://`、`memory://`…），系统浏览器打不开它们，所以这里直接报错，
/// 而不是把那种路径交给启动器去猜。相对路径按进程的当前目录补成绝对路径 —— 与 DuckDB 的本地文件系统写它
/// 时的解释一致（`duck_vfs` 也是以同一个当前目录为准）。
///
/// Turn the configured output path into a local absolute path a browser can open.
///
/// A DuckDB VFS path is not necessarily a local file (`s3://`, `memory://` …) and no system browser can open
/// those, so this reports an error instead of handing such a path to a launcher to guess at. A relative path
/// is made absolute against the process's current directory, which is how DuckDB's local file system
/// interprets it as well (`duck_vfs` uses the same current directory).
pub(super) fn local_path(path: &str) -> DuckResult<PathBuf> {
    if path.contains("://") {
        return Err(duck_error(format!(
            "qs_html_report_options.open_in_browser cannot open '{path}': \
             only local file paths can be opened in a browser"
        )));
    }

    let local = Path::new(path);
    if local.is_absolute() {
        return Ok(local.to_path_buf());
    }

    let current_directory = env::current_dir().map_err(|err| {
        duck_error(format!(
            "qs_html_report_options.open_in_browser cannot make '{path}' absolute: {err}"
        ))
    })?;
    Ok(current_directory.join(local))
}

/// 用系统默认浏览器打开 [`local_path`] 给出的路径。
///
/// 只负责「把浏览器叫起来」：报告已经落盘了，所以这里既不等待子进程、也不关心它怎么处理这个文件。唯一会
/// 报错的情形是启动器本身起不来（比如系统里没有 `xdg-open`）。
///
/// Launch the path produced by [`local_path`] with the system default browser.
///
/// All this does is get that browser started: the report is already on disk, so it neither waits for the
/// child nor cares what the child does with the file. The only failure it reports is the launcher itself not
/// starting (no `xdg-open` on the system, say).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn open_report(path: &Path) -> DuckResult<()> {
    let path = path.to_string_lossy().into_owned();

    open_command(&path).spawn().map_err(|err| {
        duck_error(format!(
            "qs_html_report_options.open_in_browser could not launch a browser for '{path}': {err}"
        ))
    })?;

    Ok(())
}

/// wasm 构建里的 [`open_report`]：没有浏览器进程可以启动，空操作。
///
/// 之所以有这个分支而不是让调用方判断，是为了让 `report.rs` 完全不出现平台分支 —— `is_requested` 在
/// wasm 下已经恒为 false，这里再兜一层只是为了让签名一致。
///
/// [`open_report`] for a wasm build: there is no browser process to launch, so it does nothing.
///
/// This exists so that `report.rs` needs no platform branch at all — `is_requested` already answers false on
/// wasm, and this stub just keeps the signature the same.
#[cfg(target_arch = "wasm32")]
pub(super) fn open_report(_path: &Path) -> DuckResult<()> {
    Ok(())
}

/// Windows：交给 cmd 的内建命令 `start`。
///
/// [`local_path`] 已经挡掉了非本地路径，所以这里的入参一定是本地路径。
///
/// Windows: handed to cmd's builtin `start`.
///
/// [`local_path`] has already ruled out non-local paths, so the argument here is always a local path.
#[cfg(not(target_arch = "wasm32"))]
#[cfg(target_os = "windows")]
fn open_command(path: &str) -> Command {
    use std::os::windows::process::CommandExt;

    // `start` 把**第一个参数**当窗口标题：不显式给一个空标题时，被引号包起来的路径会被当成标题，文件
    // 反而打不开（这是它最常见的坑）。
    //
    // 路径用 `raw_arg` 自己加引号塞进命令行：Rust 默认只给含空格的参数加引号，而 cmd 的 `&`、`^`、`(`
    // 这些元字符必须落在引号里才不会被当成命令分隔符或转义符。
    //
    // `start` treats its **first argument** as the window title: without an explicit empty title the quoted
    // path is taken for the title and nothing opens (its most common pitfall).
    //
    // The path is quoted by hand and pushed with `raw_arg`: Rust only quotes arguments that contain spaces,
    // while cmd's `&`, `^`, `(` … are only literal inside quotes.
    let mut command = Command::new("cmd");
    command
        .arg("/C")
        .raw_arg("start")
        .raw_arg("\"\"")
        .raw_arg(format!("\"{path}\""));
    command
}

/// 类 Unix：macOS 用 `open`，其余（Linux / BSD）用 `xdg-open`。
///
/// Unix: `open` on macOS, `xdg-open` elsewhere (Linux / BSD).
#[cfg(not(target_arch = "wasm32"))]
#[cfg(unix)]
fn open_command(path: &str) -> Command {
    let program = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };

    let mut command = Command::new(program);
    command.arg(path);
    command
}

/// 8 位十六进制的随机尾缀。
///
/// 用 `std::hash::RandomState` 当一次性的随机源：std 每实例化一个就从操作系统的随机源取一份新种子（它正是
/// HashMap 抗哈希碰撞用的东西），拿来当「不重名」的尾缀足够，也不值得为 8 个字符引入 `rand` 这类依赖。
///
/// An eight-hex-digit random suffix.
///
/// `std::hash::RandomState` doubles as a one-shot source of randomness here: every instance takes a fresh seed
/// from the operating system (it is what `HashMap` uses for collision resistance), which is plenty for a
/// "never the same name twice" suffix and not worth pulling in a `rand` dependency for eight characters.
fn random_suffix() -> String {
    let hasher = RandomState::new().build_hasher();
    format!("{:08x}", hasher.finish() as u32)
}

/// 把一段文本变成能安全放进文件名的一段。
///
/// 只保留字母与数字（非 ASCII 字母也算 —— 中文标题在三大平台的文件名里都合法），其余一律换成 `_`：空格、
/// `/`、`\`、`:`、`*`、`?`、`"`、`<`、`>`、`|` 以及控制字符都在此列。连续的下划线与开头的下划线压掉，
/// 结尾的下划线也去掉，最多留 [`MAX_PART_CHARS`] 个字符。整段都是非法字符时返回空串，调用方据此决定要不
/// 要这一段。
///
/// Turn a piece of text into something that can safely go into a file name.
///
/// Letters and digits are kept (non-ASCII ones too — a Chinese title is a legal file name on all three
/// platforms) and everything else becomes `_`: spaces, `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|` and control
/// characters. Runs of underscores and a leading underscore are collapsed away, a trailing one is trimmed, and
/// at most [`MAX_PART_CHARS`] characters survive. A part made purely of illegal characters comes back empty,
/// which is how the caller decides to leave it out.
fn sanitize_part(part: &str) -> String {
    let mut sanitized = String::new();
    let mut kept_chars = 0;

    for character in part.chars() {
        if kept_chars == MAX_PART_CHARS {
            break;
        }
        if character.is_alphanumeric() {
            sanitized.push(character);
            kept_chars += 1;
        } else if !sanitized.is_empty() && !sanitized.ends_with('_') {
            sanitized.push('_');
        }
    }

    sanitized.trim_end_matches('_').to_string()
}
