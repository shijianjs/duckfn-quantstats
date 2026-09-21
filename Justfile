set windows-shell := ["C:\\Program Files\\Git\\bin\\bash.exe", "-c"]

extension_name := "duckfn_quantstats"

config_env:
	rustup override set 1.86.0
	rustup target add wasm32-unknown-emscripten
	rustup target list --installed

release:
  cargo build --release

ext_build:
    cargo duckdb-ext build

duckdb_ext sql: ext_build
    duckdb -unsigned -c "LOAD './target/debug/{{extension_name}}.duckdb_extension'; {{sql}}"

duckdb_ext_debug sql: ext_build
    duckdb -unsigned -cmd "LOAD './target/debug/{{extension_name}}.duckdb_extension'; {{sql}}"

# 初始化extension-ci-tools环境
# 这个命令会生成一个configure文件夹，里面包含了python venv
ci-init:
	make configure

ci-build: ci-init
	make debug

# 这个just命令可以跑所有test/sql/**/*.text测试
test: ci-build
    make test

build_wasm:
	cargo build --release --target wasm32-unknown-emscripten --example {{extension_name}}

