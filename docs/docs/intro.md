---
title: Introduction
sidebar_position: 1
slug: /intro
description: What duckfn_quantstats does, what one SQL call returns, and where to start reading.
---

# Introduction

`duckfn_quantstats` turns a DuckDB table of prices or periodic returns into **one complete quantstats
HTML report per instrument** — the tearsheet you would otherwise generate from a Python notebook:
equity curve, drawdown, rolling Sharpe, the full metrics table, the benchmark column, all the plots.
One `SELECT` produces the whole set, and each report comes back as a row you can write to disk, open
in a browser, or read straight out of the result.

It is a **DuckDB extension**, so nothing has to be installed on top of DuckDB: no `pip install
quantstats`, no Python, no notebook. It runs wherever DuckDB runs — Linux, macOS, Windows, and in the
browser through DuckDB-Wasm. Installing it is one line:

```sql
INSTALL duckfn_quantstats FROM community;   -- once; needs network
LOAD duckfn_quantstats;                     -- afterwards, this is all a session needs
```

:::tip[DuckDB 1.5 or newer]
The host file system that writes the reports only reached DuckDB's C API in 1.5, so 1.4 is not
supported. The extension is built and tested against v1.5.5.
:::

## The two functions

Two aggregate names, **one signature each**:

| Signature | Input | Returns |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | return series | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | price/NAV series | the same; returns are derived inside the function |

There is **no `GROUP BY`** in SQL: the `symbol` column is the grouping key, so one call over a whole
table yields one report per instrument, and naming a benchmark — another symbol of that same table —
yields one report per instrument *and* benchmark.

The block below runs right here in your browser: the site preloads the extension, so there is no
`LOAD` to write, and the series is built inside the block so it needs no network. The [demo
snapshot](https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv) every other example reads —
1435 trading days of `GOOGL`, `MSFT` and the S&P 500 index (`SPX`) — is at the same URL.

```sql {"type":"duckfn","show":"table"}
WITH series AS (
    SELECT 'SYN-A' AS symbol, DATE '2024-01-01' + CAST(i AS INTEGER) AS date,
           100.0 * pow(1.002, i) * (1 + 0.01 * sin(i / 3.0)) AS price
    FROM range(0, 60) t(i)
    UNION ALL
    SELECT 'SYN-B', DATE '2024-01-01' + CAST(i AS INTEGER),
           100.0 * pow(1.001, i) * (1 + 0.01 * cos(i / 4.0))
    FROM range(0, 60) t(i)
    UNION ALL
    SELECT 'SYN-SPX', DATE '2024-01-01' + CAST(i AS INTEGER),
           100.0 * pow(1.0005, i)
    FROM range(0, 60) t(i)
)
SELECT (r).symbol, (r).benchmark, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SYN-SPX'],
                'benchmark_title': ['Synthetic index'],
                'title': symbol,
                'strategy_title': symbol}::qs_html_report_options)) AS r
    FROM series
)
ORDER BY (r).symbol;
```

Two rows out, one per instrument — `SYN-SPX` is input only: it is the benchmark of both reports and
gets none of its own. `html` holds the whole self-contained tearsheet, and `file_path` is where it
was written if you asked for that.

## Writing them out, or opening them

```sql
-- One file per report, in the current directory (the function names the files).
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX'], 'title': symbol, 'output_dir': './'}::qs_html_report_options)) AS report
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');

-- Or hand each report to the system browser as soon as it exists (a temporary file, one tab each).
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX'], 'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

## Where to go next

- [Quick start](./getting-started/quick-start.md) — install it and produce the first reports.
- [Functions](./guide/functions.md) — the two names, the result shape, the benchmark semantics.
- [Options](./guide/options.md) — every field of `qs_html_report_options`.
- [Output and browser](./guide/output-and-browser.md) — `output_dir`, the generated file names,
  `open_in_browser`.
- [Error paths](./guide/error-paths.md) — what a failure looks like.

Everything about building, testing or publishing the extension itself is under
[Development guide](./development-guide/architecture/project-structure.md) at the end of the sidebar —
using it never requires any of that.
