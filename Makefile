.PHONY: clean clean_all

PROJ_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

EXTENSION_NAME=duckfn_quantstats

# 置 1 开启 Unstable API（产物只能在 TARGET_DUCKDB_VERSION 上工作，会失去向前兼容）。
# 注：当前扩展模板要求开启，因为 duckdb-rs 依赖 unstable C API。
#
# Set to 1 to enable Unstable API (binaries will only work on TARGET_DUCKDB_VERSION, forwards compatibility will be broken)
# Note: currently extension-template-rs requires this, as duckdb-rs relies on unstable C API functionality
USE_UNSTABLE_C_API=1

# 目标 DuckDB 版本 / Target DuckDB version
TARGET_DUCKDB_VERSION=v1.5.6

# 告诉 quack-rs 这份绑定是按哪个 DuckDB 发行版编译的，好让它的 ABI 检查接受一个**比它布局表更新**的发行版
# —— DuckDB 一发版、而 quack-rs 还没收录那次不稳定区布局时就是这个情形（quack-rs 自带的脚手架里也有这一行）。
#
# Tells quack-rs which DuckDB release these bindings were built against, so its ABI check accepts a release
# its layout table predates — the case every time DuckDB ships before quack-rs learns that layout. quack-rs'
# own scaffold emits exactly this line, and it follows `set_duckdb_version`, which rewrites the pin above in CI.
export QUACK_RS_TARGET_DUCKDB_VERSION = $(TARGET_DUCKDB_VERSION)

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
