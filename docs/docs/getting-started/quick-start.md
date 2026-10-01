---
title: Quick start
sidebar_position: 1
description: Install the extension, point it at a table of prices or returns, and get a quantstats HTML report per instrument.
---

# Quick start

Nothing is compiled and nothing is installed besides the extension itself: if you can send a SQL
query to DuckDB — from the CLI, from Python, from any client — you can produce the reports. Every
block on this page runs against one snapshot, and every one of them runs *here*, in your browser.

## Prerequisites

- **DuckDB 1.3 or newer.** The extension is built against DuckDB 1.5.5 headers, but the version it
  carries is a floor rather than an exact match: it was measured on 1.3.2, 1.4.0, 1.4.5, 1.5.0, 1.5.5
  and 1.5.6.
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

A benchmark is just another `symbol` in the same table, so there is nothing to join. The examples all
read a snapshot served next to this site: `GOOGL`, `MSFT` and the S&P 500 index (`SPX`), 1435 trading
days each, 2021-01-04 … 2026-09-21:

```sql {"type":"duckfn","show":"table"}
SELECT count(*) AS rows,
       count(DISTINCT symbol) AS instruments,
       min(date) AS first_day,
       max(date) AS last_day
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

## 3. Produce the reports

One call over the whole table, one report per instrument — **no `GROUP BY`**: `symbol` is the
grouping key.

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT (r).symbol, (r).benchmark, (r).benchmark_title, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol}::qs_html_report_options)) AS r
    FROM prices
)
ORDER BY (r).symbol;
```

Two rows out — `SPX` is input only: it is the benchmark of both reports and gets none of its own.
`html` holds the whole self-contained tearsheet, and `file_path` says where it was written, if you
asked for that.

## 4. Look at one

A report is one HTML document with the charts inlined, so it can be rendered right here. Click **Run**
and switch between the two tabs:

```sql {"type":"duckfn","show":"iframe","field":"html","tab_name":"symbol","option":{"height":"560px"}}
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT (r).symbol AS symbol, (r).html AS html
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol}::qs_html_report_options)) AS r
    FROM prices
)
ORDER BY (r).symbol;
```

The full-size report for `GOOGL` is also served next to the docs — that is the one on the
[home page](/).

## 5. Put them where you want them

A report is a few hundred KB of HTML, so in a terminal you rarely want it in the result set.
`output_dir` writes one file per report, and `file_path` tells you where each one went:

```sql
-- One file per report, in the current directory. Only the directory is given;
-- the function generates the file names, so two calls never collide.
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
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
)
ORDER BY (r).symbol;
```

:::note[Why that block is not runnable here]

The query is the real one, and on a native DuckDB it writes the files. A **wasm build writes none**:
there the file operation is skipped, so every `file_path` would come back `NULL` and the reports would
sit in the `html` column — a limitation of the platform rather than of the extension. In a browser every
path reports as present, even one that does not exist (a phantom one-byte entry that DuckDB's own `glob`
and `file_size` agree with), so "is this name free" has no answer worth trusting. Run the same block in
your own DuckDB and the files appear in `./`.

:::

On a desktop, `open_in_browser` saves you the trip to the file manager; it needs a browser process to
launch, so it is a no-op in this page (and `file_path` comes back empty):

```sql
-- Not runnable here: on wasm there is no browser process to hand the report to.
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX'], 'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

Just want the list, without dragging the HTML along?

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'title': symbol}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'bytes': length(x.html)}) AS reports
FROM prices;
```

## Common tasks

One instrument only — filter first, and there is still no `GROUP BY`:

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
    WHERE symbol IN ('GOOGL', 'SPX')
)
SELECT (r).symbol, (r).benchmark, (r).benchmark_title, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'], 'benchmark_title': ['S&P 500'], 'title': symbol}
               ::qs_html_report_options)) AS r
    FROM prices
)
ORDER BY (r).symbol;
```

One instrument against two benchmarks — two reports, one per benchmark:

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
    WHERE symbol IN ('GOOGL', 'SPX', 'MSFT')
)
SELECT (r).symbol, (r).benchmark, (r).benchmark_title, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX', 'MSFT'],
                'benchmark_title': ['S&P 500', 'Microsoft'],
                'title': symbol}::qs_html_report_options)) AS r
    FROM prices
)
ORDER BY (r).benchmark;
```

You already have returns — the other function takes them as they are (here they are differenced out of
the same prices, which is what the price branch does internally):

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
),
returns AS (
    SELECT symbol, date,
           price / lag(price) OVER (PARTITION BY symbol ORDER BY date) - 1.0 AS period_return
    FROM prices
)
SELECT (r).symbol, (r).benchmark, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports(
               symbol, date, period_return,
               {'benchmark': ['SPX'], 'title': symbol, 'strategy_title': symbol}
               ::qs_html_report_options)) AS r
    FROM returns
)
ORDER BY (r).symbol;
```

Several instruments sharing one benchmark is the ordinary case — nothing extra to write:

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT (r).benchmark, count(*) AS reports, count(DISTINCT (r).symbol) AS instruments
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'], 'title': symbol}::qs_html_report_options)) AS r
    FROM prices
)
GROUP BY (r).benchmark;
```

## If something does not work

The two failures worth recognising early — and both blocks below are meant to fail, so **Run** shows
you the message rather than a result:

```sql {"type":"duckfn","show":"table","expect":"error"}
-- The options literal needs its type, otherwise DuckDB looks for a STRUCT(title VARCHAR) overload
-- that does not exist.
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT unnest(qs_html_reports_by_prices(symbol, date, price, {'title': symbol})) AS report
FROM prices;
```

```sql {"type":"duckfn","show":"table","expect":"error"}
-- 'benchmark' names symbols, and every one of them has to exist in the table being aggregated.
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price, {'benchmark': ['NDX']}::qs_html_report_options)) AS report
FROM prices;
```

| Other symptom | Cause |
| --- | --- |
| `every symbol must use the same benchmark list` | `benchmark` is read per row but has to be identical for the whole call. |
| `cannot write the report to '…'` | `output_dir` must be an existing **local** directory; the function never creates it, and remote paths (`s3://…`) are not writable here. |
| `only local file paths can be opened in a browser` | `open_in_browser` cannot open `s3://…` or `memory://…`; write those to disk without the option. |

The complete table of behaviours is on [Error paths](../guide/error-paths.md).

## Next

- [Functions](../guide/functions.md) — the result shape, the ordering, how benchmarks become reports.
- [Options](../guide/options.md) — every field of `qs_html_report_options`.
- [Price (or NAV) series](../guide/price-series.md) — what the price branch does with the values.
- [Output and browser](../guide/output-and-browser.md) — where the files go and how they are named.
- [Translation](../guide/translation.md) — the `lang` option, and the notes on each element.

Building or testing the extension itself is a different subject — that lives in the
[Development guide](../development-guide/architecture/project-structure.md).
