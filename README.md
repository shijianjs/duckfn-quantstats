[English](README.md) | [简体中文](README.zh.md) | [Development](DEVELOPMENT.md)

# duckfn_quantstats

A DuckDB extension (loadable extension) that wraps
[quantstats-rs](https://crates.io/crates/quantstats-rs): hand it one date-ordered long table and it groups it by
`symbol` internally, produces **one complete quantstats HTML report per instrument** (one per benchmark as
well, when the options name benchmarks), and returns those reports — together with each one's benchmark,
display name and the path it was actually written to — as **a single list**. All in SQL.

**Requires DuckDB 1.5 or newer.** The host file system used to write the reports (`output_dir`) only reached
DuckDB's C API in 1.5, so there is no 1.4 compatibility path; the extension is built and tested against
v1.5.5.

This file is the user guide. The development notes — module layout, design decisions, dependency choices and
the test suite — are in [DEVELOPMENT.md](DEVELOPMENT.md).

## Quick start

`demo/prices.csv` is a committed snapshot of daily closes for `GOOGL`, `MSFT` and the S&P 500 index (`SPX`):
1435 trading days each, 2021-01-04 … 2026-09-21, one shared calendar. The block below is meant to be copied
and run as-is (the extension has to be built first — see [Building and loading](#building-and-loading));
`read_csv` fetches the file over HTTPS by itself (DuckDB 1.5 reads `https://` URLs — no `httpfs`, no API
key):

```sql
LOAD './target/debug/duckfn_quantstats.duckdb_extension';

CREATE TABLE prices AS
SELECT * FROM read_csv('https://raw.githubusercontent.com/shijianjs/duckfn-quantstats/main/demo/prices.csv');
-- unavailable (mainland China, for instance)? the same file is mirrored by jsDelivr:
--   read_csv('https://cdn.jsdelivr.net/gh/shijianjs/duckfn-quantstats@main/demo/prices.csv')
-- cloned the repo? then simply read_csv('demo/prices.csv')
```

The first two blocks **open the reports in your browser** (no `output_dir`, so they go to temporary files and
their paths come back in `file_path`); the ones that write files use `'./'` (the current directory) — the
directory is never created for you, and pointing at one that does not exist fails on the very first run.

```sql
-- 1. The whole table in one call: one report per instrument, opened in the system default browser
--    once it has been generated
SELECT (r).symbol, (r).strategy_title, length((r).html) AS html_bytes, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'title': symbol,
                'strategy_title': symbol,
                'open_in_browser': true}::qs_html_report_options)) AS r
    FROM prices
);

-- 2. With a benchmark: SPX is just another symbol in the table (name it in the options),
--    and it gets no report of its own
SELECT (r).symbol, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol,
                'rf': 0.04,
                'open_in_browser': true}::qs_html_report_options)) AS r
    FROM prices
);

-- 3. One instrument against several benchmarks: one report per benchmark (`benchmark` is a list,
--    and its order is the order of the reports). Naming SPX and GOOGL here leaves MSFT as the only
--    instrument that gets reports — one per benchmark. ("Several instruments against one benchmark"
--    is a different thing: that is just one report per instrument and needs no list.)
--    output_dir only takes the directory, the function names the files; './' is the current directory
--    (which always exists).
SELECT (r).symbol, (r).benchmark, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX', 'GOOGL'],
                'benchmark_title': ['S&P 500', 'Alphabet'],   -- paired with `benchmark` by index
                'title': symbol,
                'strategy_title': symbol,
                'output_dir': './'}::qs_html_report_options)) AS r
    FROM prices
);

-- 4. Already have returns? The other function; the pct_change has to sit in a subquery
SELECT (r).symbol, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports(
               symbol, date, period_return,
               {'benchmark': ['SPX']}::qs_html_report_options)) AS r
    FROM (SELECT symbol, date,
                 price / lag(price) OVER (PARTITION BY symbol ORDER BY date) - 1.0 AS period_return
          FROM prices)
);
```

A report is a few hundred KB of HTML (a dozen inline SVGs) and `unnest(...)` spreads them into rows, so in a
terminal `output_dir` (write them) or `open_in_browser` (open them) is the friendlier route. When all you want
is the list of reports, `list_transform` picks just the fields you need:

```sql
-- The report list: symbol, benchmark and written path only
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM prices;
```

## Functions

Two aggregate function names, **one signature each**, folding one long table into a whole set of reports:

| Signature | Input | Returns |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | return series | `STRUCT(symbol, benchmark, strategy_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | price/NAV series | the same; returns are derived inside the function |

The essentials:

- **One call produces the whole set of reports.** There is **no `GROUP BY`** in SQL: the `symbol` column is
  the grouping key, the function splits by it internally and renders one full report per symbol. A hundred
  instruments mean a hundred full reports (each with a dozen inline SVGs), so time and memory grow linearly
  with the number of instruments — that is expected, not a performance bug.
- **The result is a list**, each element being `{symbol, benchmark, strategy_title, html, file_path}`:
  `unnest(...)` spreads it into rows, `list_transform(...)` picks fields, or `(qs_html_reports(...))[1].html`
  grabs one report directly.
- **The order is ascending by `symbol`**, and within one symbol the order of the `benchmark` list; it is
  independent of input order and thread count.
- **The benchmark is one or more ordinary symbols in the table**: the `benchmark` option is a **list**
  (`['SPX', 'NDX']`) and the symbols it names are input only — **they do not appear in the result**. A report
  can carry one benchmark, so "one instrument against M benchmarks" is **M reports**: the same `symbol` shows
  up in M rows, told apart by the `benchmark` field (several instruments sharing one benchmark is a different
  thing — that is simply one report per instrument). A report for the benchmark itself, or a different
  benchmark per instrument, is expressed by filtering and calling again.
- `symbol` is a `VARCHAR`, `date` is a `DATE`, `period_return` is the return per period (`DOUBLE`) and `price`
  is that day's price or NAV (`DOUBLE`). A row whose value is `NULL` in any of the four is **skipped
  entirely** (an empty-string `symbol` too), like any other SQL aggregate.
- The options argument always comes **last** and is **required** — pass `NULL` when you need no options. It is
  a **nullable** config whose type is the named STRUCT `qs_html_report_options`, created at load time.
- The options are **evaluated per row** (see [Config fields](#config-fields)), which is exactly how "each
  instrument gets its own title and display name" works: build the struct out of the `symbol` column, e.g.
  `{'title': symbol, 'output_dir': './'}`.
- **No `ORDER BY` is needed**: the aggregate only concatenates and lets the report sort by date.
- Why two names: `(symbol, date, price, options)` and `(symbol, date, period_return, options)` have exactly
  the same type sequence (`VARCHAR, DATE, DOUBLE, STRUCT`), so one name could not dispatch them.

## Price (or NAV) series

`qs_html_reports_by_prices` takes **prices** — NAVs count too — not percentage changes. The value column
is called `price`, borrowing Python quantstats' vocabulary: it lumps this kind of input under "prices" and
runs `pct_change` on anything that looks like a price series. **A NAV is not strictly a price**, but
quantstats does not distinguish either, so one key takes in all of these level values and users never have to
wonder which one to fill. The conversion rules:

- the points of each symbol are sorted by `date`, then each becomes `price_t / price_{t-1} - 1`;
- the first point of a symbol has no predecessor and is dropped;
- a point whose predecessor is missing, zero or not finite is **skipped** (the same "skip it" semantics as a
  NULL row: no error and no `inf`/`NaN` inside the series); skipping affects only that point, the next one is
  still compared with its own predecessor;
- an instrument with nothing left after the differencing simply **does not appear in the result**;
- the benchmark side goes through the same differing, except that an empty result is an **error** there (the
  benchmark was explicitly configured);
- a symbol should hold one point per date, otherwise the difference describes the movement within that date.

Writing the equivalent in SQL is noticeably clumsier: a window function **cannot** appear inside an aggregate
call (DuckDB reports `aggregate function calls cannot contain window function calls`), so the returns have to
be computed in a subquery first:

```sql
-- By hand: an extra subquery, and it is easy to get PARTITION BY / ORDER BY wrong
SELECT unnest(qs_html_reports(symbol, trade_date, period_return, NULL)) AS report
FROM (SELECT symbol, trade_date,
             nav / lag(nav) OVER (PARTITION BY symbol ORDER BY trade_date) - 1.0 AS period_return
      FROM nav_table);

-- With the shortcut: prices go straight in, the symbol column does the grouping
SELECT unnest(qs_html_reports_by_prices(symbol, trade_date, nav, NULL)) AS report
FROM nav_table;
```

## Config fields

Every field of `qs_html_report_options` is **nullable**; keys you omit take their default:

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | Report title |
| `strategy_title` | `VARCHAR` | the symbol | Strategy display name; falls back to the `symbol` |
| `benchmark_title` | `VARCHAR[]` | the benchmark symbol | Benchmark display name (presentation only): a **list**, paired with `benchmark` by index. An entry that was not given (shorter list, NULL, empty string, or the whole key absent) falls back to the benchmark symbol **that report uses**; extra entries are ignored |
| `benchmark` | `VARCHAR[]` | `NULL` | Which **symbols** are benchmarks (a **list**, whose order is the order of the reports); they are input only and get no report. Even a single benchmark is written `['SPX']` |
| `rf` | `DOUBLE` | `0.0` | Risk-free rate, **annualized** (`0.04` = 4%), matching quantstats' `rf` convention |
| `periods_per_year` | `UINTEGER` | `252` | Periods per year; must be greater than 0 |
| `match_dates` | `BOOLEAN` | `true` | Whether to align the start dates of strategy and benchmark |
| `output_dir` | `VARCHAR` | `NULL` | Also write every report into this **directory**, with file names generated by the function (see below); through DuckDB's VFS |
| `open_in_browser` | `BOOLEAN` | `false` | Open the reports in the system default browser; with no `output_dir` it writes a temporary file first (see below) |

Apart from the two display names, the defaults come straight from quantstats-rs'
`HtmlReportOptions::default()`; this extension does not invent a second set.

The two display names are the exception, and they are what makes dozens of reports out of one call usable: the
default `'Strategy'` is identical for every one of them, so the legend could not tell them apart and the
temporary file names would share one useless prefix. They therefore fall back to a name that comes from the
data (the symbol, the benchmark symbol), which keeps the report legend, the temporary file name and the
returned `strategy_title` in agreement.

**The options are a per-row column** and each symbol uses the copy from its first row, hence:

```sql
-- Every instrument's own title and display name, built out of the symbol column
-- (the output directory is one for the whole call; the file names are the function's business)
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'title': symbol,
            'strategy_title': symbol,
            'output_dir': './'}::qs_html_report_options)) AS report
FROM prices;
```

One symbol's options have to agree row by row; `benchmark` additionally has to be **the same list for every
instrument in the call** (same entries, same order — a disagreement is an error), otherwise "which one is the
benchmark" would have no single answer.

Customising anything along the benchmark dimension needs no extra work: `benchmark_title` is a list paired with
`benchmark` by index (a missing entry falls back to that report's own benchmark symbol), and the file names are
generated from "time + strategy + benchmark + random" (see below), so both already carry that part.

`rf` is **annualized** (`0.04` = 4%) and converted to a per-period rate inside the report; the crate has two
conversions that differ slightly — Sharpe (and rolling Sharpe / Sortino) uses
`(1 + rf)^(1/periods_per_year) - 1`, while PSR / Sortino in the metrics table use `rf / periods_per_year`.
Both collapse to 0 when `rf = 0` (the default).

## Usage

```sql
-- One instrument, no benchmark: just pick its rows before aggregating
SELECT unnest(qs_html_reports(symbol, trade_date, daily_return, NULL)) AS report
FROM daily_returns WHERE symbol = 'FUND';

-- The whole table with a benchmark: the benchmark is an ordinary symbol in it
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'benchmark': ['SPX'],
            'title': symbol,
            'strategy_title': symbol,
            'benchmark_title': ['S&P 500']}::qs_html_report_options)) AS report
FROM daily_returns;

-- One instrument against two benchmarks: two reports for it, each against its own benchmark
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'benchmark': ['SPX', 'NDX'], 'title': symbol}::qs_html_report_options)) AS report
FROM daily_returns;

-- A price (or NAV) series: the shortcut saves writing the pct_change window
SELECT unnest(qs_html_reports_by_prices(
           symbol, trade_date, nav, {'title': symbol}::qs_html_report_options)) AS report
FROM nav_table;

-- Also write them to a directory (only the directory is given, the function names the files;
-- through DuckDB's VFS, so this works on wasm too)
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'title': symbol, 'output_dir': './'}::qs_html_report_options)) AS report
FROM daily_returns;

-- Write them and open them in your browser (with no 'output_dir' each report goes to a temp file
-- first; one tab per report)
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM daily_returns;

-- Just the list, no HTML (a report is a few hundred KB, spreading them into rows gets noisy)
SELECT list_transform(
           qs_html_reports(symbol, trade_date, daily_return,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM daily_returns;
```

## Behaviours worth knowing

- **A struct literal must be cast with `::qs_html_report_options`.** Without it the literal is an
  anonymous `STRUCT(title VARCHAR)` that matches no signature, and DuckDB reports that no function matches.
- **`'...'::JSON::qs_html_report_options` must spell out all 9 keys** (DuckDB's JSON→STRUCT
  conversion rejects missing keys), so prefer the struct literal.
- **`benchmark` names symbols (a list), not a value series.** Every entry has to be one of the values in the
  `symbol` column and the list has to be identical across the whole call; the symbols it names act as the
  benchmarks only and never show up in the returned list. Even a single benchmark is written `['SPX']`.
- **`benchmark_title` is a list too, paired with `benchmark` by index**: `['S&P 500', 'Nasdaq 100']` belong to the
  first and second benchmark respectively. It is presentation only, hence lenient — a missing entry (shorter
  list, NULL, empty string) falls back to that report's own benchmark symbol, and extra entries are ignored.
- **The result is ordered ascending by `symbol`, and within one symbol by the benchmark list order**,
  regardless of input order or thread count.

## Error paths

| Situation | Behaviour |
| --- | --- |
| Not a single row, or no instrument able to produce a report | `NULL` |
| An instrument has no valid point left after conversion | that instrument is left out of the result |
| A `benchmark` symbol has no row in the table | Error `no row for the benchmark symbol '…'` |
| The `benchmark` list disagrees between instruments (entries or order) | Error `every symbol must use the same benchmark list` |
| The `benchmark` list holds an empty string / a NULL element / a duplicate | Error `must not contain an empty string` / `… a NULL element` / `lists '…' twice` |
| `benchmark_title` entry missing / empty / NULL / extra | **Not an error**: a missing entry falls back to that benchmark's symbol, extra entries are ignored |
| Price branch: the benchmark yields no return (fewer than two valid points) | Error `produced no returns` |
| `periods_per_year = 0` | Error `periods_per_year must be greater than 0` |
| `output_dir = ''` | Error `output_dir must not be an empty string` |
| The `output_dir` contains a NUL byte | Error `contains a NUL byte` |
| The `output_dir` cannot be written (missing directory, unwritable remote, …) | Error from `duckfn::duck_vfs::write` naming the path |
| `open_in_browser` with an `output_dir` that is not a local path (`s3://…`, `memory://…`) | Error `only local file paths can be opened in a browser` |

Every error message starts with the registered function name (`qs_html_reports: …`), so it is obvious which
function reported it.

## Writing the reports to files

`output_dir` takes a **directory** and writes one file per report, with the name generated by the function:
`<time>-<strategy>-<benchmark>-<random>.html` (the benchmark part is absent when no benchmark is configured).
That arrangement buys three things:

- **no path to build in SQL**: a name has to carry "which instrument, against which benchmark, at what time",
  which only the function knows — and with one instrument against several benchmarks, a path built from the
  instrument alone would necessarily overwrite itself;
- **no overwriting, ever**: the random suffix plus an existence check before writing (retrying with another
  suffix on a collision) means two calls each write their own files and nothing existing is ever replaced;
- **names you can read**: the last two parts are the display names the report itself uses (`strategy_title`
  and the benchmark's), so a directory full of reports still says which is which.

The directory **has to exist already** (it is not created for you). The write goes through **DuckDB's VFS**
rather than `std::fs`, so local disk, in-memory file systems, whatever file system the wasm build exposes, and
`s3://` / `http(s)://` once `httpfs` is loaded all go through the same path with the same semantics (VFS paths
are joined with `/`, so `output_dir` can be `s3://bucket/reports`).

The `file_path` in each returned row is the path that very call wrote to (`NULL` when nothing was written), so
"which files were written" can be read off the result instead of guessed:

```sql
SELECT (r).symbol, (r).benchmark, (r).file_path FROM (
    SELECT unnest(qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options)) AS r
    FROM prices
);
```

## Opening the report in a browser

`open_in_browser` hands the reports to the system default browser once they have been generated, so a terminal
session does not have to end with "…and now go find that file and double-click it". A browser needs a local
file that actually exists, which decides the rest:

- with `output_dir` set, those files are written there and then opened;
- without it, each report is written to a temporary file first —
  `<temp dir>/<time>-<strategy>-<benchmark>-<random>.html`, the same naming rule `output_dir` uses. The name
  is for humans: the time, then `strategy_title` (falling back to `title`) and `benchmark_title` (taken by index,
  falling back to the benchmark symbol), with characters a file name cannot hold replaced by `_`. Nothing existing is ever
  overwritten, and two reports from the same second cannot collide;
- an `output_dir` that is not a local path (`s3://…`, `memory://…`) is an error rather than a silent no-op,
  since no browser can open it. That is checked **before** anything is rendered.

The browser is started in a non-blocking way: the reports are already on disk, so the query neither waits for
the browser nor looks at what the browser does with the files. The only failure reported is the launcher
itself not starting.

One call opens one tab per **report** (one instrument against two benchmarks is two tabs; and, without
`output_dir`, each report writes its own temporary file, so at least nothing overwrites anything).

## WebAssembly

`output_dir` goes through DuckDB's VFS, so the wasm build uses exactly the same code path as the native one and
the file lands wherever DuckDB's own file system points in that environment.

`open_in_browser` is the one deliberate exception: a wasm build has no browser process to launch, so the
option is ignored there — no browser, and no temporary file either. The report string comes back to the host
as it is, and showing it is the host page's job: a blob URL and `window.open`, an `<iframe>`, or whatever
else fits.

## Building and loading

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

The extension is built against DuckDB's unstable C API, so `-unsigned` is required when loading it:

```shell
duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

The development loop (Justfile recipes, clippy, wasm builds) is in [DEVELOPMENT.md](DEVELOPMENT.md).
