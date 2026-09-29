---
title: 快速开始
sidebar_position: 1
description: 装好扩展，把一张价格或收益率表交给它，每个标的一份 quantstats HTML 报告。
---

# 快速开始

除了扩展本身，什么都不用编译、什么都不用安装：只要你能把 SQL 发给 DuckDB —— 命令行、Python、
任何客户端都行 —— 就能出报告。本页每一块都跑在同一份快照上，而且**每一块都能在这里的浏览器里跑**。

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

基准就是同一张表里另一个 `symbol`，不需要任何 join。示例都读本站旁边发布的那份快照：
`GOOGL`、`MSFT` 与标普 500 指数（`SPX`），各 1435 个交易日，区间 2021-01-04 … 2026-09-21：

```sql {"type":"duckfn","show":"table"}
SELECT count(*) AS rows,
       count(DISTINCT symbol) AS instruments,
       min(date) AS first_day,
       max(date) AS last_day
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

## 3. 产出报告

整张表一次调用、每个标的一份报告 —— **不写 `GROUP BY`**：`symbol` 列就是分组依据。

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

两行结果 —— `SPX` 只作输入：它是两份报告的基准，自己不出报告。`html` 里是整份自包含的 tearsheet，
`file_path` 是你要求落盘时它写到了哪。

## 4. 看一眼报告

一份报告就是一个 HTML 文档、图表都内联在里面，所以可以直接在这里渲染。点 **Run**，再切上面两个标签页：

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

`GOOGL` 那份完整尺寸的报告也放在文档站旁边 —— 就是[首页](/)上那一份。

## 5. 把它放到你想放的地方

一份报告是几百 KB 的 HTML，终端里一般不会想让它铺在结果集里。`output_dir` 每份报告写一个文件，
`file_path` 会告诉你每份写到了哪：

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
```

:::note[这块为什么不能在这里跑]

查询本身是真的 —— 落盘在 DuckDB 支持的每个平台上都能用 —— 但浏览器构建演示不了：它的文件系统不管
问哪个文件名都回答「已被占用」，于是那道防止互相覆盖的保险永远找不到空位，调用以
`could not find a free report file name in 8 attempts` 结束。把同一段拿到你自己的 DuckDB 里跑，
文件就会出现在 `./`。

:::

在桌面端，`open_in_browser` 能省掉去文件管理器里翻文件这一步；但它需要一个浏览器进程来启动，
所以在这一页里它是空操作（`file_path` 也是空的）：

```sql
-- 这里跑不了：wasm 里没有可以把报告交给它的浏览器进程。
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX'], 'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

只要清单、不想把 HTML 拖出来？

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

## 常用写法

只要一个标的 —— 先过滤，仍然不需要 `GROUP BY`：

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

一个标的对两个基准 —— 两份报告，每个基准一份：

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

手上已经是收益率 —— 另一个函数原样收下（这里是从同一份价格里差分出来的，也就是价格那一支内部做的事）：

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

多个标的共用一个基准是最常见的情况 —— 不用额外写什么：

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

## 跑不通的时候

最值得早点认清的两种失败 —— 下面两块**本来就该失败**，点 Run 看到的是报文而不是结果：

```sql {"type":"duckfn","show":"table","expect":"error"}
-- 配置字面量必须带上类型，否则 DuckDB 会去找一个并不存在的 STRUCT(title VARCHAR) 重载。
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT unnest(qs_html_reports_by_prices(symbol, date, price, {'title': symbol})) AS report
FROM prices;
```

```sql {"type":"duckfn","show":"table","expect":"error"}
-- 'benchmark' 写的是 symbol，每一个都必须在被聚合的这张表里存在。
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price, {'benchmark': ['NDX']}::qs_html_report_options)) AS report
FROM prices;
```

| 其它现象 | 原因 |
| --- | --- |
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
