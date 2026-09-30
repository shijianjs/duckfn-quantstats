---
title: 设计取舍
sidebar_position: 2
description: 为什么分组交给函数自己、基准怎么配对、为什么配置类型全是 Option，以及报告怎么落盘与打开。
---

# 设计取舍

这一页讲的是代码背后的「为什么」：选了哪些形状、否掉了哪些做法。面向使用方的行为在
[函数](../../guide/functions.md)与[配置字段](../../guide/options.md)；这里收的是使用方不需要的内容。

本项目从 DuckDB 官方 [extension-template-rs](https://github.com/duckdb/extension-template-rs) 起步，
并已按 duckfn 的骨架约定改造（入口模块、`EXTENSION_NAME`、依赖列表）。

## 两个 SQL 名字、各一个签名

`symbol` 这一列就是分组依据，所以 SQL 里**不写 `GROUP BY`**：函数自己在聚合状态里按 symbol 分组
（见下面「symbol 表」），一次调用出整套报告。于是每个名字下只剩一个签名，参数顺序固定为
「数据列在前（`symbol`, `date`, 值），配置在后」。

价格那一支**必须**另起一个名字：`(symbol, date, price, options)` 与
`(symbol, date, period_return, options)` 的类型序列完全一样（都是 `VARCHAR, DATE, DOUBLE, STRUCT`），
同一个名字下无法按类型分派。

注册名不手写：属性宏给每个签名生成了一个 `SQL_NAME` 常量（单签名时就是函数名），错误信息前缀读它 ——
`kind.rs` 指过去，而不是再抄一份字面量。代价是这类函数得写成 `pub(super)`，因为生成的模块沿用函数的
可见性。

## symbol 表：函数自己完成分组

聚合状态不是「一个点数组」，而是 `HashMap<String, SymbolSlot>`，每个 symbol 一个槽：
该 symbol 的点，加上该 symbol 的一份配置。这么做换来三件事：

- **调用方不用写 `GROUP BY`**，SQL 侧一句 `SELECT` 就是全套报告；
- **基准就在同一份状态里**：它是表里另一个 symbol，取出来即可配对（见下）；
- **配置的粒度下沉到 symbol**：报告是每标的一份（标题、显示名、落盘路径各不相同），配置也就能按 symbol
  各取一份。

代价是聚合状态持有整张表的点（与「每组一份点数组」同阶），而 `result()` 里渲染的报告数仍然是标的数。

## 参数槽：配置按 symbol 惰性解析

`options` 用 `DuckLazy` 延迟读取：每行只构造一个 O(1) 的凭证，真正的解析只在**该 symbol 第一次出现**
时做一次（表里已经有它就只追加点）。这不是锦上添花：duckfn 的适配层是逐行读参数的，每行都解析一次
struct 就是 O(行数) 次解析，而解析次数与 symbol 数相同才是这份 API 该付的价。

解析结果用 duckfn 的 `DuckLazySlot` 缓存在槽里 —— 凭证只在产生它的那次回调内有效，状态里能留下的也就
只有解析结果。「这个 symbol 是不是第一次见到」不需要额外的标志位：表的键本身就是那个标记，只有
`insert` 新槽位的那条路径会读配置列。

## 每个 symbol 渲染一份报告

报告在 `result()` 里生成，一次调用渲染标的数那么多份。100 个标的 = 100 份完整报告（每份内嵌十几张
SVG），耗时与内存随标的数线性增长 —— 与旧版「`GROUP BY` 每组一份」是同一量级，只是分组从 SQL 挪进了
函数。结果的顺序在 `result()` 里按 symbol 排序定下，不依赖 HashMap 的迭代顺序，也不依赖 DuckDB 的
merge 顺序。

SQL 里**不需要 `ORDER BY`**：聚合内部只做拼接，排序交给 `ReturnSeries::new`（价格路径上则先按日期排序
再差分）。

## 为什么基准是表里的 symbol（而且是列表）

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

## 返回行类型不注册命名类型

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

## 配置类型

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

`output_dir` 刻意不交给它转发：quantstats-rs 自己写文件、名字也是它自己那套，而这里的名字要带「哪个标的、
对哪个基准」，而且 wasm 上整个写操作都要跳过（见下），所以目录由 `report.rs` / `storage.rs` 在拿到渲染结果
后自己写。

`benchmark` 与 `benchmark_title` 都是列表字段（`Option<Vec<Option<String>>>`），但**校验只做在 `benchmark`
上**，一次拦在 `QuantstatsHtmlOptions::benchmark_names()` 里。另外 `periods_per_year = 0`、
`output_dir = ''`、`output_dir` 里带 NUL 字节也是在这里就报掉的配置错误 —— 都发生在渲染与文件系统调用之前。

## 报告落盘

`output_dir` 只给**目录**，文件名由 `naming.rs` 生成：
`<时间>-<策略名>-<基准名>-<随机尾缀>.html`（没有基准时中间那一段不出现）。把命名收进函数有三个理由：

- 名字要带「哪个标的、对哪个基准、什么时候」，只有函数知道；而且一个标的对多个基准时，按标的拼出来的
  路径必然互相覆盖 —— 那正是旧版「配置里写完整路径」在多基准下过不去的坎；
- 后两段就是报告里的显示名（`strategy_title` 与基准显示名），所以目录里的名字自然认得出来；
- 随机尾缀（`fastrand`）+ 落盘前查一次同名文件（`storage.rs` 里的 `Path::exists`），于是「不覆盖已有文件」
  是保证而不是概率 —— 撞上就换个尾缀重试，两次调用各写各的。

合法性与随机都交给库：`sanitize-filename` 管非法字符 / 控制字符 / Windows 保留设备名 / 结尾的点与空格，
`fastrand` 管随机尾缀（它本来就是 tempfile 内部用的那个随机源）。本文件只补两条自己的策略：空格并成 `_`、
每段最多 32 个字符。

写文件本身就是 `std::fs::write`（`storage.rs`）：一次调用完成创建、截断到零、写入，所以写完之后文件里恰好
就是这份报告 —— 读回来的内容与函数返回值逐字节一致，测试里用 `md5` 钉住了它。

它以前走的是 DuckDB 的 VFS（`duck_vfs::write_string`），当时的理由站得住：VFS 能到装了 `httpfs` 的
`s3://` / `http(s)://`；而且那是聚合函数唯一写得进去的路子 —— DuckDB 的 C API 不给聚合函数客户端上下文
（没有 bind 回调，也没有 `duckdb_aggregate_function_get_client_context`），所以走 VFS 得让 duckfn 在注册期
留一条自有长连接。后来有两条把它压过去了：

- **它在 wasm 上不成立**，而那边本来就得跳过文件操作（见 [WebAssembly](#webassembly)），并且当时的失败形态
  是一个调用方无能为力的报错；
- **它要一个 feature 的代价**：宿主 VFS 正是 `owned-connection` 带来的那个，而 `std::fs` 根本不需要客户端
  上下文 —— 这也正是聚合函数能直接调它的原因。为一个本扩展别处都没用到的能力少一个 feature，划算。

两个后果值得点名：

- `output_dir` 现在只能是**本地**路径。`s3://bucket/reports` 以前能写、现在会以写入错误告终，文档也是这么
  写的；
- 目录按 `/` 拼（`storage.rs::join`），刻意不用 `Path::join`：后者按平台分隔符拼，于是同一个 `file_path`
  在 Windows 上是 `out\name.html`、在别处是 `out/name.html` —— 而这个字符串是给用户看的。

一次调用可能写好几个文件（标的数 × 基准数）。`report.rs` 的收尾分两趟：先把每个 (标的, 基准) 的
`ReportTarget` 定下来（顺带校验 `output_dir` 非空、`open_in_browser` 指的路径能不能交给浏览器），
然后才逐个渲染 + 落盘 + 按需开浏览器，最后把真正写出去的路径回填进返回行。

## 用浏览器打开报告

`open_in_browser` 在报告生成之后把它交给系统默认浏览器。浏览器要的是一个真实存在的本地文件，其余都由
这件事决定：

- 写了 `output_dir`：先落盘，再打开那些文件；
- 没写：先把每份报告落到系统临时目录里的
  `<临时目录>/<时间>-<策略名>-<基准名>-<随机尾缀>.html`，文件由
  [tempfile](https://crates.io/crates/tempfile) 新建 —— 主干（时间 + 两段显示名）复用 `naming.rs`，
  随机尾缀与「这个名字当时一定是空的」（新建失败就换个尾缀重试）则由它负责。前缀是给人看的：时间在最前，
  所以按名字排序临时目录正好排成时间顺序；`.html` 后缀决定系统把它交给浏览器渲染而不是当成下载；
- `output_dir` 不是本地路径（`s3://…`、`memory://…`）时**报错**而不是静默跳过 —— 系统浏览器打不开那种
  路径。这个检查发生在渲染**之前**。

「把浏览器叫起来」是 [open](https://crates.io/crates/open) 的事，而且用的是不阻塞的 `that_detached`：
报告已经落盘了，所以这次查询既不等待浏览器、也不关心浏览器怎么处理这个文件。Windows 上就是一次
`ShellExecute` 调用（开了 `shellexecute-on-windows`，不用它默认的那条 PowerShell 路线）；macOS 与其他
平台则是 `open` / `xdg-open` 加上它自带的后备序列。唯一会报错的情形是启动器本身起不来。

## WebAssembly

wasm 构建**整个跳过文件操作**（`storage.rs`）：`output_dir` 是「收下、然后忽略」—— 不报错、不写文件、
`file_path` 是 NULL，报告本身照常回到 `html` 列由宿主页面展示。这个选择不是偷懒，有两条撑着：

- **wasm 构建的文件系统不忠实。** 任何不存在的路径都会返回一个 **1 字节**的幻影条目 —— `glob`、
  `read_text`、`file_size` 都把它报成存在 —— 于是「这个文件名空着吗」没有一个能信的答案，那道「绝不覆盖
  已有文件」的保证也就无法兑现；裸写偏移在那边也是错的（写出来多一字节 / 错位）。这是平台的限制，不是
  duckfn 或本扩展的 bug。
- **从 SQL 侧绕不过去。** `COPY … TO` 只能把**查询结果**按格式（CSV / JSON / parquet）导出，而这些格式都
  载不动一份任意长的 HTML 文档的原样字节（CSV 会加引号、换行会把内容拆开）。

这段历史值得记一句：更早的版本走 DuckDB 的 VFS，结果 wasm **自己**就成了问题 —— 那边那道「绝不覆盖」的检查
永远找不到空位，`output_dir` 以 `could not find a free report file name in 8 attempts` 失败。把 VFS 换成
`std::fs`（见[报告落盘](#报告落盘)）之后，同一件事变成了一次明确、写在文档里的跳过。

命名逻辑（`naming.rs`）仍然是**共享**的：它对每个目标都参与编译，哪怕 wasm 上没人调用它 —— 所以它用到的
`sanitize-filename` 与 `fastrand` 仍是普通依赖，而不是 wasm 限定的那两个：让那个模块保持没有 `cfg`，比从
一个根本不拼文件名的 wasm 构建里抠掉两个小 crate 更值。

`open_in_browser` 是另一处**有意保留**的例外：wasm 构建里没有可以启动的浏览器进程，所以那边直接忽略这个
选项 —— 不打开浏览器，也不会为此写临时文件。报告字符串原样返回给宿主，展示是宿主页面的事。它背后那两个
crate（`open`、`tempfile`）声明在 `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` 下，wasm
构建连编都不编它们 —— 这其实是硬要求：`open` 根本没有 emscripten 的实现，编不过。

`just build_wasm`（`cargo build --release --target wasm32-unknown-emscripten --example duckfn_quantstats`）
能正常编过；文件操作在那边只是永远不跑。
