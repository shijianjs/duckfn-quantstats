---
title: 依赖
sidebar_position: 3
description: 哪个 crate 负责哪一段，以及每个为什么被选中。
---

# 依赖

总原则就是仓库那条：能用成熟库（或 std API）就别手写，并且把「为什么引它」写下来。下面每一条在
`Cargo.toml` 里都有一句注释说明它负责什么。

- [duckfn](https://crates.io/crates/duckfn)：属性宏，把普通 Rust 函数注册成 DuckDB 函数。依赖开了 `all`
  （一次带齐 duckfn 的常用 feature），**刻意不开 `owned-connection`** —— 那个 feature 才带来 `duck_vfs`
  （duckfn 的宿主文件系统），而这里用不着：落盘在原生构建里是 `std::fs`，wasm 上整个跳过（见
  [设计取舍](./design-notes.md)）。本扩展实际用到的是：
  - `chrono`：时间包装类型的互转（`DuckDate::to_naive_date` 等）；
  - `DuckLazySlot<T>`：把 DuckLazy 参数「解析一次」这条路径固化成类型；
  - `cli`：`src/bin/duckfn.rs` 用的命令行工具，给 duckfn 带上 clap 与 csv。
  - 属性宏还会为每个签名生成 `SQL_NAME` 常量 —— 真正注册进 DuckDB 的名字 —— 错误信息前缀读它，不再手抄
    一份函数名字面量；属性上的 `description` / `comment` / `example` 则是函数描述 CSV 的唯一来源（见
    [函数描述](../publishing/function-descriptions.md)）。
- [quack-rs](https://crates.io/crates/quack-rs)：DuckDB C API 绑定，`duckfn_entrypoint!` 展开出的代码直接
  引用它。
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys)：只取头文件，开启 `loadable-extension`，
  因此**不需要在本地编译 DuckDB**。版本下限是 `>= 1.10500`（DuckDB 1.5.0：这个 crate 把 DuckDB 版本
  编码成 `1.<major*10000 + minor*100 + patch>.0`，1.5.5 就是 `1.10505.0`），因为 `all` 会打开 duckfn 的
  `duckdb-1-5` feature —— C API 的**不稳定区**（标量 bind/init、copy 函数、客户端上下文），那些槽位只在
  1.5.x 里存在。落盘已经不再吃这条下限了，但依赖树仍然开着它。
- [quantstats-rs](https://crates.io/crates/quantstats-rs)：报告本体。它的公开 API 里只有 `html()`
  一个可调用入口（`mod stats` 是私有的，`compute_performance_metrics` 拿不到），所以两条路径都基于它，
  不自己重算指标 —— 那会与报告里的数字形成两套真相。
- [chrono](https://crates.io/crates/chrono)：本扩展自己用的是**本地时间** —— 报告文件名（落盘与临时文件
  共用）以 `%Y%m%d-%H%M%S` 时间戳开头（`chrono::Local`，见 naming.rs）。日期那头由 duckfn 的 `chrono`
  feature 换算（`DuckDate::to_naive_date`），它产出的 `NaiveDate` 正是 quantstats-rs 的
  `ReturnSeries::new` 要的；三个 crate 共用同一个 chrono 0.4。
- [sanitize-filename](https://crates.io/crates/sanitize-filename) 与
  [fastrand](https://crates.io/crates/fastrand)：报告文件名的两半 —— 「哪几段合法」（非法字符、控制字符、
  Windows 保留设备名、结尾的点与空格）与「随机尾缀」。两者仍是**共享**依赖，没有挪到非 wasm 的依赖表里：
  `naming.rs` 对每个目标都参与编译，让它保持没有 `cfg` 比从 wasm 构建里抠掉两个小 crate 更值（那边反正
  不拼文件名，没人调用它）。fastrand 本来就在依赖树里（tempfile 内部用的就是它），显式依赖不增加编译成本。
- [open](https://crates.io/crates/open) 与 [tempfile](https://crates.io/crates/tempfile)：
  `open_in_browser` 的两件事 —— 把浏览器叫起来、在没有 `output_dir` 时新建一个不重名的临时文件。
  **只用于非 wasm 目标**（见[设计取舍](./design-notes.md)），所以它们挂在 target 专属的依赖表里，
  而不是主依赖表。
