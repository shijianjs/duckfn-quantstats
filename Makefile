.PHONY: clean clean_all

PROJ_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

EXTENSION_NAME=duckfn_quantstats

# 置 1 开启 Unstable API：元数据写 `C_STRUCT_UNSTABLE`，产物只能在 TARGET_DUCKDB_VERSION 那个引擎上工作。
# 本扩展置 **0**：只用 C API 的稳定区（`C_STRUCT`），一份产物因此跨 DuckDB 发行版可用。
# 唯一会逼我们改回 1 的情形是需要不稳定区的能力（COPY 函数、宿主 VFS、标量 bind/init 那些槽位）——
# duckfn 把它们统一收在 `duckdb-1-5` feature 后面（见 Cargo.toml）。
#
# Set to 1 to enable Unstable API: the metadata says `C_STRUCT_UNSTABLE` and the binary only works on the
# TARGET_DUCKDB_VERSION engine. This extension sets it to **0**: the stable region only (`C_STRUCT`), which
# makes one binary portable across DuckDB releases. The one thing that would force it back to 1 is needing
# the unstable region (COPY functions, the host VFS, the scalar bind/init slots) — duckfn groups those
# behind its `duckdb-1-5` feature (see Cargo.toml).
USE_UNSTABLE_C_API=0

# 目标 C API 版本 / Target C API version
TARGET_DUCKDB_VERSION=v1.2.0

# 它承载的是**产物所需的 C API 版本**，不是 DuckDB 发行版本号：`C_STRUCT` 构建下，`append_extension_metadata.py
# -dv` 写进元数据 FIELD3 的这一栏由加载器按 C API 版本解释（脚本自己的帮助文本就是这么写的：
# 「depending on the ABI type this encodes the duckdb version or the C API version」）。规则是「下限」——引擎的
# C API 低于它就会拒绝加载：
#   The file was built for DuckDB C API version '…', but we can only load extensions built for DuckDB C API '…' and lower.
#
# 所以这一行填「我们还要往多旧的地方跑」，填一次就不用再跟上游发行版走：v1.2.0 是 DuckDB 1.3.2 ~ 1.5.5 这些
# 引擎共同停留的 C API 版本，一份产物通吃 —— 实测同一个二进制在 1.3.2 / 1.4.0 / 1.4.5 / 1.5.0 / 1.5.5 / 1.5.6
# 上全部加载成功并跑通真实调用。反过来，在这里填发行版本号（比如 v1.5.6）就只有那个引擎肯收，等于又把产物绑回
# 版本上。它由我们自己维护 —— workflow 与社区 registry 都不会改写它：ci-tools 里 `set_duckdb_version` 对 C API
# 扩展是 nop，registry 的 `duckdb_version` 只决定签出哪份 DuckDB 源码、产物怎么命名、deploy 到哪个版本目录。
#
# 它**不**决定头文件从哪来：本项目的头文件由 `Cargo.toml` 钉的 `libduckdb-sys` 决定（1.10505.0 → DuckDB 1.5.5，
# 见它 build.rs 的 `duckdb_version_from_pkg_version`）。`rust.Makefile` 还会把它当
# `DUCKDB_EXTENSION_MIN_DUCKDB_VERSION` 转给 cargo，但依赖树里没有 crate 读它。
#
# 哪天把 `USE_UNSTABLE_C_API` 改回 1，这一行就得**换成引擎的发行版本号**（`C_STRUCT_UNSTABLE` 下它按发行版本
# 解释、且要求逐字相等），并给 quack-rs 补上 `QUACK_RS_TARGET_DUCKDB_VERSION=$(TARGET_DUCKDB_VERSION)`：它的 ABI
# 检查只在不稳定模式下跑（feature `duckdb-1-5` 关掉时直接返回 stable-only），稳定模式下那行是多余的。
#
# This carries the **C API version the binary needs**, not a DuckDB release number: under a `C_STRUCT` build the
# loader reads this metadata field (FIELD3, written by `append_extension_metadata.py -dv`) as a C API version —
# the script's own help text says so ("depending on the ABI type this encodes the duckdb version or the C API
# version"). The rule is a floor: an engine whose C API is older refuses the file with the error quoted above.
#
# So this line says "how old are we still willing to run on" and then stops chasing upstream releases: v1.2.0 is
# the C API version DuckDB 1.3.2 – 1.5.5 all sit at, so one binary covers them — measured, the same artifact loads
# and runs a real call on 1.3.2 / 1.4.0 / 1.4.5 / 1.5.0 / 1.5.5 / 1.5.6. Putting a release number here instead
# (say v1.5.6) leaves only that engine willing to take it — version-bound again. It is ours to maintain — neither
# the workflow nor the community registry rewrites it: `set_duckdb_version` is a nop for C API extensions, and
# the registry's `duckdb_version` only picks which DuckDB source to check out, names the artifacts and chooses
# the version directory to deploy to.
#
# It does **not** decide where the headers come from: they come from the `libduckdb-sys` pinned in `Cargo.toml`
# (1.10505.0 → DuckDB 1.5.5, see `duckdb_version_from_pkg_version` in its build.rs). `rust.Makefile` also forwards
# this value to cargo as `DUCKDB_EXTENSION_MIN_DUCKDB_VERSION`, but no crate in the tree reads it.
#
# Flipping `USE_UNSTABLE_C_API` back to 1 turns this line back into an engine **release** version
# (`C_STRUCT_UNSTABLE` reads it as one and demands an exact match) and brings back
# `QUACK_RS_TARGET_DUCKDB_VERSION=$(TARGET_DUCKDB_VERSION)` for quack-rs, whose ABI check only runs in unstable mode
# (with the `duckdb-1-5` feature off it returns stable-only), so under the stable ABI that line is redundant.

all: configure debug

# 引入 DuckDB 提供的 makefile / Include makefiles from DuckDB
include extension-ci-tools/makefiles/c_api_extensions/base.Makefile
include extension-ci-tools/makefiles/c_api_extensions/rust.Makefile

configure: venv platform extension_version

debug: build_extension_library_debug build_extension_with_metadata_debug
release: build_extension_library_release build_extension_with_metadata_release

test: test_debug
test_debug: test_extension_debug
test_release: test_extension_release

clean: clean_build clean_rust
clean_all: clean_configure clean
