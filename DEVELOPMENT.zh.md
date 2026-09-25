[English](DEVELOPMENT.md) | [简体中文](DEVELOPMENT.zh.md)

# duckfn_quantstats —— 开发笔记

用户文档在 [README.zh.md](README.zh.md)：SQL 接口、配置字段、错误路径与快速上手都在那边。
本文件收的是使用方不需要的内容 —— 代码怎么分层、为什么长成现在这样、哪个 crate 负责哪一段、
以及怎么构建与测试。

duckfn 自身的通用约定（入口链路、新增函数的流程、动手前该查哪份源码）在这里**不重复**：
它们在 [AGENTS.md](AGENTS.md) 里，那里也写明了 duckfn 的文档与示例扩展在本机 cargo registry 里的位置
（0.0.11 起随 crate 发布，不需要 clone duckfn 仓库）。

本项目从 DuckDB 官方 [extension-template-rs](https://github.com/duckdb/extension-template-rs) 起步，
并已按 duckfn 的骨架约定改造（入口模块、`EXTENSION_NAME`、依赖列表）。

## 目录结构

```text
src/lib.rs            原生 crate root  ->  mod extension;
src/wasm_lib.rs       wasm crate root  ->  mod extension;   （同一组 mod，镜像）
src/extension/mod.rs  ->  duckfn_entrypoint!("duckfn_quantstats");
src/bin/duckfn.rs     duckfn CLI 入口  ->  #[path] mod extension; + duckfn::cli::run(...)
                      （只服务 `just docs_csv` 导出函数描述 CSV，不参与插件运行）

src/extension/functions/mod.rs  ->  mod aggregate_html;
src/extension/functions/aggregate_html/
    mod.rs            两个 SQL 名字 / 各一个签名的分工，以及 mod 声明
    html_returns.rs   qs_html_reports            （收益率路径）
    html_prices.rs    qs_html_reports_by_prices  （价格/净值路径，收尾里先差分）
    kind.rs           一条路径在 SQL 侧的名字：宏生成的 `SQL_NAME`
    series.rs         内部点表示、序列构造、价格差分
    slots.rs          参数槽：symbol 表 +「每个 symbol 的配置只解析一次」
    report.rs         收尾：按 (标的, 基准) 逐份渲染、落盘、按需打开浏览器、回填每份的路径
    naming.rs         报告文件名：`<时间>-<策略名>-<基准名>[-<随机尾缀>].html`（落盘与临时文件共用主干）
    browser.rs        用系统默认浏览器打开报告（wasm 下整个功能被忽略）
src/extension/types/
    html_report_options.rs  命名 STRUCT 类型 `qs_html_report_options`
    html_report.rs          返回行类型 `QuantstatsHtmlReport`（不注册命名类型）
```

扩展名 `duckfn_quantstats` 必须与 `Makefile` 的 `EXTENSION_NAME`、产物文件名一致；
`src/lib.rs` 与 `src/wasm_lib.rs` 必须声明同一组 `mod`（官方模板的 `mod lib;` 写法在嵌套模块时会报
`E0583`）。新增功能时按 duckfn 的目录分层往 `src/extension/` 下面挂。

## 设计取舍

### 两个 SQL 名字、各一个签名

`symbol` 这一列就是分组依据，所以 SQL 里**不写 `GROUP BY`**：函数自己在聚合状态里按 symbol 分组
（见下面「symbol 表」），一次调用出整套报告。于是每个名字下只剩一个签名，参数顺序固定为
「数据列在前（`symbol`, `date`, 值），配置在后」。

价格那一支**必须**另起一个名字：`(symbol, date, price, options)` 与
`(symbol, date, period_return, options)` 的类型序列完全一样（都是 `VARCHAR, DATE, DOUBLE, STRUCT`），
同一个名字下无法按类型分派。

注册名不手写：属性宏给每个签名生成了一个 `SQL_NAME` 常量（单签名时就是函数名），错误信息前缀读它 ——
`kind.rs` 指过去，而不是再抄一份字面量。代价是这类函数得写成 `pub(super)`，因为生成的模块沿用函数的
可见性。

### symbol 表：函数自己完成分组

聚合状态不是「一个点数组」，而是 `HashMap<String, SymbolSlot>`，每个 symbol 一个槽：
该 symbol 的点，加上该 symbol 的一份配置。这么做换来三件事：

- **调用方不用写 `GROUP BY`**，SQL 侧一句 `SELECT` 就是全套报告；
- **基准就在同一份状态里**：它是表里另一个 symbol，取出来即可配对（见下）；
- **配置的粒度下沉到 symbol**：报告是每标的一份（标题、显示名、落盘路径各不相同），配置也就能按 symbol
  各取一份。

代价是聚合状态持有整张表的点（与「每组一份点数组」同阶），而 `result()` 里渲染的报告数仍然是标的数。

### 参数槽：配置按 symbol 惰性解析

`options` 用 `DuckLazy` 延迟读取：每行只构造一个 O(1) 的凭证，真正的解析只在**该 symbol 第一次出现**
时做一次（表里已经有它就只追加点）。这不是锦上添花：duckfn 的适配层是逐行读参数的，每行都解析一次
struct 就是 O(行数) 次解析，而解析次数与 symbol 数相同才是这份 API 该付的价。

解析结果用 duckfn 的 `DuckLazySlot` 缓存在槽里 —— 凭证只在产生它的那次回调内有效，状态里能留下的也就
只有解析结果。「这个 symbol 是不是第一次见到」不需要额外的标志位：表的键本身就是那个标记，只有
`insert` 新槽位的那条路径会读配置列。

### 每个 symbol 渲染一份报告

报告在 `result()` 里生成，一次调用渲染标的数那么多份。100 个标的 = 100 份完整报告（每份内嵌十几张
SVG），耗时与内存随标的数线性增长 —— 与旧版「`GROUP BY` 每组一份」是同一量级，只是分组从 SQL 挪进了
函数。结果的顺序在 `result()` 里按 symbol 排序定下，不依赖 HashMap 的迭代顺序，也不依赖 DuckDB 的
merge 顺序。

SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给 `ReturnSeries::new`（价格路径上则先按日期排序
再差分）。

### 为什么基准是表里的 symbol（而且是列表）

基准本来就在同一张长表里（它也是一个 symbol），所以让它由配置里的 `benchmark` 键指名、函数自己去表里取，
是最短的路径：一句 `SELECT`、没有 `GROUP BY`、没有 `cross join`、带不带基准只差配置里一个键。

早先的做法是「一次性传入的列表参数」（`list(...)` + `cross join` + `GROUP BY` 三步走）。它确实让基准只
求值一次，但代价是「带基准」这个常态反而比不带基准多一个参数、多一个重载，而且 SQL 侧要绕两步。

`benchmark` 是**列表**（`['SPX', 'NDX']`），因为真实场景是「一个标的对多个基准序列」：quantstats-rs 的
`HtmlReportOptions` 只装得下一个 `Option<&ReturnSeries>`（指标表的一列、图里的一条基准线、rolling beta
都围绕它建），所以多基准只能落成**多份报告** —— 1 标的 × M 基准 = M 行，`benchmark` 字段负责区分，
列表顺序就是报告顺序。

要注意「一个基准多个标的」与「一个标的多个基准」不是一回事：前者只是每个标的各出一份（不需要列表），
后者才是这里说的多份报告。真想要「N 个策略 × M 个基准」的笛卡尔积，自己 `GROUP BY` / 过滤后多调几次。

被指为基准的 symbol **只作输入、不出报告**；每个基准的序列只转换一次（价格路径上先差分），所有标的共用。
配置里的 `benchmark` 还要求整次调用一致（元素与顺序都算），否则「谁把谁当基准」没有单一答案 ——
不一致直接报错；列表本身的问题（空串、NULL 元素、重复）见配置类型那一节。

基准的显示名（`benchmark_title`）也是一个列表，**按下标**与 `benchmark` 对齐：它只用于展示，所以很宽松 ——
缺项（列表短了、NULL、空串）就在那一份报告里退回基准 symbol，多余项忽略，都不报错。

### 返回行类型不注册命名类型

`html_report.rs` 的 `QuantstatsHtmlReport` **刻意不注册**命名类型（`create_type` 默认关闭）：聚合的返回
类型本身就带着完整的匿名 `STRUCT(symbol VARCHAR, benchmark VARCHAR, strategy_title VARCHAR,
benchmark_title VARCHAR, html VARCHAR, file_path VARCHAR)[]`，SQL 里按字段名取用即可（`unnest` /
`list_transform` / `[1].html`），再注册一个类型名只是多一份要维护的表面。

字段名就是 Rust 字段名原样（duckfn 的 `DuckStruct` 派生不支持字段级改名），六个都不是 SQL 关键字，所以
DuckDB 渲染 `typeof` 时不加引号。可空的只有三个：`benchmark` 在单序列报告（没配基准）时为 NULL，
`benchmark_title` 跟着它（它就是那一份报告所用基准的显示名），`file_path` 在没落盘时为 NULL；其余三个
字段一定存在。

两个显示名（`strategy_title` / `benchmark_title`）都回显在结果行里：它们正是报告图例与文件名真正用的那
两份（缺省时各退回自己的 symbol），要打印或比对时不必去 HTML 里抠。

同一个类型也直接当 `DuckAggregateState::Output = Vec<QuantstatsHtmlReport>` 用 —— duckfn 写 LIST 的
路径（`duck_list.rs` 的 `create_writer_batch` / `write_valid` / `write_finish`）会挂上子写入器，元素按
`#[derive(DuckStruct)]` 生成的写路径落进子向量，所以「聚合返回结构体数组」不需要任何额外机制。

### 配置类型

`#[duck(create_type = true)]` 让 duckfn 在扩展加载期执行
`CREATE TYPE IF NOT EXISTS "qs_html_report_options" AS STRUCT(...)`，SQL 里才写得出
`{'title': 'x'}::qs_html_report_options`（以及 JSON 形式）。

字段**全部**是 `Option<T>`，这是硬要求：DuckDB 的 struct 字面量缺字段时会补 NULL，而 duckfn 读到
「非 Option 字段为 NULL」时会让**整个 struct** 变成 NULL。那样用户写的 `{'rf': 0.1}` 会整体退化成默认值，
他设的 rf 被静默丢掉。

`to_report_options(strategy_title, benchmark_title)` 的做法是「从 quantstats-rs 的
`HtmlReportOptions::default()` 出发、逐字段覆盖用户显式写了的那些」，默认值因此只有一份真相。两个参数是
**已经解析好的显示名**（`report.rs` 调 `strategy_title_or` / `benchmark_title_or` 得到）：同一份名字还要
用于文件名，解析一次、两处使用，报告图例与磁盘上的文件名才不会各说各话。退路规则本身不复杂 ——
`strategy_title` 缺省用 symbol、`benchmark_title` 按下标取、缺项用那一个基准的 symbol —— 但它不能省：
一次调用出几十份报告时，默认的 `'Strategy'` 对每份都一样，图例、浏览器临时文件名与返回行里的显示名都会
失去区分度。

`output_dir` 刻意不交给它转发：quantstats-rs 落盘用的是 `std::fs`，而本扩展要的是 DuckDB 的 VFS（见下），
所以目录由 `report.rs` 在拿到渲染结果后自己写。

配置是**逐行求值的一列**，每个 symbol 只取用第一行那份（见「symbol 表」）；`benchmark` 还要求整次调用里
所有标的给同一个列表（元素与顺序都算），`report.rs` 的 `benchmark_names()` 负责这条校验。

`benchmark` 与 `benchmark_title` 都是列表字段（`Option<Vec<Option<String>>>`），但**校验只做在 `benchmark`
上**，一次拦在 `QuantstatsHtmlOptions::benchmark_names()` 里：元素不能是 NULL、不能是空串、不能在同一个列表
里重复 —— 它决定「谁把谁当基准」与「谁出报告」，写坏了不能猜。`benchmark_title` 只用于展示，所以宽松：
按下标取，缺项（短了、NULL、空串）退回那一个基准 symbol，多余的项忽略（见 `benchmark_title_or`）。另外
`periods_per_year = 0`、`output_dir = ''` 也是在这里就报掉的配置错误 —— 都发生在渲染与文件系统调用之前。

## 报告落盘

`output_dir` 只给**目录**，文件名由 `naming.rs` 生成：
`<时间>-<策略名>-<基准名>-<随机尾缀>.html`（没有基准时中间那一段不出现）。把命名收进函数有三个理由：

- 名字要带「哪个标的、对哪个基准、什么时候」，只有函数知道；而且一个标的对多个基准时，按标的拼出来的
  路径必然互相覆盖 —— 那正是旧版「配置里写完整路径」在多基准下过不去的坎；
- 后两段就是报告里的显示名（`strategy_title` 与基准显示名），所以目录里的名字自然认得出来；
- 随机尾缀（`fastrand`）+ 落盘前查一次 `duck_vfs::exists`（撞上就换个尾缀重试，见 `report_path`），
  于是「不覆盖已有文件」是保证而不是概率 —— 两次调用各写各的。

合法性与随机都交给库：`sanitize-filename` 管非法字符 / 控制字符 / Windows 保留设备名 / 结尾的点与空格，
`fastrand` 管随机尾缀（它本来就是 tempfile 内部用的那个随机源）。本文件只补两条自己的策略：空格并成 `_`、
每段最多 32 个字符。

写文件本身用 duckfn 的便捷层 `duck_vfs::write_string`，也就是经 **DuckDB 的 VFS** 而不是 `std::fs`：

- 本地磁盘、内存文件系统、wasm 构建里宿主真正的那个文件系统，以及装了 `httpfs` 之后的 `s3://` /
  `http(s)://`，都是同一条通路、同一套语义；
- 这也是聚合函数唯一写得进去的路子：DuckDB 的 C API 不给聚合函数客户端上下文（没有 bind 回调，也没有
  `duckdb_aggregate_function_get_client_context`），所以 duckfn 在注册期留了一条自有长连接，
  从这里现取 `ClientContext` → `FileSystem`；
- 目录按 `/` 拼（`report.rs::join`），刻意不用 `Path::join`：后者按平台分隔符拼，Windows 上会把
  `s3://bucket/reports` 拼成 `s3://bucket/reports\name.html`。

`write_string` 是**替换**：写完之后文件里恰好就是这份报告，哪怕它以前更长（C API 没有 truncate 这件事由
duckfn 的 `duck_vfs` 层处理 —— 旧文件更长时先清零再写正文）。本扩展因为文件名从不撞名，实际上走不到覆盖
那一步，但读回来的内容与函数返回值逐字节一致这件事仍然成立，测试里用 `md5` 钉住了它。

一次调用可能写好几个文件（标的数 × 基准数）。`report.rs` 的收尾分两趟：先把每个 (标的, 基准) 的
`ReportTarget` 定下来（顺带校验 `output_dir` 非空、`open_in_browser` 指的路径能不能交给浏览器），
然后才逐个渲染 + 落盘 + 按需开浏览器，最后把真正写出去的路径回填进返回行。

## 用浏览器打开报告

`open_in_browser` 在报告生成之后把它交给系统默认浏览器，于是终端里的一套流程不必以「现在去找那个文件、
双击打开」收尾。浏览器要的是一个真实存在的本地文件，其余都由这件事决定：

- 写了 `output_dir`：先落盘，再打开那些文件；
- 没写：先把每份报告落到系统临时目录里的
  `<临时目录>/<时间>-<策略名>-<基准名>-<随机尾缀>.html`，文件由
  [tempfile](https://crates.io/crates/tempfile) 新建 —— 主干（时间 + 两段显示名）复用 `naming.rs`，
  随机尾缀与「这个名字当时一定是空的」（新建失败就换个尾缀重试）则由它负责。前缀是给人看的：时间在最前，
  所以按名字排序临时目录正好排成时间顺序；两段显示名分别是 `strategy_title`（没写就退回 `title`）与
  `benchmark_title`（按下标取，缺项退回基准 symbol），文件名的合法性交给
  [sanitize-filename](https://crates.io/crates/sanitize-filename)
  （见 naming.rs）。`.html` 后缀决定系统把它交给浏览器渲染而不是当成下载；
- `output_dir` 不是本地路径（`s3://…`、`memory://…`）时**报错**而不是静默跳过 —— 系统浏览器打不开那种
  路径。这个检查发生在渲染**之前**。

「把浏览器叫起来」是 [open](https://crates.io/crates/open) 的事，而且用的是不阻塞的 `that_detached`：
报告已经落盘了，所以这次查询既不等待浏览器、也不关心浏览器怎么处理这个文件。Windows 上就是一次
`ShellExecute` 调用（开了 `shellexecute-on-windows`，不用它默认的那条 PowerShell 路线）；macOS 与其他
平台则是 `open` / `xdg-open` 加上它自带的后备序列。唯一会报错的情形是启动器本身起不来。

一次调用会为**每一份报告**各开一个标签页（一个标的对两个基准就是两个标签页）—— 而且没写 `output_dir` 时
每份报告各自落一个临时文件，至少不会互相覆盖。

## WebAssembly

`output_dir` 走 DuckDB 的 VFS，wasm 构建与本地是同一条代码路径，文件落在该环境下 DuckDB 自己的文件系统
里。这替代了早先的行为（在 `wasm32-unknown-emscripten` 下直接丢掉路径，因为那边的 `std::fs` 没有可写的
文件系统）。

命名逻辑（`naming.rs`）因此是**共享**的：wasm 上一样要拼文件名，所以它用到的 `sanitize-filename` 与
`fastrand` 是普通依赖，而非 wasm 限定的那两个。

`open_in_browser` 是唯一一处**有意保留**的平台分支，也是本扩展仅剩的平台相关代码（`browser.rs`）：wasm
构建里没有可以启动的浏览器进程，所以那边直接忽略这个选项 —— 不打开浏览器，也不会为此写临时文件。报告
字符串原样返回给宿主，展示是宿主页面的事：blob URL + `window.open`、`<iframe>`，或者别的。它背后那两个
crate（`open`、`tempfile`）声明在 `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` 下，wasm
构建连编都不编它们 —— 这其实是硬要求：`open` 根本没有 emscripten 的实现，编不过。

`just build_wasm`（`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`）
能正常编过；运行时行为由 DuckDB 的 VFS 决定，而不是由本扩展决定。

## 依赖

- [duckfn](https://crates.io/crates/duckfn)：属性宏，把普通 Rust 函数注册成 DuckDB 函数。开了三个 feature：
  `duckdb-1-5`（`output_dir` 用的宿主文件系统 `duckfn::duck_vfs` 在它下面）、`chrono`（时间包装类型的互转，
  如 `DuckDate::to_naive_date`）与 `cli`（`src/bin/duckfn.rs` 用的命令行工具，给 duckfn 带上 clap 与 csv）。
  属性宏还会为每个签名生成 `SQL_NAME` 常量 —— 真正注册进 DuckDB 的名字 —— 错误信息前缀读它，不再手抄一份
  函数名字面量；属性上的 `description` / `comment` / `example` 则是函数描述 CSV 的唯一来源（见下）。
- [quack-rs](https://crates.io/crates/quack-rs)：DuckDB C API 绑定，`duckfn_entrypoint!` 展开出的代码直接引用它。
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys)：只取头文件，开启 `loadable-extension`，
  因此**不需要在本地编译 DuckDB**。版本下限是 `>= 1.10500`（DuckDB 1.5.0：这个 crate 把 DuckDB 版本
  编码成 `1.<major*10000 + minor*100 + patch>.0`，1.5.5 就是 `1.10505.0`），因为客户端上下文 /
  文件系统那部分 C API 是 1.5 才有的。
- [quantstats-rs](https://crates.io/crates/quantstats-rs)：报告本体。它的公开 API 里只有 `html()`
  一个可调用入口（`mod stats` 是私有的，`compute_performance_metrics` 拿不到），所以两条路径都基于它，
  不自己重算指标 —— 那会与报告里的数字形成两套真相。
- [sanitize-filename](https://crates.io/crates/sanitize-filename) 与
  [fastrand](https://crates.io/crates/fastrand)：报告文件名的两半 —— 「哪几段合法」（非法字符、控制字符、
  Windows 保留设备名、结尾的点与空格）与「随机尾缀」。两者都是**共享**依赖：`naming.rs` 在 wasm 上同样要
  拼文件名。fastrand 本来就在依赖树里（tempfile 内部用的就是它），显式依赖不增加编译成本。
- [open](https://crates.io/crates/open) 与 [tempfile](https://crates.io/crates/tempfile)：
  `open_in_browser` 的两件事 —— 把浏览器叫起来、在没有 `output_dir` 时新建一个不重名的临时文件。
  **只用于非 wasm 目标**（见上面的 WebAssembly 一节），所以它们挂在 target 专属的依赖表里，
  而不是主依赖表。
- [chrono](https://crates.io/crates/chrono)：本扩展自己用的是**本地时间** —— 报告文件名（落盘与临时文件
  共用）以 `%Y%m%d-%H%M%S` 时间戳开头（`chrono::Local`，见 naming.rs）。日期那头由 duckfn 的 `chrono`
  feature 换算（`DuckDate::to_naive_date`），它产出的 `NaiveDate` 正是 quantstats-rs 的
  `ReturnSeries::new` 要的；三个 crate 共用同一个 chrono 0.4。

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
`just test`、`just lint`、`just build_wasm`、`just docs_csv`。

## 函数描述（社区扩展文档页）

DuckDB 的 C 扩展 API **没有**设置函数描述与示例的接口：`duckdb_scalar_function_set_name`、
`_set_return_type`、`_set_varargs`、`_set_volatile`…… 就到这儿，没有 `_set_description`，
也没有 `_add_example`。所以社区扩展页（<https://duckdb.org/community_extensions/list_of_extensions>）
上那张 `Added Functions` 表要是没人帮忙，就只是一列光秃秃的函数名。

这份文本紧挨着被描述的函数，写在 `#[duck_*]` 属性上（本扩展的两处分别在
`functions/aggregate_html/html_returns.rs` 与 `html_prices.rs`）：

```rust
#[duck_aggregate_function(
    description = "Renders one quantstats HTML report per symbol from a long table of periodic returns",
    comment = "Groups by symbol internally, so the SQL needs no GROUP BY …",
    examples = ["SELECT unnest(qs_html_reports(…)) FROM daily_returns", "…"]
)]
```

三个键都可选（`example` 单条、`examples` 多条，二者互斥），**不参与注册**：宏只把它们连同注册名收进
inventory。导出：

```shell
just docs_csv                                           # -> target/function_descriptions.csv
cargo run --bin duckfn -- function_descriptions --all    # -> target/function_descriptions_all.csv
                                                         #    （含还没写描述的函数，当清单用）
```

这一步不加载扩展、不查 catalog、也不需要 DuckDB 在场：纯粹读编译期记下来的东西，路径固定为项目的
`target/` 下。`src/bin/duckfn.rs` 里那句 `#[path = "../extension/mod.rs"] mod extension;` 是必需的 ——
inventory 的静态构造器只在**真正被链接进最终二进制**的目标文件里生效，改成 `use duckfn_quantstats::…`
的话 CSV 会静默变空（不报错，只是没内容）。

文本本身还有三条规矩：多条示例导出时用 `"; "` 拼接、每条去掉结尾分号；换行会压成一个空格（生成页是
Markdown 表格，单元格里的换行会断行）；逗号、引号与非 ASCII 原样通过。所以照「一句一条完整 SQL」
写即可。文案一律英文 —— 它会被原样贴到文档页上。

发社区扩展时，把这份 CSV 放进 `community-extensions` 仓的
`extensions/duckfn_quantstats/docs/function_descriptions.csv`（它由那个仓的 `generate_md.sh` 按
`function_name` 左连接覆盖函数表）；本仓不必留副本，改完代码重新生成即可。

## 测试

测试用 SQLLogicTest 格式写在 `test/sql/` 下：

```shell
make debug && make test    # make test 不会自动重新构建，改完 Rust 必须先 make debug
```

测试分两类，分工不要混：

| 文件 | 覆盖什么 | 额外依赖 |
| --- | --- | --- |
| `test/sql/quantstats/html_reports.test` | **收益率路径的行为**：注册面（两个名字各一个签名）、结果形状与排序（symbol 升序 + 标的内按基准列表顺序）、无基准时 `benchmark` 为 NULL、显示名退回 symbol、`NULL` 行跳过与被跳空的标的、空输入返回 `NULL`、多线程 `combine` 一致性（单线程 vs 4 线程 md5 相等）、`output_dir` 自动命名与路径回填、两次调用互不覆盖 | 无 |
| `test/sql/quantstats/html_reports_by_prices.test` | **价格路径的行为**：与 `lag()` 差分的结果逐字节一致、多个基准时每一份都与对应基准的差分结果一致、基准侧同样先差分、前值为 0 时跳过、单点标的略过、基准 symbol 不存在 / 点数不足 | 无 |
| `test/sql/quantstats/html_reports_errors.test` | **错误路径**：基准列表的四种写法错误（symbol 不存在 / 空串 / NULL 元素 / 重复）、基准列表跨 symbol 不一致（含顺序）、`benchmark_title` 多余项被忽略（不报错）、`periods_per_year = 0`、`output_dir = ''`、NUL 路径、`open_in_browser` 配非本地路径、没 cast 的配置字面量、旧 API 已不存在 | 无 |
| `test/sql/quantstats/html_reports_values.test` | **输出内容**：用 [webbed](https://duckdb.org/community_extensions/extensions/webbed) 的 XPath 解析生成的 HTML，断言标题、统计区间、`rf` 回显、逐行指标数字、图表/表格数量、带基准时多出的那一列、**多基准时每份报告各自带自己的基准列**、**基准显示名按下标对齐（缺项 / NULL / 空串退回 symbol）**、每个 symbol 各自的标题与文件名 | 社区扩展 `webbed` |

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

## 演示数据（`demo/prices.csv`）

这是一份固定快照：`GOOGL`、`MSFT` 与标普 500 指数（`SPX`）的日收盘价，各 1435 个交易日，
区间 2021-01-04 … 2026-09-21，三者的交易日历完全一致。它是一张长表（`date`、`symbol`、`price`），
提交进仓库就是为了 README 的快速上手可以原样复制运行。

**为什么是快照、而不是实时 URL。** 写这份文档时，个股日线没有「免费 + 免 key + 稳定」的 HTTP 端点：
stooq 的 CSV 下载被套上了 JavaScript 校验，Yahoo 的接口回的是地区跳转页，EODHD 的公开 `demo` token
几次请求就用完配额。FRED 确实提供指数的 CSV
（`https://fred.stlouisfed.org/graph/fredgraph.csv?id=SP500`），但它会拒绝 `read_csv` 先发的那个 `HEAD`
探测，所以也读不了。于是快照取的是 2026-09-22 那一份 —— 指数来自上面那份 FRED CSV，两只个股来自 Nasdaq
的公开行情接口
（`https://api.nasdaq.com/api/quote/MSFT/historical?assetclass=stocks&fromdate=2021-01-01&todate=2026-09-21&limit=2000`），
价格为接口给出的收盘价。
