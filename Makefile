.PHONY: clean clean_all

PROJ_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

EXTENSION_NAME=duckfn_quantstats

# 置 1 开启 Unstable API（产物只能在 TARGET_DUCKDB_VERSION 上工作，会失去向前兼容）。
# 本扩展置 **0**：只用 C API 的稳定区，产物因此跨 DuckDB 发行版可用，上游发新版本不必跟着改下面的
# `TARGET_DUCKDB_VERSION`（实测：声明 v1.5.5 的产物在 1.5.6 引擎里正常加载并跑完全部 sqllogictest）。
# 唯一会逼我们改回 1 的情形是需要不稳定区的能力（COPY 函数、宿主 VFS、标量 bind/init 那些槽位）——
# duckfn 把它们统一收在 `duckdb-1-5` feature 后面（见 Cargo.toml）。
#
# Set to 1 to enable Unstable API (binaries will only work on TARGET_DUCKDB_VERSION, forwards compatibility will be broken).
# This extension sets it to **0**: it stays inside the stable region of the C API, which makes the binary portable
# across DuckDB releases — a new upstream release no longer forces an edit of `TARGET_DUCKDB_VERSION` below
# (measured: a binary declaring v1.5.5 loads into a 1.5.6 engine and passes the whole sqllogictest suite).
# The one thing that would force it back to 1 is needing the unstable region (COPY functions, the host VFS,
# the scalar bind/init slots) — duckfn groups those behind its `duckdb-1-5` feature (see Cargo.toml).
USE_UNSTABLE_C_API=0

# 目标 DuckDB 版本 / Target DuckDB version
TARGET_DUCKDB_VERSION=v1.5.6

# 这里填的是**构建所依据的 DuckDB 发行版**，是它决定编出什么样的二进制（`libduckdb-sys` 解析到哪个版本的
# 头文件），也是 `append_extension_metadata.py -dv` 写进扩展元数据的那个版本号（实测确认过：FIELD3
# `duckdb_version` 一栏就是它）。它由我们自己维护 —— 我们的 workflow 与社区 registry 都不会改写它：
# ci-tools 里 `set_duckdb_version` 对 C API 扩展是 nop，官方 CI / registry 的 `duckdb_version` 只决定签出
# 哪份 DuckDB 源码、产物怎么命名、deploy 到哪个版本目录。
#
# 稳定 ABI 下它**不再需要跟着每个上游发行版走**：`USE_UNSTABLE_C_API=0` 写的是 `abi_type = C_STRUCT`，
# 加载器对稳定 C API 的产物不要求与引擎版本精确相等，只有换成 `C_STRUCT_UNSTABLE` 时才会 —— 所以这一行
# 只在「想用更新的 DuckDB 头文件」时才动。哪天把 `USE_UNSTABLE_C_API` 改回 1，就同时要在这里跟上引擎版本，
# 并给 quack-rs 补上 `QUACK_RS_TARGET_DUCKDB_VERSION=$(TARGET_DUCKDB_VERSION)`：它的 ABI 检查只在不稳定
# 模式下跑（feature `duckdb-1-5` 关掉时直接返回 stable-only），稳定模式下那行是多余的。
#
# This is the **DuckDB release the build rests on**: it decides what binary comes out (which version's headers
# `libduckdb-sys` resolves to) and is the version `append_extension_metadata.py -dv` writes into the extension
# metadata (verified: FIELD3, `duckdb_version`, is exactly this). It is ours to maintain — neither our workflow
# nor the community registry rewrites it: `set_duckdb_version` is a nop for C API extensions, and the official
# CI's / the registry's `duckdb_version` only picks which DuckDB source to check out, names the artifacts and
# chooses the version directory to deploy to.
#
# With the stable ABI it **no longer has to follow every upstream release**: `USE_UNSTABLE_C_API=0` writes
# `abi_type = C_STRUCT`, and the loader does not demand an exact engine match for a stable C API binary — only
# `C_STRUCT_UNSTABLE` does. So this line moves only when we want newer DuckDB headers. Flipping
# `USE_UNSTABLE_C_API` back to 1 means tracking the engine version here again and adding
# `QUACK_RS_TARGET_DUCKDB_VERSION=$(TARGET_DUCKDB_VERSION)` for quack-rs: its ABI check only runs in unstable
# mode (with the `duckdb-1-5` feature off it returns stable-only), so under the stable ABI that line is redundant.

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
