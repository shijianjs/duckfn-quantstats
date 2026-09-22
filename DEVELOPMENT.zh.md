[English](DEVELOPMENT.md) | [简体中文](DEVELOPMENT.zh.md)

# duckfn_quantstats —— 开发笔记

用户文档在 [README.zh.md](README.zh.md)：SQL 接口、配置字段、错误路径与快速上手都在那边。
本文件收的是使用方不需要的内容 —— 代码怎么分层、为什么长成现在这样、哪个 crate 负责哪一段、
以及怎么构建与测试。

duckfn 自身的通用约定（入口链路、新增函数的流程、动手前该查哪份源码）在这里**不重复**：
它们在 [AGENTS.md](AGENTS.md) 里，由它指向 duckfn clone 里的 `templates/duckfn-conventions.md`。

本项目从 DuckDB 官方 [extension-template-rs](https://github.com/duckdb/extension-template-rs) 起步，
并已按 duckfn 的骨架约定改造（入口模块、`EXTENSION_NAME`、依赖列表）。

## 目录结构

```text
src/lib.rs            原生 crate root  ->  mod extension;
src/wasm_lib.rs       wasm crate root  ->  mod extension;   （同一组 mod，镜像）
src/extension/mod.rs  ->  duckfn_entrypoint!("duckfn_quantstats");

src/extension/functions/mod.rs  ->  mod aggregate_html;
src/extension/functions/aggregate_html/
    mod.rs            两个 SQL 名字 / 四个重载的分工，以及 mod 声明
    html_returns.rs   qs_html_report            （单序列、带基准）
    html_prices.rs    qs_html_report_by_prices  （单序列、带基准）
    kind.rs           一个分支的 SQL 侧名字：宏生成的 `SQL_NAME` 加值列名
    series.rs         内部点表示、序列构造、价格差分
    slots.rs          参数槽：DuckLazySlot 的用法，以及基准列表的报错与归一化
    report.rs         收尾：点 → ReturnSeries → HTML、落盘、按需打开浏览器
    browser.rs        用系统默认浏览器打开报告（wasm 下整个功能被忽略）
src/extension/types/
    html_report_options.rs  命名 STRUCT 类型 `qs_html_report_options`
    return_point.rs         收益率侧基准列表里的一个点
    price_point.rs          价格侧基准列表里的一个点
```

扩展名 `duckfn_quantstats` 必须与 `Makefile` 的 `EXTENSION_NAME`、产物文件名一致；
`src/lib.rs` 与 `src/wasm_lib.rs` 必须声明同一组 `mod`（官方模板的 `mod lib;` 写法在嵌套模块时会报
`E0583`）。新增功能时按 duckfn 的目录分层往 `src/extension/` 下面挂。

## 设计取舍

### 两个 SQL 名字、四个重载

每个名字下两个签名只差一个 `benchmark` 参数，所以用 `overloads_name` 各注册成一个**函数集**，
按参数个数分派（`register_all_aggregate_overload` 会按名字分组，每个重载各自带参数表与返回类型）。
于是 SQL 里只见到两个名字，参数顺序固定为「数据列在前、配置在后」。

价格那一支**必须**另起一个名字：`(date, price, options)` 与 `(date, period_return, options)` 的类型序列
完全一样（都是 `DATE, DOUBLE, STRUCT`），同一个名字下无法按类型分派。

注册名不手写：属性宏给每个签名生成了一个 `SQL_NAME` 常量（设了 `overloads_name` 时是函数集名），
错误信息前缀读它 —— `kind.rs` 指过去，而不是再抄一份字面量。代价是这类函数得写成 `pub(super)`，
因为生成的模块沿用函数的可见性。

### 参数槽是惰性的

`options` 与 `benchmark` 都用 `DuckLazy` 延迟读取：每行只构造一个 O(1) 的凭证，真正的解析只在
**每组首行做一次**。这不是锦上添花：duckfn 的适配层是逐行读参数的，裸写 `Vec<...>` 会让整条基准序列被
复制「行数」次，直接退化成 O(行数 × 基准长度)。解析结果用 duckfn 的 `DuckLazySlot` 缓存在聚合状态里 ——
凭证只在产生它的那次回调内有效，状态里能留下的也就只有解析结果。

### 每组渲染一份报告

报告在 `result()` 里生成，即**每组渲染一次**。`GROUP BY` 100 个标的 = 渲染 100 份完整报告
（每份内嵌十几张 SVG），耗时与内存随分组数线性增长；同理每个分组都会各自持有一份解析好的基准点
（聚合状态不跨分组共享，这部分省不掉，能省掉的是 DuckDB 层的行展开与扫描）。

SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给 `ReturnSeries::new`。

### 为什么基准是一个列表参数

若把基准写成「同一张长表里按标签区分的行」，为了让**每个**分组都拿得到基准，基准行就必须在每个分组里
各出现一遍：100 个标的 × 1000 天 = 10 万行被物化/扫描，而基准本身只有 1000 行，还得额外配一个
「哪个标签是基准」的配置项。改成一次性传入的列表后，基准只写一次、只求值一次，策略侧仍然靠 `GROUP BY`
自然分组。

代价是它得先在子查询里聚合好：`list(...)` 本身是聚合函数，**不能内联写进聚合调用**
（DuckDB 会报 `aggregate function calls cannot be nested`），必须先聚合成单行再 `cross join` 进来。

### 基准点类型不注册命名类型

`return_point.rs` / `price_point.rs` **刻意不注册**命名类型（`create_type` 默认关闭）：
`list({'date': ..., 'period_return': ...})` 产出的匿名 `STRUCT(date DATE, period_return DOUBLE)[]`
与它的字段名、顺序、类型完全一致，可以直接匹配，调用方不必再写一次 cast，SQL 名字面上也就只多一个类型
（配置那个）。实测确认：不写 `create_type` 时 `#[derive(DuckStruct)]` 不提交任何注册项。

字段名固定是 `date` / `period_return`（价格侧是 `price`）：duckfn 的 `DuckStruct` 派生不支持字段级改名，
字段的 SQL 名就是 Rust 字段名原样。也刻意避开 SQL 关键字 —— `return` 渲染正常但 `returns`、`value`
会被 DuckDB 加上引号，用 `period_return` 则到处都不用引号。

字段写成 `Option<...>`：列表里某一项缺日期或缺值时读成 `None`，聚合函数跳过它 —— 与策略侧
「`date` 或值列是 NULL 的行整行跳过」保持同一语义。`Option<T>` 的逻辑类型与 `T` 相同，所以类型形状不变。

### 配置类型

`#[duck(create_type = true)]` 让 duckfn 在扩展加载期执行
`CREATE TYPE IF NOT EXISTS "qs_html_report_options" AS STRUCT(...)`，SQL 里才写得出
`{'title': 'x'}::qs_html_report_options`（以及 JSON 形式）。

字段**全部**是 `Option<T>`，这是硬要求：DuckDB 的 struct 字面量缺字段时会补 NULL，而 duckfn 读到
「非 Option 字段为 NULL」时会让**整个 struct** 变成 NULL。那样用户写的 `{'rf': 0.1}` 会整体退化成默认值，
他设的 rf 被静默丢掉。

`to_report_options()` 的做法是「从 quantstats-rs 的 `HtmlReportOptions::default()` 出发、逐字段覆盖
用户显式写了的那些」，默认值因此只有一份真相。`output` 刻意不交给它转发：quantstats-rs 落盘用的是
`std::fs`，而本扩展要的是 DuckDB 的 VFS（见下），所以路径由 `report.rs` 在拿到渲染结果后自己写。

`periods_per_year = 0` 与 `output = ''` 是配置错误，在这里就报掉，不会变成后面一次莫名其妙的文件系统调用。

## 报告落盘

`output` 用 duckfn 的便捷层 `duck_vfs::write_string` 落盘，也就是经 **DuckDB 的 VFS** 而不是 `std::fs`：

- 本地磁盘、内存文件系统、wasm 构建里宿主真正的那个文件系统，以及装了 `httpfs` 之后的 `s3://` /
  `http(s)://`，都是同一条通路、同一套语义；
- 这也是聚合函数唯一写得进去的路子：DuckDB 的 C API 不给聚合函数客户端上下文（没有 bind 回调，也没有
  `duckdb_aggregate_function_get_client_context`），所以 duckfn 在注册期留了一条自有长连接，
  从这里现取 `ClientContext` → `FileSystem`。

`output` 是**替换**：写完之后文件里恰好就是这份报告，哪怕它以前更长。这件事由 duckfn 负责 ——
DuckDB 的 C API 没有 truncate（`DUCKDB_FILE_FLAG_CREATE` 只表示「需要时新建」，映射到 `O_TRUNC` /
`CREATE_ALWAYS` 的标志在 C++ 侧），所以 duckfn 的 `duck_vfs` 层会先把更长的旧文件清零再写正文，
本扩展只调 `duck_vfs::write_string`。于是 `read_text()` 读回来的与函数返回值逐字节一致 —— 测试里用
`md5` 钉住了这一点，其中就包含「旧文件更长」这个用例。

## 用浏览器打开报告

`open_in_browser` 在报告生成之后把它交给系统默认浏览器，于是终端里的一套流程不必以「现在去找那个文件、
双击打开」收尾。浏览器要的是一个真实存在的本地文件，其余都由这件事决定：

- 写了 `output`：先落盘，再打开那个文件；
- 没写：先把报告落到系统临时目录里的
  `<临时目录>/<时间>-<策略名>-<基准名>-<随机尾缀>.html`，文件由
  [tempfile](https://crates.io/crates/tempfile) 新建。前缀是给人看的 —— 时间（因此按名字排序临时目录，
  排出来正好是时间顺序）、`strategy_title`（没写就退回 `title`）与 `benchmark_title`；这两段显示名过一遍
  [sanitize-filename](https://crates.io/crates/sanitize-filename)，文件名的合法性（非法字符、控制字符、
  Windows 保留设备名、结尾的点与空格）由它按规则处理、换成 `_`，本扩展只在其上补两条自己的策略：空格并
  成 `_`、每段最多 32 个字符（名字里有两段，而平台上限是 255）。随机尾缀、`.html` 后缀（决定系统把它交给
  浏览器渲染而不是当成下载）以及「这个名字当时一定是空的」都由 tempfile 负责，所以既不会覆盖已有文件，
  同一秒里连着出几份报告也不会撞名；
- `output` 不是本地路径（`s3://…`、`memory://…`）时**报错**而不是静默跳过 —— 系统浏览器打不开那种路径。
  这个检查发生在渲染**之前**。

「把浏览器叫起来」是 [open](https://crates.io/crates/open) 的事，而且用的是不阻塞的 `that_detached`：
报告已经落盘了，所以这次查询既不等待浏览器、也不关心浏览器怎么处理这个文件。Windows 上就是一次
`ShellExecute` 调用（开了 `shellexecute-on-windows`，不用它默认的那条 PowerShell 路线）；macOS 与其他
平台则是 `open` / `xdg-open` 加上它自带的后备序列。唯一会报错的情形是启动器本身起不来。

这个选项是为单份报告准备的。`GROUP BY` 下每个分组都会被依次打开 —— 而且没写 `output` 时每个分组各自落
一个临时文件，至少不会互相覆盖。

## WebAssembly

`output` 走 DuckDB 的 VFS，wasm 构建与本地是同一条代码路径，文件落在该环境下 DuckDB 自己的文件系统里。
这替代了早先的行为（在 `wasm32-unknown-emscripten` 下直接丢掉路径，因为那边的 `std::fs` 没有可写的
文件系统）。

`open_in_browser` 是唯一一处**有意保留**的平台分支，也是本扩展仅剩的平台相关代码（`browser.rs`）：wasm
构建里没有可以启动的浏览器进程，所以那边直接忽略这个选项 —— 不打开浏览器，也不会为此写临时文件。报告
字符串原样返回给宿主，展示是宿主页面的事：blob URL + `window.open`、`<iframe>`，或者别的。它背后那三个
crate 声明在 `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` 下，wasm 构建连编都不编它们 ——
这其实是硬要求：`open` 根本没有 emscripten 的实现，编不过。

`just build_wasm`（`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`）
能正常编过；运行时行为由 DuckDB 的 VFS 决定，而不是由本扩展决定。

## 依赖

- [duckfn](https://crates.io/crates/duckfn)：属性宏，把普通 Rust 函数注册成 DuckDB 函数。开了两个 feature：
  `duckdb-1-5`（`output` 用的宿主文件系统 `duckfn::duck_vfs` 在它下面）与 `chrono`（时间包装类型的互转，
  如 `DuckDate::to_naive_date`）。属性宏还会为每个签名生成 `SQL_NAME` 常量 —— 真正注册进 DuckDB 的名字 ——
  错误信息前缀读它，不再手抄一份 `overloads_name` 字面量。
- [quack-rs](https://crates.io/crates/quack-rs)：DuckDB C API 绑定，`duckfn_entrypoint!` 展开出的代码直接引用它。
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys)：只取头文件，开启 `loadable-extension`，
  因此**不需要在本地编译 DuckDB**。版本下限是 `>= 1.10500`（DuckDB 1.5.0：这个 crate 把 DuckDB 版本
  编码成 `1.<major*10000 + minor*100 + patch>.0`，1.5.5 就是 `1.10505.0`），因为客户端上下文 /
  文件系统那部分 C API 是 1.5 才有的。
- [quantstats-rs](https://crates.io/crates/quantstats-rs)：报告本体。它的公开 API 里只有 `html()`
  一个可调用入口（`mod stats` 是私有的，`compute_performance_metrics` 拿不到），所以两个重载都基于它，
  不自己重算指标 —— 那会与报告里的数字形成两套真相。
- [open](https://crates.io/crates/open)、[tempfile](https://crates.io/crates/tempfile) 与
  [sanitize-filename](https://crates.io/crates/sanitize-filename)：`open_in_browser` 的三件事 —— 把浏览器
  叫起来、新建一个不重名的临时文件、以及知道平台认哪些文件名。**只用于非 wasm 目标**（见上面的
  WebAssembly 一节），所以它们挂在 target 专属的依赖表里，而不是主依赖表。
- [chrono](https://crates.io/crates/chrono)：本扩展自己用的是**本地时间** —— `open_in_browser` 拼的临时文件名
  以 `%Y%m%d-%H%M%S` 时间戳开头（`chrono::Local`）。日期那头由 duckfn 的 `chrono` feature 换算
  （`DuckDate::to_naive_date`），它产出的 `NaiveDate` 正是 quantstats-rs 的 `ReturnSeries::new` 要的；
  三个 crate 共用同一个 chrono 0.4。

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

仓库根目录的 `Justfile` 把两者都包了一层：`just build`、`just sql "SELECT …"`、`just repl`、
`just test`、`just lint`、`just build_wasm`。

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

提交前：`cargo clippy --all-targets -- -D warnings`。
