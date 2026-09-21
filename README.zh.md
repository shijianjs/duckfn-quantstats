[English](README.md) | [简体中文](README.zh.md)

# duckfn_quantstats

用 [duckfn](https://crates.io/crates/duckfn) 写的 DuckDB 扩展（loadable extension）：在 SQL 里直接产出 quantstats 报告。

本项目从 DuckDB 官方 [extension-template-rs](https://github.com/duckdb/extension-template-rs) 起步，
并已按 duckfn 的骨架约定改造（入口模块、`EXTENSION_NAME`、依赖列表）。

**要求 DuckDB 1.5 及以上。** 报告落盘（`output`）用的宿主文件系统是 1.5 才进 DuckDB C API 的，
所以不再保留 1.4 兼容性；本扩展在 v1.5.5 上构建与测试。

## 函数

两个聚合函数名字、各自**两个重载**（靠参数个数分派），都把「按日期排列的收益」归约成一份完整的
quantstats HTML 报告（`VARCHAR`）：

| 签名 | 输入 | 说明 |
| --- | --- | --- |
| `duckfn_quantstats_html(date, period_return, options)` | 收益率序列 | 单序列报告，一行 = 一个周期。 |
| `duckfn_quantstats_html(date, period_return, benchmark, options)` | 收益率序列 | 带基准报告，基准侧是一个**一次性传入的列表参数**。 |
| `duckfn_quantstats_html_prices(date, price, options)` | 价格/净值序列 | 单序列报告，函数内部换算成收益率。 |
| `duckfn_quantstats_html_prices(date, price, benchmark, options)` | 价格/净值序列 | 带基准报告，两侧都是价格/净值。 |

前两个由 `overloads_name` 注册成一个函数集、后两个注册成另一个，SQL 里各占一个名字。

- 配置参数 `options` **固定在参数列表最后**（数据列在前、配置在后）。它是**可空**配置，类型是加载期建好的
  命名 STRUCT 类型 `duckfn_quantstats_html_options`；传 `NULL` 表示全默认。
- `date` 是 `DATE`，`period_return` 是按周期计的收益率（`DOUBLE`），`price` 是当天的价格或净值（`DOUBLE`）。
  两列任一为 `NULL` 的行会被**整行跳过**，与其它 SQL 聚合函数一致。
- `benchmark` 是 `STRUCT(date DATE, <值列名> DOUBLE)[]`（`period_return` 或 `price`）。它是 `NULL`、是空列表、
  或列表里没有任何有效点时都会**报错** —— 这一支重载就是为带基准的场景存在的，只想要单序列报告就少传这个参数。
- 价格那一支**必须另起一个名字**：`(date, price, options)` 与 `(date, period_return, options)` 的类型序列
  完全一样（都是 `DATE, DOUBLE, STRUCT`），同一个名字下无法按类型分派。
- `options` 与 `benchmark` 都用 `DuckLazy` 延迟读取：每行只构造一个 O(1) 的凭证，真正的解析只在**每组首行做一次**。
  这不是锦上添花：duckfn 的适配层是逐行读参数的，裸写 `Vec<...>` 会让整条基准序列被复制「行数」次，
  直接退化成 O(行数 × 基准长度)。解析结果用 duckfn 的 `DuckLazySlot` 缓存在聚合状态里 —— 凭证只在产生它的
  那次回调内有效，状态里能留下的也就只有解析结果。
- 该组一行都没有 → 返回 `NULL`（不是空串，也不是报错）。
- 报告在 `result()` 里生成，即**每组渲染一次**。`GROUP BY` 100 个标的 = 渲染 100 份完整报告
  （每份内嵌十几张 SVG），耗时与内存随分组数线性增长；同理每个分组都会各自持有一份解析好的基准点
  （聚合状态不跨分组共享，这部分省不掉，能省掉的是 DuckDB 层的行展开与扫描）。
- SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给 `ReturnSeries::new`。

### 价格/净值路径

`duckfn_quantstats_html_prices` 收的是**价格、净值这类水平值**，不是百分比变化。值列叫 `price` 是照搬
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
SELECT fund, duckfn_quantstats_html(trade_date, period_return, NULL) AS html
FROM (
    SELECT fund, trade_date,
           nav / lag(nav) OVER (PARTITION BY fund ORDER BY trade_date) - 1.0 AS period_return
    FROM nav_table
)
GROUP BY fund;

-- 用快捷方式：价格/净值直接进去，分区交给 GROUP BY
SELECT fund, duckfn_quantstats_html_prices(trade_date, nav, NULL) AS html
FROM nav_table
GROUP BY fund;
```

### 为什么基准是一个列表参数

若把基准写成「同一张长表里按标签区分的行」，为了让**每个**分组都拿得到基准，基准行就必须在每个分组里
各出现一遍：100 个标的 × 1000 天 = 10 万行被物化/扫描，而基准本身只有 1000 行，还得额外配一个
「哪个标签是基准」的配置项。改成一次性传入的列表后，基准只写一次、只求值一次，策略侧仍然靠 `GROUP BY`
自然分组。

代价是它得先在子查询里聚合好：`list(...)` 本身是聚合函数，**不能内联写进聚合调用**
（DuckDB 会报 `aggregate function calls cannot be nested`），必须先聚合成单行再 `cross join` 进来。

### 配置字段

`duckfn_quantstats_html_options` 的字段**全部可空**，没写的键取默认值：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `title` | `VARCHAR` | `'Strategy Tearsheet'` | 报告标题 |
| `strategy_title` | `VARCHAR` | `'Strategy'` | 策略显示名 |
| `benchmark_title` | `VARCHAR` | `NULL` | 基准显示名（纯展示） |
| `rf` | `DOUBLE` | `0.0` | 无风险利率，**年化**（`0.04` = 4%），与 quantstats 的 `rf` 口径一致 |
| `periods_per_year` | `UINTEGER` | `252` | 年化周期数，必须大于 0 |
| `match_dates` | `BOOLEAN` | `true` | 是否把策略与基准的起始日对齐 |
| `output` | `VARCHAR` | `NULL` | 额外把 HTML 落盘到该路径，走 DuckDB 的 VFS（见下） |

默认值直接取自 quantstats-rs 的 `HtmlReportOptions::default()`，本扩展不另立一套。

`rf` 按**年化**口径传（`0.04` = 4%），报告内部再换算成周期利率；crate 里有两处换算略有差别 ——
Sharpe（含滚动 Sharpe / Sortino）用 `(1 + rf)^(1/periods_per_year) - 1`，而指标表里的 PSR / Sortino 用
`rf / periods_per_year`。`rf = 0`（默认）时两者都退化为 0。

### 用法

```sql
-- 单序列：全部默认配置
SELECT duckfn_quantstats_html(trade_date, daily_return, NULL) FROM daily_returns;

-- 单序列：只写关心的几个键；struct 字面量必须显式转成配置类型
SELECT symbol,
       duckfn_quantstats_html(
           trade_date, daily_return,
           {'title': 'My Fund', 'rf': 0.02}::duckfn_quantstats_html_options) AS html
FROM daily_returns
GROUP BY symbol;

-- 带基准：多传一个 benchmark 参数（4 参重载）；基准单独聚合成一行再 cross join 进来
WITH benchmark AS (
    SELECT list({'date': trade_date, 'period_return': daily_return}) AS series
    FROM benchmark_returns
)
SELECT s.fund,
       duckfn_quantstats_html(
           s.trade_date, s.daily_return, benchmark.series,
           {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::duckfn_quantstats_html_options) AS html
FROM strategy_returns s, benchmark
GROUP BY s.fund;

-- 价格/净值序列：直接用快捷方式，不用自己写 pct_change 窗口
SELECT fund,
       duckfn_quantstats_html_prices(
           trade_date, nav, {'title': 'My Fund'}::duckfn_quantstats_html_options) AS html
FROM nav_table
GROUP BY fund;

-- 顺带落盘一份（走 DuckDB 的 VFS，所以 wasm 下同样可用）
SELECT duckfn_quantstats_html(
           trade_date, daily_return,
           {'title': 'My Fund', 'output': 'fund.html'}::duckfn_quantstats_html_options)
FROM daily_returns;
```

`benchmark` 也可以写成标量子查询（实测可行），效果与 `cross join` 单行一样：

```sql
SELECT fund,
       duckfn_quantstats_html(
           trade_date, daily_return,
           (SELECT list({'date': trade_date, 'period_return': daily_return}) FROM benchmark_returns),
           NULL)
FROM strategy_returns
GROUP BY fund;
```

### 三个必须知道的行为

- **配置的 struct 字面量必须显式写 `::duckfn_quantstats_html_options`。** 不写的话它是匿名的
  `STRUCT(title VARCHAR)`，字段个数与配置类型不同，DuckDB 会直接说找不到匹配的函数 —— 注册这个
  命名类型就是为了这一步 cast。
- **`'...'::JSON::duckfn_quantstats_html_options` 要把 7 个键写全**（DuckDB 的 JSON→STRUCT 转换
  不允许缺键），所以推荐直接用 struct 字面量。
- **基准点的键名固定是 `date` / `period_return`**（duckfn 的 `DuckStruct` 派生不支持字段改名）。
  它和匿名 `STRUCT(date DATE, period_return DOUBLE)` 完全一致，所以**不需要 cast**；只有当列类型不是
  `DATE` / `DOUBLE` 时才要显式转一下：`{'date': trade_date::DATE, 'period_return': daily_return::DOUBLE}`。

### 错误路径

| 情况 | 行为 |
| --- | --- |
| 组内没有任何有效行 | 返回 `NULL` |
| 基准参数为 `NULL` | 报错 `the benchmark list must not be NULL` |
| 基准是空列表，或列表里没有任何有效点 | 报错 `the benchmark list is empty` |
| 基准列表里有整体为 `NULL` 的元素 | 报错 `cannot read the benchmark list` |
| `duckfn_quantstats_html_prices`：基准价格点不足两个，差分不出收益率 | 报错 `the benchmark prices produced no returns` |
| `periods_per_year = 0` | 报错 `periods_per_year must be greater than 0` |
| `output = ''` | 报错 `output must not be an empty string` |
| `output` 路径里有 NUL 字节 | 报错 `contains a NUL byte` |
| `output` 指向的旧文件**比报告更长** | 报错 `is longer`（见下） |

### 报告落盘（`output`）

`output` 把渲染好的 HTML 经 **DuckDB 的 VFS**（`duckfn::with_file_system`）写出，而不是 `std::fs`：

- 本地磁盘、内存文件系统、wasm 构建里宿主真正的那个文件系统，以及装了 `httpfs` 之后的 `s3://` /
  `http(s)://`，都是同一条通路、同一套语义；
- 这也是聚合函数唯一写得进去的路子：DuckDB 的 C API 不给聚合函数客户端上下文（没有 bind 回调，也没有
  `duckdb_aggregate_function_get_client_context`），所以 duckfn 在注册期留了一条自有长连接，
  从这里现取 `ClientContext` → `FileSystem`。

**已知限制：不能 truncate。** DuckDB 的 C API 只有「需要时新建」（`DUCKDB_FILE_FLAG_CREATE`），映射到
`O_TRUNC` / `CREATE_ALWAYS` 的那个标志（`FILE_FLAGS_FILE_CREATE_NEW`）只在 C++ 侧。于是覆盖一个
**更长的旧文件**会在尾部留下残渣 —— 这里选择**报错**而不是静默写下「报告 + 垃圾」：请删掉文件或换个
路径。写到不存在的文件、等长或更短的文件都正常，`read_text()` 读回来的与函数返回值逐字节一致
（测试里用 `md5` 钉住了这一点）。

路径是配置的一部分，所以在 `GROUP BY` 下要给每个分组各自的文件（`'report-' || symbol || '.html'`），
而不是所有分组都指向同一个路径。

### WebAssembly

代码里已经没有任何 wasm 专属分支：`output` 走 DuckDB 的 VFS，wasm 构建与本地是同一条代码路径。
这替代了早先的行为（在 `wasm32-unknown-emscripten` 下直接丢掉路径，因为那边的 `std::fs` 没有可写的
文件系统）—— 现在文件落在该环境下 DuckDB 自己的文件系统里，本扩展不做任何特殊处理。
`just build_wasm`（`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`）
能正常编过；运行时行为由 DuckDB 的 VFS 决定，而不是由本扩展决定。

## 入口链路

```text
src/lib.rs           ->  mod extension;
src/wasm_lib.rs      ->  mod extension;   （同一组 mod，镜像）
src/extension/mod.rs ->  duckfn_entrypoint!("duckfn_quantstats");
```

扩展名 `duckfn_quantstats` 必须与 `Makefile` 的 `EXTENSION_NAME`、产物文件名一致；
`src/lib.rs` 与 `src/wasm_lib.rs` 必须声明同一组 `mod`（官方模板的 `mod lib;` 写法在嵌套模块时会报 `E0583`）。
新增功能时按 duckfn 的目录分层往 `src/extension/` 下面挂。

## 依赖

- [duckfn](https://crates.io/crates/duckfn)：属性宏，把普通 Rust 函数注册成 DuckDB 函数。开启了它的
  `duckdb-1-5` feature —— `output` 用的宿主文件系统（`duckfn::with_file_system`）就在这个 feature 下。
- [quack-rs](https://crates.io/crates/quack-rs)：DuckDB C API 绑定，`duckfn_entrypoint!` 展开出的代码直接引用它。
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys)：只取头文件，开启 `loadable-extension`，
  因此**不需要在本地编译 DuckDB**。版本下限是 `>= 1.10500`（DuckDB 1.5.0：这个 crate 把 DuckDB 版本
  编码成 `1.<major*10000 + minor*100 + patch>.0`，1.5.5 就是 `1.10505.0`），因为客户端上下文 /
  文件系统那部分 C API 是 1.5 才有的。
- [quantstats-rs](https://crates.io/crates/quantstats-rs)：报告本体。它的公开 API 里只有 `html()`
  一个可调用入口（`mod stats` 是私有的，`compute_performance_metrics` 拿不到），所以两个重载都基于它，
  不自己重算指标 —— 那会与报告里的数字形成两套真相。
- [chrono](https://crates.io/crates/chrono)：`ReturnSeries` 要的是 `NaiveDate`，而 duckfn 的 `DuckDate`
  只存「自 1970-01-01 起的天数」，换算在扩展里做。

## 构建

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

## 加载

扩展基于 DuckDB 的 unstable C API 构建，必须加 `-unsigned`：

```shell
duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

## 测试

测试用 SQLLogicTest 格式写在 `test/sql/` 下：

```shell
make debug && make test    # make test 不会自动重新构建，改完 Rust 必须先 make debug
```

测试分两类，分工不要混：

| 文件 | 覆盖什么 | 额外依赖 |
| --- | --- | --- |
| `test/sql/quantstats/html_report.test`、`html_report_benchmark.test`、`html_report_prices.test` | **行为**：配置解析与默认值、`NULL` 行跳过、空输入返回 `NULL`、多线程 `combine` 一致性（单线程 vs 4 线程 md5 相等）、价格路径与 `lag()` 差分的结果逐字节一致、错误路径 | 无 |
| `test/sql/quantstats/html_report_values.test` | **输出内容**：用 [webbed](https://duckdb.org/community_extensions/extensions/webbed) 的 XPath 解析生成的 HTML，断言标题、统计区间、`rf` 回显、逐行指标数字、图表/表格数量、带基准时多出的那一列 | 社区扩展 `webbed` |

`webbed` 的安装写在测试文件里（`INSTALL webbed FROM community;`），**首次运行需要网络**，之后走本机
DuckDB 扩展缓存。不想要这个依赖就删掉该文件，其余文件不受影响。

用 XPath 断言长 HTML 比 `length(...) > N` 有用，也比 md5 相等更容易定位失败原因：

```sql
-- 报告标题与统计区间
SELECT html_extract_text(html, '//h1')[1] FROM report;
-- -> My Fund 2 Jan, 2024 - 12 Jan, 2024

-- 指标表里某一行的策略列（列顺序是「基准在前、策略在后」）
SELECT html_extract_text(html, '//div[@id="right"]/table[1]//tr[td[1]="Sharpe"]/td[2]')[1] FROM report;
-- -> 4.93
```

注意 webbed 的 `html_extract_text(html, xpath)` 返回 `VARCHAR[]`（**所有**匹配项）：取单个值要 `[1]`，
断言一整组可以用 `array_to_string(..., ' | ')` 或 `array_length(...)`。

新增函数时至少覆盖：正常值、`NULL`、边界值、错误路径（`statement error`）。
