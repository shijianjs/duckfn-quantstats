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

### 注册到 DuckDB 的函数名统一加前缀

所有注册到 DuckDB 的函数名一律以 `duckfn_quantstats_` 开头，例如 `duckfn_quantstats_sharpe()`。
范围包括标量函数、聚合函数、表函数、COPY 格式、cast、SQL 宏、replacement scan
—— 凡是出现在 SQL 里的名字都要带前缀。

前缀与扩展名 `duckfn_quantstats` 完全一致：知道扩展名就能猜出函数名，
也便于按前缀在 `duckdb_functions()` 里检索。不要为了省 3 个字符改成 `dfn_quantstats_`
（省下的字符有限，却要长期在两套名字之间做映射）。

duckfn 的属性宏默认拿 **Rust 函数名**当注册名，所以直接把函数定义成 `fn duckfn_quantstats_xxx(...)` 即可。

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
