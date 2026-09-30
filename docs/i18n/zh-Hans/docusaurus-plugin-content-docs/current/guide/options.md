---
title: 配置字段
sidebar_position: 2
description: qs_html_report_options 的每个字段、默认值，以及「按行求值」如何让每个标的各有各的配置。
---

# 配置字段

两个函数的第四个参数是一个 `qs_html_report_options` STRUCT —— 扩展在加载期建好的命名类型。它的字段
**全部可空**，没写的键取默认值：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | 报告标题 |
| `strategy_title` | `VARCHAR` | 该 symbol | 策略显示名；没写就退回 `symbol` 本身 |
| `benchmark_title` | `VARCHAR[]` | 基准 symbol | 基准显示名（纯展示）：**列表**，按下标与 `benchmark` 一一对应。这一项没给（列表短了、是 NULL 或空串、整个键没写）就退回**那一份报告所用的**基准 symbol；多余的表项忽略 |
| `benchmark` | `VARCHAR[]` | `NULL` | 哪些 **symbol** 当基准（**列表**，顺序即报告顺序）；它们只作输入、不出报告。一个基准也要写成 `['SPX']` |
| `rf` | `DOUBLE` | `0.0` | 无风险利率，**年化**（`0.04` = 4%），与 quantstats 的 `rf` 口径一致 |
| `periods_per_year` | `UINTEGER` | `252` | 年化周期数，必须大于 0 |
| `match_dates` | `BOOLEAN` | `true` | 是否把策略与基准的起始日对齐 |
| `output_dir` | `VARCHAR` | `NULL` | 把每份报告落盘到该**本地目录**，文件名由函数生成（见[落盘与浏览器](./output-and-browser.md)）；wasm 构建不落盘 |
| `open_in_browser` | `BOOLEAN` | `false` | 用系统默认浏览器打开报告；没写 `output_dir` 时会先落一个临时文件（见[落盘与浏览器](./output-and-browser.md)） |

除两个显示名之外，默认值都直接取自 quantstats-rs 的 `HtmlReportOptions::default()`，本扩展不另立一套。

## 两个显示名是例外

也是这套 API 能一次出几十份报告的前提：默认的 `'Strategy'` 对每一份都一样，图例里认不出谁是谁，
浏览器临时文件名也会撞成同一串前缀 —— 所以缺省时退回数据里那个名字（symbol / 基准 symbol），
报告里的图例、临时文件名与结果里的 `strategy_title` / `benchmark_title` 因此始终一致。

## 配置是按行求值的一列

每个 symbol 只取用第一行出现的那份，所以用 `symbol` 列把配置拼出来，就是「每个标的各有一套标题与
显示名」，基准显示名也按下标各自对齐：

```sql {"type":"duckfn","show":"table"}
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT (r).symbol, (r).strategy_title, (r).benchmark, (r).benchmark_title,
       length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,               -- 逐行求值：这份报告自己的标题
                'strategy_title': symbol,      -- 逐行求值：图例上的名字
                'rf': 0.04,
                'periods_per_year': 252}::qs_html_report_options)) AS r
    FROM prices
)
ORDER BY (r).symbol;
```

`strategy_title` 与 `benchmark_title` 都会回显在结果行里，所以图例上写的是什么不必去 HTML 里抠 ——
而且它们正是生成的文件名所用的那两份名字。

同一个 symbol 的配置要逐行一致；`benchmark` 这一项还要求**整次调用里所有标的给出同一个列表**（元素与
顺序都一致，不一致直接报错），否则「谁把谁当基准」就没有单一答案。[错误路径](./error-paths.md)里
每一种都有一个可运行的例子。

## 哪些项会被校验

`benchmark` 决定「谁把谁当基准」与「谁出报告」，所以它是**严格的**：元素不能是 NULL、不能是空串、
不能在同一个列表里重复。`benchmark_title` 只用于展示，所以宽松（缺项退回，多余项忽略）。
`periods_per_year = 0` 与 `output_dir = ''` 也作为配置错误报出 —— 都发生在渲染与文件系统调用之前。
完整清单见[错误路径](./error-paths.md)。

## `rf` 是年化口径

`0.04` 表示 4%，报告内部再换算成周期利率；crate 里有两处换算略有差别 —— Sharpe（含滚动 Sharpe / Sortino）
用 `(1 + rf)^(1/periods_per_year) - 1`，而指标表里的 PSR / Sortino 用 `rf / periods_per_year`。
`rf = 0`（默认）时两者都退化为 0。
