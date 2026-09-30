---
title: Project structure
sidebar_position: 1
description: The module chain from the two crate roots to the registered functions, where the SQL-facing types live, and the naming rules that keep the extension loadable.
---

# Project structure

```text
src/lib.rs            native crate root  ->  mod extension;
src/wasm_lib.rs       wasm crate root    ->  mod extension;   (the same set of mods, mirrored)
src/extension/mod.rs  ->  duckfn_entrypoint!("duckfn_quantstats");
src/bin/duckfn.rs     duckfn CLI entry   ->  #[path] mod extension; + duckfn::cli::run(...)
                      (only serves `just docs_csv`, not part of the extension runtime)

src/extension/functions/mod.rs  ->  mod aggregate_html;
src/extension/functions/aggregate_html/
    mod.rs            the two SQL names / one signature each, and the mod list
    html_returns.rs   qs_html_reports            (the return branch)
    html_prices.rs    qs_html_reports_by_prices  (the price/NAV branch, differenced in the tail)
    kind.rs           one path's SQL-side name: the macro-generated `SQL_NAME`
    series.rs         the internal point type, series building, price differencing
    slots.rs          the argument slots: the symbol table plus "parse each symbol's options once"
    report.rs         the tail: render one report per (symbol, benchmark), persist, open the browser,
                      fill in the path
    naming.rs         the report file name: `<time>-<strategy>-<benchmark>[-<random>].html`
                      (persistence and the temporary file share the stem)
    storage.rs        persistence: pick a free name, write with std::fs (skipped entirely on wasm)
    browser.rs        opening in the system default browser (the whole feature is ignored on wasm)
src/extension/types/
    html_report_options.rs  the named STRUCT type `qs_html_report_options`
    html_report.rs          the result row type `QuantstatsHtmlReport` (no named type registered)

test/sql/quantstats/   SQLLogicTest files
demo/prices.csv        the committed market snapshot
scripts/release.sh     version bump, tag, development version
Justfile               the everyday commands
docs/                  this documentation site
community-extension/   the community-extension registration draft
```

## The two crate roots

`src/lib.rs` and `src/wasm_lib.rs` both declare exactly one module, `mod extension;`, and
`extension/mod.rs` attaches everything else. The official Rust template instead writes `mod lib;`
and forwards it a second time from the wasm root, which breaks as soon as modules nest
(`error[E0583]: file not found for module …`): there would be two copies of the same path set to keep
in sync.

Adding a module therefore means editing `extension/mod.rs` (and the `mod.rs` of the layer below),
never the crate roots.

## The command-line bin

`src/bin/duckfn.rs` compiles the extension a second time with `#[path = "../extension/mod.rs"] mod
extension;` and calls `duckfn::cli::run(...)`. It exists to export the function-description CSV
(`just docs_csv`) and takes no part in the extension itself.

The `#[path]` attribute is not a shortcut, it is necessary: the documentation metadata behind
`#[duck_*]` is collected by `inventory`'s static constructors, which only fire for object files that
are really linked into the final binary. With `use duckfn_quantstats::…` the linker may drop those
modules and the exported CSV comes out empty — silently. See
[Function descriptions](../publishing/function-descriptions.md).

## Naming rules

| Rule | Why |
| --- | --- |
| The extension name is `duckfn_quantstats`, and identical in five places. | It is the entry-point symbol and the artifact file name; DuckDB looks the symbol up by the file name. |
| Every registered SQL name carries the `qs_` prefix. | DuckDB has no namespaces, and community extensions almost never put the package name into function names — but a shared prefix is what makes the two names findable in `duckdb_functions()`. See the conventions in `AGENTS.md`. |
| `src/lib.rs` and `src/wasm_lib.rs` always declare the same set of `mod`s. | Otherwise the wasm build fails to compile the module tree. |
| `output_dir` takes a directory, never a file path. | The function names the files; with one instrument against several benchmarks a caller-built path would necessarily overwrite itself (see [Design notes](./design-notes.md)). |
| Temporary files (scripts, data, logs) go to `target/`. | `target/` is git-ignored and never pollutes the tracked tree. |
| Text files use LF. | The repository stores LF. |

## Where the development notes are

The design notes the docs site does not cover from the user's side — why the function groups by
symbol, how the benchmark pairing works, which dependency carries which part — live in
[Design notes](./design-notes.md) and [Dependencies](./dependencies.md).

duckfn's own conventions (the entry-point chain, the standard procedure for adding a function, which
source to consult before writing against the macros) are **not** repeated here: they live in the
repository's `AGENTS.md`.

Since duckfn 0.0.11 its documentation, plus a runnable example extension and its SQLLogicTest files,
ship **inside the crate package**, so they always match the version in `Cargo.toml` and need no clone
of the duckfn repository:

```shell
# after any build: the sources cargo actually compiled against
ls -d ~/.cargo/registry/src/*/duckfn-*/
```

| Path under that directory | What it is |
| --- | --- |
| `docs/docs/**` | The user guide's text (English), including the chapter per registration kind. |
| `docs/i18n/zh-Hans/…/current/**` | The same guide in Simplified Chinese. |
| `src/extension/**` | The example extension: one file per registration kind, plus custom types and a combined demo. |
| `test/sql/**` | SQLLogicTest files for the example, worth copying the structure of. |

The rendered guide is also online — [shijianjs.github.io/duckfn](https://shijianjs.github.io/duckfn/) —
but it may be newer than your dependency; the copy in the registry is what this project is compiled
against.
