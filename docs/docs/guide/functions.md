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

The essentials:

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
  one benchmark is a different thing — that is simply one report per instrument). A report for the
  benchmark itself, or a different benchmark per instrument, is expressed by filtering and calling
  again.
- `symbol` is a `VARCHAR`, `date` is a `DATE`, `period_return` is the return per period (`DOUBLE`) and
  `price` is that day's price or NAV (`DOUBLE`). A row whose value is `NULL` in any of the four is
  **skipped entirely** (an empty-string `symbol` too), like any other SQL aggregate.
- The options argument always comes **last** and is **required** — pass `NULL` when you need no
  options. It is a **nullable** config whose type is the named STRUCT `qs_html_report_options`,
  created at load time.
- The options are **evaluated per row** (see [Options](./options.md)), which is exactly how "each
  instrument gets its own title and display name" works: build the struct out of the `symbol` column,
  e.g. `{'title': symbol, 'output_dir': './'}`.
- **No `ORDER BY` is needed**: the aggregate only concatenates and lets the report sort by date.
- Why two names: `(symbol, date, price, options)` and `(symbol, date, period_return, options)` have
  exactly the same type sequence (`VARCHAR, DATE, DOUBLE, STRUCT`), so one name could not dispatch
  them.

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
  anonymous `STRUCT(title VARCHAR)` that matches no signature, and DuckDB reports that no function
  matches.
- **`'...'::JSON::qs_html_report_options` must spell out all 9 keys** (DuckDB's JSON→STRUCT
  conversion rejects missing keys), so prefer the struct literal.
- **`benchmark` names symbols (a list), not a value series.** Every entry has to be one of the values
  in the `symbol` column and the list has to be identical across the whole call; the symbols it names
  act as the benchmarks only and never show up in the returned list. Even a single benchmark is
  written `['SPX']`.
- **`benchmark_title` is a list too, paired with `benchmark` by index**: `['S&P 500', 'Nasdaq 100']`
  belong to the first and second benchmark respectively. It is presentation only, hence lenient — a
  missing entry (shorter list, NULL, empty string) falls back to that report's own benchmark symbol,
  and extra entries are ignored.
- **The result is ordered ascending by `symbol`, and within one symbol by the benchmark list order**,
  regardless of input order or thread count.
- **`demo/prices.csv` is a good table to try all of this on** — see the
  [demo dataset](../development/demo-dataset.md).
