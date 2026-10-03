# 与 duckfn-extension-template 里那份 Justfile 同源（本仓库是那一套模板的实例）。
#
# extension_name 必须与下面几处一致，否则 LOAD 会失败：
#   - src/extension/mod.rs 里 duckfn_entrypoint!("...") 的名字
#   - Cargo.toml 的 [package] name 与 [[example]] name
#   - Makefile 里的 EXTENSION_NAME
#   - 构建产物 <extension_name>.duckdb_extension
#
# 日常命令（build / sql / repl / lint / test / docs_* / ci-* / release_* …）都在
# scripts/common.just 里 —— 那是**共享源**的副本（源在 duckfn 仓库的 scripts/common.just，
# 各扩展项目一份、内容逐字节相同），由本文件 import 进来。要在本项目里覆盖某条共享 recipe，
# 加 `set allow-duplicate-recipes := true` 再重写它。细节见 scripts/common.just 的头注释。
#
# 共享文件更新：just sync-common（默认跟 main，改动看 git diff）；比对：just check-common。
#
# 前置工具：
#   cargo install just          # recipe 驱动器
#   make + python3              # 官方 extension-ci-tools 构建/测试流程（Windows 上 make 用 Git Bash）
#
# 说明：
#   - 构建走官方 makefile：just build = make configure + make debug；just sql / just repl 加载
#     build/debug/<扩展名>.duckdb_extension。
#   - sqllogictest 与 CI 走同一套（just ci-init 一次，之后 just test）。

# Windows 下 recipe 交给 Git Bash 执行；按自己的 Git 安装路径调整。
# 非 Windows 上这一行不生效。
#
# 这是机器相关的路径，所以留在本地文件里，不放进共享文件。
set windows-shell := ["C:\\Program Files\\Git\\bin\\bash.exe", "-c"]

# 本文件在 import 之后覆盖了同名 recipe（default），需要显式允许重名（浅层覆盖深层）。
set allow-duplicate-recipes := true

import "scripts/common.just"

# 不带参数运行 just 时列出所有 recipe。
# 从 common.just import 进来的 default 不会成为本文件的默认（just 取根文件里第一个 recipe），
# 所以在入口文件这里显式定义一份。
default:
    @just --list

# 扩展名：全小写、只含下划线
extension_name := "duckfn_quantstats"
