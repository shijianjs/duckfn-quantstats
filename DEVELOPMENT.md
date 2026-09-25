[English](DEVELOPMENT.md) | [简体中文](DEVELOPMENT.zh.md)

# duckfn_quantstats — development notes

The user guide is [README.md](README.md): the SQL surface, the configuration fields, the error paths and
the quick start live there. This file holds what users do not need — how the code is laid out, why it is
shaped the way it is, which crate carries which part, and how it is built and tested.

duckfn's own conventions (entry-point chain, the standard procedure for adding a function, which source to
consult before writing against the macros) are **not** repeated here: they live in [AGENTS.md](AGENTS.md),
which also says where duckfn's documentation and example extension sit in the local cargo registry (they
ship with the crate since 0.0.11, so no duckfn clone is needed).

The project started from DuckDB's official
[extension-template-rs](https://github.com/duckdb/extension-template-rs) and has been reshaped to follow
duckfn's skeleton conventions (entry module, `EXTENSION_NAME`, dependency list).

## Layout

```text
src/lib.rs            native crate root  ->  mod extension;
src/wasm_lib.rs       wasm crate root    ->  mod extension;    (the same set of mods, mirrored)
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
    report.rs         the tail: render one report per (symbol, benchmark), persist, open the browser, fill in
    naming.rs         the report file name: `<time>-<strategy>-<benchmark>[-<random>].html` (persistence and
                      the temporary file share the stem)
    browser.rs        opening in the system default browser (the whole feature is ignored on wasm)
src/extension/types/
    html_report_options.rs  the named STRUCT type `qs_html_report_options`
    html_report.rs          the result row type `QuantstatsHtmlReport` (no named type registered)
```

The extension name `duckfn_quantstats` must match `EXTENSION_NAME` in the `Makefile` and the artifact file
name. `src/lib.rs` and `src/wasm_lib.rs` must declare the same set of `mod`s (the official template's
`mod lib;` style fails with `E0583` once modules are nested). Add new functionality under `src/extension/`,
following duckfn's module layout.

## Design notes

### Two SQL names, one signature each

The `symbol` column is the grouping key, so there is **no `GROUP BY` in SQL**: the function groups inside
the aggregate state itself (see "the symbol table" below) and a single call produces the whole set of
reports. Each name therefore has exactly one signature, with the argument order fixed to "data columns first
(`symbol`, `date`, value), options last".

The price branch **needs its own name**: `(symbol, date, price, options)` and
`(symbol, date, period_return, options)` have exactly the same type sequence
(`VARCHAR, DATE, DOUBLE, STRUCT`), so one name could not dispatch them.

The registered name is never written by hand: the attribute macro generates a `SQL_NAME` constant per
signature (the function name itself, now that there is a single signature), and error prefixes read it —
`kind.rs` points at it instead of repeating the literal. The price of that is that such functions have to be
`pub(super)`, because the generated module inherits the function's visibility.

### The symbol table: the function does its own grouping

The aggregate state is not "one point array" but a `HashMap<String, SymbolSlot>`, one slot per symbol: that
symbol's points plus that symbol's copy of the options. Three things come out of that:

- **the caller writes no `GROUP BY`** — one `SELECT` yields the whole set of reports;
- **the benchmark is in the same state** — it is another symbol of the table, so it is right there to pair up
  (see below);
- **the options' granularity drops to the symbol** — reports are per instrument (each with its own title and
  display name), so the options are too.

The price is an aggregate state holding the whole table's points (the same order as one point array per
group), while the number of reports rendered in `result()` is still the number of instruments.

### The argument slots: options are lazy per symbol

`options` is read through `DuckLazy`: every row only builds an O(1) token, and the single real parse happens
the **first time a symbol appears** (once the table holds it, later rows merely append a point). This is not
a nicety — duckfn's adapter reads arguments per row, so parsing the struct on every row would be O(rows)
parses, whereas parses = number of symbols is what this API should cost.

The parse result is cached in the slot through duckfn's `DuckLazySlot`: a `DuckLazy` token is only valid
inside the callback that produced it, so the state can hold the parse result and nothing else. "Is this the
symbol's first row?" needs no extra flag — the key of the table *is* that marker, because only the path that
inserts a fresh slot reads the options column.

### One report per symbol

The report is rendered in `result()`, one per instrument per call. 100 instruments render 100 full reports
(each with a dozen inline SVGs), so time and memory grow linearly with the number of instruments — the same
order as the old "one report per `GROUP BY` group", only with the grouping moved from SQL into the function.
The result order is settled in `result()` by sorting on the symbol; it does not follow HashMap iteration or
DuckDB's merge order.

No `ORDER BY` is needed: the aggregate only concatenates and lets `ReturnSeries::new` sort by date (the price
branch sorts by date first, to difference).

### Why the benchmark is a symbol in the table (a list of them, in fact)

The benchmark is already in the same long table (it is a symbol like any other), so naming it in the
`benchmark` option and letting the function look it up is the shortest path: one `SELECT`, no `GROUP BY`, no
`cross join`, and with/without a benchmark differs by a single key.

The earlier design passed the benchmark in as a **list argument** (`list(...)` plus `cross join` plus
`GROUP BY`, a three-step dance). It did evaluate the benchmark exactly once, but the price was a "with a
benchmark" case that — the norm rather than the exception — needed an extra argument, an extra overload and
two extra SQL steps.

`benchmark` is a **list** (`['SPX', 'NDX']`) because the real scenario is "one instrument against several
benchmark series": quantstats-rs' `HtmlReportOptions` holds exactly one `Option<&ReturnSeries>` (the metrics
column, the benchmark line in the plots and rolling beta are all built around it), so several benchmarks can
only become **several reports** — 1 instrument × M benchmarks = M rows, told apart by the `benchmark` field,
with the list order being the report order.

Note that "several instruments against one benchmark" is a different thing: that is one report per instrument
and needs no list. A "N strategies × M benchmarks" cartesian product is left out of the API on purpose —
whoever wants it groups/filters and calls again.

The symbols named as benchmarks are **input only and get no report**; each benchmark's series is converted
once (differenced first, on the price branch) and shared by every instrument. The `benchmark` option
additionally has to be the same list across the whole call (entries and order) — otherwise "which one is the
benchmark" would have no single answer, so a disagreement is an error. Problems inside the list itself (an
empty string, a NULL element, a duplicate) are caught in the options type, see below.

The benchmark's display name (`benchmark_title`) is a list too, **paired by index** with `benchmark`. It is
presentation only, hence lenient: a missing entry (shorter list, NULL, empty string) falls back to that report's
benchmark symbol and extra entries are ignored — neither is an error.

### The result row type registers no named type

`QuantstatsHtmlReport` in `html_report.rs` deliberately leaves `create_type` off: the aggregate's return type
already carries the full anonymous `STRUCT(symbol VARCHAR, benchmark VARCHAR, strategy_title VARCHAR,
benchmark_title VARCHAR, html VARCHAR, file_path VARCHAR)[]`, so SQL can read it by field name (`unnest` /
`list_transform` / `[1].html`) — a type name on top would only add another surface to maintain.

The field names are the Rust field names verbatim (duckfn's `DuckStruct` derive has no field-level renaming)
and none of the six is an SQL keyword, so DuckDB renders `typeof` without quotes. There are only three
`Option`s: `benchmark` is NULL for a single-series report (none configured), `benchmark_title` goes with it
(it is that report's benchmark's display name), and `file_path` is NULL when nothing was written — the other
three are always there.

Both display names are echoed into the row (`strategy_title` / `benchmark_title`): they are the very ones the
legend and the file name used (each falling back to its own symbol), so printing or comparing them needs no
HTML parsing.

The same type doubles as `DuckAggregateState::Output = Vec<QuantstatsHtmlReport>` — duckfn's list write path
(`create_writer_batch` / `write_valid` / `write_finish` in `duck_list.rs`) attaches a child writer and the
elements go into the child vector through the write path `#[derive(DuckStruct)]` generates, so "an aggregate
returning an array of structs" needs no extra machinery at all.

### The options type

`#[duck(create_type = true)]` makes duckfn run
`CREATE TYPE IF NOT EXISTS "qs_html_report_options" AS STRUCT(...)` at load time, which is what makes
`{'title': 'x'}::qs_html_report_options` (and the JSON form) possible in SQL.

Every field is an `Option<T>` on purpose: DuckDB fills the missing keys of a struct literal with NULL, and
duckfn turns the **whole struct** into NULL when a non-Option field reads NULL — so a user's `{'rf': 0.1}`
would silently fall back to all defaults and their `rf` would be dropped.

`to_report_options(strategy_title, benchmark_title)` converts by starting from quantstats-rs'
`HtmlReportOptions::default()` and overriding only the fields the user actually wrote, so the defaults have a
single source of truth. The two arguments are **the already-resolved display names** (`report.rs` gets them from
`strategy_title_or` / `benchmark_title_or`): the same names also go into the file name, so resolving once and
using them twice is what keeps the report legend and the file on disk in agreement. The fallback rules
themselves are simple — `strategy_title` defaults to the symbol, `benchmark_title` is taken by index and falls
back to that benchmark's symbol — but they cannot be dropped: with dozens of reports out of one call, the
default `'Strategy'` is identical for every one of them, so the legend, the temporary file name and the
returned display name would all lose their distinguishing power.

`output_dir` is deliberately not forwarded: quantstats-rs writes with `std::fs`, while this extension wants
DuckDB's VFS (see below) — the directory is handled by `report.rs` after the report has been rendered.

The options are a **per-row column**, and each symbol uses the copy from its first row (see "the symbol
table"); `benchmark` additionally has to be the same list for every instrument, which `benchmark_names()` in
`report.rs` enforces.

`benchmark` and `benchmark_title` are both list fields (`Option<Vec<Option<String>>>`), but **only `benchmark` is
validated**, in one place, `QuantstatsHtmlOptions::benchmark_names()`: an element may not be NULL, may not be an
empty string and may not repeat — it decides who is the benchmark and who gets a report, so a mistake there
cannot be guessed around. `benchmark_title` is presentation only, hence lenient: it is taken by index, a missing
entry (shorter list, NULL, empty string) falls back to that benchmark's symbol and extra entries are ignored
(see `benchmark_title_or`). `periods_per_year = 0` and `output_dir = ''` are configuration errors reported right
here as well — all of them before anything is rendered or any file-system call happens.

## Persisting the reports

`output_dir` takes a **directory** and `naming.rs` builds the file name:
`<time>-<strategy>-<benchmark>-<random>.html` (the benchmark part is absent when none is configured). Naming
lives in the function for three reasons:

- a name has to carry "which instrument, against which benchmark, at what time", which only the function
  knows — and with one instrument against several benchmarks, a path built from the instrument alone would
  necessarily overwrite itself, which is exactly where the old "the caller writes the full path" design broke
  down;
- the last two parts are the display names the report itself uses (`strategy_title` and the benchmark's), so a
  directory full of reports still says which is which;
- the random suffix (`fastrand`) plus an existence check through `duck_vfs::exists` before writing (retrying
  with another suffix on a collision — see `report_path`) makes "nothing existing is overwritten" a guarantee
  rather than a probability: two calls each write their own files.

Legality and randomness are both delegated: `sanitize-filename` owns the illegal/control characters, the
Windows reserved device names and the trailing dots and spaces, `fastrand` the random suffix (the very random
source tempfile uses internally). This file only adds two policies of its own: spaces become `_`, and a part
is capped at 32 characters.

The write itself goes through duckfn's convenience layer `duck_vfs::write_string`, i.e. through **DuckDB's
VFS** rather than `std::fs`:

- local disk, in-memory file systems, whatever file system the wasm build exposes, and `s3://` /
  `http(s)://` once `httpfs` is loaded all go through the same path with the same semantics;
- it is also the only way an aggregate can write at all. DuckDB's C API gives aggregate functions no client
  context (no bind callback, no `duckdb_aggregate_function_get_client_context`), so duckfn keeps an owned
  long-lived connection from registration time and hands out a fresh `ClientContext` → `FileSystem` from it;
- the directory is joined with `/` (`report.rs::join`), deliberately not with `Path::join`: the latter joins
  with the platform separator, so on Windows `s3://bucket/reports` would come out as
  `s3://bucket/reports\name.html`.

`write_string` **replaces** its target: afterwards the file holds exactly the report, even when it previously
held something longer (the C API's missing truncate is handled inside duckfn's `duck_vfs` layer, which zeroes
a longer file before writing). With never-colliding names this extension does not actually hit that path, but
`read_text()` returning exactly what the function returned still holds and the test suite pins it with `md5`.

One call may write several files (instruments × benchmarks). The tail in `report.rs` runs in two passes: it
first settles every (symbol, benchmark) pair's `ReportTarget` (validating `output_dir` and `open_in_browser` on
the way), and only then renders, writes and opens each one, filling the path that was actually written into
the result row.

## Opening the report in a browser

`open_in_browser` hands the report to the system default browser once it has been generated, so a terminal
session does not have to end with "…and now go find that file and double-click it". A browser needs a local
file that actually exists, which decides the rest:

- with `output_dir` set, those files are written there and then opened;
- without it, every report is written to a temporary file first —
  `<temp dir>/<time>-<strategy>-<benchmark>-<random>.html`, created by
  [tempfile](https://crates.io/crates/tempfile). The stem (time + the two display names) is shared with
  `naming.rs`; tempfile picks the random suffix, appends the `.html` (what makes the browser render the file
  instead of downloading it) and guarantees the name was free at that moment, so nothing existing is
  overwritten and two reports from the same second cannot collide. The time comes first so that sorting the
  temp directory by name sorts it by time; `strategy_title` (falling back to `title`) and `benchmark_title`
  (taken by index, falling back to the benchmark symbol) are there for humans, and their legality is
  `sanitize-filename`'s business (see naming.rs);
- an `output_dir` that is not a local path (`s3://…`, `memory://…`) is an error rather than a silent no-op,
  since no browser can open it. That is checked **before** anything is rendered.

Launching is [open](https://crates.io/crates/open)'s job, and it is the non-blocking `that_detached` variant:
the reports are already on disk, so the query neither waits for the browser nor looks at what the browser does
with the files. On Windows that is a single `ShellExecute` call (the `shellexecute-on-windows` feature, rather
than the crate's PowerShell-based default); on macOS and elsewhere it is `open` / `xdg-open` plus the crate's
fallback list. The only failure reported is the launcher itself not starting.

One call opens one tab per **report** (one instrument against two benchmarks is two tabs) — and, without
`output_dir`, each report gets its own temporary file, so at least nothing overwrites anything.

## WebAssembly

`output_dir` goes through DuckDB's VFS, so the wasm build uses exactly the same code path as the native one and
the files land wherever DuckDB's own file system points in that environment. That replaces the earlier
behaviour, where the path was dropped on `wasm32-unknown-emscripten` because `std::fs` has no writable file
system there.

The naming logic (`naming.rs`) is therefore **shared**: a wasm build names its report files too, which is why
`sanitize-filename` and `fastrand` are ordinary dependencies rather than non-wasm ones.

`open_in_browser` is the one deliberate exception, and it is also the only platform-specific code left in the
extension (`browser.rs`): a wasm build has no browser process to launch, so the option is ignored there — no
browser, and no temporary file either. The report string comes back to the host as it is, and showing it is
the host page's job: a blob URL and `window.open`, an `<iframe>`, or whatever else fits. The two crates behind
that option (`open`, `tempfile`) are declared under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`,
so a wasm build does not compile them at all — a hard requirement, in fact, since `open` has no emscripten
implementation and would not build.

`just build_wasm` (`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`)
compiles fine; the runtime behaviour is DuckDB's VFS's, not ours.

## Dependencies

- [duckfn](https://crates.io/crates/duckfn): attribute macros that register ordinary Rust functions with
  DuckDB. Three of its features are enabled: `duckdb-1-5`, which provides the host file system
  (`duckfn::duck_vfs`) used by `output_dir`; `chrono`, which converts the time wrapper types
  (`DuckDate::to_naive_date` and friends); and `cli`, the command-line tool behind `src/bin/duckfn.rs`
  (it pulls clap and csv into duckfn). The macros also generate a `SQL_NAME` constant per signature —
  the name the function is really registered under — so error prefixes read that instead of a hand-written
  copy of the function-name literal, while `description` / `comment` / `example` on the attribute are the
  one source of the function-description CSV (see below).
- [quack-rs](https://crates.io/crates/quack-rs): DuckDB C API bindings; the code expanded from
  `duckfn_entrypoint!` refers to it directly.
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys): headers only, with `loadable-extension` enabled —
  so **no local DuckDB build is required**. The version floor is `>= 1.10500` (DuckDB 1.5.0: the crate
  encodes a DuckDB version as `1.<major*10000 + minor*100 + patch>.0`, so 1.5.5 is `1.10505.0`), because the
  client-context / file-system part of the C API is 1.5-only.
- [quantstats-rs](https://crates.io/crates/quantstats-rs): the report itself. Its public API exposes only
  `html()` as a callable entry point (`mod stats` is private, so `compute_performance_metrics` is unreachable),
  so both paths are built on it instead of recomputing metrics — that would create a second source of
  truth for numbers the report already prints.
- [chrono](https://crates.io/crates/chrono): used directly for **local time** — report file names (shared by
  persistence and the temporary file) start with a `%Y%m%d-%H%M%S` stamp (`chrono::Local`, see naming.rs). The
  date side is duckfn's `chrono` feature (`DuckDate::to_naive_date`), whose `NaiveDate` is exactly what
  quantstats-rs' `ReturnSeries::new` takes; all three share one chrono 0.4.
- [sanitize-filename](https://crates.io/crates/sanitize-filename) and
  [fastrand](https://crates.io/crates/fastrand): the two halves of a report file name — which parts are
  legal (illegal and control characters, Windows reserved device names, trailing dots and spaces) and the
  random suffix. Both are **shared** dependencies: `naming.rs` builds file names on wasm too. fastrand is
  already in the tree (tempfile uses it internally), so a direct dependency costs no extra compilation.
- [open](https://crates.io/crates/open) and [tempfile](https://crates.io/crates/tempfile):
  `open_in_browser`'s two jobs — starting the browser and, without `output_dir`, creating a uniquely named
  temporary file. **Non-wasm targets only** (see the WebAssembly section), which is why they live in a
  target-specific dependency table instead of the main one.

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
`just test`, `just lint`, `just build_wasm`, `just docs_csv`.

## Function descriptions (the community-extension doc page)

DuckDB's C extension API has **no** way to set a function's description or examples:
`duckdb_scalar_function_set_name`, `_set_return_type`, `_set_varargs`, `_set_volatile` … and that is it.
There is no `_set_description` and no `_add_example`. So the `Added Functions` table on a community
extension's page (<https://duckdb.org/community_extensions/list_of_extensions>) would be a bare list of
names without help from somewhere else.

That text sits next to the function it describes, on the `#[duck_*]` attribute (here: the two places in
`functions/aggregate_html/html_returns.rs` and `html_prices.rs`):

```rust
#[duck_aggregate_function(
    description = "Renders one quantstats HTML report per symbol from a long table of periodic returns",
    comment = "Groups by symbol internally, so the SQL needs no GROUP BY …",
    examples = ["SELECT unnest(qs_html_reports(…)) FROM daily_returns", "…"]
)]
```

All three keys are optional (`example` for one, `examples` for several; the two are mutually exclusive) and
take **no part in registration** — the macro only records them, along with the registered name, in an
inventory entry. To export:

```shell
just docs_csv                                           # -> target/function_descriptions.csv
cargo run --bin duckfn -- function_descriptions --all    # -> target/function_descriptions_all.csv
                                                         #    (includes undocumented functions, as a checklist)
```

No extension is loaded, the catalog is never queried and DuckDB need not be around: this reads what the
macros recorded at compile time, and the path is always the project's `target/`. The
`#[path = "../extension/mod.rs"] mod extension;` in `src/bin/duckfn.rs` is essential — `inventory`'s static
constructors only fire for object files that are really linked into the final binary, so switching to
`use duckfn_quantstats::…` would make the CSV come out empty, silently.

Three rules apply to the text itself: several examples are joined with `"; "` and lose their trailing
semicolons on export; line breaks collapse to a single space (the generated page is a Markdown table, where
a newline inside a cell ends the row); commas, quotes and non-ASCII text pass through unchanged. So write
one full statement per entry. The text is English because it is pasted onto that page as it is.

When the extension goes to the community repository, drop this CSV at
`extensions/duckfn_quantstats/docs/function_descriptions.csv` in `community-extensions` (its
`generate_md.sh` LEFT JOINs it on `function_name` to override the function tables). No copy needs to live in
this repository — regenerate it after changing the code.

## Testing

Tests are written in the SQLLogicTest format under `test/sql/`:

```shell
make debug && make test    # make test does not rebuild; run make debug after editing Rust
```

They come in two kinds, and the split is deliberate:

| File | Covers | Extra dependency |
| --- | --- | --- |
| `test/sql/quantstats/html_reports.test` | **Return-path behaviour**: the registration surface (two names, one signature each), the result shape and its ordering (symbols ascending, benchmarks in list order inside one symbol), `benchmark` NULL with none configured, display names falling back to the symbol, `NULL` rows skipped and instruments dropped with them, empty input → `NULL`, multi-threaded `combine` consistency (single vs 4 threads, md5-equal), `output_dir` auto-naming and path filling, two calls never overwriting each other | none |
| `test/sql/quantstats/html_reports_by_prices.test` | **Price-path behaviour**: byte-identical to `lag()`-derived returns, each of several benchmarks byte-identical to its own differenced reference, the benchmark side differenced too, points with a zero predecessor skipped, single-point instruments dropped, benchmark symbol missing / too few points | none |
| `test/sql/quantstats/html_reports_errors.test` | **Error paths**: the four ways a benchmark list can be wrong (symbol missing / empty string / NULL element / duplicate), the list disagreeing across symbols (order included), extra `benchmark_title` entries being ignored (no error), `periods_per_year = 0`, `output_dir = ''`, a NUL path, `open_in_browser` with a non-local path, an uncast options literal, the old API being gone | none |
| `test/sql/quantstats/html_reports_values.test` | **Output content**: parses the generated HTML with [webbed](https://duckdb.org/community_extensions/extensions/webbed)'s XPath and asserts the title, the date range, the `rf` echo, per-row metric numbers, the chart/table counts, the extra benchmark column, **each report carrying its own benchmark column when there are several**, **benchmark display names paired by index (missing / NULL / empty falling back to the symbol)**, and each symbol's own title and file name | the `webbed` community extension |

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
