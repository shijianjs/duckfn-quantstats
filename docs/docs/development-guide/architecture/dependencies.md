---
title: Dependencies
sidebar_position: 3
description: Which crate carries which part of the extension, and why each one was chosen.
---

# Dependencies

The general rule is the repository's: prefer a mature crate (or a std API) over hand-rolled code,
and write down why a dependency is there. Every entry below has a comment in `Cargo.toml` saying what
it is responsible for.

- [duckfn](https://crates.io/crates/duckfn): attribute macros that register ordinary Rust functions
  with DuckDB. The dependency turns on `all` (duckfn's common features at once) and deliberately **not**
  `owned-connection` — that one is what brings in `duck_vfs`, duckfn's host file system, and nothing
  here needs it: persistence is `std::fs` on a native build and skipped altogether on wasm (see
  [Design notes](./design-notes.md)). What this extension uses is:
  - `chrono`, which converts the time wrapper types (`DuckDate::to_naive_date` and friends);
  - `DuckLazySlot<T>`, which turns "parse the DuckLazy argument once" into a type;
  - `cli`, the command-line tool behind `src/bin/duckfn.rs` (it pulls clap and csv into duckfn).
  - The macros also generate a `SQL_NAME` constant per signature — the name the function is really
    registered under — so error prefixes read that instead of a hand-written copy of the
    function-name literal, while `description` / `comment` / `example` on the attribute are the one
    source of the function-description CSV (see [Function descriptions](../publishing/function-descriptions.md)).
- [quack-rs](https://crates.io/crates/quack-rs): DuckDB C API bindings; the code expanded from
  `duckfn_entrypoint!` refers to it directly.
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys): headers only, with `loadable-extension`
  enabled — so **no local DuckDB build is required**. The version floor is `>= 1.10500` (DuckDB 1.5.0:
  the crate encodes a DuckDB version as `1.<major*10000 + minor*100 + patch>.0`, so 1.5.5 is
  `1.10505.0`), because `all` switches on duckfn's `duckdb-1-5` feature — the unstable region of the C
  API (scalar bind/init, copy functions, the client context), whose slots only exist in 1.5.x.
  Persistence no longer has a stake in that floor; the dependency tree still turns the feature on.
- [quantstats-rs](https://crates.io/crates/quantstats-rs): the report itself. Its public API exposes
  only `html()` as a callable entry point (`mod stats` is private, so `compute_performance_metrics` is
  unreachable), so both paths are built on it instead of recomputing metrics — that would create a
  second source of truth for numbers the report already prints.
- [chrono](https://crates.io/crates/chrono): used directly for **local time** — report file names
  (shared by persistence and the temporary file) start with a `%Y%m%d-%H%M%S` stamp (`chrono::Local`,
  see naming.rs). The date side is duckfn's `chrono` feature (`DuckDate::to_naive_date`), whose
  `NaiveDate` is exactly what quantstats-rs' `ReturnSeries::new` takes; all three share one chrono 0.4.
- [sanitize-filename](https://crates.io/crates/sanitize-filename) and
  [fastrand](https://crates.io/crates/fastrand): the two halves of a report file name — which parts
  are legal (illegal and control characters, Windows reserved device names, trailing dots and spaces)
  and the random suffix. Both stay **shared** dependencies rather than moving to the non-wasm table:
  `naming.rs` is compiled for every target, and keeping that module free of `cfg`s is worth more than
  dropping two small crates from a wasm build that names no files anyway (nothing calls it there).
  fastrand is already in the tree (tempfile uses it internally), so a direct dependency costs no extra
  compilation.
- [lol_html](https://crates.io/crates/lol_html): the translation feature's one HTML rewrite
  ([Translation](../../guide/translation.md)). It targets CSS selectors, streams, and can rewrite text
  and markup as it goes — which is what "only the positions in the catalog, not one character anywhere
  else" needs. Hand-rolled string replacement cannot do that, and hand-writing an HTML parser for it
  would not be worth it. Its version is **pinned exactly**: 3.x uses let-chains, which need Rust 1.88
  while this project pins 1.86 (the `rust-version = "1.85"` 3.x declares is an upstream mistake), so
  2.7.0 is the last release that compiles here.
- [open](https://crates.io/crates/open) and [tempfile](https://crates.io/crates/tempfile):
  `open_in_browser`'s two jobs — starting the browser and, without `output_dir`, creating a uniquely
  named temporary file. **Non-wasm targets only** (see [Design notes](./design-notes.md)), which is why
  they live in a target-specific dependency table instead of the main one.
