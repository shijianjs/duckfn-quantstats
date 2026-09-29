[English](README.md) | [简体中文](README.zh.md) | [文档](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/)

# duckfn_quantstats

一个 DuckDB 扩展（loadable extension），包装 [quantstats-rs](https://crates.io/crates/quantstats-rs)：
把「一张按日期排列的长表」一次交给它，函数内部按 `symbol` 分组，**每个标的产出一份完整的 quantstats
HTML 报告**（配置里指名基准时，一个标的对几个基准就出几份），并把这些报告（连同各自的基准、显示名与实际
落盘路径）作为**一个数组**返回 —— 全部在 SQL 里完成。

**要求 DuckDB 1.5 及以上。** 报告落盘（`output_dir`）用的宿主文件系统是 1.5 才进 DuckDB C API 的，
所以不再保留 1.4 兼容性；本扩展在 v1.5.5 上构建与测试。

## 安装

扩展发布在 DuckDB 的[社区仓](https://duckdb.org/community_extensions/extensions/duckfn_quantstats)：

```sql
INSTALL duckfn_quantstats FROM community;   -- 只需一次，需要网络
LOAD duckfn_quantstats;
```

## 两个函数

两个聚合函数名字、**各一个签名**，都把「一张长表」归约成整套报告：

| 签名 | 输入 | 返回 |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | 收益率序列 | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | 价格/净值序列 | 同上；函数内部先换算成收益率 |

SQL 里**不写 `GROUP BY`**：`symbol` 列就是分组依据，一次调用出整套报告。`demo/prices.csv` 是一份提交进
仓库的日收盘价快照：`GOOGL`、`MSFT` 与标普 500 指数（`SPX`）：

```sql
CREATE TABLE prices AS
SELECT * FROM read_csv('https://raw.githubusercontent.com/shijianjs/duckfn-quantstats/main/demo/prices.csv');

-- 每个标的各一份报告（SPX 只作输入：它是基准，自己不出报告），落盘到当前目录；
-- output_dir 只给目录，文件名由函数生成。
SELECT (r).symbol, (r).benchmark, (r).file_path
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

## 文档

用户指南与开发笔记都在一个中英双语的 Docusaurus 站点里（[源码在 `docs/`](docs/README.md)）：

| 页面 | 内容 |
| --- | --- |
| [简介](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/intro) | 一次调用返回什么，从哪读起。 |
| [快速开始](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/getting-started/quick-start) | 安装、构建、第一份报告，以及几个坑。 |
| [函数](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/functions) | 两个名字、返回形状、基准语义、用法。 |
| [配置字段](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/options) | `qs_html_report_options` 的每一个字段。 |
| [价格/净值序列](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/price-series) | 价格路径的差分规则。 |
| [落盘与浏览器](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/output-and-browser) | `output_dir`、文件命名、`open_in_browser`、wasm。 |
| [错误路径](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/error-paths) | 每种失败与它的确切报文。 |
| [目录结构](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/getting-started/project-structure) | 模块布局与命名规则。 |
| [设计取舍](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/development/design-notes) | 为什么分组交给函数、收尾怎么落盘与打开。 |
| [依赖](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/development/dependencies) | 哪个 crate 负责哪一段，以及为什么。 |
| [测试](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/development/testing) | SQLLogicTest 用例与怎么跑。 |
| [构建与发版](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/build-and-release) | 构建路径、发版流程、wasm 目标。 |
| [社区扩展](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/community-extension) | 注册流程与需要的两个文件。 |

英文版从 <https://shijianjs.github.io/duckfn-quantstats/docs/intro> 开始。

## 从源码构建

```shell
cargo install cargo-duckdb-ext-tools   # 只需安装一次
cargo duckdb-ext build                 # -> target/debug/duckfn_quantstats.duckdb_extension

duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

`make configure` + `make debug` 是 CI 走的官方模板流程（Windows 上 `make` 要在 Git Bash 里跑）。
`Justfile` 把两者都包了一层：`just build`、`just sql "SELECT …"`、`just repl`、`just test`、`just lint`、
`just docs_csv`、`just docs_start`。

## 参与开发

仓库约定、duckfn 的资料索引与发版流程都在 [`AGENTS.md`](AGENTS.md)。
