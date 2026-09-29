---
title: 快速开始
sidebar_position: 1
description: 装好扩展，把一张价格或收益率表交给它，每个标的一份 quantstats HTML 报告。
---

# 快速开始

除了扩展本身，什么都不用编译、什么都不用安装：只要你能把 SQL 发给 DuckDB —— 命令行、Python、
任何客户端都行 —— 就能出报告。

## 前置条件

- **DuckDB 1.5 及以上。** `output_dir` 背后的宿主文件系统是 1.5 才进 DuckDB C API 的，所以不支持
  1.4；本扩展在 v1.5.5 上构建与测试。
- 任何能把 SQL 发给它的方式都行。本文档用 `duckdb` 命令行，Python / Java / Node 客户端或图形界面
  完全一样。

## 1. 安装并加载

扩展发布在 DuckDB 的[社区仓](https://duckdb.org/community_extensions/extensions/duckfn_quantstats)，
一条 `INSTALL` 就把当前平台的签名产物取回来：

```sql
INSTALL duckfn_quantstats FROM community;   -- 只需一次，需要网络
LOAD duckfn_quantstats;                     -- 之后每个会话只要这一句
```

## 2. 准备数据

两个函数都吃**长表** —— 一行一个（标的, 日期, 值）：

| 列 | 类型 | 含义 |
| --- | --- | --- |
| `symbol` | `VARCHAR` | 标的。它同时是分组依据：一个不同取值出一份报告。 |
| `date` | `DATE` | 这个值属于哪一期。 |
| 值 | `DOUBLE` | 周期收益率；给 `qs_html_reports_by_prices` 时是价格/净值。 |

基准就是同一张表里另一个 `symbol`，不需要任何 join。

本文档所有示例都跑在同一份快照上：`GOOGL`、`MSFT` 与标普 500 指数（`SPX`），各 1435 个交易日，
区间 2021-01-04 … 2026-09-21。它就发布在本站旁边：

```sql
CREATE OR REPLACE TABLE prices AS
SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

## 3. 产出报告

整张表一次调用、每个标的一份报告 —— **不写 `GROUP BY`**：`symbol` 列就是分组依据。

本页的可运行块自己构造了一小段序列，所以在浏览器里直接就能跑、不需要取任何文件。你自己写查询时，
把下面的 `series` CTE 换成上面的 `prices` 表即可。

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

两行结果 —— `SYN-SPX` 只作输入：它是两份报告的基准，自己不出报告。`html` 里是整份自包含的 tearsheet，
`file_path` 是你要求落盘时它写到了哪。

## 4. 看一眼报告

一份报告就是一个 HTML 文档、图表都内联在里面，所以可以直接在这里渲染。点 **Run**，再切上面两个标签页：

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

同一套调用跑在演示快照上，出的就是一份完整尺寸的报告 —— 也就是[首页](/)上那份用 `GOOGL` 对
标普 500 生成的报告。

## 5. 把它放到你想放的地方

一份报告是几百 KB 的 HTML，终端里一般不会想让它铺在结果集里。`output_dir` 每份报告写一个文件，
`open_in_browser` 直接打开：

```sql
-- 每份报告一个文件，写到当前目录。只给目录，文件名由函数生成，所以两次调用不会撞名。
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

-- 或者生成一份就用系统浏览器打开一份
-- （不写 output_dir 时每份先落一个临时文件；每份一个标签页）。
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM prices;
```

只要清单、不想把 HTML 拖出来？

```sql
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM prices;
```

## 常用写法

```sql
-- 只要一个标的：先过滤，仍然不需要 GROUP BY
SELECT unnest(qs_html_reports(symbol, trade_date, daily_return, NULL)) AS report
FROM daily_returns WHERE symbol = 'FUND';

-- 一个标的对两个基准 → 两份报告，每个基准一份
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'benchmark': ['SPX', 'NDX'], 'title': symbol}::qs_html_report_options)) AS report
FROM daily_returns;

-- 手上已经是收益率：另一个函数原样收下
SELECT unnest(qs_html_reports(symbol, trade_date, daily_return, NULL)) AS report
FROM daily_returns;

-- 多个标的共用一个基准是最常见的情况：不用额外写什么
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, nav,
           {'benchmark': ['SPX'], 'title': symbol}::qs_html_report_options)) AS report
FROM nav_table;
```

## 跑不通的时候

| 现象 | 原因 |
| --- | --- |
| `No function matches the given name and argument types 'qs_html_reports(VARCHAR, DATE, DOUBLE, STRUCT(title VARCHAR))'` | 配置字面量要带上类型：写成 `{…}::qs_html_report_options`。 |
| `no row for the benchmark symbol 'XYZ'` | `benchmark` 写的是 **symbol**，每一个都必须在被聚合的这张表里存在。 |
| `every symbol must use the same benchmark list` | `benchmark` 逐行读取，但整次调用必须一致。 |
| 报 `duckfn::duck_vfs::write` 之类的路径错误 | `output_dir` 指向的目录必须已经存在；函数不会替你创建。 |
| `only local file paths can be opened in a browser` | `open_in_browser` 打不开 `s3://…`、`memory://…`，那种路径只能不用这个选项、只落盘。 |

完整的行为清单见[错误路径](../guide/error-paths.md)。

## 接下来

- [函数](../guide/functions.md) —— 返回形状、排序、基准怎么变成多份报告。
- [配置字段](../guide/options.md) —— `qs_html_report_options` 的每一个字段。
- [价格/净值序列](../guide/price-series.md) —— 价格那一支对值做了什么。
- [落盘与浏览器](../guide/output-and-browser.md) —— 文件写到哪、怎么命名。

构建或测试扩展本身是另一个话题，在[开发指南](../development-guide/architecture/project-structure.md)。
