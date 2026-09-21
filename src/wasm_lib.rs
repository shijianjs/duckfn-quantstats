#![cfg(target_arch = "wasm32")]
#![allow(special_module_name)]

/// 和 lib.rs 保持同一组 mod，理由见 src/lib.rs。
///
/// Keep the same set of `mod`s as `lib.rs`; see `src/lib.rs` for the reason.
mod extension;

// 构建 Wasm 目标需要 `staticlib` crate-type，与原生构建默认的 `cdylib` 不同，
// 而目前无法按 target 选择 crate-type。
//
// 本文件唯一的作用是把 lib 的内容以 example 的形式重新映射一份，请勿修改其内容。
//
// 显式构建 Wasm 目标：
//   cargo build --example duckfn_quantstats
//
// To build the Wasm target, a `staticlib` crate-type is required
//
// This is different than the default needed in native, and there is
// currently no way to select crate-type depending on target.
//
// This file sole purpose is remapping the content of lib as an
// example, do not change the content of the file.
//
// To build the Wasm target explicitly, use:
//   cargo build --example duckfn_quantstats
