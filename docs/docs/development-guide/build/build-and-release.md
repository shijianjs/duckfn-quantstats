---
title: Build and release
sidebar_position: 1
description: The two build paths, the Justfile commands, the release flow that produces GitHub Release binaries, and the WebAssembly target.
---

# Build and release

## Two build paths

Both are kept in sync; use whichever is faster for what you are doing.

```shell
cargo duckdb-ext build   # fast loop, no make
# -> target/debug/duckfn_quantstats.duckdb_extension

make configure           # once: builds configure/venv (Python + the sqllogictest runner)
make debug               # the official template path, also what CI runs
# -> build/debug/extension/duckfn_quantstats/duckfn_quantstats.duckdb_extension
```

`make release` is the optimized version of the same flow. On Windows `make` has to run inside Git
Bash.

Either path lands a loadable extension:

```mermaid
flowchart LR
  A["src/<br/>Rust sources"] --> B["cargo duckdb-ext build"]
  A --> C["make debug<br/>the official path"]
  A --> H["just build_wasm"]
  B --> D["target/debug/<br/>duckfn_quantstats.duckdb_extension"]
  C --> E["build/debug/<br/>duckfn_quantstats.duckdb_extension"]
  H --> F["wasm32-unknown-emscripten<br/>staticlib"]
```

## Justfile

| Command | What it does |
| --- | --- |
| `just build` | `cargo duckdb-ext build` |
| `just sql "SELECT …"` | Build, then run one statement and exit |
| `just repl` | A DuckDB REPL with the extension loaded |
| `just lint` | `cargo clippy --all-targets -- -D warnings` |
| `just test` | The official build and the sqllogictest run |
| `just docs_csv` | Export the function descriptions to `target/function_descriptions.csv` |
| `just docs_build` / `just docs_start` | Build / serve this documentation site |
| `just release_*` | The release flow below |

## Release flow

A release is four steps, and only the second one is manual:

| Step | Command |
| --- | --- |
| 0. Pre-flight | `just release_check` (clippy + build); `just test` when the change warrants it |
| 1. Bump the version | `just release_bump {{EXTENSION_VERSION}}` |
| 2. Commit and tag | `git commit …` then `just release_tag {{EXTENSION_VERSION}}` |
| 3. Watch CI | `just release_ci`, then `gh run watch <run-id>` |
| 4. Next development version | `just release_dev 0.1.1-dev.0` |

The version lives in `Cargo.toml` (`[package] version`) and nowhere else:
`scripts/release.sh bump` rewrites that one line (only that line — this manifest also holds
`quantstats-rs = "<version>"`, which must not be touched), updates the occurrences in the docs and the
CI comments, syncs `Cargo.lock` and fixes `docs/extension-version.ts`, which is where the documentation
site gets the version number it prints. The tag has to match `Cargo.toml`, because cargo writes the
version into the built extension — a mismatch would publish a release claiming another version.

Only real releases get a tag. A version like `0.1.1-dev.0` stays on the branch: no tag, no release, no
site deployment.

### What a tag triggers

Pushing `v*.*.*` starts **Main Extension Distribution Pipeline** — it builds the extension for every
supported platform, runs the tests, then creates (or updates) a GitHub Release for that tag with the
built binaries attached as `<extension>-<arch>.duckdb_extension` (the wasm ones as
`.duckdb_extension.wasm`). Release notes are the commits since the previous version tag.

**Deploy Docs** is not started by the tag but by that pipeline *finishing*: it builds `docs/` and
publishes it to GitHub Pages (it waits for the release, which the site then preloads). It needs the
one-time *Settings → Pages → Source: GitHub Actions* setting.

Pull requests run the build and the tests only; publishing is gated on the ref being a version tag.

What a pushed version tag sets off:

```mermaid
flowchart LR
  tag["push a version tag"] --> pipe["extension pipeline:<br/>build every platform"]
  pipe --> rel["GitHub Release<br/>binaries per platform"]
  pipe --> docs["Deploy Docs<br/>triggered by the pipeline"]
  docs --> pages["GitHub Pages"]
  pr["open a pull request"] --> ci["pipeline only:<br/>build and tests,<br/>no release"]
```

### Installing a release

```sql
LOAD 'https://github.com/shijianjs/duckfn-quantstats/releases/latest/download/duckfn_quantstats-windows_amd64.duckdb_extension';
```

A locally built extension needs `duckdb -unsigned`, and so does a released file a user downloads,
because it is not signed by DuckDB's distribution key. That is what the
[community-extension](../publishing/community-extension.md) route fixes: once registered, `INSTALL … FROM community`
fetches a signed build for the user's platform.

## WebAssembly

```shell
just config_env   # once: pin the toolchain and add the wasm32-unknown-emscripten target
just build_wasm
```

The wasm build goes through `src/wasm_lib.rs`, a `staticlib` mirror of `src/lib.rs`. The two crate roots
must always declare the same set of `mod`s, and platform-specific dependencies belong under
`[target.'cfg(…'.dependencies]` in `Cargo.toml` so the wasm target does not pay for them (some crates
do not compile for emscripten at all). What the extension's wasm build does and does not do is in
[Output and browser](../../guide/output-and-browser.md) and [Design
notes](../architecture/design-notes.md).
