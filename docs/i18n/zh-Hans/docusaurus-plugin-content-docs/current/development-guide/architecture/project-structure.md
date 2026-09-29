---
title: 目录结构
sidebar_position: 1
description: 从两个 crate root 到注册函数的模块链路、SQL 类型放在哪，以及让扩展能被加载的命名规则。
---

# 目录结构

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

test/sql/quantstats/   SQLLogicTest 用例
demo/prices.csv        提交进仓库的行情快照
scripts/release.sh     版本号提升、打 tag、切开发版本
Justfile               日常命令
docs/                  本站
community-extension/   社区扩展注册草稿
```

## 两个 crate root

`src/lib.rs` 与 `src/wasm_lib.rs` 都只声明一个模块 `mod extension;`，其余都由
`extension/mod.rs` 挂上。官方 Rust 模板写的是 `mod lib;` 并从 wasm root 再转发一次，
模块一旦嵌套就会报 `error[E0583]: file not found for module …`：那样会有两份同样的路径集合要同步。

所以新增模块改的是 `extension/mod.rs`（以及下一层的 `mod.rs`），永远不动 crate root。

## 命令行 bin

`src/bin/duckfn.rs` 用 `#[path = "../extension/mod.rs"] mod extension;` 把扩展再编译一次，然后调
`duckfn::cli::run(...)`。它只为导出函数描述 CSV（`just docs_csv`）而存在，不参与扩展本身。

`#[path]` 不是偷懒，是必需的：`#[duck_*]` 背后的文档元数据由 `inventory` 的静态构造器收集，
而它只在**真正被链接进最终二进制**的目标文件里生效。改成 `use duckfn_quantstats::…` 的话，
链接器可能把这些模块丢掉，导出的 CSV 会**静默**变空。见
[函数描述](../publishing/function-descriptions.md)。

## 命名规则

| 规则 | 为什么 |
| --- | --- |
| 扩展名是 `duckfn_quantstats`，且五处一致。 | 它既是入口符号也是产物文件名；DuckDB 按文件名去找符号。 |
| 注册进 DuckDB 的每个 SQL 名都带 `qs_` 前缀。 | DuckDB 没有命名空间，社区扩展也几乎不把包名写进函数名 —— 但一个统一前缀是在 `duckdb_functions()` 里能检索到这两个名字的关键。约定见 `AGENTS.md`。 |
| `src/lib.rs` 与 `src/wasm_lib.rs` 永远声明同一组 `mod`。 | 否则 wasm 构建编不过模块树。 |
| `output_dir` 只收目录，不收文件路径。 | 文件名由函数生成；一个标的对多个基准时，调用方拼出来的路径必然互相覆盖（见[设计取舍](./design-notes.md)）。 |
| 临时文件（脚本、数据、日志）放 `target/`。 | `target/` 已被 git 忽略，不会污染工作区。 |
| 文本文件一律 LF。 | 仓库存的就是 LF。 |

## 开发笔记都在哪

用户角度看不需要的设计细节 —— 为什么分组交给函数、基准怎么配对、哪个依赖负责哪一段 —— 分别在
[设计取舍](./design-notes.md)与[依赖](./dependencies.md)。

duckfn 自身的通用约定（入口链路、新增函数的流程、动手前该查哪份源码）在这里**不重复**：
它们在仓库根目录的 `AGENTS.md` 里。

0.0.11 起，duckfn 的文档、可运行的示例扩展与其 sqllogictest 范例都**随 crate 包发布**，所以它们与
`Cargo.toml` 钉的版本严格对应，不需要 clone duckfn 仓库：

```shell
# 跑过一次构建之后：cargo 真正编译的就是这一份源码
ls -d ~/.cargo/registry/src/*/duckfn-*/
```

| 该目录下的路径 | 是什么 |
| --- | --- |
| `docs/docs/**` | 用户文档正文（英文），含每种注册方式各一章。 |
| `docs/i18n/zh-Hans/…/current/**` | 同一份文档的简体中文。 |
| `src/extension/**` | 示例扩展：每种注册方式一个文件，外加自定义类型与组合示例。 |
| `test/sql/**` | 示例扩展的 SQLLogicTest 用例，值得照着抄结构。 |

在线那份文档站（[shijianjs.github.io/duckfn](https://shijianjs.github.io/duckfn/)）随时可能是更新的一版，
而本机 registry 里那份才是本项目实际编译的代码。
