[English](README.md) | [简体中文](README.zh.md) | [Documentation](https://shijianjs.github.io/duckfn-quantstats/)

# duckfn_quantstats

A DuckDB extension (loadable extension) that wraps
[quantstats-rs](https://crates.io/crates/quantstats-rs): hand it one date-ordered long table and it groups it by
`symbol` internally, produces **one complete quantstats HTML report per instrument** (one per benchmark as
well, when the options name benchmarks), and returns those reports — together with each one's benchmark,
display name and the path it was actually written to — as **a single list**. All in SQL.

**Requires DuckDB 1.5 or newer.** The host file system used to write the reports (`output_dir`) only reached
DuckDB's C API in 1.5, so there is no 1.4 compatibility path; the extension is built and tested against
v1.5.5.

## Install

The extension is published in DuckDB's
[community repository](https://duckdb.org/community_extensions/extensions/duckfn_quantstats):

```sql
INSTALL duckfn_quantstats FROM community;   -- once; needs network
LOAD duckfn_quantstats;
```

## The functions

Two aggregate names, **one signature each**, folding one long table into a whole set of reports:

| Signature | Input | Returns |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | return series | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | price/NAV series | the same; returns are derived inside the function |

There is **no `GROUP BY`** in SQL: the `symbol` column is the grouping key, and a single call produces the
whole set of reports. `demo/prices.csv` is a committed snapshot of daily closes for `GOOGL`, `MSFT` and the
S&P 500 index (`SPX`):

```sql
CREATE TABLE prices AS
SELECT * FROM read_csv('https://raw.githubusercontent.com/shijianjs/duckfn-quantstats/main/demo/prices.csv');

-- One report per instrument (SPX is input only: it is the benchmark and gets none of its own),
-- written to the current directory; output_dir only takes the directory, the function names the files.
SELECT (r).symbol, (r).benchmark, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol,
                'output_dir': './'}::qs_html_report_options)) AS r
    FROM prices
);
```

## Documentation

The user guide and the development notes live in a bilingual Docusaurus site
([source in `docs/`](docs/README.md)):

| Page | What is in it |
| --- | --- |
| [Introduction](https://shijianjs.github.io/duckfn-quantstats/docs/intro) | What one call returns, and where to start. |
| [Quick start](https://shijianjs.github.io/duckfn-quantstats/docs/getting-started/quick-start) | Installing it, building it, the first reports, and the traps. |
| [Functions](https://shijianjs.github.io/duckfn-quantstats/docs/guide/functions) | The two names, the result shape, the benchmark semantics, usage patterns. |
| [Options](https://shijianjs.github.io/duckfn-quantstats/docs/guide/options) | Every field of `qs_html_report_options`. |
| [Price (or NAV) series](https://shijianjs.github.io/duckfn-quantstats/docs/guide/price-series) | The differencing rules of the price branch. |
| [Output and browser](https://shijianjs.github.io/duckfn-quantstats/docs/guide/output-and-browser) | `output_dir`, the generated file names, `open_in_browser`, wasm. |
| [Error paths](https://shijianjs.github.io/duckfn-quantstats/docs/guide/error-paths) | Every failure, and its exact message. |
| [Project structure](https://shijianjs.github.io/duckfn-quantstats/docs/getting-started/project-structure) | The module layout and the naming rules. |
| [Design notes](https://shijianjs.github.io/duckfn-quantstats/docs/development/design-notes) | Why the function groups by symbol, and how the tail persists and opens. |
| [Dependencies](https://shijianjs.github.io/duckfn-quantstats/docs/development/dependencies) | Which crate carries which part, and why. |
| [Testing](https://shijianjs.github.io/duckfn-quantstats/docs/development/testing) | The SQLLogicTest files and how to run them. |
| [Build and release](https://shijianjs.github.io/duckfn-quantstats/docs/build-and-release) | The build paths, the release flow, the wasm target. |
| [Community extensions](https://shijianjs.github.io/duckfn-quantstats/docs/community-extension) | The registration and its two files. |

The same pages in Simplified Chinese start at
<https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/intro>.

## Build from source

```shell
cargo install cargo-duckdb-ext-tools   # once
cargo duckdb-ext build                 # -> target/debug/duckfn_quantstats.duckdb_extension

duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

`make configure` + `make debug` is the official template flow CI uses (on Windows, `make` must run in Git
Bash). The `Justfile` wraps both: `just build`, `just sql "SELECT …"`, `just repl`, `just test`, `just lint`,
`just docs_csv`, `just docs_start`.

## Contributing

[`AGENTS.md`](AGENTS.md) holds the repository conventions, the duckfn knowledge map and the release flow.
