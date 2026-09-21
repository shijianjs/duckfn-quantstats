// 子模块只在 `extension/` 树下挂：src/lib.rs 与 src/wasm_lib.rs 都只写 `mod extension;`，
// 新增模块时不要动那两个 crate root，改这里即可。
//
// Submodules hang off the `extension/` subtree only: both src/lib.rs and src/wasm_lib.rs just declare
// `mod extension;`, so new modules are attached here instead of in the crate roots.
mod functions;
mod types;

use duckfn::duckfn_entrypoint;

// {extension_name}: 全部小写，仅包含下划线。如果符号缺失或名称错误，DuckDB 将无法加载扩展。
// 必须与 Makefile 的 EXTENSION_NAME、产物文件名一致。
//
// {extension_name}: all lowercase, underscores only. If the symbol is missing or
// misnamed, DuckDB cannot load the extension. It must match `EXTENSION_NAME` in
// the Makefile and the resulting `.duckdb_extension` file name.
duckfn_entrypoint!("duckfn_quantstats");
