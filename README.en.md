[English](README.en.md) | [简体中文](README.md)

# duckfn_quantstats

A DuckDB extension (loadable extension) written with [duckfn](https://crates.io/crates/duckfn):
produce quantstats reports directly from SQL.

The project started from DuckDB's official
[extension-template-rs](https://github.com/duckdb/extension-template-rs) and has been reshaped to follow
duckfn's skeleton conventions (entry module, `EXTENSION_NAME`, dependency list).

## Entry-point chain

```text
src/lib.rs           ->  mod extension;
src/wasm_lib.rs      ->  mod extension;   (same set of mods, mirrored)
src/extension/mod.rs ->  duckfn_entrypoint!("duckfn_quantstats");
```

The extension name `duckfn_quantstats` must match `EXTENSION_NAME` in the `Makefile` and the artifact file name.
`src/lib.rs` and `src/wasm_lib.rs` must declare the same set of `mod`s (the official template's `mod lib;` style
fails with `E0583` once modules are nested). Add new functionality under `src/extension/`, following duckfn's
module layout.

## Dependencies

- [duckfn](https://crates.io/crates/duckfn): attribute macros that register ordinary Rust functions with DuckDB.
- [quack-rs](https://crates.io/crates/quack-rs): DuckDB C API bindings; the code expanded from
  `duckfn_entrypoint!` refers to it directly.
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys): headers only, with `loadable-extension` enabled —
  so **no local DuckDB build is required**.

## Building

For day-to-day iteration use `cargo-duckdb-ext-tools` (a global cargo subcommand that adds no dependency to
the project):

```shell
cargo install cargo-duckdb-ext-tools   # once
cargo duckdb-ext build                 # -> target/debug/duckfn_quantstats.duckdb_extension
```

The official template's `make` flow is kept as well (CI and sqllogictest use it); run `make configure` once to
create the Python venv it needs:

```shell
make configure   # once
make debug       # -> build/debug/extension/duckfn_quantstats/duckfn_quantstats.duckdb_extension
```

`make release` is the same flow with optimizations. On Windows, `make` must run in Git Bash.

## Loading

The extension is built against DuckDB's unstable C API, so `-unsigned` is required:

```shell
duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

## Testing

Tests are written in the SQLLogicTest format under `test/sql/`:

```shell
make test_debug     # or make test_release
```

When adding a function, cover at least: normal values, `NULL`, boundary values and error paths
(`statement error`).
