---
title: 快速开始
sidebar_position: 1
description: 装好扩展（或从源码构建），然后用一条 SQL 产出整套 quantstats HTML 报告。
---

# 快速开始

## 前置条件

- **DuckDB 1.5 及以上** —— `duckdb` 在 `PATH` 里。`output_dir` 背后的宿主文件系统是 1.5 才进
  DuckDB C API 的，所以 1.4 不行；本扩展在 v1.5.5 上构建与测试。
- 想从源码构建，还需要项目用的这套工具：
  - **Rust** 1.86 及以上（`Cargo.toml` 里的 `rust-version`）；
  - **[just](https://github.com/casey/just)** 与 **cargo-duckdb-ext-tools**：
    `cargo install just cargo-duckdb-ext-tools`；
  - 可选：**make**（Windows 上要在 Git Bash 里跑）与 Python，CI 走的官方构建 / 测试流程需要它们。

## 1. 安装并加载

扩展发布在 DuckDB 的[社区仓](https://duckdb.org/community_extensions/extensions/duckfn_quantstats)，
一条 `INSTALL` 就把当前平台的签名产物取回来 —— 不需要 `-unsigned`，本地也不编译任何东西：

```sql
INSTALL duckfn_quantstats FROM community;   -- 只需一次，需要网络
LOAD duckfn_quantstats;                     -- 之后每个会话只要这一句
```

社区扩展**针对最新的稳定版 DuckDB** 构建，而本扩展用的是 DuckDB 的 unstable C API，所以 `INSTALL`
取回的产物与构建它的那个版本严格绑定。更老的 DuckDB（比如 1.4）没有对应产物 —— 那种情况改用
[从源码构建](#3-从源码构建)。

## 2. 产出报告

一次调用就把整张按日期排列的长表折成「每个标的一份报告」。下面这段在你的浏览器里就能跑（站点会预加载
已发布的扩展，所以这里不用 `LOAD`）；`series` CTE 是一张合成的两标的表，`SYN-SPX` 当基准：

```sql {"type":"duckfn","show":"table"}
WITH series AS (
    SELECT 'SYN-A' AS symbol, DATE '2024-01-01' + CAST(i AS INTEGER) AS date, 100.0 * pow(1.002, i) AS price
    FROM range(0, 60) t(i)
    UNION ALL
    SELECT 'SYN-B', DATE '2024-01-01' + CAST(i AS INTEGER), 100.0 * pow(1.001, i) FROM range(0, 60) t(i)
    UNION ALL
    SELECT 'SYN-SPX', DATE '2024-01-01' + CAST(i AS INTEGER), 100.0 * pow(1.0005, i) FROM range(0, 60) t(i)
)
SELECT (r).symbol, (r).benchmark, (r).benchmark_title, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'title': symbol,
                'strategy_title': symbol,
                'benchmark': ['SYN-SPX'],
                'benchmark_title': ['Synthetic index']}::qs_html_report_options)) AS r
    FROM series
);
```

`SYN-SPX` 只作输入：它是两份报告的基准，自己不出报告。

### 用提交进仓库的行情快照

`demo/prices.csv` 是一份提交进仓库的日收盘价快照：`GOOGL`、`MSFT` 与标普 500 指数（`SPX`），各 1435
个交易日，区间 2021-01-04 … 2026-09-21，三者交易日历完全一致。下面这段直接复制粘贴就能跑 ——
`read_csv` 自己走 HTTP 取回文件（DuckDB 1.5 自带 `https://` 读取，不需要 `httpfs`，也不需要 API key）：

```sql
INSTALL duckfn_quantstats FROM community;   -- 只需一次，需要网络
LOAD duckfn_quantstats;

CREATE TABLE prices AS
SELECT * FROM read_csv('https://raw.githubusercontent.com/shijianjs/duckfn-quantstats/main/demo/prices.csv');
-- 拉不动（比如国内网络）？同一个文件有 jsDelivr 镜像：
--   read_csv('https://cdn.jsdelivr.net/gh/shijianjs/duckfn-quantstats@main/demo/prices.csv')
-- 已经 clone 了仓库？直接 read_csv('demo/prices.csv')

-- 整张表带基准，落盘到当前目录。output_dir 只给目录，文件名由函数生成
SELECT (r).symbol, (r).benchmark, (r).file_path
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol,
                'rf': 0.04,
                'output_dir': './'}::qs_html_report_options)) AS r
    FROM prices
);
```

一份报告是几百 KB 的 HTML（内嵌十几张 SVG），`unnest(...)` 会把它们一行份地铺开，所以终端里更适合让
`output_dir`（落盘）或 `open_in_browser`（用浏览器打开）接手。只想看清单、不看 HTML 时，用
`list_transform` 只挑需要的字段即可：

```sql
SELECT list_transform(
           qs_html_reports_by_prices(symbol, date, price,
               {'benchmark': ['SPX'], 'output_dir': './'}::qs_html_report_options),
           lambda x: {'symbol': x.symbol, 'benchmark': x.benchmark, 'file': x.file_path}) AS reports
FROM prices;
```

## 3. 从源码构建

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

自己构建出的产物没有签名，而且用的是 DuckDB 的 unstable C API，加载时必须加 `-unsigned`
（社区仓那份不用：它是签过名的，并且与你的 DuckDB 版本严格匹配）：

```shell
duckdb -unsigned -c "
LOAD './target/debug/duckfn_quantstats.duckdb_extension';
SELECT ...;
"
```

## 4. 跑测试

```shell
just test           # make configure + make debug + make test
```

更快的迭代方式（不用 `make`、不重建 Python venv）以及每个文件覆盖什么，见
[测试](../development/testing.md)。

## 几个坑

::::warning[看着像 bug，其实不是]

- **本地构建的产物加载时必须加 `-unsigned`。** 不加 DuckDB 会直接拒绝这个文件。
- **产物文件名必须保持 `duckfn_quantstats.duckdb_extension`。** DuckDB 是按文件名去找入口符号的，
  复制成 `win.duckdb_extension` 会报 `did not contain function "duckfn_quantstats_init_c_api"`。
- **`make test` 不会自动重新构建。** 改完 Rust 必须先 `just ci-build`（或 `make debug`），
  否则跑的还是上一次的产物。
- **`output_dir` 指向的目录必须已经存在。** 函数不会替你创建；写一个不存在的目录会在第一次跑的时候就报错。
- **配置的 struct 字面量必须显式写 `::qs_html_report_options`。** 不写的话它是匿名的
  `STRUCT(title VARCHAR)`，匹配不上任何签名，DuckDB 会直接说找不到函数。

::::

Windows 上还有一条：如果 `cargo duckdb-ext build` 报产物被占用，说明有 DuckDB 进程正持有
`target/debug/duckfn_quantstats.duckdb_extension`。换一个输出路径构建即可 ——
`cargo duckdb-ext build -o build/debug/duckfn_quantstats.duckdb_extension` —— 或者关掉那个进程。
`.duckdb_extension` 不是改了名的 DLL：DuckDB 的元数据在文件末尾，把一个 DLL 拷过去会报
`The metadata at the end of the file is invalid`。
