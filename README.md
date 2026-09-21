# duckfn_quantstats

用 [duckfn](https://crates.io/crates/duckfn) 写的 DuckDB 扩展（loadable extension）：在 SQL 里直接产出 quantstats 报告。

本项目从 DuckDB 官方 [extension-template-rs](https://github.com/duckdb/extension-template-rs) 起步，
并已按 duckfn 的骨架约定改造（入口模块、`EXTENSION_NAME`、依赖列表）。

## 入口链路

```text
src/lib.rs           ->  mod extension;
src/wasm_lib.rs      ->  mod extension;   （同一组 mod，镜像）
src/extension/mod.rs ->  duckfn_entrypoint!("duckfn_quantstats");
```

扩展名 `duckfn_quantstats` 必须与 `Makefile` 的 `EXTENSION_NAME`、产物文件名一致；
`src/lib.rs` 与 `src/wasm_lib.rs` 必须声明同一组 `mod`（官方模板的 `mod lib;` 写法在嵌套模块时会报 `E0583`）。
新增功能时按 duckfn 的目录分层往 `src/extension/` 下面挂。

## 依赖

- [duckfn](https://crates.io/crates/duckfn)：属性宏，把普通 Rust 函数注册成 DuckDB 函数。
- [quack-rs](https://crates.io/crates/quack-rs)：DuckDB C API 绑定，`duckfn_entrypoint!` 展开出的代码直接引用它。
- [libduckdb-sys](https://crates.io/crates/libduckdb-sys)：只取头文件，开启 `loadable-extension`，
  因此**不需要在本地编译 DuckDB**。

## 构建

日常迭代用 `cargo-duckdb-ext-tools`（全局 cargo 子命令，不给项目加依赖）：

```shell
cargo install cargo-duckdb-ext-tools   # 只需安装一次
cargo duckdb-ext build                 # -> target/debug/duckfn_quantstats.duckdb_extension
```

官方模板那条 `make` 流程仍然保留（CI 与 sqllogictest 走它），首次需要 `make configure` 建 Python venv：

```shell
make configure   # 只做一次
make debug       # -> build/debug/extension/duckfn_quantstats/duckfn_quantstats.duckdb_extension
```

`make release` 是带优化的同一套流程。Windows 上 `make` 需要在 Git Bash 里跑。

## 加载

扩展基于 DuckDB 的 unstable C API 构建，必须加 `-unsigned`：

```shell
duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

## 测试

测试用 SQLLogicTest 格式写在 `test/sql/` 下：

```shell
make test_debug     # 或 make test_release
```

新增函数时至少覆盖：正常值、`NULL`、边界值、错误路径（`statement error`）。
