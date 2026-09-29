---
title: Quick start
sidebar_position: 1
description: Install the extension, point it at a table of prices or returns, and get a quantstats HTML report per instrument.
---

# Quick start

Nothing is compiled and nothing is installed besides the extension itself: if you can send a SQL
query to DuckDB — from the CLI, from Python, from any client — you can produce the reports.

## Prerequisites

- **DuckDB 1.5 or newer.** The host file system behind `output_dir` only reached DuckDB's C API in
  1.5, so 1.4 is not supported; the extension is built and tested against v1.5.5.
- Any way to send SQL to it. These pages use the `duckdb` CLI, but a Python/Java/Node client or a GUI
  works the same.

## 1. Install and load

The extension is published in DuckDB's
[community repository](https://duckdb.org/community_extensions/extensions/duckfn_quantstats), so one
`INSTALL` fetches a signed build for the platform you are on:

```sql
INSTALL duckfn_quantstats FROM community;   -- once; needs network
LOAD duckfn_quantstats;                     -- afterwards, this is all a session needs
```

## 2. Bring your data

Both functions consume a **long table** — one row per (instrument, date, value):

| Column | Type | Meaning |
| --- | --- | --- |
| `symbol` | `VARCHAR` | The instrument. This is the grouping key: one report per distinct value. |
| `date` | `DATE` | The period the value belongs to. |
| value | `DOUBLE` | A periodic return, or a price/NAV for `qs_html_reports_by_prices`. |

A benchmark is just another `symbol` in the same table, so there is nothing to join.

Every example in these docs runs on one snapshot: `GOOGL`, `MSFT` and the S&P 500 index (`SPX`), 1435
trading days each, 2021-01-04 … 2026-09-21. It is served next to this site:

```sql
CREATE OR REPLACE TABLE prices AS
SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

## 3. Produce the reports

One call over the whole table, one report per instrument — **no `GROUP BY`**: `symbol` is the
grouping key.

The runnable blocks on this page build a small series of their own, so they run in your browser
without fetching anything. In your own queries, swap the `series` CTE for the `prices` table above.

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
SELECT (r).symbol, (r).benchmark, (r).benchmark_title, length((r).html) AS html_bytes
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

Two rows out — `SYN-SPX` is input only: it is the benchmark of both reports and gets none of its own.
`html` holds the whole self-contained tearsheet, and `file_path` says where it was written, if you
asked for that.

## 4. Look at one

A report is one HTML document with the charts inlined, so it can be rendered right here. Click **Run**
and switch between the two tabs:

```sql {"type":"duckfn","show":"iframe","field":"html","tab_name":"symbol","option":{"height":"560px"}}
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
SELECT (r).symbol, (r).html
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

The same call on the demo snapshot produces a full-size report — the one on the
[home page](/), built from `GOOGL` against the S&P 500.

## 5. Put them where you want them

A report is a few hundred KB of HTML, so in a terminal you rarely want it in the result set.
`output_dir` writes one file per report, and `open_in_browser` shows them:

```sql
-- One file per report in the current directory. Only the directory is given;
-- the function generates the file names, so two calls never collide.
SELECT (r).symbol, (r).file_path
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

-- Or hand each report to your system browser as soon as it exists
-- (with no 'output_dir' each report goes to a temporary file first; one tab each).
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM prices;
```

Just want the list, without dragging the HTML along?

```sql
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM prices;
```

## Common tasks

```sql
-- One instrument only: filter first, and there is still no GROUP BY
SELECT unnest(qs_html_reports(symbol, trade_date, daily_return, NULL)) AS report
FROM daily_returns WHERE symbol = 'FUND';

-- One instrument against two benchmarks → two reports, one per benchmark
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'benchmark': ['SPX', 'NDX'], 'title': symbol}::qs_html_report_options)) AS report
FROM daily_returns;

-- You already have returns: the other function takes them as they are
SELECT unnest(qs_html_reports(symbol, trade_date, daily_return, NULL)) AS report
FROM daily_returns;

-- Several instruments sharing one benchmark is the ordinary case: nothing extra to write
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, nav,
           {'benchmark': ['SPX'], 'title': symbol}::qs_html_report_options)) AS report
FROM nav_table;
```

## If something does not work

| Symptom | Cause |
| --- | --- |
| `No function matches the given name and argument types 'qs_html_reports(VARCHAR, DATE, DOUBLE, STRUCT(title VARCHAR))'` | The options literal needs the type: write `{…}::qs_html_report_options`. |
| `no row for the benchmark symbol 'XYZ'` | `benchmark` names **symbols**, and each one has to exist in the table you are aggregating over. |
| `every symbol must use the same benchmark list` | `benchmark` is read per row but has to be identical for the whole call. |
| A path error from `duckfn::duck_vfs::write` | `output_dir` must already exist; the function never creates the directory. |
| `only local file paths can be opened in a browser` | `open_in_browser` cannot open `s3://…` or `memory://…`; write those to disk without the option. |

The complete table of behaviours is on [Error paths](../guide/error-paths.md).

## Next

- [Functions](../guide/functions.md) — the result shape, the ordering, how benchmarks become reports.
- [Options](../guide/options.md) — every field of `qs_html_report_options`.
- [Price (or NAV) series](../guide/price-series.md) — what the price branch does with the values.
- [Output and browser](../guide/output-and-browser.md) — where the files go and how they are named.

Building or testing the extension itself is a different subject — that lives in the
[Development guide](../development-guide/architecture/project-structure.md).
