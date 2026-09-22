<!--
AGENTS.md 模板：复制到你的 DuckDB 扩展项目根目录，命名为 AGENTS.md。
只需把下面两个 {{...}} 填好。填完**保留**这两行（不是一次性的）：
以后 clone 路径或项目目标变了，还在这里改。
-->

# AGENTS.md

这是一个用 [duckfn](https://crates.io/crates/duckfn) 写的 DuckDB 扩展（loadable extension）。

## 项目事实（唯一需要人维护的一段）

- 这个扩展做什么：{{PROJECT_GOAL}} = duckdb插件：提供quantstats报告
- duckfn 仓库在本机的 clone：{{DUCKFN_REPO}} = `S:\workspace\my\rust\duckdb\duckdb-extension-template-rs`

> 扩展名、crate 名、duckfn 版本**不要抄到这里**：
> 扩展名读 `src/extension/mod.rs` 里的 `duckfn_entrypoint!("...")`（也是 `Makefile` 的 `EXTENSION_NAME`），
> crate 名与 duckfn 版本读 `Cargo.toml`。它们本来就在代码里，复制一份只会变成第二份会过期的真相。

## 动手前先读

**`{{DUCKFN_REPO}}/templates/duckfn-conventions.md`** —— 知识源（先读哪、再读哪）、
硬约束、开发循环、新增函数的完整流程都在这份文件里。

它由 duckfn 仓库维护，本文件**只引用、不复制**，所以 duckfn 升级时不需要重做本文件，
只要 `git -C {{DUCKFN_REPO}} pull`（或 `checkout` 到对应 tag）。

本机还没有 clone 时先来一份（文档、示例扩展、sqllogictest 范例都在里面，
而且它们不会随依赖进入项目）：

```shell
git clone https://github.com/shijianjs/duckfn
```

## 升级 duckfn 时

1. 改 `Cargo.toml` 里的 duckfn 版本，`cargo update -p duckfn -p duckfn-macro`。
2. `git -C {{DUCKFN_REPO}} fetch --tags && git -C {{DUCKFN_REPO}} checkout v<新版本>`，
   让文档与示例跟依赖对齐；约定文件随这次切换一起更新。
3. 本文件不用改。


## 仓库约定

### 尽量用成熟三方库实现，不要自己造轮子

写任何「通用」逻辑之前先问一句：这件事是不是已经有 crate（或 std API）在做？

- **平台差异、临时文件与随机名、文件名的合法性规则、编码、哈希、日期时间算术、序列化** ——
  这类通用问题一律先找库。已经这么做的先例：`open_in_browser` 用 `open`（各平台的启动命令与
  参数引用）、`tempfile`（临时文件与随机尾缀）、`sanitize-filename`（文件名字法）。
- **能用 std 就用 std**，别自己拼底层积木：路径绝对化用 `std::path::absolute`，而不是
  `env::current_dir()?.join(path)`。
- 手写只允许出现在**领域逻辑**上（quantstats 的差分规则、报告怎么渲染），或者已知的库都不合适 ——
  后者必须在这段代码的注释里写明「为什么不用库」，例如 `browser.rs::local_path`（判断一个 VFS 路径能不能
  交给浏览器，看的是字面上的 `://` 而不是引一个 URL 解析库 —— Windows 的 `C:\…` 在 URL 语法里同样是
  scheme）。反过来，日期换算这类通用算术不要手写：`series.rs` 曾经自己算 epoch，现在交给 duckfn 的
  chrono 桥（`DuckDate::to_naive_date`）。
- 依赖不是免费的：引入 crate 时在 `Cargo.toml` 里写一句它负责什么、为什么选它，让取舍一眼看得出来；
  只服务某个平台的依赖挂到 target 专属依赖表下
  （见 `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`），别让别的目标替它付编译成本 ——
  有时这甚至是硬要求（`open` 没有 emscripten 的实现，编到 wasm 直接失败）。
- 自查标准：一个「通用」函数如果在 crates.io 上能查到现成实现，它就需要一个留下来的理由。

### 注册到 DuckDB 的函数名统一加 `qs_` 前缀

所有注册到 DuckDB 的函数名一律以 `qs_`（quantstats）开头，例如 `qs_html_report(...)`。
范围包括标量函数、聚合函数、表函数、COPY 格式、cast、SQL 宏、replacement scan
—— 凡是出现在 SQL 里的名字都要带前缀。

社区扩展几乎都不把包名/扩展名写进函数名（见
<https://duckdb.org/community_extensions/list_of_extensions>）：`duckfn_quantstats_html`
这样的全名在每个调用点上都是纯噪声，而 `qs_` 短到可以忽略，又足以在 `duckdb_functions()` 里
按前缀检索。**前缀只是命名空间，不再是扩展名的缩写**，不要因为扩展名变了就跟着改。

前缀之后的部分要能读懂，不要拿缩写堆砌。同一分支的两个重载共用同一个名字、靠参数个数分派：

```text
qs_html_report(date, period_return, options)              / (..., benchmark, options)
qs_html_report_by_prices(date, price, options)            / (..., benchmark, options)
```

duckfn 的属性宏默认拿 **Rust 函数名**当注册名，所以直接把函数定义成 `fn qs_xxx(...)` 即可。
`overloads_name` 的函数集名只写在属性字面量里，但宏会为每个签名生成 `SQL_NAME` 常量：代码里要引用
注册名（错误信息前缀、日志）就读它 —— `functions/aggregate_html/kind.rs` 就是这么做的 —— 不要再抄
一份字面量。代价是这类函数得写成 `pub(super)`，因为生成的模块沿用函数的可见性。

### 临时文件放到 target/

生成的临时文件（脚本、数据、日志、一次性验证代码等）一律放到 `target/` 下，
不要放在仓库根目录或其它已跟踪的目录里。`target/` 已被 git 忽略，不会污染工作区，
用完顺手删掉。

### 文本文件一律用 LF

所有新增或修改的文本文件使用 LF（`\n`）换行，不要 CRLF（`\r\n`）。

任务结束时，对本次新增的文本文件**机械地跑一遍替换命令即可，不需要先检测**
里面是否真的有 CRLF：

```powershell
# PowerShell：逐个文件把 CRLF 换成 LF（保持 UTF-8 无 BOM）
foreach ($f in @('path/to/new-file.md', 'path/to/new-script.sh')) {
    $p = Join-Path (Get-Location) $f
    $c = [IO.File]::ReadAllText($p)
    [IO.File]::WriteAllText($p, ($c -replace "`r`n", "`n"), [System.Text.UTF8Encoding]::new($false))
}
```

```bash
# Git Bash / Linux / macOS
sed -i 's/\r$//' path/to/new-file.md path/to/new-script.sh
```

> 仓库开启了 `core.autocrlf`，所以 `git diff` 偶尔会提示 "LF will be replaced by CRLF"，
> 那是检出到工作区时的行为，提交进仓库的内容始终是 LF。
