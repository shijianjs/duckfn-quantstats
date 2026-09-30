---
title: Functions
sidebar_position: 1
description: The two aggregate names, what one call returns, how the benchmark becomes reports, and the usage patterns worth copying.
---

# Functions

Two aggregate function names, **one signature each**, folding one long table into a whole set of
reports:

| Signature | Input | Returns |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | return series | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | price/NAV series | the same; returns are derived inside the function |

Every block below runs in your browser against the demo snapshot this site serves
(`GOOGL`, `MSFT` and the S&P 500 index, 1435 trading days each) — swap the `read_csv(…)` for your own
table and the query is the same.

## The essentials

- **One call produces the whole set of reports.** There is **no `GROUP BY`** in SQL: the `symbol`
  column is the grouping key, the function splits by it internally and renders one full report per
  symbol. A hundred instruments mean a hundred full reports (each with a dozen inline SVGs), so time
  and memory grow linearly with the number of instruments — that is expected, not a performance bug.
- **The result is a list**, each element being
  `{symbol, benchmark, strategy_title, benchmark_title, html, file_path}`: `unnest(...)` spreads it
  into rows, `list_transform(...)` picks fields, or `(qs_html_reports(...))[1].html` grabs one report
  directly.
- **The order is ascending by `symbol`**, and within one symbol the order of the `benchmark` list; it
  is independent of input order and thread count.
- **The benchmark is one or more ordinary symbols in the table**: the `benchmark` option is a **list**
  (`['SPX', 'NDX']`) and the symbols it names are input only — **they do not appear in the result**.
  A report can carry one benchmark, so "one instrument against M benchmarks" is **M reports**: the
  same `symbol` shows up in M rows, told apart by the `benchmark` field (several instruments sharing
  one benchmark is a different thing — that is simply one report per instrument).
- `symbol` is a `VARCHAR`, `date` is a `DATE`, `period_return` is the return per period (`DOUBLE`) and
  `price` is that day's price or NAV (`DOUBLE`). A row whose value is `NULL` in any of the four is
  **skipped entirely** (an empty-string `symbol` too), like any other SQL aggregate.
- The options argument always comes **last** and is **required** — pass `NULL` when you need no
  options. It is a **nullable** config whose type is the named STRUCT `qs_html_report_options`,
  created at load time.
- The options are **evaluated per row** (see [Options](./options.md)), which is exactly how "each
  instrument gets its own title and display name" works.
- **No `ORDER BY` is needed**: the aggregate only concatenates and lets the report sort by date. (The
  `ORDER BY` in these examples only tidies the rows you see.)
- Why two names: `(symbol, date, price, options)` and `(symbol, date, period_return, options)` have
  exactly the same type sequence (`VARCHAR, DATE, DOUBLE, STRUCT`), so one name could not dispatch
  them.

## Usage

The whole table, with a benchmark — the ordinary case:

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
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

One instrument, no benchmark — filter first:

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
    WHERE symbol = 'GOOGL'
)
SELECT (r).symbol, (r).benchmark, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(symbol, date, price, NULL)) AS r
    FROM prices
);
```

One instrument against two benchmarks — the `benchmark` list decides the order of the reports:

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
    WHERE symbol IN ('GOOGL', 'SPX', 'MSFT')
)
SELECT (r).symbol, (r).benchmark, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX', 'MSFT'], 'title': symbol}::qs_html_report_options)) AS r
    FROM prices
)
ORDER BY (r).benchmark;
```

A return series instead of a price series:

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
),
returns AS (
    SELECT symbol, date,
           price / lag(price) OVER (PARTITION BY symbol ORDER BY date) - 1.0 AS period_return
    FROM prices
)
SELECT (r).symbol, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports(symbol, date, period_return, NULL)) AS r
    FROM returns
)
ORDER BY (r).symbol;
```

Keep only the list, without dragging the HTML along (see [Output and browser](./output-and-browser.md)
for writing the reports out as well):

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'title': symbol}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'bytes': length(x.html)}) AS reports
FROM prices;
```

## Behaviours worth knowing

- **A struct literal must be cast with `::qs_html_report_options`.** Without it the literal is an
  anonymous `STRUCT(title VARCHAR)` that matches no signature, and DuckDB reports that no function
  matches.
- **`'...'::JSON::qs_html_report_options` must spell out all 10 keys** (DuckDB's JSON→STRUCT
  conversion rejects missing keys), so prefer the struct literal.
- **`benchmark` names symbols (a list), not a value series.** Every entry has to be one of the values
  in the `symbol` column and the list has to be identical across the whole call; the symbols it names
  act as the benchmarks only and never show up in the returned list. Even a single benchmark is
  written `['SPX']`.
- **`benchmark_title` is a list too, paired with `benchmark` by index**: `['S&P 500', 'Nasdaq 100']`
  belong to the first and second benchmark respectively. It is presentation only, hence lenient — a
  missing entry (shorter list, NULL, empty string) falls back to that report's own benchmark symbol,
  and extra entries are ignored.
- **On wasm nothing is written and no browser is opened**: the file operation is skipped (`file_path`
  comes back `NULL`) and `open_in_browser` does nothing, since there is no browser process to launch; see
  [Output and browser](./output-and-browser.md).
- **The demo snapshot is a good table to try all of this on** — `read_csv` it straight from this site:
  `read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')`. See the
  [demo dataset](../development-guide/demo-data/demo-dataset.md) for what is in it.
