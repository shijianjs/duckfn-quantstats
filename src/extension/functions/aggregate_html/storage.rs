// ============================================================================
// 报告落盘：本机文件系统，wasm 上整个跳过
//
// 这里只有两件事 —— 挑一个没被占用的文件名（[`report_path`]），把字节写下去（[`write_report`]）。
// 两件都交给 `std::fs`：目录在不在、路径合法不合法、权限够不够，都是它的答案，我们只负责把错误加上
// 「写的是哪份报告」这个上下文。
//
// 平台差异集中在 [`CAN_WRITE_FILES`] 一处，与 browser.rs 的 `CAN_LAUNCH_BROWSER` 一个路子：
//
//   - 原生构建：正常落盘 —— 挑一个空闲路径，真写；
//   - wasm 构建：**整个跳过文件操作**。DuckDB-Wasm 的文件系统不忠实：任何不存在的路径都会返回一条
//     1 字节 `\0` 的幻影条目，连 DuckDB 自己的 `glob` / `read_text` / `file_size` 都把它报成存在，
//     于是「这个名字空着吗」在那边没有可靠答案。所以 `output_dir` 在 wasm 下不落盘：不报错、不写
//     文件，返回行里的 `file_path` 是 NULL，报告本身照常渲染并返回（宿主页面拿 iframe 之类展示）。
//
// 这也是本扩展不需要 duckfn 的 `owned-connection`（以及它带来的 `duck_vfs`）的原因：宿主 VFS 那条
// 路经由 DuckDB 的文件系统，多出来的是 `s3://` 之类的远端目标；这里要的只是「本地磁盘 + wasm 跳过」
// 这一档，`std::fs` 就够，而且少一个 feature。
//
// Persisting the report: the local file system, skipped entirely on wasm.
//
// All there is to it are two things — pick a free file name ([`report_path`]) and write the bytes
// ([`write_report`]) — and `std::fs` answers both: whether the directory exists, whether the path is
// legal, whether the permissions allow it. All we add is the context "which report this was".
//
// The platform difference lives in [`CAN_WRITE_FILES`] alone, the same shape as `CAN_LAUNCH_BROWSER` in
// browser.rs:
//
//   - a native build writes as usual — pick a free path, write it;
//   - a wasm build **skips the whole file operation**. DuckDB-Wasm's file system is not faithful: a
//     path that does not exist comes back as a phantom one-byte `\0` entry that even DuckDB's own
//     `glob` / `read_text` / `file_size` report as present, so "is this name free" has no reliable
//     answer there. `output_dir` therefore writes nothing in a wasm build: no error, no file, and a
//     NULL `file_path` in the returned row — the report itself is still rendered and returned (a host
//     page can show it in an iframe, say).
//
// It is also why this extension does not need duckfn's `owned-connection` feature (and the `duck_vfs`
// module it brings): that path goes through DuckDB's file system and buys remote targets such as
// `s3://`, while all that is wanted here is "local disk, skipped on wasm" — `std::fs` covers that and
// costs one feature less.
// ============================================================================

use std::path::Path;

use duckfn::{DuckResult, duck_error};

use super::naming;

/// 这个平台会不会真的写文件。
///
/// wasm 构建恒为 false（见模块头）：那边整个跳过文件操作 —— 不挑名字、不写字节，`output_dir` 因此
/// 不落盘，返回行里的 `file_path` 是 NULL。刻意不做成条件编译：`std::fs` 在 emscripten 下照样编得过，
/// 一个常量判断就够，平台差异也就不必散到 `naming.rs` 那类只被这条路径用到的模块里去。
///
/// Whether this platform actually writes files.
///
/// Always false in a wasm build (see the module header): the file operation is skipped there — no name is
/// picked and no bytes are written, so `output_dir` persists nothing and the returned `file_path` is NULL.
/// Deliberately not conditional compilation: `std::fs` compiles for emscripten just fine, one constant
/// check is enough, and the platform difference does not have to be scattered into modules such as
/// `naming.rs` that only this path uses.
pub(super) const CAN_WRITE_FILES: bool = !cfg!(target_arch = "wasm32");

/// 名字撞上上限时重试几次：随机尾缀是 32 位，撞上几乎不可能，但「不覆盖已有文件」要是保证而不是概率。
///
/// How many times to retry when a name is taken: the random suffix is 32 bits wide, so a collision is
/// practically impossible, but "nothing existing is overwritten" should be a guarantee, not a probability.
const MAX_NAME_ATTEMPTS: usize = 8;

/// 在 `output_dir` 下挑一个没被占用的文件名，返回完整路径；wasm 上不落盘，返回 `None`。
///
/// 名字由 naming.rs 按「时间 + 策略名 + 基准名 + 随机尾缀」生成（名字里的那两段就是报告图例里那两段），
/// 这里只解决「万一撞上已存在的文件」：换一个尾缀重试；重试到上限仍然撞上就报错。判据是
/// `Path::exists` —— 只看，不创建；它出错时按「不存在」算，写的时候再由 `std::fs` 报真正的错。
///
/// Pick a free file name under `output_dir` and return the full path; `None` on wasm, where nothing is
/// persisted.
///
/// naming.rs builds the name from "time + strategy + benchmark + random suffix" (the two display names
/// in it are the ones the report legend shows); all this does is handle "what if that file already
/// exists": it retries with another suffix, and errors out if it keeps colliding. The test is
/// `Path::exists` — look, never create; an error from it counts as "absent", and `std::fs` reports the
/// real problem when the write happens.
pub(super) fn report_path(
    dir: &str,
    strategy_title: &str,
    benchmark_title: Option<&str>,
) -> DuckResult<Option<String>> {
    // wasm：整个跳过文件操作。`None` 就是「这一份不落盘」，调用方因此连写都不会去写，返回行里的
    // `file_path` 是 NULL；连名字都不挑，那套「不存在的路径也算存在」的幻影条目也就根本不参与判断。
    //
    // wasm: the whole file operation is skipped. `None` means "this report is not persisted", so the
    // caller does not even try to write and the returned `file_path` is NULL; no name is picked either,
    // which keeps those phantom entries ("a path that does not exist still counts as present") out of
    // the decision entirely.
    if !CAN_WRITE_FILES {
        return Ok(None);
    }

    for _ in 0..MAX_NAME_ATTEMPTS {
        let path = join(dir, &naming::file_name(strategy_title, benchmark_title));
        if !Path::new(&path).exists() {
            return Ok(Some(path));
        }
    }

    Err(duck_error(format!(
        "qs_html_report_options.output_dir={dir}: could not find a free report file name in \
         {MAX_NAME_ATTEMPTS} attempts"
    )))
}

/// 把渲染好的报告写到 [`report_path`] 定下的路径。
///
/// `std::fs::write` 一次做完全部三件事：创建、截断到零、写入 —— 于是「旧文件更长」这个 C API 时代的
/// 麻烦不存在，也不需要在写之前先清一次。错误带上路径再抛出，因为 `std::fs` 自己的报文只有
/// `No such file or directory (os error 2)` 这类，不说它说的是哪一份报告。
///
/// wasm 下不会走到这里（[`report_path`] 给的是 `None`，`open_in_browser` 在那边也恒为 false），但这里
/// 仍挡一道：那边没有可写的文件系统，跳过比报错诚实。
///
/// Persists the rendered report to the path [`report_path`] settled on.
///
/// `std::fs::write` does the whole job in one call: create, truncate to zero, write — so the "the old
/// file was longer" trouble of the C API era does not exist here and there is no need to zero anything
/// first. The error is re-thrown with the path attached, because `std::fs`'s own message is something
/// like `No such file or directory (os error 2)` and does not say which report it means.
///
/// A wasm build never gets here ([`report_path`] answers `None`, and `open_in_browser` is always false
/// there), but the guard stays: that side has no writable file system, and skipping is more honest than
/// an error.
pub(super) fn write_report(path: &str, report: &str) -> DuckResult<()> {
    if !CAN_WRITE_FILES {
        return Ok(());
    }

    std::fs::write(path, report).map_err(|err| {
        duck_error(format!("cannot write the report to '{path}': {err}"))
    })
}

/// 目录 + 文件名。
///
/// 刻意不用 `Path::join`：它按**平台**的分隔符拼，于是同一个 `file_path` 在 Windows 上是
/// `out\name.html`、在别处是 `out/name.html` —— 而这个字符串是给用户看的、也常被下游直接拿去拼路径
/// （`__TEST_DIR__/name.html` 这种形状也被测试盯着）。于是这里统一用 `/`，只把用户写在末尾的分隔符
/// 去掉（`/` 与 `\` 都算）；`std::fs` 在 Windows 上同样接受 `/`。
///
/// Directory + file name.
///
/// `Path::join` is deliberately not used: it joins with the **platform** separator, so the same
/// `file_path` would read `out\name.html` on Windows and `out/name.html` elsewhere — and that string is
/// what the caller sees and often concatenates further paths from (the `__TEST_DIR__/name.html` shape is
/// even asserted by the tests). So `/` is used throughout and only a trailing separator the user wrote
/// (either `/` or `\`) is trimmed; `std::fs` accepts `/` on Windows too.
fn join(dir: &str, file_name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches(['/', '\\']), file_name)
}
