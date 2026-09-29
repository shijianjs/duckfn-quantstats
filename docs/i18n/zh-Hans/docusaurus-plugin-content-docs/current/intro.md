---
title: 简介
sidebar_position: 1
slug: /intro
description: duckfn_quantstats 能做什么、一次调用返回什么，以及从哪读起。
---

# 简介

`duckfn_quantstats` 把一张 DuckDB 的价格表或收益率表，变成**每个标的一份完整的 quantstats HTML 报告** ——
就是平时在 Python notebook 里生成的那种 tearsheet：净值曲线、回撤、滚动 Sharpe、完整的指标表、
基准列、所有图表。一条 `SELECT` 出整套，每份报告作为一行返回，可以直接落盘、用浏览器打开，或者就在
结果里读。

它是一个 **DuckDB 扩展**，所以在 DuckDB 之上不需要装任何东西：不用 `pip install quantstats`、不用
Python、不用 notebook。DuckDB 能跑的地方它就能跑 —— Linux、macOS、Windows，以及在浏览器里通过
DuckDB-Wasm。安装就一行：

```sql
INSTALL duckfn_quantstats FROM community;   -- 只需一次，需要网络
LOAD duckfn_quantstats;                     -- 之后每个会话只要这一句
```

:::tip[DuckDB 1.5 及以上]
报告落盘用的宿主文件系统是 1.5 才进 DuckDB C API 的，所以不支持 1.4；本扩展在 v1.5.5 上构建与测试。
:::

## 两个函数

两个聚合函数名字、**各一个签名**：

| 签名 | 输入 | 返回 |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | 收益率序列 | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | 价格/净值序列 | 同上；函数内部先换算成收益率 |

SQL 里**不写 `GROUP BY`**：`symbol` 列就是分组依据，所以整张表一次调用就是「每个标的一份报告」；
再指名一个基准（同一张表里的另一个 symbol），就是「每个标的 × 每个基准一份」。

下面这块在你的浏览器里就能跑：站点会预加载扩展，所以不用写 `LOAD`；序列在块内构造，因此不需要任何网络。
[演示快照](https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv) —— `GOOGL`、`MSFT` 与标普 500
指数（`SPX`）各 1435 个交易日 —— 就在同一个地址上，本站其余示例读的都是它。

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

两行结果，一个标的一行 —— `SYN-SPX` 只作输入：它是两份报告的基准，自己不出报告。`html` 里就是那份
自包含的 tearsheet，`file_path` 则是你要求落盘时它写到了哪。

## 落盘，或者直接打开

```sql
-- 每份报告一个文件，写到当前目录（文件名由函数生成）。
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX'], 'title': symbol, 'output_dir': './'}::qs_html_report_options)) AS report
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');

-- 或者生成一份就用系统浏览器打开一份（先落临时文件；每份一个标签页）。
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX'], 'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

## 接下来去哪

- [快速开始](./getting-started/quick-start.md) —— 装好之后产出第一份报告。
- [函数](./guide/functions.md) —— 两个名字、返回形状、基准语义。
- [配置字段](./guide/options.md) —— `qs_html_report_options` 的每一个字段。
- [落盘与浏览器](./guide/output-and-browser.md) —— `output_dir`、文件命名、`open_in_browser`。
- [错误路径](./guide/error-paths.md) —— 出错时是什么样。

与「构建、测试、发布这个扩展本身」有关的内容都在侧边栏最后的一级目录
[开发指南](./development-guide/architecture/project-structure.md)里 —— 使用它完全不需要看那些。
