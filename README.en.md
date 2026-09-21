[English](README.en.md) | [简体中文](README.md)

# duckfn_quantstats

A DuckDB extension (loadable extension) written with [duckfn](https://crates.io/crates/duckfn):
produce quantstats reports directly from SQL.

The project started from DuckDB's official
[extension-template-rs](https://github.com/duckdb/extension-template-rs) and has been reshaped to follow
duckfn's skeleton conventions (entry module, `EXTENSION_NAME`, dependency list).

## Functions

Two aggregate functions, both folding a date-ordered return series into one complete quantstats HTML
report (`VARCHAR`):

| Function | Description |
| --- | --- |
| `duckfn_quantstats_html(opt, dt, ret)` | Single-series report; one row per day. |
| `duckfn_quantstats_html_benchmark(opt, name, dt, ret)` | Long-table report: `name` is a label; rows whose label equals `opt.benchmark_name` are the benchmark and the rest are the strategy. |

- `opt` is a **nullable** config whose type is the named STRUCT `duckfn_quantstats_html_options`, created at
  load time; `NULL` means "all defaults".
- `dt` is a `DATE` and `ret` is the return per period (`DOUBLE`). A row whose `dt` or `ret` is `NULL` is
  **skipped entirely**, like any other SQL aggregate.
- A group without any valid row returns `NULL` (not an empty string, not an error).
- The report is rendered in `result()`, i.e. **once per group**. `GROUP BY` over 100 instruments renders 100
  full reports (each with a dozen inline SVGs); time and memory grow linearly with the number of groups.
- No `ORDER BY` is needed: the aggregate only concatenates and lets `ReturnSeries::new` sort by date.

### Config fields

Every field of `duckfn_quantstats_html_options` is **nullable**; keys you omit take their default:

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | Report title |
| `strategy_title` | `VARCHAR` | `'Strategy'` | Strategy display name |
| `benchmark_title` | `VARCHAR` | `NULL` | Benchmark display name (presentation only) |
| `benchmark_name` | `VARCHAR` | `NULL` | The label marking benchmark rows; read by `duckfn_quantstats_html_benchmark` only |
| `rf` | `DOUBLE` | `0.0` | Risk-free rate per period (not annualized) |
| `periods_per_year` | `UINTEGER` | `252` | Periods per year; must be greater than 0 |
| `match_dates` | `BOOLEAN` | `true` | Whether to align the start dates of strategy and benchmark |
| `output` | `VARCHAR` | `NULL` | Also write the HTML to this path (ignored on wasm, see below) |

Defaults come straight from quantstats-rs' `HtmlReportOptions::default()`; this extension does not invent a
second set.

### Usage

```sql
-- All-default config
SELECT duckfn_quantstats_html(NULL, dt, ret) FROM daily_returns;

-- Only the keys you care about; a struct literal must be cast to the config type explicitly
SELECT symbol,
       duckfn_quantstats_html(
           {'title': 'My Fund', 'rf': 0.02}::duckfn_quantstats_html_options, dt, ret) AS html
FROM daily_returns
GROUP BY symbol;

-- With a benchmark: a long table (label + date + return), 'SPY' being the benchmark
SELECT fund,
       duckfn_quantstats_html_benchmark(
           {'title': 'My Fund', 'benchmark_name': 'SPY', 'benchmark_title': 'S&P 500'}::duckfn_quantstats_html_options,
           name, dt, ret) AS html
FROM returns
GROUP BY fund;

-- Also write the report to a file
SELECT duckfn_quantstats_html(
           {'title': 'My Fund', 'output': 'fund.html'}::duckfn_quantstats_html_options, dt, ret)
FROM daily_returns;
```

### Two behaviours worth knowing

- **A struct literal must be cast with `::duckfn_quantstats_html_options`.** Without it the literal is an
  anonymous `STRUCT(title VARCHAR)` whose field count differs from the config type, and DuckDB reports that
  no function matches — registering that named type is exactly what makes the cast possible.
- **`'...'::JSON::duckfn_quantstats_html_options` must spell out all 8 keys** (DuckDB's JSON→STRUCT
  conversion rejects missing keys), so prefer the struct literal.

### Error paths

| Situation | Behaviour |
| --- | --- |
| The group has no valid row | `NULL` |
| `periods_per_year = 0` | Error `periods_per_year must be greater than 0` |
| `output = ''` | Error `output must not be an empty string` |
| `duckfn_quantstats_html_benchmark` without `benchmark_name` | Error `'benchmark_name' is required` |
| `benchmark_name` matches no row in the group | Error `no row matches benchmark_name = '...'` |
| The non-benchmark rows carry several distinct labels | Error `distinct strategy labels`, suggesting `GROUP BY` |
| The group holds only benchmark rows | Error `only rows labelled '...'`: no strategy to report on |

### WebAssembly

`output` is **ignored** on `wasm32-unknown-emscripten`: there is no writable file system there, so writing at
runtime would raise an IO error and kill the query — the extension simply does not hand the path to
quantstats-rs and returns the HTML as usual. `just build_wasm`
(`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`) compiles fine.

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
- [quantstats-rs](https://crates.io/crates/quantstats-rs): the report itself. Its public API exposes only
  `html()` as a callable entry point (`mod stats` is private, so `compute_performance_metrics` is unreachable),
  so both aggregates are built on it instead of recomputing metrics — that would create a second source of
  truth for numbers the report already prints.
- [chrono](https://crates.io/crates/chrono): `ReturnSeries` wants `NaiveDate`, while duckfn's `DuckDate` only
  stores days since 1970-01-01, so the conversion lives in the extension.

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
