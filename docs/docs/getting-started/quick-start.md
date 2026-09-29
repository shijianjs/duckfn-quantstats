---
title: Quick start
sidebar_position: 1
description: Install the extension (or build it from source), then produce a set of quantstats HTML reports from one SQL call.
---

# Quick start

## Prerequisites

- **DuckDB 1.5 or newer** — `duckdb` on `PATH`. The host file system behind `output_dir` only
  reached DuckDB's C API in 1.5, so 1.4 is out; the extension is built and tested against v1.5.5.
- To build from source, the same tools the project uses:
  - **Rust** 1.86 or newer (`rust-version` in `Cargo.toml`);
  - **[just](https://github.com/casey/just)** and **cargo-duckdb-ext-tools**:
    `cargo install just cargo-duckdb-ext-tools`;
  - optional: **make** (inside Git Bash on Windows) and Python for the official build/test flow the
    CI uses.

## 1. Install and load

The extension is published in DuckDB's
[community repository](https://duckdb.org/community_extensions/extensions/duckfn_quantstats), so one
`INSTALL` fetches a signed build for the platform you are on — no `-unsigned`, nothing compiled
locally:

```sql
INSTALL duckfn_quantstats FROM community;   -- once; needs network
LOAD duckfn_quantstats;                     -- afterwards, this is all a session needs
```

Community extensions are built against the **latest stable DuckDB**, and this one uses DuckDB's
unstable C API, so the build `INSTALL` fetches is tied to the exact DuckDB version it was built for.
On an older DuckDB (1.4, say) there is no build at all — [build from source](#3-build-from-source)
instead.

## 2. Produce the reports

One call folds a whole date-ordered long table into one report per instrument. The block below runs
in your browser (the site preloads the released extension, so no `LOAD` here); the `series` CTE is a
synthetic two-instrument table with `SYN-SPX` as the benchmark:

```sql {"type":"duckfn","show":"table"}
WITH series AS (
    SELECT 'SYN-A' AS symbol, DATE '2024-01-01' + CAST(i AS INTEGER) AS date, 100.0 * pow(1.002, i) AS price
    FROM range(0, 60) t(i)
    UNION ALL
    SELECT 'SYN-B', DATE '2024-01-01' + CAST(i AS INTEGER), 100.0 * pow(1.001, i) FROM range(0, 60) t(i)
    UNION ALL
    SELECT 'SYN-SPX', DATE '2024-01-01' + CAST(i AS INTEGER), 100.0 * pow(1.0005, i) FROM range(0, 60) t(i)
)
SELECT (r).symbol, (r).benchmark, (r).benchmark_title, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'title': symbol,
                'strategy_title': symbol,
                'benchmark': ['SYN-SPX'],
                'benchmark_title': ['Synthetic index']}::qs_html_report_options)) AS r
    FROM series
);
```

`SYN-SPX` is input only: it is the benchmark of both reports and gets none of its own.

### With the committed demo snapshot

`demo/prices.csv` is a committed snapshot of daily closes for `GOOGL`, `MSFT` and the S&P 500 index
(`SPX`): 1435 trading days each, 2021-01-04 … 2026-09-21, one shared calendar. The block is meant to
be copied and run as-is — `read_csv` fetches the file over HTTPS by itself (DuckDB 1.5 reads
`https://` URLs — no `httpfs`, no API key):

```sql
INSTALL duckfn_quantstats FROM community;   -- once; needs network
LOAD duckfn_quantstats;

CREATE TABLE prices AS
SELECT * FROM read_csv('https://raw.githubusercontent.com/shijianjs/duckfn-quantstats/main/demo/prices.csv');
-- unavailable (mainland China, for instance)? the same file is mirrored by jsDelivr:
--   read_csv('https://cdn.jsdelivr.net/gh/shijianjs/duckfn-quantstats@main/demo/prices.csv')
-- cloned the repo? then simply read_csv('demo/prices.csv')

-- The whole table incl. the benchmark, written to the current directory.
-- output_dir only takes the directory; the function names the files.
SELECT (r).symbol, (r).benchmark, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol,
                'rf': 0.04,
                'output_dir': './'}::qs_html_report_options)) AS r
    FROM prices
);
```

A report is a few hundred KB of HTML (a dozen inline SVGs) and `unnest(...)` spreads them into rows,
so in a terminal `output_dir` (write them) or `open_in_browser` (open them) is the friendlier route.
When all you want is the list of reports, `list_transform` picks just the fields you need:

```sql
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM prices;
```

## 3. Build from source

For day-to-day iteration use `cargo-duckdb-ext-tools` (a global cargo subcommand that adds no
dependency to the project):

```shell
cargo install cargo-duckdb-ext-tools   # once
cargo duckdb-ext build                 # -> target/debug/duckfn_quantstats.duckdb_extension
```

The official template's `make` flow is kept as well (CI and sqllogictest use it); run
`make configure` once to create the Python venv it needs:

```shell
make configure   # once
make debug       # -> build/debug/extension/duckfn_quantstats/duckfn_quantstats.duckdb_extension
```

`make release` is the same flow with optimizations. On Windows, `make` must run in Git Bash.

A binary you built yourself is unsigned and uses DuckDB's unstable C API, so loading it needs
`-unsigned` (the community build does not — it is signed and matched to your version):

```shell
duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

## 4. Run the tests

```shell
just test           # make configure + make debug + make test
```

The faster loop (no `make`, no Python venv rebuild) and what each file covers are in
[Testing](../development/testing.md).

## Traps

::::warning[Things that look like bugs and are not]

- **`-unsigned` is mandatory** when you load a locally built extension. Without it DuckDB refuses
  the file.
- **The artifact file name must stay `duckfn_quantstats.duckdb_extension`.** DuckDB finds the
  entry-point symbol through the file name, so a copy called `win.duckdb_extension` fails with
  `did not contain function "duckfn_quantstats_init_c_api"`.
- **`make test` does not rebuild.** After changing Rust code run `just ci-build` (or `make debug`)
  first, otherwise the tests run against the previous artifact.
- **`output_dir` has to exist already.** The directory is never created for you; pointing at one
  that does not exist fails on the very first run.
- **A struct literal must be cast with `::qs_html_report_options`.** Without it the literal is an
  anonymous `STRUCT(title VARCHAR)` that matches no signature, and DuckDB reports that no function
  matches.

::::

One more, on Windows: if `cargo duckdb-ext build` reports the artifact is in use, a DuckDB process
is holding `target/debug/duckfn_quantstats.duckdb_extension`. Build to another path instead —
`cargo duckdb-ext build -o build/debug/duckfn_quantstats.duckdb_extension` — or close that process.
A `.duckdb_extension` is not a renamed DLL: DuckDB's metadata lives at the end of the file, so
copying a DLL over it produces `The metadata at the end of the file is invalid`.
