[English](DEVELOPMENT.md) | [简体中文](DEVELOPMENT.zh.md)

# duckfn_quantstats — development notes

The user guide is [README.md](README.md): the SQL surface, the configuration fields, the error paths and
the quick start live there. This file holds what users do not need — how the code is laid out, why it is
shaped the way it is, which crate carries which part, and how it is built and tested.

duckfn's own conventions (entry-point chain, the standard procedure for adding a function, which source to
consult before writing against the macros) are **not** repeated here: they live in [AGENTS.md](AGENTS.md),
which points at `templates/duckfn-conventions.md` in a duckfn clone.

The project started from DuckDB's official
[extension-template-rs](https://github.com/duckdb/extension-template-rs) and has been reshaped to follow
duckfn's skeleton conventions (entry module, `EXTENSION_NAME`, dependency list).

## Layout

```text
src/lib.rs            native crate root  ->  mod extension;
src/wasm_lib.rs       wasm crate root    ->  mod extension;    (the same set of mods, mirrored)
src/extension/mod.rs  ->  duckfn_entrypoint!("duckfn_quantstats");

src/extension/functions/mod.rs  ->  mod aggregate_html;
src/extension/functions/aggregate_html/
    mod.rs            the two SQL names / four overloads, and the mod list
    html_returns.rs   qs_html_report            (single series, with a benchmark)
    html_prices.rs    qs_html_report_by_prices  (single series, with a benchmark)
    kind.rs           one branch's SQL-side names: the macro-generated `SQL_NAME` plus the value field
    series.rs         the internal point type, series building, price differencing
    slots.rs          the argument slots: how DuckLazySlot is used, plus the benchmark errors/normalisation
    report.rs         the tail: points -> ReturnSeries -> HTML, persistence, opening the browser
    browser.rs        opening in the system default browser (the whole feature is ignored on wasm)
src/extension/types/
    html_report_options.rs  the named STRUCT type `qs_html_report_options`
    return_point.rs         one point of the return-side benchmark list
    price_point.rs          one point of the price-side benchmark list
```

The extension name `duckfn_quantstats` must match `EXTENSION_NAME` in the `Makefile` and the artifact file
name. `src/lib.rs` and `src/wasm_lib.rs` must declare the same set of `mod`s (the official template's
`mod lib;` style fails with `E0583` once modules are nested). Add new functionality under `src/extension/`,
following duckfn's module layout.

## Design notes

### Two SQL names, four overloads

Each name carries two signatures that differ only by the `benchmark` argument, so `overloads_name`
registers each pair as **one function set**, dispatched by argument count
(`register_all_aggregate_overload` groups by name and every overload keeps its own parameter list and
return type). SQL therefore sees exactly two names, and the argument order is always "data columns first,
options last".

The price branch **needs its own name**: `(date, price, options)` and `(date, period_return, options)` have
exactly the same type sequence (`DATE, DOUBLE, STRUCT`), so one name could not dispatch them.

The registered name is never written by hand: the attribute macro generates a `SQL_NAME` constant per
signature (the function-set name when `overloads_name` is set), and error prefixes read it — `kind.rs`
points at it instead of repeating the literal. The price of that is that such functions have to be
`pub(super)`, because the generated module inherits the function's visibility.

### The argument slots are lazy

Both `options` and `benchmark` are read through `DuckLazy`: every row only builds an O(1) token, and the
single real parse happens on the **first row of each group**. This is not a nicety — duckfn's adapter reads
arguments per row, so a bare `Vec<...>` would copy the whole benchmark series once per row, degrading to
O(rows × benchmark length). The parsed values are cached in the aggregate state through duckfn's
`DuckLazySlot`, which exists for exactly this shape: a `DuckLazy` token is only valid inside the callback
that produced it, so the state can hold the parse result and nothing else.

### One report per group

The report is rendered in `result()`, i.e. **once per group**. `GROUP BY` over 100 instruments renders 100
full reports (each with a dozen inline SVGs); time and memory grow linearly with the number of groups, and
every group also holds its own parsed copy of the benchmark points (aggregate states are not shared across
groups, so that part cannot be avoided — what this design removes is DuckDB's row expansion and scanning).

No `ORDER BY` is needed: the aggregate only concatenates and lets `ReturnSeries::new` sort by date.

### Why the benchmark is a list argument

Representing the benchmark as rows of the same long table would force the benchmark rows to appear once per
group: 100 instruments × 1000 days = 100k rows materialised and scanned, while the benchmark itself is only
1000 rows — plus a "which label is the benchmark" config key. With a list passed in once, the benchmark is
written once and evaluated once, and the strategy side still gets its grouping from `GROUP BY`.

The price is that the list has to be aggregated in a subquery first: `list(...)` is itself an aggregate and
**cannot be inlined into an aggregate call** (DuckDB reports `aggregate function calls cannot be nested`), so
it must be reduced to a single row and then cross-joined in.

### The benchmark point type registers no named type

`return_point.rs` / `price_point.rs` deliberately leave `create_type` off: the anonymous
`STRUCT(date DATE, period_return DOUBLE)[]` produced by `list({'date': ..., 'period_return': ...})` has
exactly the same field names, order and types, so it matches directly — the caller writes no cast, and the
SQL surface only gains one type (the options one). Verified: without `create_type`,
`#[derive(DuckStruct)]` submits no registration item.

The field names are fixed to `date` / `period_return` (`price` on the price side): duckfn's `DuckStruct`
derive has no field-level renaming, the SQL field name is the Rust field name verbatim. They also avoid SQL
keywords — `return` renders fine but `returns` and `value` get quoted by DuckDB, while `period_return` never
needs quotes.

The fields are `Option<...>` so that an element missing its date or its value reads as `None` and the
aggregate skips it — the same semantics as the strategy side, where a row with a NULL `date` or value is
skipped entirely. `Option<T>` has the same logical type as `T`, so the type shape is unchanged.

### The options type

`#[duck(create_type = true)]` makes duckfn run
`CREATE TYPE IF NOT EXISTS "qs_html_report_options" AS STRUCT(...)` at load time, which is what makes
`{'title': 'x'}::qs_html_report_options` (and the JSON form) possible in SQL.

Every field is an `Option<T>` on purpose: DuckDB fills the missing keys of a struct literal with NULL, and
duckfn turns the **whole struct** into NULL when a non-Option field reads NULL — so a user's `{'rf': 0.1}`
would silently fall back to all defaults and their `rf` would be dropped.

`to_report_options()` converts by starting from quantstats-rs' `HtmlReportOptions::default()` and overriding
only the fields the user actually wrote, so the defaults have a single source of truth. `output` is
deliberately not forwarded: quantstats-rs writes with `std::fs`, while this extension wants DuckDB's VFS
(see below) — the path is handled by `report.rs` after the report has been rendered.

`periods_per_year = 0` and `output = ''` are configuration errors and are reported here rather than turning
into a broken file-system call later.

## Persisting the report

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

## Opening the report in a browser

`open_in_browser` hands the report to the system default browser once it has been generated, so a terminal
session does not have to end with "…and now go find that file and double-click it". A browser needs a local
file that actually exists, which decides the rest:

- with `output` set, the report is written there and that file is opened;
- without it, the report is written to a temporary file first —
  `<temp dir>/<time>-<strategy>-<benchmark>-<random>.html`, created by
  [tempfile](https://crates.io/crates/tempfile). The prefix is for humans: the time (sorting the temp
  directory by name therefore sorts it by time), then `strategy_title` (falling back to `title`) and
  `benchmark_title`. Those two display names go through
  [sanitize-filename](https://crates.io/crates/sanitize-filename), which owns the file-name rules — illegal
  and control characters, Windows reserved device names, trailing dots and spaces — and replaces what is
  left with `_`; on top of that this extension collapses spaces into `_` and caps each part at 32
  characters, since a name holds two of them and the platform limit is 255. tempfile picks the random
  suffix, appends the `.html` (what makes the browser render the file instead of downloading it), and
  guarantees the name was free at that moment, so nothing existing is overwritten and two reports from the
  same second cannot collide;
- an `output` that is not a local path (`s3://…`, `memory://…`) is an error rather than a silent no-op, since
  no browser can open it. That is checked **before** the report is rendered.

Launching is [open](https://crates.io/crates/open)'s job, and it is the non-blocking `that_detached` variant:
the report is already on disk, so the query neither waits for the browser nor looks at what the browser does
with the file. On Windows that is a single `ShellExecute` call (the `shellexecute-on-windows` feature, rather
than the crate's PowerShell-based default); on macOS and elsewhere it is `open` / `xdg-open` plus the crate's
fallback list. The only failure reported is the launcher itself not starting.

The option is meant for a single report. Under `GROUP BY` every group is opened in turn — and, without
`output`, each group gets its own temporary file, so at least nothing overwrites anything.

## WebAssembly

`output` goes through DuckDB's VFS, so the wasm build uses exactly the same code path as the native one and
the file lands wherever DuckDB's own file system points in that environment. That replaces the earlier
behaviour, where the path was dropped on `wasm32-unknown-emscripten` because `std::fs` has no writable file
system there.

`open_in_browser` is the one deliberate exception, and it is also the only platform-specific code left in the
extension (`browser.rs`): a wasm build has no browser process to launch, so the option is ignored there — no
browser, and no temporary file either. The report string comes back to the host as it is, and showing it is
the host page's job: a blob URL and `window.open`, an `<iframe>`, or whatever else fits. The three crates
behind that option are declared under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, so a wasm
build does not compile them at all — a hard requirement, in fact, since `open` has no emscripten
implementation and would not build.

`just build_wasm` (`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`)
compiles fine; the runtime behaviour is DuckDB's VFS's, not ours.

## Dependencies

- [duckfn](https://crates.io/crates/duckfn): attribute macros that register ordinary Rust functions with
  DuckDB. Two of its features are enabled: `duckdb-1-5`, which provides the host file system
  (`duckfn::duck_vfs`) used by `output`, and `chrono`, which converts the time wrapper types
  (`DuckDate::to_naive_date` and friends). The macros also generate a `SQL_NAME` constant per signature —
  the name the function is really registered under — so error prefixes read that instead of a hand-written
  copy of the `overloads_name` literal.
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
- [chrono](https://crates.io/crates/chrono): used directly for **local time** — the temporary file name
  `open_in_browser` builds starts with a `%Y%m%d-%H%M%S` stamp (`chrono::Local`). The date side is duckfn's
  `chrono` feature (`DuckDate::to_naive_date`), whose `NaiveDate` is exactly what quantstats-rs'
  `ReturnSeries::new` takes; all three share one chrono 0.4.
- [open](https://crates.io/crates/open), [tempfile](https://crates.io/crates/tempfile) and
  [sanitize-filename](https://crates.io/crates/sanitize-filename): `open_in_browser` — starting the browser,
  creating a uniquely named temporary file, and knowing which file names the platform accepts. **Non-wasm
  targets only** (see the WebAssembly section), which is why they live in a target-specific dependency table
  instead of the main one.

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

The `Justfile` in the repository root wraps both: `just build`, `just sql "SELECT …"`, `just repl`,
`just test`, `just lint`, `just build_wasm`.

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

Before committing: `cargo clippy --all-targets -- -D warnings`.

## The demo dataset (`demo/prices.csv`)

The file is a fixed snapshot of daily closes for `GOOGL`, `MSFT` and the S&P 500 index (`SPX`): 1435 trading
days each, 2021-01-04 … 2026-09-21, one calendar shared by all three. It is a long table
(`date`, `symbol`, `price`) committed on purpose, so the README's quick start can be copied and run as-is.

**Why a snapshot and not a live URL.** When this was written there was no free, key-less *and* stable HTTP
endpoint for the daily closes of individual tickers: stooq puts a JavaScript challenge in front of its CSV
download, Yahoo's endpoint answers with region redirects, and EODHD's public `demo` token dies on quota after
a handful of requests. FRED does export the index as CSV
(`https://fred.stlouisfed.org/graph/fredgraph.csv?id=SP500`), but it rejects the `HEAD` probe `read_csv` sends
first, so that one cannot be read directly either. The snapshot was therefore taken on 2026-09-22 — the index
from that FRED CSV, the stocks from Nasdaq's public quote API
(`https://api.nasdaq.com/api/quote/MSFT/historical?assetclass=stocks&fromdate=2021-01-01&todate=2026-09-21&limit=2000`),
close prices as served.
