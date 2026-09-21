[English](README.en.md) | [简体中文](README.md)

# duckfn_quantstats

用 [duckfn](https://crates.io/crates/duckfn) 写的 DuckDB 扩展（loadable extension）：在 SQL 里直接产出 quantstats 报告。

本项目从 DuckDB 官方 [extension-template-rs](https://github.com/duckdb/extension-template-rs) 起步，
并已按 duckfn 的骨架约定改造（入口模块、`EXTENSION_NAME`、依赖列表）。

## 函数

两个聚合函数，都把「按日期排列的收益」归约成一份完整的 quantstats HTML 报告（`VARCHAR`）：

| 函数 | 说明 |
| --- | --- |
| `duckfn_quantstats_html(date, period_return, options)` | 单序列报告，一行 = 一个周期。 |
| `duckfn_quantstats_html_benchmark(date, period_return, benchmark, options)` | 带基准报告：策略侧逐行聚合，基准侧是一个**一次性传入的列表参数**。 |

- 配置参数 `options` **固定在参数列表最后**（数据列在前、配置在后）。它是**可空**配置，类型是加载期建好的
  命名 STRUCT 类型 `duckfn_quantstats_html_options`；传 `NULL` 表示全默认。
- `date` 是 `DATE`，`period_return` 是按周期计的收益率（`DOUBLE`）。两列任一为 `NULL` 的行会被**整行跳过**，
  与其它 SQL 聚合函数一致。
- `benchmark` 是 `STRUCT(date DATE, period_return DOUBLE)[]`。它是 `NULL`、是空列表、或列表里没有任何有效点时
  都会**报错** —— 函数名里就有 benchmark，没基准就该改用 `duckfn_quantstats_html`。
- `options` 与 `benchmark` 都用 `DuckLazy` 延迟读取：每行只构造一个 O(1) 的凭证，真正的解析只在**每组首行做一次**。
  这不是锦上添花：duckfn 的适配层是逐行读参数的，裸写 `Vec<...>` 会让整条基准序列被复制「行数」次，
  直接退化成 O(行数 × 基准长度)。
- 该组一行都没有 → 返回 `NULL`（不是空串，也不是报错）。
- 报告在 `result()` 里生成，即**每组渲染一次**。`GROUP BY` 100 个标的 = 渲染 100 份完整报告
  （每份内嵌十几张 SVG），耗时与内存随分组数线性增长；同理每个分组都会各自持有一份解析好的基准点
  （聚合状态不跨分组共享，这部分省不掉，能省掉的是 DuckDB 层的行展开与扫描）。
- SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给 `ReturnSeries::new`。

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
| `rf` | `DOUBLE` | `0.0` | 无风险利率（按周期计，不是年化） |
| `periods_per_year` | `UINTEGER` | `252` | 年化周期数，必须大于 0 |
| `match_dates` | `BOOLEAN` | `true` | 是否把策略与基准的起始日对齐 |
| `output` | `VARCHAR` | `NULL` | 额外把 HTML 落盘到该路径（wasm 下忽略，见下） |

默认值直接取自 quantstats-rs 的 `HtmlReportOptions::default()`，本扩展不另立一套。

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

-- 带基准：基准单独聚合成一行，再 cross join 进来（基准只写一次、只求值一次）
WITH benchmark AS (
    SELECT list({'date': trade_date, 'period_return': daily_return}) AS series
    FROM benchmark_returns
)
SELECT s.fund,
       duckfn_quantstats_html_benchmark(
           s.trade_date, s.daily_return, benchmark.series,
           {'title': 'My Fund', 'benchmark_title': 'S&P 500'}::duckfn_quantstats_html_options) AS html
FROM strategy_returns s, benchmark
GROUP BY s.fund;

-- 顺带落盘一份
SELECT duckfn_quantstats_html(
           trade_date, daily_return,
           {'title': 'My Fund', 'output': 'fund.html'}::duckfn_quantstats_html_options)
FROM daily_returns;
```

`benchmark` 也可以写成标量子查询（实测可行），效果与 `cross join` 单行一样：

```sql
SELECT fund,
       duckfn_quantstats_html_benchmark(
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
| `periods_per_year = 0` | 报错 `periods_per_year must be greater than 0` |
| `output = ''` | 报错 `output must not be an empty string` |

### WebAssembly

`output` 在 `wasm32-unknown-emscripten` 下**被忽略**：那边没有可写的文件系统，写盘只会在运行时抛
IO 错误、把整条查询带崩，所以扩展干脆不把路径交给 quantstats-rs，HTML 照常返回。
`just build_wasm`（`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`）
能正常编过。

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

- [duckfn](https://crates.io/crates/duckfn)：属性宏，把普通 Rust 函数注册成 DuckDB 函数。
- [quack-rs](https://crates.io/crates/quack-rs)：DuckDB C API 绑定，`duckfn_entrypoint!` 展开出的代码直接引用它。
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys)：只取头文件，开启 `loadable-extension`，
  因此**不需要在本地编译 DuckDB**。
- [quantstats-rs](https://crates.io/crates/quantstats-rs)：报告本体。它的公开 API 里只有 `html()`
  一个可调用入口（`mod stats` 是私有的，`compute_performance_metrics` 拿不到），所以两个聚合函数都基于它，
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
make test_debug     # 或 make test_release
```

新增函数时至少覆盖：正常值、`NULL`、边界值、错误路径（`statement error`）。
