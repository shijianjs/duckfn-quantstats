[English](README.md) | [简体中文](README.zh.md) | [Documentation](https://shijianjs.github.io/duckfn-quantstats/)

# duckfn_quantstats

A DuckDB extension that turns a table of prices or returns into **quantstats HTML tearsheets straight
from SQL**: hand it one date-ordered long table and it groups by `symbol` internally, produces one
complete report per instrument (and one per benchmark, when the options name several), and returns
them — together with each one's benchmark, display name and the path it was written to — as a single
list.

No Python, no `pip install quantstats`, no notebook: it is a DuckDB extension, so it runs wherever
DuckDB runs — Linux, macOS, Windows, and DuckDB-Wasm in the browser.

**Requires DuckDB 1.5 or newer** (built and tested against v1.5.5).

## Install

The extension is published in DuckDB's
[community repository](https://duckdb.org/community_extensions/extensions/duckfn_quantstats):

```sql
INSTALL duckfn_quantstats FROM community;   -- once; needs network
LOAD duckfn_quantstats;
```

## One query, one report per instrument

Two aggregate names, **one signature each**:

| Signature | Input | Returns |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | return series | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | price/NAV series | the same; returns are derived inside the function |

There is **no `GROUP BY`** in SQL: the `symbol` column is the grouping key, and a benchmark is just
another symbol of the same table. `demo/prices.csv` — committed here and served by the documentation
site — is a snapshot of daily closes for `GOOGL`, `MSFT` and the S&P 500 index (`SPX`):

```sql
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
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

Three reports, one file each; `SPX` is input only — it is the benchmark and gets none of its own.
A [generated example](https://shijianjs.github.io/duckfn-quantstats/demo/qs_report_GOOGL-S&P_500.html)
(`GOOGL` against the S&P 500) is served next to the docs.

## Documentation

The user guide and the development notes live in a bilingual Docusaurus site
([source in `docs/`](docs/README.md)):

| Page | What is in it |
| --- | --- |
| [Introduction](https://shijianjs.github.io/duckfn-quantstats/docs/intro) | What one call returns, and where to start. |
| [Quick start](https://shijianjs.github.io/duckfn-quantstats/docs/getting-started/quick-start) | Installing it and producing the first reports. |
| [Functions](https://shijianjs.github.io/duckfn-quantstats/docs/guide/functions) | The two names, the result shape, the benchmark semantics. |
| [Options](https://shijianjs.github.io/duckfn-quantstats/docs/guide/options) | Every field of `qs_html_report_options`. |
| [Price (or NAV) series](https://shijianjs.github.io/duckfn-quantstats/docs/guide/price-series) | The differencing rules of the price branch. |
| [Output and browser](https://shijianjs.github.io/duckfn-quantstats/docs/guide/output-and-browser) | `output_dir`, the generated file names, `open_in_browser`. |
| [Error paths](https://shijianjs.github.io/duckfn-quantstats/docs/guide/error-paths) | Every failure, and its exact message. |
| [Architecture](https://shijianjs.github.io/duckfn-quantstats/docs/development-guide/architecture/project-structure) | How the code is laid out, why, and what it depends on. |
| [Build, test and release](https://shijianjs.github.io/duckfn-quantstats/docs/development-guide/build/build-and-release) | The build paths, the tests, the release flow, the wasm target. |
| [Publishing](https://shijianjs.github.io/duckfn-quantstats/docs/development-guide/publishing/community-extension) | Function descriptions and the community-extension registration. |

Everything a *user* needs is the first seven; the last three are for contributors. The same pages in
Simplified Chinese start at
<https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/intro>.

## Contributing

[`AGENTS.md`](AGENTS.md) holds the repository conventions, the duckfn knowledge map and the release
flow; the [development guide](https://shijianjs.github.io/duckfn-quantstats/docs/development-guide/architecture/project-structure)
explains the code itself.
