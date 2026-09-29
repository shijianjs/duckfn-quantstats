---
title: 函数
sidebar_position: 1
description: 两个聚合函数名字、一次调用返回什么、基准怎么变成多份报告，以及可以直接照抄的用法。
---

# 函数

两个聚合函数名字、**各一个签名**，都把「一张长表」归约成整套报告：

| 签名 | 输入 | 返回 |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | 收益率序列 | `STRUCT(symbol, benchmark, strategy_title, benchmark_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | 价格/净值序列 | 同上；函数内部先换算成收益率 |

要点：

- **一次调用出整套报告。** SQL 里**不写 `GROUP BY`**：`symbol` 列就是分组依据，函数内部按它分组，
  每个 symbol 渲染一份完整报告。100 个标的就是 100 份完整报告（每份内嵌十几张 SVG），耗时与内存随标的
  数线性增长 —— 这是预期行为，不是性能 bug。
- **返回的是一个数组**，每个元素是 `{symbol, benchmark, strategy_title, benchmark_title, html, file_path}`。
  `unnest(...)` 把它铺成行，`list_transform(...)` 只取需要的字段，也可以 `(qs_html_reports(...))[1].html`
  直接取某一份。
- **顺序按 `symbol` 升序**，同一标的内按 `benchmark` 列表给出的顺序；与输入顺序、线程数都无关。
- **基准是表里一个或多个普通的 symbol**：配置里的 `benchmark` 是**列表**（`['SPX', 'NDX']`），列到的
  symbol 只作输入、**不出现在结果里**。报告只能带一个基准，所以「一个标的对 M 个基准」就是**M 份报告**：
  同一 `symbol` 出现 M 行，靠 `benchmark` 字段区分（一个基准多个标的则只是每个标的各出一份）。
  想让基准自己也出一份报告、或给不同标的配不同基准，自己过滤后分几次调用即可。
- `symbol` 是 `VARCHAR`，`date` 是 `DATE`，`period_return` 是按周期计的收益率（`DOUBLE`），`price`
  是当天的价格或净值（`DOUBLE`）。这四者任一为 `NULL` 的行会被**整行跳过**（`symbol` 是空串的行同样
  跳过），与其它 SQL 聚合函数一致。
- 配置参数 `options` **固定在参数列表最后**且必给 —— 不需要配置就写 `NULL`。它是**可空**配置，类型是
  加载期建好的命名 STRUCT 类型 `qs_html_report_options`。
- 配置**按行求值**（见[配置字段](./options.md)），所以「每个标的一套标题 / 显示名 / 落盘目录」就是用
  `symbol` 列把配置拼出来，例如 `{'title': symbol, 'output_dir': './'}`。
- SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给报告自己去排。
- 名字为什么是两个：`(symbol, date, price, options)` 与 `(symbol, date, period_return, options)` 的类型
  序列完全一样（`VARCHAR, DATE, DOUBLE, STRUCT`），同一个名字下无法分派。

## 用法

```sql
-- 一个标的、不带基准：把它的行挑出来再聚合即可
SELECT unnest(qs_html_reports(symbol, trade_date, daily_return, NULL)) AS report
FROM daily_returns WHERE symbol = 'FUND';

-- 整张表、带基准：基准是表里一个普通的 symbol
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'benchmark': ['SPX'],
            'title': symbol,
            'strategy_title': symbol,
            'benchmark_title': ['S&P 500']}::qs_html_report_options)) AS report
FROM daily_returns;

-- 一个标的对两个基准：该标的两份报告，每份只跟它自己那个基准比
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'benchmark': ['SPX', 'NDX'], 'title': symbol}::qs_html_report_options)) AS report
FROM daily_returns;

-- 价格/净值序列：直接用快捷方式，不用自己写 pct_change 窗口
SELECT unnest(qs_html_reports_by_prices(
           symbol, trade_date, nav, {'title': symbol}::qs_html_report_options)) AS report
FROM nav_table;

-- 顺带落盘（只给目录，文件名函数自己拼；走 DuckDB 的 VFS，所以 wasm 下同样可用）
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'title': symbol, 'output_dir': './'}::qs_html_report_options)) AS report
FROM daily_returns;

-- 落盘之后直接用浏览器打开（不写 output_dir 就先落一个临时文件，再打开它；每份报告开一个标签页）
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM daily_returns;

-- 只要清单，不要 HTML（一份报告几百 KB，铺成行会很吵）
SELECT list_transform(
           qs_html_reports(symbol, trade_date, daily_return,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM daily_returns;
```

## 几个必须知道的行为

- **配置的 struct 字面量必须显式写 `::qs_html_report_options`。** 不写的话它是匿名的
  `STRUCT(title VARCHAR)`，匹配不上任何签名，DuckDB 会直接说找不到函数。
- **`'...'::JSON::qs_html_report_options` 要把 9 个键写全**（DuckDB 的 JSON→STRUCT 转换不允许缺键），
  所以推荐直接用 struct 字面量。
- **`benchmark` 写的是 symbol 名（列表），不是值。** 每一项都必须是表里 `symbol` 列的某个取值，整次调用
  一致；列到的 symbol 只当基准，不出现在返回的数组里。一个基准也要写成 `['SPX']`。
- **`benchmark_title` 也是列表，按下标与 `benchmark` 对齐**：`['S&P 500', 'Nasdaq 100']` 分别对应第一个与
  第二个基准。它只是展示，所以很宽松 —— 缺项（列表短了、NULL、空串）就退回那一份报告所用的基准 symbol，
  多出来的表项直接忽略。
- **结果的顺序按 `symbol` 升序、同一标的内按基准列表顺序**，不随输入顺序或线程数变化。
- **演示快照很适合拿来试这些** —— 直接用
  `read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')` 读本站这份即可；
  里面有什么见[演示数据](../development-guide/demo-data/demo-dataset.md)。
