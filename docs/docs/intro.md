---
title: Introduction
sidebar_position: 1
slug: /intro
description: What duckfn_quantstats is, what one call returns, and where to start reading.
---

# Introduction

`duckfn_quantstats` is a DuckDB [loadable extension](https://duckdb.org/docs/stable/extensions/extension_development)
that wraps [quantstats-rs](https://crates.io/crates/quantstats-rs): hand it **one date-ordered long
table** and it groups by `symbol` internally, produces **one complete quantstats HTML report per
instrument** (one per benchmark too, when the options name several), and returns those reports —
together with each one's benchmark, display name and the path it was actually written to — as **a
single list**. All in SQL.

**Requires DuckDB 1.5 or newer.** The host file system used to write the reports (`output_dir`) only
reached DuckDB's C API in 1.5, so there is no 1.4 compatibility path; the extension is built and
tested against v1.5.5.

```sql
INSTALL duckfn_quantstats FROM community;   -- once; needs network
LOAD duckfn_quantstats;
```

The whole SQL surface is two aggregate names, **one signature each**:

| Signature | Input | Returns |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | return series | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | price/NAV series | the same; returns are derived inside the function |

One call returns the whole set of reports. The block below runs in your browser — the site preloads
the extension from the repository's latest release, so there is no `LOAD` to write here:

```sql {"type":"duckfn","show":"table"}
-- A synthetic two-instrument series, so the block needs no data file
WITH series AS (
    SELECT 'SYN-A' AS symbol, DATE '2024-01-01' + CAST(i AS INTEGER) AS date, 100.0 * pow(1.002, i) AS price
    FROM range(0, 60) t(i)
    UNION ALL
    SELECT 'SYN-B', DATE '2024-01-01' + CAST(i AS INTEGER), 100.0 * pow(1.001, i)
    FROM range(0, 60) t(i)
)
SELECT (r).symbol, (r).benchmark, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'title': symbol, 'strategy_title': symbol}::qs_html_report_options)) AS r
    FROM series
);
```

::::note[This site is new]
The pages here were written for this extension and are maintained with it. If something is missing
or wrong, the repository is the place to say so — the maintenance conventions for the site itself
(layout, commands, translations, deployment) are in `docs/README.md`.
::::

## What is in the box

| Path | What it is |
| --- | --- |
| `src/extension/mod.rs` | The entry point: `duckfn_entrypoint!("duckfn_quantstats")` plus the module tree. |
| `src/extension/functions/aggregate_html/` | The two registered functions and the whole render/persist/open tail. |
| `src/extension/types/` | The SQL-facing types: the named STRUCT `qs_html_report_options` and the result row. |
| `test/sql/quantstats/` | SQLLogicTest files, one per concern (returns, prices, errors, output content). |
| `demo/prices.csv` | A committed snapshot of daily closes for `GOOGL`, `MSFT` and `SPX`, used by the quick start. |
| `Justfile` | The everyday commands: build, run SQL, repl, test, lint, docs, release. |
| `.github/workflows/` | The build matrix, the GitHub Release on a version tag, and this site's deployment. |
| `community-extension/` | The two files a [community extension](https://duckdb.org/community_extensions/list_of_extensions) registration needs. |
| `docs/` | This site: Docusaurus, English and Simplified Chinese. |

## Where to go next

- [Quick start](./getting-started/quick-start.md) — install the extension (or build it), then produce the
  first reports from SQL.
- [Project structure](./getting-started/project-structure.md) — where the entry point, the functions and
  the SQL types live, and the naming rules that hold them together.
- [Functions](./guide/functions.md) — the two names, the result shape, the benchmark semantics.
- [Options](./guide/options.md) — every field of `qs_html_report_options`.
- [Design notes](./development/design-notes.md) — why the function groups by symbol, and why the
  benchmark is a symbol of the table.
- [Build and release](./build-and-release.md) — the build paths and the release flow.
- [Community extensions](./community-extension.md) — publishing to DuckDB's community repository.
