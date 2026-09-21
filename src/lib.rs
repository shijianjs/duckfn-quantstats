/// 和 wasm_lib 保持同一组 mod，解决官方模板 `mod lib;` 方式在嵌套模块时
/// 路径不一致、无法嵌套的问题：
/// error[E0583]: file not found for module functions --> src\lib.rs:3:1
/// 这里让两个 crate root 都只挂 `mod extension;`，再由 `extension/mod.rs` 往下挂子模块。
mod extension;
