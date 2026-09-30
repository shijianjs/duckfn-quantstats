<!--
本文件是 duckfn-extension-template 里那份 AGENTS.md 的**项目实例**：约定与流程照抄，
只有下面「项目事实」一节是本项目自己的。模板更新时对照那份，别把共享的约定改歪。
-->

# AGENTS.md

这是一个用 [duckfn](https://crates.io/crates/duckfn) 写的 DuckDB 扩展（loadable extension）。

## 项目事实（唯一需要人维护的一段）

- 这个扩展做什么：duckdb 插件，提供 quantstats 报告。
- 社区扩展注册仓（`duckdb/community-extensions`）的 fork 在本机的 clone：
  `S:\workspace\my\rust\duckdb\duckdb-community-extensions`（`origin` = `shijianjs/duckdb-community-extensions`）
  —— 扩展就是在这份克隆里注册的（往 `extensions/` 下加目录），字段草稿在本仓的 `community-extension/`

> 扩展名、crate 名、duckfn 版本**不要抄到这里**：
> 扩展名读 `src/extension/mod.rs` 里的 `duckfn_entrypoint!("...")`（也是 `Makefile` 的 `EXTENSION_NAME`），
> crate 名与 duckfn 版本读 `Cargo.toml`。它们本来就在代码里，复制一份只会变成第二份会过期的真相。

## 动手前先读

**duckfn 的文档与示例随 crate 一起发布**（0.0.11 起）：跑过一次 `cargo build` 之后它们就在本机 cargo 的
解包目录里，与 `Cargo.toml` 钉的版本严格对应 —— 不需要 clone duckfn 仓库，也不需要联网。

```powershell
# Windows：版本号从 Cargo.toml 读（例如 0.0.11）
Get-ChildItem "$env:CARGO_HOME\registry\src\*\duckfn-<版本>" -Directory | Select-Object -ExpandProperty FullName
```

```bash
# Linux / macOS
ls -d ~/.cargo/registry/src/*/duckfn-*/
```

| 资源 | 路径（`<crate>` = 上面那个目录） |
| --- | --- |
| 示例扩展（各类注册方式都有可运行实现） | `<crate>/src/extension/**`：`functions/` 每类一个文件、`types/` 自定义类型、`demo/` 组合示例、`entry.rs` 入口 |
| sqllogictest 范例（41 份 `.test`） | `<crate>/test/sql/**` |
| 用户文档正文（英文） | `<crate>/docs/docs/**` |
| 用户文档正文（简体中文） | `<crate>/docs/i18n/zh-Hans/docusaurus-plugin-content-docs/current/**` |
| 一组可直接跑的 `just sql` 示例 | `<crate>/demo.sh` |

按主题查表（路径都相对 `<crate>`）：

| 主题 | 文档 | 参考实现 |
| --- | --- | --- |
| 示例扩展逐个功能讲解（先读它更快） | `docs/docs/examples/duckfn.md` | `src/extension/**`（`demo/` 是组合示例） |
| 全部属性与参数 | `docs/docs/guide/attributes.md` | — |
| 标量函数 | `docs/docs/guide/scalar-functions.md` | `src/extension/functions/scalar_function.rs` |
| 聚合函数 | `docs/docs/guide/aggregate-functions.md` | `src/extension/functions/aggregate_function.rs` |
| 表函数 | `docs/docs/guide/table-functions.md` | `src/extension/functions/table_function.rs`、`dynamic_table_function.rs` |
| `COPY ... TO` / `FROM` | `docs/docs/guide/copy-functions.md` | `src/extension/functions/copy_function.rs`、`copy_from_function.rs` |
| 类型转换 cast | `docs/docs/guide/casts.md` | `src/extension/functions/cast_function.rs` |
| 替换扫描 | `docs/docs/guide/replacement-scans.md` | `src/extension/functions/replacement_scan.rs` |
| SQL 宏 | `docs/docs/guide/sql-macros.md` | `src/extension/functions/sql_macro.rs`（脚本见 `src/extension/functions/sql/*.sql`） |
| STRUCT / ENUM 等自定义类型 | `docs/docs/guide/custom-types.md` | `src/extension/types/duck_struct_scalar_echo.rs`、`duck_enum_echo.rs` |
| Rust ↔ DuckDB 类型映射 | `docs/docs/guide/types.md` | `src/extension/types/**` |
| 宿主文件系统（`duck_vfs`） | `docs/docs/guide/file-system.md` | `src/extension/functions/file_system.rs` |
| 错误与 panic | `docs/docs/guide/errors-and-panics.md` | — |
| 构建与发布 | `docs/docs/development/build-and-release.md` | — |
| 排错（已知问题） | `docs/docs/known-issues.md` | — |
| 社区扩展文档页（`function_descriptions.csv`） | `docs/docs/community-extension-docs.md` | `src/extension/functions/*.rs`（带 `description` / `example` 的那几个） |
| 文档站部件：可运行 SQL 块（`<dfk-sql>`）、首页组件、TOC 折叠、版本占位 | `docs/docs/docs-kit/**`（**`runnable-sql.md` 是可运行块的全部配置参考**） | 本仓的 `docs/`（装配见 `docs/docusaurus.config.ts`） |

duckfn-docs-kit（npm 包，站点直接依赖）另有一份**给 agent 看的用法契约**，本仓装在
`docs/node_modules/duckfn-docs-kit/AGENTS.md`：渲染契约（几处最容易写错的地方）、DuckDB-Wasm 与
浏览器运行器的已知事实、扩展预加载的文件名契约、版本耦合。**写或审可运行 SQL 块之前先读它**，
上面那张表里 crate 内的 `docs/docs/docs-kit/` 是同一内容的用户文档版 —— `runnable-sql.md` 是块配置、
`sql-test.md` 是 `duckfn-sql-verify` 的行为与全部选项。

属性宏接受哪些参数、允许哪些返回形状，**真相在 `duckfn-macro` 的源码里** —— 它是独立发布的 crate，
解包在同一个 registry 目录下的 `duckfn-macro-<版本>/src/**`；文档与示例只覆盖常用面。

在线版本（文档站 <https://shijianjs.github.io/duckfn/zh-Hans/>、API <https://docs.rs/duckfn>）随时可能是
更新的一版，**与本机依赖冲突时以本地那份为准** —— 它就是实际编译的代码。

**铁律**：任何来源都拿不到时，停下来告诉用户「我查不到 duckfn 的这部分 API」，
不要凭记忆编属性名、参数或返回类型。写错的宏会以编译错误的形式暴露，
但更常见的是一路编到底、最后没法编译。

## 升级 duckfn 时

1. 改 `Cargo.toml` 里的 duckfn 版本，`cargo update -p duckfn -p duckfn-macro`。
2. `cargo build --all-targets` 跑一次：新版本的 crate 会被解包到 registry，文档、示例与 sqllogictest
   范例随包而来，自动与依赖对齐 —— 不需要任何 git 操作。
3. 本文件不用改：它只写占位符，不钉具体版本号。

> **本仓刻意不开 `owned-connection`**：0.0.15 起宿主文件系统 `duck_vfs` 只从这个 feature 来，而落盘已经不走
> 它了 —— `storage.rs` 用 `std::fs`，wasm 侧整个跳过文件操作。升级 duckfn 后如果依赖树里又冒出 `duck_vfs`
> 的需求（比如某个新 feature 默认带上它），那是别的代码在用它，先看清楚再决定要不要跟。
>
> **This repository deliberately does not turn on `owned-connection`**: since 0.0.15 that feature is where the
> host file system (`duck_vfs`) comes from, and persistence no longer goes through it — `storage.rs` uses
> `std::fs` and a wasm build skips the file operation altogether. If an upgrade ever makes `duck_vfs` show up
> as required again, that is some other code asking for it: look before following.


## 仓库约定

### 尽量用成熟三方库实现，不要自己造轮子

写任何「通用」逻辑之前先问一句：这件事是不是已经有 crate（或 std API）在做？

- **平台差异、临时文件与随机名、文件名的合法性规则、编码、哈希、日期时间算术、序列化** ——
  这类通用问题一律先找库。已经这么做的先例：`open_in_browser` 用 `open`（各平台的启动命令与
  参数引用）、`tempfile`（临时文件与随机尾缀）、`sanitize-filename`（文件名字法）。
- **能用 std 就用 std**，别自己拼底层积木：路径绝对化用 `std::path::absolute`，而不是
  `env::current_dir()?.join(path)`。
- 手写只允许出现在**领域逻辑**上（quantstats 的差分规则、报告怎么渲染），或者已知的库都不合适 ——
  后者必须在这段代码的注释里写明「为什么不用库」，例如 `browser.rs::local_path`（判断一个路径能不能交给
  浏览器，看的是字面上的 `://` 而不是引一个 URL 解析库 —— Windows 的 `C:\…` 在 URL 语法里同样是 scheme）。
  反过来，日期换算这类通用算术不要手写：`series.rs` 曾经自己算 epoch，现在交给 duckfn 的 chrono 桥
  （`DuckDate::to_naive_date`）。
- 依赖不是免费的：引入 crate 时在 `Cargo.toml` 里写一句它负责什么、为什么选它，让取舍一眼看得出来；
  只服务某个平台的依赖挂到 target 专属依赖表下
  （见 `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`），别让别的目标替它付编译成本 ——
  有时这甚至是硬要求（`open` 没有 emscripten 的实现，编到 wasm 直接失败）。
- 自查标准：一个「通用」函数如果在 crates.io 上能查到现成实现，它就需要一个留下来的理由。

### 注册到 DuckDB 的函数名统一加 `qs_` 前缀

所有注册到 DuckDB 的函数名一律以 `qs_`（quantstats）开头，例如 `qs_html_reports(...)`。
范围包括标量函数、聚合函数、表函数、COPY 格式、cast、SQL 宏、replacement scan
—— 凡是出现在 SQL 里的名字都要带前缀。

社区扩展几乎都不把包名/扩展名写进函数名（见
<https://duckdb.org/community_extensions/list_of_extensions>）：`duckfn_quantstats_html`
这样的全名在每个调用点上都是纯噪声，而 `qs_` 短到可以忽略，又足以在 `duckdb_functions()` 里
按前缀检索。**前缀只是命名空间，不再是扩展名的缩写**，不要因为扩展名变了就跟着改。

前缀之后的部分要能读懂，不要拿缩写堆砌。当前的两个名字各只有**一个**签名：

```text
qs_html_reports(symbol, date, period_return, options)
qs_html_reports_by_prices(symbol, date, price, options)
```

基准是配置里的一个键（表里的一个 symbol），不占参数位，所以也不需要 `overloads_name` 去把重载并成
函数集；真需要「同一名字下按参数个数分派」时它仍然可用（见 duckfn 的文档）。

duckfn 的属性宏默认拿 **Rust 函数名**当注册名，所以直接把函数定义成 `fn qs_xxx(...)` 即可。
宏还会为每个签名生成 `SQL_NAME` 常量：代码里要引用注册名（错误信息前缀、日志）就读它 ——
`functions/aggregate_html/kind.rs` 就是这么做的 —— 不要再抄一份字面量。代价是这类函数得写成
`pub(super)`，因为生成的模块沿用函数的可见性。

### 文档站：用户文档在前，开发内容统一进「开发指南」

`docs/` 是中英双语的 Docusaurus 站，读者分成两类，顶栏因此分两项、各带一个侧边栏（见
`docs/sidebars.ts`）：

- **用户指南**（顶栏 `User guide` / 中文「用户指南」，sidebar `userGuide`）：`docs/docs/intro.md`、
  `getting-started/`、`guide/`。这个扩展是装一条 `INSTALL` 就能用的插件，所以这一档里不出现
  「编译、构建、cargo、make、`-unsigned`」这类内容 —— 它们属于下面那一档。示例代码一律面向
  「已经装好扩展」的读者。
- **开发指南**（顶栏 `Development guide` / 中文「开发指南」，sidebar `development`）：
  `docs/docs/development-guide/<主题>/` 整棵树 —— `architecture/`（目录结构、设计取舍、依赖）、
  `build/`（构建与发版、测试）、`publishing/`（函数描述、社区扩展注册）、`demo-data/`（演示数据）。
  二级目录的分类写在各自的 `_category_.json` 里（`label` + `position`）；用户指南里那两个分类
  （Getting started / Guide）写在 `sidebars.ts`，因为 `autogenerated` 生成的是目录的**内容**、不是
  目录本身。

新增页面时：英文正文写 `docs/docs/…`，中文镜像写
`docs/i18n/zh-Hans/docusaurus-plugin-content-docs/current/…`，**相对路径必须一致**，
`id` / `slug` / `sidebar_position` 也保持一致；页面之间用相对路径链接（`./x.md`、`../guide/y.md`），
不要写 `/docs/...`（那会把中文页带去英文页）。正文里**不要手写版本号**，写 `{{EXTENSION_VERSION}}`，
构建时由 `docs/extension-version.ts` 替换（发版脚本会更新它）。

**新建内容目录后确认那些文件真的进了 git**：根 `.gitignore` 里的裸名字会匹配任意层级，`build` 曾把
`docs/docs/development-guide/build/` 整个静默吞掉（本地能构建，CI 检出后页面不存在，
`onBrokenLinks: throw` 于是报断链）。`git status` **看不到**被忽略的文件，用 `git check-ignore -v <path>`
查；根 `.gitignore` 里那条已经锚定成 `/build`，别再改回裸名。

站点的其余维护约定（布局、命令、翻译、部署）见 `docs/README.md`。

### 可运行 SQL 块（文档站里的 `sql {"type":"duckfn",…}`）

只有 info string 能解析成 JSON、且带 `"type":"duckfn"` 的块才会变成可运行示例；普通的 ```sql 块
仍然是普通代码块，写「给读者抄走」的示例用它。可运行块的配置（**完整参考见 duckfn 包内的
`docs/docs/docs-kit/runnable-sql.md`**，路径见上面「动手前先读」）：

- `show` 决定渲染方式：`table`（默认）、`text`、`html`、`iframe`（与 `html` 同一个渲染器：把 markup
  放进 iframe 的 `srcdoc`，一行一个标签页，末尾始终留一个 `Table` 标签）、`svg`（内联进页面，脚本 /
  `foreignObject` / `on*` 等会被清掉）；
- **`html` / `iframe` / `svg` 必须同时给 `field`（装 markup 的列）和 `tab_name`（给每个标签页起名的列）**，
  否则渲染出来是一片空白 —— 尤其报表这种多列结果（`symbol` + `html`），只给 `show` 是不够的；
- `option.height` / `option.width` 是预览框的 CSS 长度（报告类内容要限高），`option.sandbox` 替换
  iframe 的 sandbox token（默认 `allow-scripts`，故意不含 `allow-same-origin`）；
- `expect:"error"` 声明「这一块必须失败」，由 `npm test` 双向校验；`extensions` / `repository` 可在
  站点预加载之外再 `LOAD` 别的扩展；
- **示例一律用真实数据**：读文档站自己的
  `https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv`（= `docs/static/demo/prices.csv`，
  与仓库根的 `demo/prices.csv` 是同一份内容，**两份都要留**）。不要用 `range()` 现造的假数据充数 ——
  那种序列画出来的 tearsheet 一眼就是假的。
- **`npm test` 跑在真浏览器里（kit 0.3.0 起）**：`duckfn-sql-verify` 用 `playwright-core` 驱动本机
  的 Chrome/Edge（走 `executablePath`，因为它不会下载浏览器），引擎与扩展都从本地文件经 loopback http
  提供，所以套件本身是离线的 —— 但读 `https://…` 的块因此要真联网才跑得动。页面用什么环境它就用什么
  环境，所以上面那条「示例一律用真实数据」是被测试覆盖的：远程读不再需要手动去浏览器里点一遍。
  找不到浏览器时用 `--browser <exe>`（或 `DFK_BROWSER`）指一个；块卡死由 `--timeout` 兜底，`--report`
  导出逐块结果，`--quiet` 只报意外失败。
- **wasm 下不落盘（有意如此，别当 bug 修）**：扩展在 wasm 上**整个跳过文件操作** —— `output_dir` 收下就忽略，
  不报错、不写文件，`file_path` 是 NULL，报告仍在 `html` 列里。这不是偷懒：DuckDB-Wasm 的文件系统不忠实，
  任何不存在的路径都会返回一条 1 字节 `\0` 的幻影条目（连 DuckDB 自带的 `glob` / `read_text` / `file_size`
  都把它报成存在），所以「这个文件名空着吗」没有可信答案，那道「绝不覆盖已有文件」的保证也就无法兑现；
  `COPY … TO` 也替代不了：它按 CSV / JSON / parquet 导出**查询结果**，载不动任意长的 HTML 原样字节。
  因此**带 `output_dir` 的示例仍是普通 ```sql 块**，并在页面上说明「只有原生构建才写得出文件」——
  别为了让它「看起来能跑」而做成可运行块（那样读者只会看到一列 `NULL`）。
- **块里只写一条语句**：运行器与页面都只展示**最后一条语句**的结果，一个块里塞几个独立示例等于白写 ——
  一个示例一个块。

改完文档站跑两个检查：`just docs_build`（`onBrokenLinks: throw`，中英双语都要过）与
`just test_wasm`（构建 wasm_eh 扩展 → 放进 `docs/static/duckdb-extensions/` → 在真浏览器里把每个
可运行块用 DuckDB-Wasm 跑一遍，并校验 `expect`）。只想跑最后那一步用 `cd docs && npm test`，但要先
有那份 wasm：文档站的预加载本地取自 `static/`，不拉 release（`docs/README.md` 有完整说明）。

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

## 共享 justfile：`scripts/common.just`

日常命令（`build` / `sql` / `repl` / `lint` / `test` / `docs_*` / `ci-*` / `build_wasm*` / `test_wasm` /
`release_*` …）
都在 `scripts/common.just` 里；根 `Justfile` 只 `import "scripts/common.just"`，再留下机器相关的
`set windows-shell` 与本项目相关的 `extension_name`。

这份副本的**源在 duckfn 仓库**（`shijianjs/duckfn/scripts/common.just`）—— 与
[duckfn-extension-template](https://github.com/shijianjs/duckfn-extension-template) 及其他由模板
生成的项目共用同一份，逐字节相同。

| 命令 | 作用 |
| --- | --- |
| `just sync-common` | 拉回最新副本；默认跟 `main`，`DUCKFN_JUST_REF=vX.Y.Z just sync-common` 可钉到某个已发布版本 |
| `just check-common` | 只比对不写回，副本与源不一致时非零退出（可挂进 CI） |

两条规则：

- **不要在副本里改共享 recipe** —— 改了下次同步就没了。要改就改 duckfn 仓库里那份，然后各项目
  `just sync-common`；只想在本项目里改行为，就在根 `Justfile` 里覆盖它，那需要先加
  `set allow-duplicate-recipes := true`：不开这个开关，重名 recipe 会让 just 在解析期直接报错
  （连 `just --list` 都跑不了）。
- 共享文件里**不写具体版本号**（用 `X.Y.Z` 占位），所以 `scripts/release.sh` 的版本替换与它无关，
  每次同步也不会多出一行噪音 diff。

## 发版流程

发版命令都在 `scripts/common.just` 里（`just --list` 可查，见上面「共享 justfile」），实际逻辑在
`scripts/release.sh`。
放进脚本而不是直接写进 Justfile，是因为 just 的 shebang recipe 在 Windows 上需要 `cygpath`
翻译解释器路径，而 Git Bash 并不提供它。

版本号形如 `X.Y.Z`（例如 `0.1.0`）。一次完整的发版 =
提升版本号 → 提交并打 tag → 等 CI 产出 GitHub Release → 切回下一开发版本。

**本项目不发 crates.io。** 它是 DuckDB 的 loadable extension，分发靠 GitHub Release 上的
`.duckdb_extension` 文件（`LOAD` 一个文件即用），所以 duckfn 流程里的发布 crate 那一步
在这里不存在。

只有**正式版本**才打 tag；`0.1.1-dev.0` 这类预发布版本留在分支上，不打 tag、不发布。

### 命令速查

| 步骤 | 命令 |
| --- | --- |
| 0. 前置检查 | `just release_check`（需要时再 `just test`） |
| 1. 提升版本号 | `just release_bump 0.1.0` |
| 2. 提交并打 tag | `git commit …` 后 `just release_tag 0.1.0` |
| 3. 查看 CI | `just release_ci` |
| 4. 切开发版本 | `just release_dev 0.1.1-dev.0` |

### 0. 前置检查

```bash
just release_check   # lint（clippy --all-targets -- -D warnings）+ cargo build --all-targets
just test            # 需要时（等价 make configure debug test，make 部分要在 Git Bash 里跑）
```

有 warning 先修好再提交。确认 `git status` 干净、`main` 已与远程同步。

### 1. 提升版本号

```bash
just release_bump 0.1.0
```

脚本做三件事，并打印每个被改动的文件：

- **Cargo.toml**：`[package]` 段的 `version` 从项目当前版本改成新版本。**只改这一行**，不做整份文件的
  全局替换 —— 本文件里 `quantstats-rs = "<版本>"` 这类依赖需求可能出现同一个字符串，全局替换会把它
  一起改掉（duckfn 本体的脚本没有这个问题，它的版本号是唯一的）。
- **文档 / README / CI 注释 / Justfile 示例**：取**最近一次 tag** 的版本改成新版本，文件由 `git grep`
  自动找出，不需要维护清单；排除 `Cargo.toml`、`Cargo.lock`、`AGENTS.md`、`scripts/`、`test/`、`demo/`，
  以及文档站的 `docs/package-lock.json`（里面是依赖自己的版本号）、`docs/docs`、`docs/i18n`、`docs/build`。
  其中 `test/` 是必须排掉的：那里的 `Generated by QuantStats-RS (v. X.Y.Z)` 是 quantstats-rs
  报出来的版本号，与本扩展的版本号无关，改了会把测试改坏。文档站正文里只写 `{{EXTENSION_VERSION}}`
  占位符，真正的版本号集中在 `docs/extension-version.ts`，由脚本单独替换（它在排除列表里，且可能是
  尚未被 git 跟踪的新文件）。
- `cargo update -p duckfn_quantstats` 同步 `Cargo.lock`；然后回读 `Cargo.toml` 的
  `[package] version` 确认改写生效，并在**刚改过的那些文件**里核对旧版本号残留（应当为空 ——
  不整棵树 grep，同样是为了避开 `test/` 里那个无关的版本号）。

还没有任何版本 tag 时（首次发版），文档那一步整体跳过：没有「上一个版本」可以替换。

### 2. 提交并打 tag

```bash
git add -A
git commit -m "chore(release): 发布 v0.1.0" -m "- 版本号 0.1.1-dev.0 -> 0.1.0"
just release_tag 0.1.0     # 打 tag v0.1.0，推送 main 与 tag
```

`release_tag` 先检查工作区是否干净，再核对 `Cargo.toml` 的 `[package] version` 与 tag 一致 ——
扩展二进制里的版本号（`cargo duckdb-ext build` 打印的 `Packing Extension Version`）是构建时由 cargo
写进去的，对不上就会发出一个自称别的版本的 Release。

推送 tag 触发两个工作流：`.github/workflows/MainDistributionPipeline.yml` 为各平台构建扩展并跑测试，
然后为该 tag 创建（或更新）GitHub Release，把构建出的 `.duckdb_extension` 全部挂上去（推 main 本身不构建）；
`.github/workflows/DeployDocs.yml` 构建 `docs/` 并发布到 GitHub Pages（需要一次性在仓库设置里把
Pages 的 Source 设为 GitHub Actions）。

### 3. 等 CI 全绿

```bash
just release_ci            # gh run list --limit 5
gh run watch <run-id>
```

失败就修到成功为止。若已推送的 tag 需要重发（修复后重新指向新的提交）：

```bash
git push origin --delete v0.1.0   # 删除远程 tag
git tag -f v0.1.0                 # 本地 tag 指向修复后的提交
git push origin v0.1.0            # 重新推送
```

> 删除 / 移动已发布的 tag 会影响已有的 GitHub Release，谨慎操作。

网络报错（`schannel: failed to receive handshake`、`SSL connect error` 之类）是**间歇性**的，原样重试
一两次即可，**不要擅自更改网络 / 代理设置**：本机 git 是全局配了代理的，`github.com` 不走代理基本用
不了，动了它反而让 `release_tag` 的推送直接失败。

### 4. 切到下一开发版本

```bash
just release_dev 0.1.1-dev.0
```

这一步只动 `Cargo.toml` 与 `Cargo.lock`：文档与 README 里的示例始终指向最新**已发布**版本，
不打 tag、不发布。

## 相关文档

- [`scripts/release.sh`](scripts/release.sh)：`release_bump` / `release_dev` / `release_tag` 的实际实现。
- [`.github/workflows/MainDistributionPipeline.yml`](.github/workflows/MainDistributionPipeline.yml)：
  构建矩阵、触发面与 Release 发布。
- [`.github/workflows/DeployDocs.yml`](.github/workflows/DeployDocs.yml)：文档站的 GitHub Pages 部署。
- [`docs/`](docs/README.md)：中英双语文档站（Docusaurus）。用户指南与开发笔记都在 `docs/docs/`（中文在
  `docs/i18n/zh-Hans/`）—— 目录结构、设计取舍、依赖、测试、构建与发版都在那边，根目录不再另存一份。
  站点自身的维护约定见 [`docs/README.md`](docs/README.md)。
- [`community-extension/AGENTS.md`](community-extension/AGENTS.md)：社区扩展注册（上游 `description.yml` 的
  草稿、字段依据、提交 PR 的步骤）。

