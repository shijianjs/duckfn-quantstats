---
title: 简介
sidebar_position: 1
slug: /intro
description: duckfn_quantstats 是什么、一次调用返回什么，以及从哪读起。
---

# 简介

`duckfn_quantstats` 是一个 DuckDB [loadable extension](https://duckdb.org/docs/stable/extensions/extension_development)，
包装 [quantstats-rs](https://crates.io/crates/quantstats-rs)：把**一张按日期排列的长表**一次交给它，
函数内部按 `symbol` 分组，**每个标的产出一份完整的 quantstats HTML 报告**（配置里指名多个基准时，
一个标的对几个基准就出几份），并把这些报告（连同各自的基准、显示名与实际落盘路径）作为**一个数组**
返回 —— 全部在 SQL 里完成。

**要求 DuckDB 1.5 及以上。** 报告落盘（`output_dir`）用的宿主文件系统是 1.5 才进 DuckDB C API 的，
所以不再保留 1.4 兼容性；本扩展在 v1.5.5 上构建与测试。

```sql
INSTALL duckfn_quantstats FROM community;   -- 只需一次，需要网络
LOAD duckfn_quantstats;
```

整个 SQL 接口就是两个聚合函数名字，**各一个签名**：

| 签名 | 输入 | 返回 |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | 收益率序列 | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | 价格/净值序列 | 同上；函数内部先换算成收益率 |

一次调用出整套报告。下面这段在你的浏览器里就能跑 —— 站点会预加载仓库最新 Release 上的扩展，
所以这里不用写 `LOAD`：

```sql {"type":"duckfn","show":"table"}
-- 两个标的的合成序列，这块示例不需要任何数据文件
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

::::note[这份站点是新的]
这里的页面是为本扩展写的，跟代码一起维护。发现遗漏或错误，欢迎在仓库里提；站点自身的维护约定
（目录、命令、翻译、部署）在 `docs/README.md`。
::::

## 仓库里都有什么

| 路径 | 是什么 |
| --- | --- |
| `src/extension/mod.rs` | 入口：`duckfn_entrypoint!("duckfn_quantstats")` 加上模块树。 |
| `src/extension/functions/aggregate_html/` | 两个注册函数，以及「渲染 / 落盘 / 打开浏览器」那一整套收尾。 |
| `src/extension/types/` | SQL 侧的类型：命名 STRUCT `qs_html_report_options` 与返回行。 |
| `test/sql/quantstats/` | SQLLogicTest 用例，按关注点分文件（收益率、价格、错误、输出内容）。 |
| `demo/prices.csv` | 提交进仓库的行情快照（`GOOGL`、`MSFT`、`SPX`），快速上手用它。 |
| `Justfile` | 日常命令：构建、跑 SQL、repl、测试、lint、文档、发版。 |
| `.github/workflows/` | 构建矩阵、打 tag 时的 GitHub Release、以及本站在 GitHub Pages 上的部署。 |
| `community-extension/` | [社区扩展](https://duckdb.org/community_extensions/list_of_extensions)注册需要的两个文件。 |
| `docs/` | 本站：Docusaurus，中英双语。 |

## 接下来去哪

- [快速开始](./getting-started/quick-start.md) —— 装好扩展（或从源码构建），然后在 SQL 里产出第一份报告。
- [目录结构](./getting-started/project-structure.md) —— 入口点、函数与 SQL 类型各自放在哪，以及把它们绑在一起的命名规则。
- [函数](./guide/functions.md) —— 两个名字、返回形状、基准语义。
- [配置字段](./guide/options.md) —— `qs_html_report_options` 的每一个字段。
- [设计取舍](./development/design-notes.md) —— 为什么分组交给函数、为什么基准是表里的 symbol。
- [构建与发版](./build-and-release.md) —— 两条构建路径与发版流程。
- [社区扩展](./community-extension.md) —— 发布到 DuckDB 的社区仓。
