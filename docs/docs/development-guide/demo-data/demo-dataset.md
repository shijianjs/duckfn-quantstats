---
title: The demo dataset
sidebar_position: 1
description: What demo/prices.csv contains, why it is a committed snapshot rather than a live URL, and where it came from.
---

# The demo dataset (`demo/prices.csv`)

The file is a fixed snapshot of daily closes for `GOOGL`, `MSFT` and the S&P 500 index (`SPX`): 1435
trading days each, 2021-01-04 … 2026-09-21, one calendar shared by all three. It is a long table
(`date`, `symbol`, `price`) committed on purpose, so the [quick start](../../getting-started/quick-start.md)
can be copied and run as-is.

It ships twice, on purpose: in the repository as `demo/prices.csv`, and under `docs/static/demo/` so
the docs site serves it at
`https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv` — the URL every example in these
pages reads, and the only one that works from a browser (DuckDB-Wasm cannot read a repository URL).

**Why a snapshot and not a live URL.** When this was written there was no free, key-less *and* stable
HTTP endpoint for the daily closes of individual tickers: stooq puts a JavaScript challenge in front
of its CSV download, Yahoo's endpoint answers with region redirects, and EODHD's public `demo` token
dies on quota after a handful of requests. FRED does export the index as CSV
(`https://fred.stlouisfed.org/graph/fredgraph.csv?id=SP500`), but it rejects the `HEAD` probe
`read_csv` sends first, so that one cannot be read directly either.

The snapshot was therefore taken on 2026-09-22 — the index from that FRED CSV, the stocks from
Nasdaq's public quote API
(`https://api.nasdaq.com/api/quote/MSFT/historical?assetclass=stocks&fromdate=2021-01-01&todate=2026-09-21&limit=2000`),
close prices as served.
