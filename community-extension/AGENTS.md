# community-extension/ 的约定

这个目录不参与扩展运行，它是**向 [duckdb/community-extensions](https://github.com/duckdb/community-extensions)
提 PR 的暂存处**：那个 PR 只加两个文件，这里按目标路径摆好，提交时复制过去即可。

| 本目录 | 社区仓里的位置 |
| --- | --- |
| `description.yml` | `extensions/duckfn_quantstats/description.yml` |
| `docs/function_descriptions.csv` | `extensions/duckfn_quantstats/docs/function_descriptions.csv` |

目录名必须与 `extension.name` **逐字一致** —— 社区仓的 `scripts/build.py` 会校验，不一致直接报错。

## `description.yml` 里**不要写注释**

这份文件会被原样复制到上游，所以刻意保持「只有字段」。需要解释的东西写在本文件里，不要写回 YAML。

`repo.ref` 写**发布那一版的提交 SHA（40 位）**（现在是 v0.1.0 那个提交 `2c2c1a47a15d2649d214a8bf1351c7be5426d6bb`），
**不要写 `main`、也不要写 tag 名**。三者都是合法 git ref、社区仓都能照着 clone，但上游已收录的扩展清一色用提交
SHA（`extensions/h3`、`extensions/orc` 都是），跟着走既不用解释，也天生不可变 —— 注册项指向的东西不会随时间漂移。
写 `main` 的代价是实打实的：构建出来的二进制会自称 main 上的开发版本（`X.Y.Z-dev.N`），与这里声明的 `version`
对不上，而且同一个注册项在不同时间构建出的是不同代码。

## 字段为什么这么写（都不是猜的）

- `language: Rust` / `build: cargo`：本仓 Makefile 就是官方 Rust 模板那一套（include
  `extension-ci-tools/makefiles/c_api_extensions/rust.Makefile`）；社区仓里的 `rusty_quack`
  （仓库就是 `duckdb/extension-template-rs`，即该模板）用的正是 `build: cargo`。
- `requires_toolchains: "rust;python3"`：与 `.github/workflows/MainDistributionPipeline.yml` 的
  `extra_toolchains` 一致；`python3` 是 extension-ci-tools 的 configure / 测试流程要的。
- **不写 `excluded_platforms`**：没有需要排除的平台。CI 已在 linux_amd64、linux_arm64、osx_amd64、
  osx_arm64、windows_amd64、windows_amd64_mingw、wasm_mvp、wasm_eh、wasm_threads 上全部构建成功
  （见 Actions 里最近一次成功的 Main Extension Distribution Pipeline）；两个 musl 平台
  （`linux_amd64_musl` / `linux_arm64_musl`）在矩阵里都是 `opt_in`，不主动点名就不会构建，所以也不必写进
  排除列表 —— 本仓 CI 里那行 `exclude_archs: 'linux_amd64_musl'` 因此已删掉，它与 `opt_in` 是重复的。
- `version`：写**要发布的那一版**，不要 `-dev.N`（本仓开发版本是 `0.1.0-dev.0`，发版流程把它抬成 `0.1.0`）。
  它与 `repo.ref` 配套 —— `ref: v0.1.0` ↔ `version: 0.1.0`，所以社区仓构建出的二进制自称的版本、
  文档页上的版本号、这里的字段三者一致，不存在漂移。
- `license: MIT`：对应仓库根目录的 `LICENSE`。注意 duckdb.org 的社区扩展文档页把字段名写成 `licence`，
  那是**文档的错**，真实 schema 是 `license`（以已收录扩展的 `description.yml` 为准）。
- `docs.hello_world`：社区文档页会把它渲染进代码块，所以必须是**可直接复制跑**的真实例子 —— 现在这两段
  就是 README 快速上手的第 1、2 个示例（先把 `demo/prices.csv` 读成 `prices` 再出报告）。报告要真出图，
  别用几十天的假数据糊弄，那样的 tearsheet 看着就不像个东西。页面上「Installing and Loading」那段由
  社区仓的 `layout/default.md` 自动加，`hello_world` 里不要再写 `INSTALL` / `LOAD`。

## 那份 CSV 是必须的，而且会过期

社区文档页「Added Functions」表里，函数的 description / comment / example **只有一个来源**：这个 CSV
（DuckDB 的 C 扩展 API 没有设置它们的接口）。它由 `just docs_csv` 从属性宏上的 `description` /
`comment` / `example` 生成。**改了那些属性就要重新生成、覆盖这里这份**，否则文档页停在旧文案上。

## 提交：在 fork 的克隆里做，本仓只出那两张文件

上游注册仓（`duckdb/community-extensions`）的 fork 与本地克隆：

- fork：`shijianjs/duckdb-community-extensions`
- 克隆：`S:\workspace\my\rust\duckdb\duckdb-community-extensions`（`origin` 就是这个 fork，没有配 `upstream`）

流程是「加分支 → 复制两个文件 → 提交 → 推分支 → 开 PR」，本仓这边一行都不用改：

```powershell
$p = 'S:\workspace\my\rust\duckdb\duckdb-community-extensions'
git -C $p checkout main; git -C $p pull              # 先和 fork 的 main 同步
git -C $p checkout -b add-duckfn-quantstats          # 已有同名分支就跳过这步

New-Item -ItemType Directory -Force "$p\extensions\duckfn_quantstats\docs" | Out-Null
Copy-Item community-extension\description.yml "$p\extensions\duckfn_quantstats\description.yml" -Force
Copy-Item community-extension\docs\function_descriptions.csv "$p\extensions\duckfn_quantstats\docs\function_descriptions.csv" -Force

git -C $p add extensions/duckfn_quantstats
git -C $p commit -m "Add duckfn_quantstats: …"       # 上游惯例是 `Add <扩展名>` 再跟一句说明
git -C $p push -u origin add-duckfn-quantstats
gh pr create --repo duckdb/community-extensions --base main --head shijianjs:add-duckfn-quantstats `
  --title '…' --body-file …
```

- 首次提交的 PR：<https://github.com/duckdb/community-extensions/pull/2778>（2026-09-23，只加那两张文件）。
- **每次发版都要回来改这一行**：`repo.ref` 换成新发布提交的 SHA（`git rev-list -n 1 v0.1.1`），
  `version` 跟着改成 `0.1.1`，改完在本仓 `community-extension/` 里改，再复制进那份克隆，推到同一条 PR 分支
  （PR 会自动更新）或另开一个。注册项钉的是具体提交，不会自己跟。
- PR 一开，上游会起三条构建（`Community Extension Build`、`Community Extension with latest DuckDB`、
  `Community Extension with DuckDB on Andium`）。外部贡献者的 PR 会被 GitHub 挂成 `action_required`
  —— 等维护者点 Run，属正常状态，别因此重推。
- 想把 fork 与上游同步时，那份克隆里得先补个上游远程（它只配了 `origin`）：
  `git -C $p remote add upstream https://github.com/duckdb/community-extensions.git`，之后
  `git -C $p fetch upstream && git -C $p merge upstream/main`。

## 提交前自查

```powershell
# 1. 键集要和社区仓 generate_extensions_json.py 读的一致（三组顶层键 + 各组的子键）
.\configure\venv\Scripts\python.exe -c "import yaml;d=yaml.safe_load(open('community-extension/description.yml',encoding='utf-8'));print(list(d));print(list(d['extension']));print(list(d['repo']));print(list(d['docs']))"

# 2. hello_world 原样能跑：把 docs.hello_world 抽成 .sql，前面补一行 LOAD 本地产物，再交给本机 duckdb
#    （验证时把 'open_in_browser': true 改成 false，免得弹一堆浏览器标签页）
duckdb -unsigned -c ".read target/hello_from_yml.sql"
```

- 仓库根目录必须有 `LICENSE`（社区扩展要求开源，`license` 字段只是声明）。
- `INSTALL duckfn_quantstats FROM community` 要等 PR 合并后才真的可用 —— 合并前 README 里那套加载方式
  只对之后的版本成立。
