[English](README.md) | [简体中文](README.zh.md) | [Development](DEVELOPMENT.md)

# duckfn_quantstats

A DuckDB extension (loadable extension) that wraps
[quantstats-rs](https://crates.io/crates/quantstats-rs): it folds a date-ordered series, grouped by your own
`GROUP BY`, into one complete quantstats HTML report per group — straight from SQL.

**Requires DuckDB 1.5 or newer.** The host file system used to write the report (`output`) only reached
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

```sql
-- 1. One report: written to a file, then opened in your browser
SELECT qs_html_report_by_prices(date, price,
           {'title': 'Microsoft', 'output': 'msft.html', 'open_in_browser': true}::qs_html_report_options)
FROM prices WHERE symbol = 'MSFT';

-- 2. Against the index: the benchmark enters as a list of points
WITH benchmark AS (
    SELECT list({'date': date, 'price': price}) AS series FROM prices WHERE symbol = 'SPX'
)
SELECT qs_html_report_by_prices(
           p.date, p.price, b.series,
           {'title': 'Microsoft', 'benchmark_title': 'S&P 500', 'rf': 0.04}::qs_html_report_options) AS html
FROM prices p, benchmark b
WHERE p.symbol = 'MSFT';

-- 3. Already have returns? The other function; the pct_change has to sit in a subquery
SELECT qs_html_report(date, period_return, {'title': 'Microsoft', 'rf': 0.04}::qs_html_report_options)
FROM (SELECT date, price / lag(price) OVER (ORDER BY date) - 1.0 AS period_return
      FROM prices WHERE symbol = 'MSFT');

-- 4. One report per symbol
SELECT symbol,
       qs_html_report_by_prices(date, price, {'title': symbol, 'rf': 0.04}::qs_html_report_options) AS html
FROM prices
GROUP BY symbol;
```

A report is a few hundred KB of HTML (a dozen inline SVGs) and query 4 prints one per symbol, so in a
terminal `output` — or `open_in_browser` — is the friendlier route.

## Functions

Two aggregate function names, each with **two overloads** (dispatched by argument count), folding a
date-ordered series into one complete quantstats HTML report (`VARCHAR`):

| Signature | Input | Description |
| --- | --- | --- |
| `qs_html_report(date, period_return, options)` | return series | Single-series report; one row per period. |
| `qs_html_report(date, period_return, benchmark, options)` | return series | Benchmark report; the benchmark is a **list passed in once**. |
| `qs_html_report_by_prices(date, price, options)` | price series | Single-series report; returns are derived inside the function. |
| `qs_html_report_by_prices(date, price, benchmark, options)` | price series | Benchmark report; both sides are prices. |

SQL sees exactly two names (the two overloads of each are one function set):

- The options argument always comes **last** (data columns first, options last). It is a **nullable** config
  whose type is the named STRUCT `qs_html_report_options`, created at load time; `NULL` means "all
  defaults".
- `date` is a `DATE`, `period_return` is the return per period (`DOUBLE`) and `price` is that day's price or
  NAV (`DOUBLE`). A row whose `date` or value is `NULL` is **skipped entirely**, like any other SQL
  aggregate.
- `benchmark` is `STRUCT(date DATE, <value field> DOUBLE)[]` (`period_return` or `price`). A `NULL` benchmark,
  an empty one, or one without a single valid point is an **error** — that overload exists for the benchmark
  case, so a single-series report should simply omit the argument.
- A group without any valid row returns `NULL` (not an empty string, not an error).
- **One report per group.** `GROUP BY` over 100 instruments renders 100 full reports (each with a dozen
  inline SVGs), so time and memory grow linearly with the number of groups.
- No `ORDER BY` is needed: the aggregate only concatenates and lets the report sort by date.

## Price (or NAV) series

`qs_html_report_by_prices` takes **prices** — NAVs count too — not percentage changes. The value column
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
SELECT fund, qs_html_report(trade_date, period_return, NULL) AS html
FROM (
    SELECT fund, trade_date,
           nav / lag(nav) OVER (PARTITION BY fund ORDER BY trade_date) - 1.0 AS period_return
    FROM nav_table
)
GROUP BY fund;

-- With the shortcut: prices go straight in, grouping is plain GROUP BY
SELECT fund, qs_html_report_by_prices(trade_date, nav, NULL) AS html
FROM nav_table
GROUP BY fund;
```

## Config fields

Every field of `qs_html_report_options` is **nullable**; keys you omit take their default:

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | Report title |
| `strategy_title` | `VARCHAR` | `'Strategy'` | Strategy display name |
| `benchmark_title` | `VARCHAR` | `NULL` | Benchmark display name (presentation only) |
| `rf` | `DOUBLE` | `0.0` | Risk-free rate, **annualized** (`0.04` = 4%), matching quantstats' `rf` convention |
| `periods_per_year` | `UINTEGER` | `252` | Periods per year; must be greater than 0 |
| `match_dates` | `BOOLEAN` | `true` | Whether to align the start dates of strategy and benchmark |
| `output` | `VARCHAR` | `NULL` | Also write the HTML to this path, through DuckDB's VFS (see below) |
| `open_in_browser` | `BOOLEAN` | `false` | Open the report in the system default browser; with no `output` it writes a temporary file first (see below) |

Defaults come straight from quantstats-rs' `HtmlReportOptions::default()`; this extension does not invent a
second set.

`rf` is **annualized** (`0.04` = 4%) and converted to a per-period rate inside the report; the crate has two
conversions that differ slightly — Sharpe (and rolling Sharpe / Sortino) uses
`(1 + rf)^(1/periods_per_year) - 1`, while PSR / Sortino in the metrics table use `rf / periods_per_year`.
Both collapse to 0 when `rf = 0` (the default).

## Usage

```sql
-- Single series, all-default options
SELECT qs_html_report(trade_date, daily_return, NULL) FROM daily_returns;

-- Single series, only the keys you care about; a struct literal must be cast to the options type
SELECT symbol,
       qs_html_report(
           trade_date, daily_return,
           {'title': 'My Fund', 'rf': 0.02}::qs_html_report_options) AS html
FROM daily_returns
GROUP BY symbol;

-- With a benchmark: it is aggregated into a single row once, then cross-joined in
WITH benchmark AS (
    SELECT list({'date': trade_date, 'period_return': daily_return}) AS series
    FROM benchmark_returns
)
SELECT s.fund,
       qs_html_report(
           s.trade_date, s.daily_return, benchmark.series,
           {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::qs_html_report_options) AS html
FROM strategy_returns s, benchmark
GROUP BY s.fund;

-- A price (or NAV) series: the shortcut saves writing the pct_change window
SELECT fund,
       qs_html_report_by_prices(
           trade_date, nav, {'title': 'My Fund'}::qs_html_report_options) AS html
FROM nav_table
GROUP BY fund;

-- Also write the report to a file (through DuckDB's VFS, so this works on wasm too)
SELECT qs_html_report(
           trade_date, daily_return,
           {'title': 'My Fund', 'output': 'fund.html'}::qs_html_report_options)
FROM daily_returns;

-- Write it and open it in your browser (with no 'output' the report goes to a temp file first)
SELECT qs_html_report(
           trade_date, daily_return,
           {'title': 'My Fund', 'open_in_browser': true}::qs_html_report_options)
FROM daily_returns;
```

The benchmark argument is one single list, so it has to be aggregated into one row first: `list(...)` is
itself an aggregate and **cannot be inlined into an aggregate call** (DuckDB reports
`aggregate function calls cannot be nested`). A scalar subquery works just as well as the cross join
(verified), with the same effect:

```sql
SELECT fund,
       qs_html_report(
           trade_date, daily_return,
           (SELECT list({'date': trade_date, 'period_return': daily_return}) FROM benchmark_returns),
           NULL)
FROM strategy_returns
GROUP BY fund;
```

## Behaviours worth knowing

- **A struct literal must be cast with `::qs_html_report_options`.** Without it the literal is an
  anonymous `STRUCT(title VARCHAR)` whose field count differs from the options type, and DuckDB reports that
  no function matches.
- **`'...'::JSON::qs_html_report_options` must spell out all 8 keys** (DuckDB's JSON→STRUCT
  conversion rejects missing keys), so prefer the struct literal.
- **The benchmark point keys are fixed to `date` / `period_return`** (`price` on the price side). They match
  the anonymous `STRUCT(date DATE, period_return DOUBLE)` exactly, so **no cast is needed**; only when the
  source columns are not `DATE` / `DOUBLE` do you add one:
  `{'date': trade_date::DATE, 'period_return': daily_return::DOUBLE}`.

## Error paths

| Situation | Behaviour |
| --- | --- |
| The group has no valid row | `NULL` |
| The benchmark argument is `NULL` | Error `the benchmark list must not be NULL` |
| The benchmark is an empty list, or holds no valid point | Error `the benchmark list is empty` |
| The benchmark list contains a whole-NULL element | Error `cannot read the benchmark list` |
| `qs_html_report_by_prices`: fewer than two benchmark prices, so no return can be derived | Error `the benchmark prices produced no returns` |
| `periods_per_year = 0` | Error `periods_per_year must be greater than 0` |
| `output = ''` | Error `output must not be an empty string` |
| The `output` path contains a NUL byte | Error `contains a NUL byte` |
| The `output` path cannot be written (missing directory, unwritable remote, …) | Error from `duckfn::duck_vfs::write` naming the path |
| `open_in_browser` with an `output` that is not a local path (`s3://…`, `memory://…`) | Error `only local file paths can be opened in a browser` |

## Writing the report to a file

`output` writes the rendered HTML through **DuckDB's VFS** rather than `std::fs`, so local disk, in-memory
file systems, whatever file system the wasm build exposes, and `s3://` / `http(s)://` once `httpfs` is loaded
all go through the same path with the same semantics.

`output` **replaces** the target: afterwards the file holds exactly the report, even when it previously held
something longer.

The path is part of the configuration, so under `GROUP BY` give each group its own file
(`'report-' || symbol || '.html'`) instead of pointing every group at one path.

## Opening the report in a browser

`open_in_browser` hands the report to the system default browser once it has been generated, so a terminal
session does not have to end with "…and now go find that file and double-click it". A browser needs a local
file that actually exists, which decides the rest:

- with `output` set, the report is written there and that file is opened;
- without it, the report is written to a temporary file first —
  `<temp dir>/<time>-<strategy>-<benchmark>-<random>.html`. The prefix is for humans: the time, then
  `strategy_title` (falling back to `title`) and `benchmark_title`, with characters a file name cannot hold
  replaced by `_`. Nothing existing is ever overwritten, and two reports from the same second cannot collide;
- an `output` that is not a local path (`s3://…`, `memory://…`) is an error rather than a silent no-op, since
  no browser can open it. That is checked **before** the report is rendered.

The browser is started in a non-blocking way: the report is already on disk, so the query neither waits for
the browser nor looks at what the browser does with the file. The only failure reported is the launcher
itself not starting.

The option is meant for a single report. Under `GROUP BY` every group is opened in turn — and, without
`output`, each group gets its own temporary file, so at least nothing overwrites anything.

## WebAssembly

`output` goes through DuckDB's VFS, so the wasm build uses exactly the same code path as the native one and
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
