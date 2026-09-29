---
title: 演示数据
sidebar_position: 1
description: demo/prices.csv 里是什么、为什么用提交进仓库的快照而不是实时 URL，以及它从哪来。
---

# 演示数据（`demo/prices.csv`）

这是一份固定快照：`GOOGL`、`MSFT` 与标普 500 指数（`SPX`）的日收盘价，各 1435 个交易日，
区间 2021-01-04 … 2026-09-21，三者的交易日历完全一致。它是一张长表（`date`、`symbol`、`price`），
提交进仓库就是为了[快速上手](../../getting-started/quick-start.md)可以原样复制运行。

它有意存了两份：仓库里的 `demo/prices.csv`，以及 `docs/static/demo/` 下的一份 —— 后者让文档站把它发布在
`https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv`，也就是本站所有示例读的那个地址，
也是唯一能在浏览器里读的地址（DuckDB-Wasm 读不了仓库地址）。

**为什么是快照、而不是实时 URL。** 写这份文档时，个股日线没有「免费 + 免 key + 稳定」的 HTTP 端点：
stooq 的 CSV 下载被套上了 JavaScript 校验，Yahoo 的接口回的是地区跳转页，EODHD 的公开 `demo` token
几次请求就用完配额。FRED 确实提供指数的 CSV
（`https://fred.stlouisfed.org/graph/fredgraph.csv?id=SP500`），但它会拒绝 `read_csv` 先发的那个 `HEAD`
探测，所以也读不了。

于是快照取的是 2026-09-22 那一份 —— 指数来自上面那份 FRED CSV，两只个股来自 Nasdaq 的公开行情接口
（`https://api.nasdaq.com/api/quote/MSFT/historical?assetclass=stocks&fromdate=2021-01-01&todate=2026-09-21&limit=2000`），
价格为接口给出的收盘价。
