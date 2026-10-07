[English](README.md) | [简体中文](README.zh.md) | [文档](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/)

# duckfn_quantstats

[![上周下载量](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fcommunity-extensions.duckdb.org%2Fdownloads-last-week.json&query=%24.duckfn_quantstats&label=%E4%B8%8A%E5%91%A8%E4%B8%8B%E8%BD%BD%E9%87%8F&color=blue&logo=duckdb)](https://duckdb.org/community_extensions/extensions/duckfn_quantstats)

一个 DuckDB 扩展，用 **SQL 直接出 quantstats HTML tearsheet**：把一张按日期排列的长表交给它，
函数内部按 `symbol` 分组，每个标的产出一份完整的报告（配置里指名基准时，一个标的对几个基准就出几份），
并把这些报告（连同各自的基准、显示名与实际落盘路径）作为**一个数组**返回。

不用 Python、不用 `pip install quantstats`、不用 notebook：它是 DuckDB 扩展，所以 DuckDB 能跑的地方
它就能跑 —— Linux、macOS、Windows，以及浏览器里的 DuckDB-Wasm。

**要求 DuckDB 1.5 及以上**（本扩展在 v1.5.6 上构建与测试）。

## 安装

扩展发布在 DuckDB 的[社区仓](https://duckdb.org/community_extensions/extensions/duckfn_quantstats)：

```sql
INSTALL duckfn_quantstats FROM community;   -- 只需一次，需要网络
LOAD duckfn_quantstats;
```

## 一条查询，每个标的一份报告

两个聚合函数名字、**各一个签名**：

| 签名 | 输入 | 返回 |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | 收益率序列 | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | 价格/净值序列 | 同上；函数内部先换算成收益率 |

SQL 里**不写 `GROUP BY`**：`symbol` 列就是分组依据，基准只是同一张表里另一个 symbol。
`demo/prices.csv`（仓库里有一份，文档站也发布了一份）是 `GOOGL`、`MSFT` 与标普 500 指数（`SPX`）
的日收盘价快照：

```sql
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
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

三份报告、三个文件；`SPX` 只作输入 —— 它是基准，自己不出报告。
[一份生成好的示例报告](https://shijianjs.github.io/duckfn-quantstats/demo/qs_report_GOOGL-S&P_500.html)
（`GOOGL` 对 S&P 500）就放在文档站旁边。

### 翻译报告，并在每个元素上挂一句说明

写上 `lang` 配置项，报告**自己说的那些固定文本** —— 分节标题、指标名、图表标题、月份列、图例 —— 就会换成
那个语言，而且每一条都带一句简短说明，浏览器会从它所在的那个元素上浮出（原生提示，没有 JavaScript，报告依然
是一个静态文件）。内置 `en`、`zh-CN`、`ja`、`de`、`fr`、`es` 六种；不写 `lang` 就一个字符都不动，写
`'en'` 则文字不变、只补英文说明。

这张表活在当前 DuckDB 进程里，由两个函数改写：

```sql
SELECT qs_set_translation('zh-CN', [
    {'key': 'metric.sharpe', 'label': '夏普比率', 'description': '每单位波动换来的超额收益 ⚖️'}
]);                                       -- -> true；label 传 NULL 即删除，description 传 NULL 保留原说明
SELECT * FROM qs_list_translations();     -- 整张表：lang、key、label、description
```

不做持久化：重新加载扩展就恢复内置数据。覆盖范围（以及有意不翻译的：你自己的 `symbol`、`title` 与显示名）
见[翻译](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/translation)。

## 文档

用户指南与开发笔记都在一个中英双语的 Docusaurus 站点里（[源码在 `docs/`](docs/README.md)）：

| 页面 | 内容 |
| --- | --- |
| [简介](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/intro) | 一次调用返回什么，从哪读起。 |
| [快速开始](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/getting-started/quick-start) | 装好并产出第一份报告。 |
| [函数](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/functions) | 两个名字、返回形状、基准语义。 |
| [配置字段](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/options) | `qs_html_report_options` 的每一个字段。 |
| [价格/净值序列](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/price-series) | 价格路径的差分规则。 |
| [落盘与浏览器](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/output-and-browser) | `output_dir`、文件命名、`open_in_browser`。 |
| [翻译](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/translation) | `lang` 配置项、浮出说明、`qs_set_translation` 与 `qs_list_translations`。 |
| [错误路径](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/guide/error-paths) | 每种失败与它的确切报文。 |
| [架构](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/development-guide/architecture/project-structure) | 代码怎么分层、为什么这样、依赖了谁。 |
| [构建、测试与发布](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/development-guide/build/build-and-release) | 构建路径、测试、发版流程、wasm 目标。 |
| [发布](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/development-guide/publishing/community-extension) | 函数描述与社区扩展注册。 |

**使用方**只需要前八页，后三页是给贡献者的。英文版从
<https://shijianjs.github.io/duckfn-quantstats/docs/intro> 开始。

## 参与开发

仓库约定、duckfn 的资料索引与发版流程都在 [`AGENTS.md`](AGENTS.md)；
代码本身怎么组织见[开发指南](https://shijianjs.github.io/duckfn-quantstats/zh-Hans/docs/development-guide/architecture/project-structure)。
