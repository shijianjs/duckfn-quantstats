[English](README.md) | [简体中文](README.zh.md)

# duckfn_quantstats

A DuckDB extension (loadable extension) written with [duckfn](https://crates.io/crates/duckfn):
produce quantstats reports directly from SQL.

The project started from DuckDB's official
[extension-template-rs](https://github.com/duckdb/extension-template-rs) and has been reshaped to follow
duckfn's skeleton conventions (entry module, `EXTENSION_NAME`, dependency list).

**Requires DuckDB 1.5 or newer.** The host file system used to write the report (`output`) only reached
DuckDB's C API in 1.5, so there is no 1.4 compatibility path; the extension is built and tested against
v1.5.5.

## Functions

Two aggregate function names, each with **two overloads** (dispatched by argument count), folding a
date-ordered series into one complete quantstats HTML report (`VARCHAR`):

| Signature | Input | Description |
| --- | --- | --- |
| `duckfn_quantstats_html(date, period_return, options)` | return series | Single-series report; one row per period. |
| `duckfn_quantstats_html(date, period_return, benchmark, options)` | return series | Benchmark report; the benchmark is a **list passed in once**. |
| `duckfn_quantstats_html_prices(date, price, options)` | price series | Single-series report; returns are derived inside the function. |
| `duckfn_quantstats_html_prices(date, price, benchmark, options)` | price series | Benchmark report; both sides are prices. |

The first pair is registered as one function set via `overloads_name` and the second pair as another, so SQL
sees exactly two names.

- The options argument always comes **last** (data columns first, options last). It is a **nullable** config
  whose type is the named STRUCT `duckfn_quantstats_html_options`, created at load time; `NULL` means "all
  defaults".
- `date` is a `DATE`, `period_return` is the return per period (`DOUBLE`) and `price` is that day's price or
  NAV (`DOUBLE`). A row whose `date` or value is `NULL` is **skipped entirely**, like any other SQL
  aggregate.
- `benchmark` is `STRUCT(date DATE, <value field> DOUBLE)[]` (`period_return` or `price`). A `NULL` benchmark,
  an empty one, or one without a single valid point is an **error** — that overload exists for the benchmark
  case, so a single-series report should simply omit the argument.
- The price branch **needs its own name**: `(date, price, options)` and `(date, period_return, options)` have
  exactly the same type sequence (`DATE, DOUBLE, STRUCT`), so one name could not dispatch them.
- Both `options` and `benchmark` are read through `DuckLazy`: every row only builds an O(1) token, and the
  single real parse happens on the **first row of each group**. This is not a nicety — duckfn's adapter reads
  arguments per row, so a bare `Vec<...>` would copy the whole benchmark series once per row, degrading to
  O(rows × benchmark length). The parsed values are cached in the aggregate state through duckfn's
  `DuckLazySlot`, which exists for exactly this shape: a `DuckLazy` token is only valid inside the callback
  that produced it, so the state can hold the parse result and nothing else.
- A group without any valid row returns `NULL` (not an empty string, not an error).
- The report is rendered in `result()`, i.e. **once per group**. `GROUP BY` over 100 instruments renders 100
  full reports (each with a dozen inline SVGs); time and memory grow linearly with the number of groups, and
  every group also holds its own parsed copy of the benchmark points (aggregate states are not shared across
  groups, so that part cannot be avoided — what this design removes is DuckDB's row expansion and scanning).
- No `ORDER BY` is needed: the aggregate only concatenates and lets `ReturnSeries::new` sort by date.

### Price (or NAV) series

`duckfn_quantstats_html_prices` takes **prices** — NAVs count too — not percentage changes. The value column
is called `price`, borrowing Python quantstats' vocabulary: it lumps this kind of input under "prices" and
runs `pct_change` on anything that looks like a price series. **A NAV is not strictly a price**, but
quantstats does not distinguish either, so one key takes in all of these level values and users never have to
wonder which one to fill. The conversion rules:

- the points of a group are sorted by `date`, then each becomes `price_t / price_{t-1} - 1`;
- the first point of a group has no predecessor and is dropped;
- a point whose predecessor is missing, zero or not finite is **skipped** (the same "skip it" semantics as a
  NULL row: no error and no `inf`/`NaN` inside the series); skipping affects only that point, the next one is
  still compared with its own predecessor;
- if nothing survives the differencing the result is `NULL` (e.g. a group with a single point);
- a group should hold one point per date, otherwise the difference describes the movement within that date.

Writing the equivalent in SQL is noticeably clumsier: a window function **cannot** appear inside an aggregate
call (DuckDB reports `aggregate function calls cannot contain window function calls`), so the returns have to
be computed in a subquery first:

```sql
-- By hand: an extra subquery, and it is easy to get PARTITION BY / ORDER BY wrong
SELECT fund, duckfn_quantstats_html(trade_date, period_return, NULL) AS html
FROM (
    SELECT fund, trade_date,
           nav / lag(nav) OVER (PARTITION BY fund ORDER BY trade_date) - 1.0 AS period_return
    FROM nav_table
)
GROUP BY fund;

-- With the shortcut: prices go straight in, grouping is plain GROUP BY
SELECT fund, duckfn_quantstats_html_prices(trade_date, nav, NULL) AS html
FROM nav_table
GROUP BY fund;
```

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
| `output` | `VARCHAR` | `NULL` | Also write the HTML to this path, through DuckDB's VFS (see below) |

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
       duckfn_quantstats_html(
           s.trade_date, s.daily_return, benchmark.series,
           {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::duckfn_quantstats_html_options) AS html
FROM strategy_returns s, benchmark
GROUP BY s.fund;

-- A price (or NAV) series: the shortcut saves writing the pct_change window
SELECT fund,
       duckfn_quantstats_html_prices(
           trade_date, nav, {'title': 'My Fund'}::duckfn_quantstats_html_options) AS html
FROM nav_table
GROUP BY fund;

-- Also write the report to a file (through DuckDB's VFS, so this works on wasm too)
SELECT duckfn_quantstats_html(
           trade_date, daily_return,
           {'title': 'My Fund', 'output': 'fund.html'}::duckfn_quantstats_html_options)
FROM daily_returns;
```

A scalar subquery works just as well as the cross join (verified), with the same effect:

```sql
SELECT fund,
       duckfn_quantstats_html(
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
| `duckfn_quantstats_html_prices`: fewer than two benchmark prices, so no return can be derived | Error `the benchmark prices produced no returns` |
| `periods_per_year = 0` | Error `periods_per_year must be greater than 0` |
| `output = ''` | Error `output must not be an empty string` |
| The `output` path contains a NUL byte | Error `contains a NUL byte` |
| The `output` path cannot be written (missing directory, unwritable remote, …) | Error from `duckfn::duck_vfs::write` naming the path |

### Writing the report to a file

`output` writes the rendered HTML with duckfn's convenience layer `duck_vfs::write_string`, i.e. through
**DuckDB's VFS** rather than `std::fs`:

- local disk, in-memory file systems, whatever file system the wasm build exposes, and `s3://` /
  `http(s)://` once `httpfs` is loaded all go through the same path with the same semantics;
- it is also the only way an aggregate can write at all. DuckDB's C API gives aggregate functions no client
  context (no bind callback, no `duckdb_aggregate_function_get_client_context`), so duckfn keeps an owned
  long-lived connection from registration time and hands out a fresh `ClientContext` → `FileSystem` from it.

`output` **replaces** the target: afterwards the file holds exactly the report, even when it previously held
something longer. That is duckfn's job — DuckDB's C API has no truncate (`DUCKDB_FILE_FLAG_CREATE` only means
"create if needed", and the flag that maps to `O_TRUNC` / `CREATE_ALWAYS` lives on the C++ side), so duckfn's
`duck_vfs` layer zeroes a longer file before writing; this extension just calls `duck_vfs::write_string`.
`read_text()` therefore returns exactly what the function returned — the test suite pins that with `md5`,
including the "existing file is longer" case.

The path is part of the configuration, so under `GROUP BY` give each group its own file
(`'report-' || symbol || '.html'`) instead of pointing every group at one path.

### WebAssembly

Nothing in the code is wasm-specific any more: `output` goes through DuckDB's VFS, so the wasm build uses
exactly the same code path as the native one. That replaces the earlier behaviour, where the path was dropped
on `wasm32-unknown-emscripten` because `std::fs` has no writable file system there — the file now lands
wherever DuckDB's own file system points in that environment, without this extension special-casing anything.
`just build_wasm` (`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`)
compiles fine; the runtime behaviour is DuckDB's VFS's, not ours.

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

- [duckfn](https://crates.io/crates/duckfn): attribute macros that register ordinary Rust functions with
  DuckDB. Its `duckdb-1-5` feature is enabled, which is what provides `DuckLazySlot`'s sibling — the host
  file system (`duckfn::duck_vfs`) used by `output`.
- [quack-rs](https://crates.io/crates/quack-rs): DuckDB C API bindings; the code expanded from
  `duckfn_entrypoint!` refers to it directly.
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys): headers only, with `loadable-extension` enabled —
  so **no local DuckDB build is required**. The version floor is `>= 1.10500` (DuckDB 1.5.0: the crate
  encodes a DuckDB version as `1.<major*10000 + minor*100 + patch>.0`, so 1.5.5 is `1.10505.0`), because the
  client-context / file-system part of the C API is 1.5-only.
- [quantstats-rs](https://crates.io/crates/quantstats-rs): the report itself. Its public API exposes only
  `html()` as a callable entry point (`mod stats` is private, so `compute_performance_metrics` is unreachable),
  so both overloads are built on it instead of recomputing metrics — that would create a second source of
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
| `test/sql/quantstats/html_report.test`, `html_report_benchmark.test`, `html_report_prices.test` | **Behaviour**: option parsing and defaults, `NULL` rows skipped, empty input → `NULL`, multi-threaded `combine` consistency (single vs 4 threads, md5-equal), the price path byte-identical to `lag()`-derived returns, error paths | none |
| `test/sql/quantstats/html_report_values.test` | **Output content**: parses the generated HTML with [webbed](https://duckdb.org/community_extensions/extensions/webbed)'s XPath and asserts the title, the date range, the `rf` echo, per-row metric numbers, the chart/table counts and the extra benchmark column | the `webbed` community extension |

`webbed` is installed from inside the test file (`INSTALL webbed FROM community;`), which needs **network on the
first run** and then goes through the local DuckDB extension cache. Delete that file if you do not want the
dependency; the other files are unaffected.

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
