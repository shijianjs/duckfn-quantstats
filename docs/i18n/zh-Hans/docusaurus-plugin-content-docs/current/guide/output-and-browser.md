---
title: 落盘与浏览器
sidebar_position: 4
description: 报告去哪 —— output_dir、文件名的生成规则、open_in_browser，以及 wasm 下的差别。
---

# 落盘与浏览器

## 报告落盘（`output_dir`）

`output_dir` 给一个**目录**，每份报告一个文件，文件名由函数生成：
`<时间>-<策略名>-<基准名>-<随机尾缀>.html`（没有基准时中间那一段就不出现）。这样安排的原因是：

- **不用自己拼路径**：文件名要带「哪个标的、对哪个基准、什么时候」，这些只有函数知道；而且「一个标的
  对多个基准」时，按标的拼出来的路径必然互相覆盖；
- **不会互相覆盖**：随机尾缀 + 落盘前查一次同名文件（撞上就换一个尾缀重试），所以两次调用各写各的，
  文件也从来不会被覆盖；
- **名字认得出来**：后两段正是报告里的显示名（`strategy_title` 与基准显示名），目录里一眼能看出这是
  哪份报告。

```sql
WITH prices AS (
    SELECT * FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
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
)
ORDER BY (r).symbol;
```

返回行里的 `file_path` 就是这次真正写出去的路径（没落盘则为 `NULL`），所以「写了哪些文件」可以直接
从结果里读，不必去猜。

目录**必须已经存在**（函数不会替你创建），而且必须是**本地**目录：落盘就是一次普通的 `std::fs` 写入，
`output_dir` 因此是一个常规的文件系统路径。写成 `s3://bucket/reports` 这种 —— 那是 DuckDB 挂上的文件
系统，不是本扩展的能力 —— 会以写入错误告终。

:::note[浏览器里不会写出文件]

查询本身没有变，但 **wasm 构建一个字节都不写**：它能跑，报告照常回到 `html` 那一列，而每一行的
`file_path` 都是 `NULL`（那边整个跳过文件操作，原因见下面的 [WebAssembly](#webassembly)）。拿到本地
DuckDB 上跑同一段，文件会写进 `./`，`file_path` 里是真实路径。

:::

## 用浏览器打开报告（`open_in_browser`）

`open_in_browser` 在报告生成之后把它交给系统默认浏览器，于是终端里的一套流程不必以「现在去找那个文件、
双击打开」收尾。浏览器要的是一个真实存在的本地文件，其余都由这件事决定：

- 写了 `output_dir`：先落盘，再打开那些文件；
- 没写：先把每份报告落到系统临时目录里的
  `<临时目录>/<时间>-<策略名>-<基准名>-<随机尾缀>.html` 文件（与 `output_dir` 用的是同一套命名规则），
  再打开它。前缀是给人看的 —— 时间、`strategy_title`（没写就退回 `title`）与 `benchmark_title`
  （按下标取，缺项退回基准 symbol），文件名里放不下的字符换成 `_`；既不会覆盖已有文件，同一秒里连着出
  几份报告也不会撞名；
- `output_dir` 不是本地路径（`s3://…`、`memory://…`）时**报错**而不是静默跳过 —— 系统浏览器打不开那种
  路径。这个检查发生在渲染**之前**。

```sql
-- 这里跑不了：它需要一个浏览器进程来启动，而 wasm 构建没有。
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'benchmark': ['SPX'], 'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv');
```

浏览器是**不阻塞**地叫起来的：报告已经落盘，所以这次查询既不等待浏览器、也不关心浏览器怎么处理这个文件。
唯一会报错的情形是启动器本身起不来。

一次调用会为**每一份报告**各开一个标签页（一个标的对两个基准就是两个标签页；没写 `output_dir` 时各落
一个临时文件，至少不会互相覆盖）。

## WebAssembly

**wasm 构建一个文件都不写**：`output_dir` 在那边是「收下、然后忽略」—— 不报错、不写文件，每一行的
`file_path` 都是 `NULL`，报告本身照常回到 `html` 那一列。那边不是「试着写、写不成」，而是**整个跳过**文件
操作：DuckDB-Wasm 的文件系统不忠实 —— 任何不存在的路径都会返回一个 1 字节的幻影条目，DuckDB 自带的
`glob` / `read_text` / `file_size` 都把它报成存在，裸写偏移在那边也差一个字节 —— 于是「这个文件名空着吗」
没有一个能信的答案，那道「绝不覆盖已有文件」的保证也就无法兑现。`COPY … TO` 也替代不了它：它按格式
（CSV / JSON / parquet）导出**查询结果**，这些格式载不动一份任意长的 HTML 文档的原样字节。

所以在 wasm 上报告就留在 `html` 那一列里，交给宿主页面处理 ——
[快速开始](../getting-started/quick-start.md)把它渲染进 iframe，宿主页面也可以把这段字符串交给
blob URL 加 `window.open`，或者干脆留着。

`open_in_browser` 是另一处**有意保留**的例外：wasm 构建里没有可以启动的浏览器进程，所以那边直接忽略这个
选项 —— 不打开浏览器，也不会为此写临时文件。
