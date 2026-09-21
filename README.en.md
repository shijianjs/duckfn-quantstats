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
| `duckfn_quantstats_html(date, period_return, options)` | Single-series report; one row per period. |
| `duckfn_quantstats_html_benchmark(date, period_return, benchmark, options)` | Benchmark report: the strategy side is aggregated row by row, the benchmark is a **list passed in once**. |

- The options argument always comes **last** (data columns first, options last). It is a **nullable** config
  whose type is the named STRUCT `duckfn_quantstats_html_options`, created at load time; `NULL` means "all
  defaults".
- `date` is a `DATE` and `period_return` is the return per period (`DOUBLE`). A row whose `date` or
  `period_return` is `NULL` is **skipped entirely**, like any other SQL aggregate.
- `benchmark` is `STRUCT(date DATE, period_return DOUBLE)[]`. A `NULL` benchmark, an empty one, or one without
  a single valid point is an **error** — the function is named after its benchmark, so without one
  `duckfn_quantstats_html` is the right call.
- Both `options` and `benchmark` are read through `DuckLazy`: every row only builds an O(1) token, and the
  single real parse happens on the **first row of each group**. This is not a nicety — duckfn's adapter reads
  arguments per row, so a bare `Vec<...>` would copy the whole benchmark series once per row, degrading to
  O(rows × benchmark length).
- A group without any valid row returns `NULL` (not an empty string, not an error).
- The report is rendered in `result()`, i.e. **once per group**. `GROUP BY` over 100 instruments renders 100
  full reports (each with a dozen inline SVGs); time and memory grow linearly with the number of groups, and
  every group also holds its own parsed copy of the benchmark points (aggregate states are not shared across
  groups, so that part cannot be avoided — what this design removes is DuckDB's row expansion and scanning).
- No `ORDER BY` is needed: the aggregate only concatenates and lets `ReturnSeries::new` sort by date.

### Why the benchmark is a list argument

Representing the benchmark as rows of the same long table would force the benchmark rows to appear once per
group: 100 instruments × 1000 days = 100k rows materialised and scanned, while the benchmark itself is only
1000 rows — plus a "which label is the benchmark" config key. With a list passed in once, the benchmark is
written once and evaluated once, and the strategy side still gets its grouping from `GROUP BY`.

The price is that the list has to be aggregated in a subquery first: `list(...)` is itself an aggregate and
**cannot be inlined into an aggregate call** (DuckDB reports `aggregate function calls cannot be nested`), so
it must be reduced to a single row and then cross-joined in.

### Config fields

Every field of `duckfn_quantstats_html_options` is **nullable**; keys you omit take their default:

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | Report title |
| `strategy_title` | `VARCHAR` | `'Strategy'` | Strategy display name |
| `benchmark_title` | `VARCHAR` | `NULL` | Benchmark display name (presentation only) |
| `rf` | `DOUBLE` | `0.0` | Risk-free rate, **annualized** (`0.04` = 4%), matching quantstats' `rf` convention |
| `periods_per_year` | `UINTEGER` | `252` | Periods per year; must be greater than 0 |
| `match_dates` | `BOOLEAN` | `true` | Whether to align the start dates of strategy and benchmark |
| `output` | `VARCHAR` | `NULL` | Also write the HTML to this path (ignored on wasm, see below) |

Defaults come straight from quantstats-rs' `HtmlReportOptions::default()`; this extension does not invent a
second set.

`rf` is **annualized** (`0.04` = 4%) and converted to a per-period rate inside the report; the crate has two
conversions that differ slightly — Sharpe (and rolling Sharpe / Sortino) uses
`(1 + rf)^(1/periods_per_year) - 1`, while PSR / Sortino in the metrics table use `rf / periods_per_year`.
Both collapse to 0 when `rf = 0` (the default).

### Usage

```sql
-- Single series, all-default options
SELECT duckfn_quantstats_html(trade_date, daily_return, NULL) FROM daily_returns;

-- Single series, only the keys you care about; a struct literal must be cast to the options type
SELECT symbol,
       duckfn_quantstats_html(
           trade_date, daily_return,
           {'title': 'My Fund', 'rf': 0.02}::duckfn_quantstats_html_options) AS html
FROM daily_returns
GROUP BY symbol;

-- With a benchmark: it is aggregated into a single row once, then cross-joined in
WITH benchmark AS (
    SELECT list({'date': trade_date, 'period_return': daily_return}) AS series
    FROM benchmark_returns
)
SELECT s.fund,
       duckfn_quantstats_html_benchmark(
           s.trade_date, s.daily_return, benchmark.series,
           {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::duckfn_quantstats_html_options) AS html
FROM strategy_returns s, benchmark
GROUP BY s.fund;

-- Also write the report to a file
SELECT duckfn_quantstats_html(
           trade_date, daily_return,
           {'title': 'My Fund', 'output': 'fund.html'}::duckfn_quantstats_html_options)
FROM daily_returns;
```

A scalar subquery works just as well as the cross join (verified), with the same effect:

```sql
SELECT fund,
       duckfn_quantstats_html_benchmark(
           trade_date, daily_return,
           (SELECT list({'date': trade_date, 'period_return': daily_return}) FROM benchmark_returns),
           NULL)
FROM strategy_returns
GROUP BY fund;
```

### Three behaviours worth knowing

- **A struct literal must be cast with `::duckfn_quantstats_html_options`.** Without it the literal is an
  anonymous `STRUCT(title VARCHAR)` whose field count differs from the options type, and DuckDB reports that
  no function matches — registering that named type is exactly what makes the cast possible.
- **`'...'::JSON::duckfn_quantstats_html_options` must spell out all 7 keys** (DuckDB's JSON→STRUCT
  conversion rejects missing keys), so prefer the struct literal.
- **The benchmark point keys are fixed to `date` / `period_return`** (duckfn's `DuckStruct` derive has no
  field renaming). They match the anonymous `STRUCT(date DATE, period_return DOUBLE)` exactly, so **no cast is
  needed**; only when the source columns are not `DATE` / `DOUBLE` do you add one:
  `{'date': trade_date::DATE, 'period_return': daily_return::DOUBLE}`.

### Error paths

| Situation | Behaviour |
| --- | --- |
| The group has no valid row | `NULL` |
| The benchmark argument is `NULL` | Error `the benchmark list must not be NULL` |
| The benchmark is an empty list, or holds no valid point | Error `the benchmark list is empty` |
| The benchmark list contains a whole-NULL element | Error `cannot read the benchmark list` |
| `periods_per_year = 0` | Error `periods_per_year must be greater than 0` |
| `output = ''` | Error `output must not be an empty string` |

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
make debug && make test    # make test does not rebuild; run make debug after editing Rust
```

They come in two kinds, and the split is deliberate:

| File | Covers | Extra dependency |
| --- | --- | --- |
| `test/sql/quantstats/html_report.test`, `html_report_benchmark.test` | **Behaviour**: option parsing and defaults, `NULL` rows skipped, empty input → `NULL`, multi-threaded `combine` consistency (single vs 4 threads, md5-equal), error paths | none |
| `test/sql/quantstats/html_report_values.test` | **Output content**: parses the generated HTML with [webbed](https://duckdb.org/community_extensions/extensions/webbed)'s XPath and asserts the title, the date range, the `rf` echo, per-row metric numbers, the chart/table counts and the extra benchmark column | the `webbed` community extension |

`webbed` is installed from inside the test file (`INSTALL webbed FROM community;`), which needs **network on the
first run** and then goes through the local DuckDB extension cache. Delete that file if you do not want the
dependency; the other two are unaffected.

Asserting on long HTML via XPath beats `length(...) > N` and localises failures far better than an md5
comparison:

```sql
-- Report title and date range
SELECT html_extract_text(html, '//h1')[1] FROM report;
-- -> My Fund 2 Jan, 2024 - 12 Jan, 2024

-- The strategy column of one row of the metrics table (benchmark column comes first)
SELECT html_extract_text(html, '//div[@id="right"]/table[1]//tr[td[1]="Sharpe"]/td[2]')[1] FROM report;
-- -> 4.93
```

Note that webbed's `html_extract_text(html, xpath)` returns `VARCHAR[]` (**all** matches): index with `[1]`
for a single value, or use `array_to_string(..., ' | ')` / `array_length(...)` to assert a whole group.

When adding a function, cover at least: normal values, `NULL`, boundary values and error paths
(`statement error`).
