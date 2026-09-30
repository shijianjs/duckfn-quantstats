---
title: Error paths
sidebar_position: 5
description: What each failure looks like — a NULL result, a skipped instrument, or a query-failing error with the exact message.
---

# Error paths

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
| The `output_dir` cannot be written (missing directory, unwritable path, …) | Error `cannot write the report to '<path>': …` naming the path |
| `open_in_browser` with an `output_dir` that is not a local path (`s3://…`, `memory://…`) | Error `only local file paths can be opened in a browser` |
| `output_dir` in a **wasm** build (the browser blocks on this site) | **Not an error**: nothing is written and `file_path` is `NULL` |
| `language` with no entries in the translation table | Error `no translations for language '…'`, pointing at `qs_list_translations()` |
| `qs_set_translation` with a `key` that is not in the catalog | Error `unknown key '…'` |
| `qs_set_translation` with an empty `language` | Error `language must not be an empty string` |
| `qs_set_translation` with a NULL element in the list, a missing `key`, or the same `key` twice | Error `must not contain a NULL element` / `every entry needs a 'key'` / `key '…' appears twice in one call` |
| `qs_set_translation` deleting a key or a language that is not there | **Not an error**: it returns `false` (the table did not change) |

Every error message starts with the registered function name (`qs_html_reports: …`), so it is obvious
which function reported it.

## The three worth seeing

Each block below is **meant to fail**: click **Run** and the message appears where the result would
have been.

A benchmark that is not in the table — `benchmark` names symbols, and each one has to have rows:

```sql {"type":"duckfn","show":"table","expect":"error"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['NDX']}::qs_html_report_options)) AS report
FROM prices;
```

Configuration mistakes are caught before anything is rendered, so they cost nothing:

```sql {"type":"duckfn","show":"table","expect":"error"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'periods_per_year': 0}::qs_html_report_options)) AS report
FROM prices;
```

An empty string in the benchmark list is a mistake that cannot be guessed around — an empty symbol
can be neither a report's label nor a file name:

```sql {"type":"duckfn","show":"table","expect":"error"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX', '']}::qs_html_report_options)) AS report
FROM prices;
```
