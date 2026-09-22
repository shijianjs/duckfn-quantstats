[English](README.md) | [简体中文](README.zh.md) | [开发文档](DEVELOPMENT.zh.md)

# duckfn_quantstats

一个 DuckDB 扩展（loadable extension），包装 [quantstats-rs](https://crates.io/crates/quantstats-rs)：
把「一张按日期排列的长表」一次交给它，函数内部按 `symbol` 分组，**每个标的产出一份完整的 quantstats
HTML 报告**（配置里指名基准时，一个标的对几个基准就出几份），并把这些报告（连同各自的基准、显示名与实际
落盘路径）作为**一个数组**返回 —— 全部在 SQL 里完成。

**要求 DuckDB 1.5 及以上。** 报告落盘（`output_dir`）用的宿主文件系统是 1.5 才进 DuckDB C API 的，
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

上面几条都往 `reports` 目录里写报告 —— **目录要先建好**（`mkdir reports`，函数不会替你创建）。

```sql
-- 1. 整张表一次调用：每个标的各一份报告，各写各的文件（output_dir 只给目录，文件名由函数生成）
SELECT (r).symbol, (r).strategy_title, length((r).html) AS html_bytes, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'title': symbol,
                'strategy_title': symbol,
                'output_dir': 'reports'}::qs_html_report_options)) AS r
    FROM prices
);

-- 2. 带上基准：SPX 就是表里一个普通的 symbol（配置里指名它即可），它自己不出报告
SELECT (r).symbol, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol,
                'rf': 0.04,
                'output_dir': 'reports'}::qs_html_report_options)) AS r
    FROM prices
);

-- 3. 一个标的对多个基准：每个基准一份报告（`benchmark` 是列表，顺序就是报告顺序）
--    这里把 SPX 与 GOOGL 都当基准，所以出报告的就只剩 MSFT —— 每个基准一份
--    （「一个基准多个标的」是另一回事：那只是每个标的各出一份，不必列成列表）
SELECT (r).symbol, (r).benchmark, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX', 'GOOGL'],
                'benchmark_title': ['S&P 500', 'Alphabet'],   -- 按下标对齐 benchmark
                'title': symbol,
                'strategy_title': symbol,
                'output_dir': 'reports'}::qs_html_report_options)) AS r
    FROM prices
);

-- 4. 手上已经是收益率？用另一个名字；pct_change 得放在子查询里
SELECT (r).symbol, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports(
               symbol, date, period_return,
               {'benchmark': ['SPX']}::qs_html_report_options)) AS r
    FROM (SELECT symbol, date,
                 price / lag(price) OVER (PARTITION BY symbol ORDER BY date) - 1.0 AS period_return
          FROM prices)
);
```

一份报告是几百 KB 的 HTML（内嵌十几张 SVG），`unnest(...)` 会把它们一行份地铺开，所以终端里更适合让
`output_dir`（落盘）或 `open_in_browser`（用浏览器打开）接手。只想看清单、不看 HTML 时，用
`list_transform` 只挑需要的字段即可：

```sql
-- 报告清单：只回 symbol、基准与落盘路径
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': 'reports'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM prices;
```

## 函数

两个聚合函数名字、**各一个签名**，都把「一张长表」归约成整套报告：

| 签名 | 输入 | 返回 |
| --- | --- | --- |
| `qs_html_reports(symbol, date, period_return, options)` | 收益率序列 | `STRUCT(symbol, benchmark, strategy_title, html, file_path)[]` |
| `qs_html_reports_by_prices(symbol, date, price, options)` | 价格/净值序列 | 同上；函数内部先换算成收益率 |

要点：

- **一次调用出整套报告。** SQL 里**不写 `GROUP BY`**：`symbol` 列就是分组依据，函数内部按它分组，
  每个 symbol 渲染一份完整报告。100 个标的就是 100 份完整报告（每份内嵌十几张 SVG），耗时与内存随标的
  数线性增长 —— 这是预期行为，不是性能 bug。
- **返回的是一个数组**，每个元素是 `{symbol, benchmark, strategy_title, html, file_path}`。
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
- 配置**按行求值**（见 [配置字段](#配置字段)），所以「每个标的一套标题 / 显示名 / 落盘目录」就是用
  `symbol` 列把配置拼出来，例如 `{'title': symbol, 'output_dir': 'reports'}`。
- SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给报告自己去排。
- 名字为什么是两个：`(symbol, date, price, options)` 与 `(symbol, date, period_return, options)` 的类型
  序列完全一样（`VARCHAR, DATE, DOUBLE, STRUCT`），同一个名字下无法分派。

## 价格/净值路径

`qs_html_reports_by_prices` 收的是**价格、净值这类水平值**，不是百分比变化。值列叫 `price` 是照搬
Python quantstats 的词汇：它把这类输入统称 prices，内部对看起来像价格的序列自动做 `pct_change`。
**净值（NAV）严格说不是 price**，但 quantstats 也不区分，净值序列照样当 prices 喂 —— 所以这里用同一个
键收下，不用去想该填哪个。换算规则：

- 每个 symbol 的点先按 `date` 排序，再逐点算 `price_t / price_{t-1} - 1`；
- 每个 symbol 的第一个点没有前值，丢弃；
- 前值缺失、为 0 或不是有限数时，该点**跳过**（与「`NULL` 行跳过」同一语义，不报错、也不会往序列里塞
  `inf`/`NaN`）；跳过只影响它自己，下一个点仍然和它自己的前一个点比；
- 差分后没有任何有效点的标的**不会出现在结果里**；
- 基准侧走同一套差分规则，差别是它差分后为空时报错（那个基准是配置里明确要的）；
- 同一 symbol 内同一天只应有一个点，否则差分出来的是那一天内部的变动。

用 SQL 自己写等价物别扭得多：窗口函数**不能**直接写进聚合调用（DuckDB 会报
`aggregate function calls cannot contain window function calls`），必须先在一个子查询里算好收益率：

```sql
-- 自己算：多一层子查询，而且窗口的 PARTITION BY / ORDER BY 很容易写漏
SELECT unnest(qs_html_reports(symbol, trade_date, period_return, NULL)) AS report
FROM (SELECT symbol, trade_date,
             nav / lag(nav) OVER (PARTITION BY symbol ORDER BY trade_date) - 1.0 AS period_return
      FROM nav_table);

-- 用快捷方式：价格/净值直接进去，分组交给 symbol 列
SELECT unnest(qs_html_reports_by_prices(symbol, trade_date, nav, NULL)) AS report
FROM nav_table;
```

## 配置字段

`qs_html_report_options` 的字段**全部可空**，没写的键取默认值：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | 报告标题 |
| `strategy_title` | `VARCHAR` | 该 symbol | 策略显示名；没写就退回 `symbol` 本身 |
| `benchmark_title` | `VARCHAR[]` | 基准 symbol | 基准显示名（纯展示）：**列表**，按下标与 `benchmark` 一一对应。这一项没给（列表短了、是 NULL 或空串、整个键没写）就退回**那一份报告所用的**基准 symbol；多余的表项忽略 |
| `benchmark` | `VARCHAR[]` | `NULL` | 哪些 **symbol** 当基准（**列表**，顺序即报告顺序）；它们只作输入、不出报告。一个基准也要写成 `['SPX']` |
| `rf` | `DOUBLE` | `0.0` | 无风险利率，**年化**（`0.04` = 4%），与 quantstats 的 `rf` 口径一致 |
| `periods_per_year` | `UINTEGER` | `252` | 年化周期数，必须大于 0 |
| `match_dates` | `BOOLEAN` | `true` | 是否把策略与基准的起始日对齐 |
| `output_dir` | `VARCHAR` | `NULL` | 把每份报告落盘到该**目录**，文件名由函数生成（见下）；走 DuckDB 的 VFS |
| `open_in_browser` | `BOOLEAN` | `false` | 用系统默认浏览器打开报告；没写 `output_dir` 时会先落一个临时文件（见下） |

除两个显示名之外，默认值都直接取自 quantstats-rs 的 `HtmlReportOptions::default()`，本扩展不另立一套。

两个显示名是例外，也是这套 API 能一次出几十份报告的前提：默认的 `'Strategy'` 对每一份都一样，图例里
认不出谁是谁，浏览器临时文件名也会撞成同一串前缀 —— 所以缺省时退回数据里那个名字（symbol / 基准
symbol），报告里的图例、临时文件名与结果里的 `strategy_title` 因此始终一致。

**配置是按行求值的一列**，每个 symbol 只取用一次（该 symbol 第一行出现的那份），所以：

```sql
-- 每个标的各自的标题与显示名，由 symbol 列拼出来（落盘目录由 output_dir 统一给，文件名函数自己拼）
SELECT unnest(qs_html_reports_by_prices(
           symbol, date, price,
           {'title': symbol,
            'strategy_title': symbol,
            'output_dir': 'reports'}::qs_html_report_options)) AS report
FROM prices;
```

同一个 symbol 的配置要逐行一致；`benchmark` 这一项还要求**整次调用里所有标的给出同一个列表**（元素与
顺序都一致，不一致直接报错），否则「谁把谁当基准」就没有单一答案。

想按基准维度定制文案或路径时不必纠结：`benchmark_title` 也是列表、按下标与 `benchmark` 对齐，缺哪一项就
在那一份报告里退回对应的基准 symbol；落盘路径则由函数按「时间 + 策略名 + 基准名 + 随机尾缀」自动生成
（见下），两处都自带基准那一段。

`rf` 按**年化**口径传（`0.04` = 4%），报告内部再换算成周期利率；crate 里有两处换算略有差别 ——
Sharpe（含滚动 Sharpe / Sortino）用 `(1 + rf)^(1/periods_per_year) - 1`，而指标表里的 PSR / Sortino 用
`rf / periods_per_year`。`rf = 0`（默认）时两者都退化为 0。

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
           {'title': symbol, 'output_dir': 'reports'}::qs_html_report_options)) AS report
FROM daily_returns;

-- 落盘之后直接用浏览器打开（不写 output_dir 就先落一个临时文件，再打开它；每份报告开一个标签页）
SELECT unnest(qs_html_reports(
           symbol, trade_date, daily_return,
           {'title': symbol, 'open_in_browser': true}::qs_html_report_options)) AS report
FROM daily_returns;

-- 只要清单，不要 HTML（一份报告几百 KB，铺成行会很吵）
SELECT list_transform(
           qs_html_reports(symbol, trade_date, daily_return,
               {'benchmark': ['SPX'], 'output_dir': 'reports'}::qs_html_report_options),
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

## 错误路径

| 情况 | 行为 |
| --- | --- |
| 一行都没有，或没有任何标的能出报告 | 返回 `NULL` |
| 某个标的差分/构造后没有有效点 | 该标的从结果里略过 |
| `benchmark` 指的 symbol 在表里没有行 | 报错 `no row for the benchmark symbol '…'` |
| `benchmark` 列表在各标的之间不一致（元素或顺序不同） | 报错 `every symbol must use the same benchmark list` |
| `benchmark` 列表里有空串 / NULL 元素 / 重复项 | 报错 `must not contain an empty string` / `… a NULL element` / `lists '…' twice` |
| `benchmark_title` 缺项 / 空串 / NULL / 多出来 | **不算错误**：缺项退回对应的基准 symbol，多余项忽略 |
| 价格路径：基准差分不出收益率（有效点不足两个） | 报错 `produced no returns` |
| `periods_per_year = 0` | 报错 `periods_per_year must be greater than 0` |
| `output_dir = ''` | 报错 `output_dir must not be an empty string` |
| `output_dir` 里有 NUL 字节 | 报错 `contains a NUL byte` |
| `output_dir` 写不进去（目录不存在、远端不可写等） | 报错里带 `duckfn::duck_vfs::write` 与路径 |
| `open_in_browser` 配的 `output_dir` 不是本地路径（`s3://…`、`memory://…`） | 报错 `only local file paths can be opened in a browser` |

报错信息一律以注册的函数名开头（`qs_html_reports: …`），所以一眼能看出是哪个函数的问题。

## 报告落盘（`output_dir`）

`output_dir` 给一个**目录**，每份报告一个文件，文件名由函数生成：
`<时间>-<策略名>-<基准名>-<随机尾缀>.html`（没有基准时中间那一段就不出现）。这样安排的原因是：

- **不用自己拼路径**：文件名要带「哪个标的、对哪个基准、什么时候」，这些只有函数知道；而且「一个标的
  对多个基准」时，按标的拼出来的路径必然互相覆盖；
- **不会互相覆盖**：随机尾缀 + 落盘前查一次同名文件（撞上就换一个尾缀重试），所以两次调用各写各的，
  文件也从来不会被覆盖；
- **名字认得出来**：后两段正是报告里的显示名（`strategy_title` 与基准显示名），目录里一眼能看出这是
  哪份报告。

目录**必须已经存在**（函数不会替你创建）。写文件走的是 **DuckDB 的 VFS** 而不是 `std::fs`：本地磁盘、
内存文件系统、wasm 构建里宿主真正的那个文件系统，以及装了 `httpfs` 之后的 `s3://` / `http(s)://`，
都是同一条通路、同一套语义（VFS 路径按 `/` 拼，所以 `output_dir` 写 `s3://bucket/reports` 也没问题）。

返回行里的 `file_path` 就是这次真正写出去的路径（没落盘则为 `NULL`），所以「写了哪些文件」可以直接
从结果里读，不必去猜：

```sql
SELECT (r).symbol, (r).benchmark, (r).file_path FROM (
    SELECT unnest(qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': 'reports'}::qs_html_report_options)) AS r
    FROM prices
);
```

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

浏览器是**不阻塞**地叫起来的：报告已经落盘，所以这次查询既不等待浏览器、也不关心浏览器怎么处理这个文件。
唯一会报错的情形是启动器本身起不来。

一次调用会为**每一份报告**各开一个标签页（一个标的对两个基准就是两个标签页；没写 `output_dir` 时各落
一个临时文件，至少不会互相覆盖）。

## WebAssembly

`output_dir` 走 DuckDB 的 VFS，wasm 构建与本地是同一条代码路径，文件落在该环境下 DuckDB 自己的文件系统里。

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
