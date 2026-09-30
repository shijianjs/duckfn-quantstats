// 子模块只在 `extension/` 树下挂：src/lib.rs 与 src/wasm_lib.rs 都只写 `mod extension;`，
// 新增模块时不要动那两个 crate root，改这里即可。
//
// Submodules hang off the `extension/` subtree only: both src/lib.rs and src/wasm_lib.rs just declare
// `mod extension;`, so new modules are attached here instead of in the crate roots.
mod functions;
mod types;

use duckfn::{DuckResult, duck_custom_register, duckfn_entrypoint};
use quack_rs::prelude::Connection;

// {extension_name}: 全部小写，仅包含下划线。如果符号缺失或名称错误，DuckDB 将无法加载扩展。
// 必须与 Makefile 的 EXTENSION_NAME、产物文件名一致。
//
// {extension_name}: all lowercase, underscores only. If the symbol is missing or
// misnamed, DuckDB cannot load the extension. It must match `EXTENSION_NAME` in
// the Makefile and the resulting `.duckdb_extension` file name.
duckfn_entrypoint!("duckfn_quantstats");

/// 扩展加载时把内置翻译数据装进进程级翻译表。
///
/// 放在这里（`#[duck_custom_register]` 的注册回调里）而不是 `LazyLock::new` 里，是因为翻译功能的语义就是
/// **加载时初始化、重载后恢复**：`LOAD` 一次就装一次，进程重启重来一遍。而 `LazyLock` 只在第一次用到时
/// 初始化，那是另一回事（虽然结果常常一样，但「什么时候装」这件事值得写在明处）。
///
/// 校验也在这一步发生（`builtin::table`）：内置数据里出现不认识的 key、某个语言少翻译了一个 key，都会让
/// **扩展加载失败**并说明原因 —— 这类问题编译器看不见（字符串是数据），加载期是能最早发现的地方。
///
/// 注册项之间的顺序不影响这里：它不依赖任何别的注册结果。
///
/// Install the built-in translation data into the process-level table when the extension loads.
///
/// It lives here (in a `#[duck_custom_register]` callback) rather than in a `LazyLock::new` because the feature's
/// semantics *are* "initialised at load time, restored on reload": one `LOAD` installs it once, and a process
/// restart starts over. A `LazyLock` would initialise on first use instead, which is a different thing (even
/// though the outcome is usually the same, *when* it is installed is worth stating plainly).
///
/// Validation happens in this same step (`builtin::table`): an unknown key in the built-in data, or a language
/// missing one key, makes the **extension fail to load** with the reason spelled out — the compiler cannot see
/// these (the strings are data) and load time is the earliest place they can be caught. The order of the
/// registration items does not matter here: this one depends on no other registration.
#[duck_custom_register]
fn qs_install_builtin_translations(_connection: &Connection) -> DuckResult<()> {
    functions::translation::table::install_builtin()
}
