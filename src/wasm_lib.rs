#![cfg(target_arch = "wasm32")]
#![allow(special_module_name)]

/// 和 lib.rs 保持同一组 mod，理由见 src/lib.rs。
mod extension;

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
