[English](README.md) | [简体中文](README.zh.md) | [开发文档](DEVELOPMENT.zh.md)

# duckfn_quantstats

一个 DuckDB 扩展（loadable extension），包装 [quantstats-rs](https://crates.io/crates/quantstats-rs)：
把「按日期排列的一条序列」交给 `GROUP BY` 分组，直接在 SQL 里为每个分组产出一份完整的 quantstats HTML 报告。

**要求 DuckDB 1.5 及以上。** 报告落盘（`output`）用的宿主文件系统是 1.5 才进 DuckDB C API 的，
所以不再保留 1.4 兼容性；本扩展在 v1.5.5 上构建与测试。

本文件是用户文档。开发笔记（目录结构、设计取舍、依赖选择与测试）在 [DEVELOPMENT.zh.md](DEVELOPMENT.zh.md)。

## 快速上手

`demo/prices.csv` 是一份提交进仓库的日收盘价快照：`GOOGL`、`MSFT` 与标普 500 指数（`SPX`），
各 1435 个交易日，区间 2021-01-04 … 2026-09-21，三者交易日历完全一致。下面这段直接复制粘贴就能跑
（扩展需要先构建，见最后的「构建与加载」）；`read_csv` 自己走 HTTP 取回文件（DuckDB 1.5 自带 `https://`
读取，不需要 `httpfs`，也不需要 API key）：

```sql
LOAD './target/debug/duckfn_quantstats.duckdb_extension';

CREATE TABLE prices AS
SELECT * FROM read_csv('https://raw.githubusercontent.com/shijianjs/duckfn-quantstats/main/demo/prices.csv');
-- 拉不动（比如国内网络）？同一个文件有 jsDelivr 镜像：
--   read_csv('https://cdn.jsdelivr.net/gh/shijianjs/duckfn-quantstats@main/demo/prices.csv')
-- 已经 clone 了仓库？直接 read_csv('demo/prices.csv')
```

```sql
-- 1. 出一份报告：落盘到文件，然后直接用浏览器打开
SELECT qs_html_report_by_prices(date, price,
           {'title': 'Microsoft', 'output': 'msft.html', 'open_in_browser': true}::qs_html_report_options)
FROM prices WHERE symbol = 'MSFT';

-- 2. 对指数：基准以「价格点列表」的形式一次性传入
WITH benchmark AS (
    SELECT list({'date': date, 'price': price}) AS series FROM prices WHERE symbol = 'SPX'
)
SELECT qs_html_report_by_prices(
           p.date, p.price, b.series,
           {'title': 'Microsoft', 'benchmark_title': 'S&P 500', 'rf': 0.04, 'open_in_browser': true}::qs_html_report_options) AS html
FROM prices p, benchmark b
WHERE p.symbol = 'MSFT';

-- 3. 手上已经是收益率？用另一个函数；pct_change 得放在子查询里
SELECT qs_html_report(date, period_return, {'title': 'Microsoft', 'rf': 0.04}::qs_html_report_options)
FROM (SELECT date, price / lag(price) OVER (ORDER BY date) - 1.0 AS period_return
      FROM prices WHERE symbol = 'MSFT');

-- 4. 每个标的一份报告
SELECT symbol,
       qs_html_report_by_prices(date, price, {'title': symbol, 'rf': 0.04}::qs_html_report_options) AS html
FROM prices
GROUP BY symbol;
```

一份报告是几百 KB 的 HTML（内嵌十几张 SVG），第 4 条还会每个标的打印一份，所以终端里更适合让 `output`
（或 `open_in_browser`）接手。

## 函数

两个聚合函数名字、各自**两个重载**（靠参数个数分派），都把「按日期排列的收益」归约成一份完整的
quantstats HTML 报告（`VARCHAR`）：

| 签名 | 输入 | 说明 |
| --- | --- | --- |
| `qs_html_report(date, period_return, options)` | 收益率序列 | 单序列报告，一行 = 一个周期。 |
| `qs_html_report(date, period_return, benchmark, options)` | 收益率序列 | 带基准报告，基准侧是一个**一次性传入的列表参数**。 |
| `qs_html_report_by_prices(date, price, options)` | 价格/净值序列 | 单序列报告，函数内部换算成收益率。 |
| `qs_html_report_by_prices(date, price, benchmark, options)` | 价格/净值序列 | 带基准报告，两侧都是价格/净值。 |

SQL 里就只有这两个名字（各自的两个重载是一个函数集）：

- 配置参数 `options` **固定在参数列表最后**（数据列在前、配置在后）。它是**可空**配置，类型是加载期建好的
  命名 STRUCT 类型 `qs_html_report_options`；传 `NULL` 表示全默认。
- `date` 是 `DATE`，`period_return` 是按周期计的收益率（`DOUBLE`），`price` 是当天的价格或净值（`DOUBLE`）。
  两列任一为 `NULL` 的行会被**整行跳过**，与其它 SQL 聚合函数一致。
- `benchmark` 是 `STRUCT(date DATE, <值列名> DOUBLE)[]`（`period_return` 或 `price`）。它是 `NULL`、是空列表、
  或列表里没有任何有效点时都会**报错** —— 这一支重载就是为带基准的场景存在的，只想要单序列报告就少传这个参数。
- 该组一行都没有 → 返回 `NULL`（不是空串，也不是报错）。
- **每组一份报告。** `GROUP BY` 100 个标的 = 渲染 100 份完整报告（每份内嵌十几张 SVG），
  耗时与内存随分组数线性增长。
- SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给报告自己去排。

## 价格/净值路径

`qs_html_report_by_prices` 收的是**价格、净值这类水平值**，不是百分比变化。值列叫 `price` 是照搬
Python quantstats 的词汇：它把这类输入统称 prices，内部对看起来像价格的序列自动做 `pct_change`。
**净值（NAV）严格说不是 price**，但 quantstats 也不区分，净值序列照样当 prices 喂 —— 所以这里用同一个键收下，
不用去想该填哪个。换算规则：

- 组内先按 `date` 排序，再逐点算 `price_t / price_{t-1} - 1`；
- 每组的第一个点没有前值，丢弃；
- 前值缺失、为 0 或不是有限数时，该点**跳过**（与「`NULL` 行跳过」同一语义，不报错、也不会往序列里塞
  `inf`/`NaN`）；跳过只影响它自己，下一个点仍然和它自己的前一个点比；
- 差分后没有任何有效点时返回 `NULL`（例如整组只有一个点）；
- 同一分组内同一天只应有一个点，否则差分出来的是那一天内部的变动。

用 SQL 自己写等价物别扭得多：窗口函数**不能**直接写进聚合调用（DuckDB 会报
`aggregate function calls cannot contain window function calls`），必须先在一个子查询里算好收益率：

```sql
-- 自己算：多一层子查询，而且窗口的 PARTITION BY / ORDER BY 很容易写漏
SELECT fund, qs_html_report(trade_date, period_return, NULL) AS html
FROM (
    SELECT fund, trade_date,
           nav / lag(nav) OVER (PARTITION BY fund ORDER BY trade_date) - 1.0 AS period_return
    FROM nav_table
)
GROUP BY fund;

-- 用快捷方式：价格/净值直接进去，分区交给 GROUP BY
SELECT fund, qs_html_report_by_prices(trade_date, nav, NULL) AS html
FROM nav_table
GROUP BY fund;
```

## 配置字段

`qs_html_report_options` 的字段**全部可空**，没写的键取默认值：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | 报告标题 |
| `strategy_title` | `VARCHAR` | `'Strategy'` | 策略显示名 |
| `benchmark_title` | `VARCHAR` | `NULL` | 基准显示名（纯展示） |
| `rf` | `DOUBLE` | `0.0` | 无风险利率，**年化**（`0.04` = 4%），与 quantstats 的 `rf` 口径一致 |
| `periods_per_year` | `UINTEGER` | `252` | 年化周期数，必须大于 0 |
| `match_dates` | `BOOLEAN` | `true` | 是否把策略与基准的起始日对齐 |
| `output` | `VARCHAR` | `NULL` | 额外把 HTML 落盘到该路径，走 DuckDB 的 VFS（见下） |
| `open_in_browser` | `BOOLEAN` | `false` | 用系统默认浏览器打开报告；没写 `output` 时会先落一个临时文件（见下） |

默认值直接取自 quantstats-rs 的 `HtmlReportOptions::default()`，本扩展不另立一套。

`rf` 按**年化**口径传（`0.04` = 4%），报告内部再换算成周期利率；crate 里有两处换算略有差别 ——
Sharpe（含滚动 Sharpe / Sortino）用 `(1 + rf)^(1/periods_per_year) - 1`，而指标表里的 PSR / Sortino 用
`rf / periods_per_year`。`rf = 0`（默认）时两者都退化为 0。

## 用法

```sql
-- 单序列：全部默认配置
SELECT qs_html_report(trade_date, daily_return, NULL) FROM daily_returns;

-- 单序列：只写关心的几个键；struct 字面量必须显式转成配置类型
SELECT symbol,
       qs_html_report(
           trade_date, daily_return,
           {'title': 'My Fund', 'rf': 0.02}::qs_html_report_options) AS html
FROM daily_returns
GROUP BY symbol;

-- 带基准：先聚合成单行，再 cross join 进来
WITH benchmark AS (
    SELECT list({'date': trade_date, 'period_return': daily_return}) AS series
    FROM benchmark_returns
)
SELECT s.fund,
       qs_html_report(
           s.trade_date, s.daily_return, benchmark.series,
           {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::qs_html_report_options) AS html
FROM strategy_returns s, benchmark
GROUP BY s.fund;

-- 价格/净值序列：直接用快捷方式，不用自己写 pct_change 窗口
SELECT fund,
       qs_html_report_by_prices(
           trade_date, nav, {'title': 'My Fund'}::qs_html_report_options) AS html
FROM nav_table
GROUP BY fund;

-- 顺带落盘一份（走 DuckDB 的 VFS，所以 wasm 下同样可用）
SELECT qs_html_report(
           trade_date, daily_return,
           {'title': 'My Fund', 'output': 'fund.html'}::qs_html_report_options)
FROM daily_returns;

-- 落盘之后直接用浏览器打开（不写 output 就先落一个临时文件，再打开它）
SELECT qs_html_report(
           trade_date, daily_return,
           {'title': 'My Fund', 'open_in_browser': true}::qs_html_report_options)
FROM daily_returns;
```

基准参数是一整个列表，所以得先聚合成单行：`list(...)` 本身就是聚合函数，**不能内联写进聚合调用**
（DuckDB 会报 `aggregate function calls cannot be nested`）。`benchmark` 也可以写成标量子查询（实测可行），
效果与 `cross join` 单行一样：

```sql
SELECT fund,
       qs_html_report(
           trade_date, daily_return,
           (SELECT list({'date': trade_date, 'period_return': daily_return}) FROM benchmark_returns),
           NULL)
FROM strategy_returns
GROUP BY fund;
```

## 三个必须知道的行为

- **配置的 struct 字面量必须显式写 `::qs_html_report_options`。** 不写的话它是匿名的
  `STRUCT(title VARCHAR)`，字段个数与配置类型不同，DuckDB 会直接说找不到匹配的函数。
- **`'...'::JSON::qs_html_report_options` 要把 8 个键写全**（DuckDB 的 JSON→STRUCT 转换
  不允许缺键），所以推荐直接用 struct 字面量。
- **基准点的键名固定是 `date` / `period_return`**（价格侧是 `price`）。它和匿名
  `STRUCT(date DATE, period_return DOUBLE)` 完全一致，所以**不需要 cast**；只有当列类型不是 `DATE` /
  `DOUBLE` 时才要显式转一下：`{'date': trade_date::DATE, 'period_return': daily_return::DOUBLE}`。

## 错误路径

| 情况 | 行为 |
| --- | --- |
| 组内没有任何有效行 | 返回 `NULL` |
| 基准参数为 `NULL` | 报错 `the benchmark list must not be NULL` |
| 基准是空列表，或列表里没有任何有效点 | 报错 `the benchmark list is empty` |
| 基准列表里有整体为 `NULL` 的元素 | 报错 `cannot read the benchmark list` |
| `qs_html_report_by_prices`：基准价格点不足两个，差分不出收益率 | 报错 `the benchmark prices produced no returns` |
| `periods_per_year = 0` | 报错 `periods_per_year must be greater than 0` |
| `output = ''` | 报错 `output must not be an empty string` |
| `output` 路径里有 NUL 字节 | 报错 `contains a NUL byte` |
| `output` 路径写不进去（目录不存在、远端不可写等） | 报错里带 `duckfn::duck_vfs::write` 与路径 |
| `open_in_browser` 配的 `output` 不是本地路径（`s3://…`、`memory://…`） | 报错 `only local file paths can be opened in a browser` |

## 报告落盘（`output`）

`output` 把渲染好的 HTML 写出去，走的是 **DuckDB 的 VFS** 而不是 `std::fs`：本地磁盘、内存文件系统、
wasm 构建里宿主真正的那个文件系统，以及装了 `httpfs` 之后的 `s3://` / `http(s)://`，
都是同一条通路、同一套语义。

`output` 是**替换**：写完之后文件里恰好就是这份报告，哪怕它以前更长。

路径是配置的一部分，所以在 `GROUP BY` 下要给每个分组各自的文件（`'report-' || symbol || '.html'`），
而不是所有分组都指向同一个路径。

## 用浏览器打开报告（`open_in_browser`）

`open_in_browser` 在报告生成之后把它交给系统默认浏览器，于是终端里的一套流程不必以「现在去找那个文件、
双击打开」收尾。浏览器要的是一个真实存在的本地文件，其余都由这件事决定：

- 写了 `output`：先落盘，再打开那个文件；
- 没写：先把报告落到系统临时目录里的
  `<临时目录>/<时间>-<策略名>-<基准名>-<随机尾缀>.html`。前缀是给人看的 —— 时间、`strategy_title`
  （没写就退回 `title`）与 `benchmark_title`，文件名里放不下的字符换成 `_`；既不会覆盖已有文件，
  同一秒里连着出几份报告也不会撞名；
- `output` 不是本地路径（`s3://…`、`memory://…`）时**报错**而不是静默跳过 —— 系统浏览器打不开那种路径。
  这个检查发生在渲染**之前**。

浏览器是**不阻塞**地叫起来的：报告已经落盘，所以这次查询既不等待浏览器、也不关心浏览器怎么处理这个文件。
唯一会报错的情形是启动器本身起不来。

这个选项是为单份报告准备的。`GROUP BY` 下每个分组都会被依次打开 —— 而且没写 `output` 时每个分组各自落
一个临时文件，至少不会互相覆盖。

## WebAssembly

`output` 走 DuckDB 的 VFS，wasm 构建与本地是同一条代码路径，文件落在该环境下 DuckDB 自己的文件系统里。

`open_in_browser` 是唯一一处**有意保留**的例外：wasm 构建里没有可以启动的浏览器进程，所以那边直接忽略这个
选项 —— 不打开浏览器，也不会为此写临时文件。报告字符串原样返回给宿主，展示是宿主页面的事：
blob URL + `window.open`、`<iframe>`，或者别的。

## 构建与加载

日常迭代用 `cargo-duckdb-ext-tools`（全局 cargo 子命令，不给项目加依赖）：

```shell
cargo install cargo-duckdb-ext-tools   # 只需安装一次
cargo duckdb-ext build                 # -> target/debug/duckfn_quantstats.duckdb_extension
```

官方模板那条 `make` 流程仍然保留（CI 与 sqllogictest 走它），首次需要 `make configure` 建 Python venv：

```shell
make configure   # 只做一次
make debug       # -> build/debug/extension/duckfn_quantstats/duckfn_quantstats.duckdb_extension
```

`make release` 是带优化的同一套流程。Windows 上 `make` 需要在 Git Bash 里跑。

扩展基于 DuckDB 的 unstable C API 构建，加载时必须加 `-unsigned`：

```shell
duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

开发循环（Justfile 的 recipe、clippy、wasm 构建）见 [DEVELOPMENT.zh.md](DEVELOPMENT.zh.md)。
