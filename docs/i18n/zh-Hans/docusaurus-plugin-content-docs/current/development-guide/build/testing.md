---
title: 测试
sidebar_position: 2
description: 四份 SQLLogicTest 用例各钉住了什么、怎么跑，以及跳过 make 的快速迭代方式。
---

# 测试

测试用 SQLLogicTest 格式写在 `test/sql/` 下：

```shell
make debug && make test    # make test 不会自动重新构建，改完 Rust 必须先 make debug
```

测试分两类，分工不要混：

| 文件 | 覆盖什么 | 额外依赖 |
| --- | --- | --- |
| `test/sql/quantstats/html_reports.test` | **收益率路径的行为**：注册面（两个名字各一个签名）、结果形状与排序（symbol 升序 + 标的内按基准列表顺序）、无基准时 `benchmark` 为 NULL、显示名退回 symbol、`NULL` 行跳过与被跳空的标的、空输入返回 `NULL`、多线程 `combine` 一致性（单线程 vs 4 线程 md5 相等）、`output_dir` 自动命名与路径回填、两次调用互不覆盖 | 无 |
| `test/sql/quantstats/html_reports_by_prices.test` | **价格路径的行为**：与 `lag()` 差分的结果逐字节一致、多个基准时每一份都与对应基准的差分结果一致、基准侧同样先差分、前值为 0 时跳过、单点标的略过、基准 symbol 不存在 / 点数不足 | 无 |
| `test/sql/quantstats/html_reports_errors.test` | **错误路径**：基准列表的四种写法错误（symbol 不存在 / 空串 / NULL 元素 / 重复）、基准列表跨 symbol 不一致（含顺序）、`benchmark_title` 多余项被忽略（不报错）、`periods_per_year = 0`、`output_dir = ''`、NUL 路径、`open_in_browser` 配非本地路径、没 cast 的配置字面量、旧 API 已不存在 | 无 |
| `test/sql/quantstats/html_reports_values.test` | **输出内容**：用 [webbed](https://duckdb.org/community_extensions/extensions/webbed) 的 XPath 解析生成的 HTML，断言标题、统计区间、`rf` 回显、逐行指标数字、图表/表格数量、带基准时多出的那一列、**多基准时每份报告各自带自己的基准列**、**基准显示名按下标对齐（缺项 / NULL / 空串退回 symbol）**、每个 symbol 各自的标题与文件名 | 社区扩展 `webbed` |

`webbed` 的安装写在测试文件里（`INSTALL webbed FROM community;`），**首次运行需要网络**，之后走本机
DuckDB 扩展缓存。不想要这个依赖就删掉该文件，其余文件不受影响。

## 快速迭代

DuckDB 的测试运行器可以直接驱动产物，整个跳过 `make`。这条命令需要一个装了 `duckdb_sqllogictest` 的
Python 环境 —— `make configure` 会在 `configure/venv` 建一个，也可以用手边任何装了它的 Python 3：

```bash
# Linux / macOS
./configure/venv/bin/python -m duckdb_sqllogictest \
    --test-dir test/sql \
    --external-extension target/debug/duckfn_quantstats.duckdb_extension
```

```powershell
# Windows
.\configure\venv\Scripts\python.exe -m duckdb_sqllogictest `
    --test-dir test/sql `
    --external-extension target/debug/duckfn_quantstats.duckdb_extension
```

`--test-dir` 是必给的：它同时也是 `__TEST_DIR__` 的取值，也就是会写文件的用例拿到的目录。只跑单个文件
加 `--file-path test/sql/quantstats/html_reports.test` 即可。

## 断言 HTML

用 XPath 断言长 HTML 比 `length(...) > N` 有用，也比 md5 相等更容易定位失败原因：

```sql
-- 报告标题与统计区间
SELECT html_extract_text(html, '//h1')[1] FROM report;
-- -> My Fund 2 Jan, 2024 - 12 Jan, 2024

-- 指标表里某一行的策略列（列顺序是「基准在前、策略在后」）
SELECT html_extract_text(html, '//div[@id="right"]/table[1]//tr[td[1]="Sharpe"]/td[2]')[1] FROM report;
-- -> 4.93
```

注意 webbed 的 `html_extract_text(html, xpath)` 返回 `VARCHAR[]`（**所有**匹配项）：取单个值要 `[1]`，
断言一整组可以用 `array_to_string(..., ' | ')` 或 `array_length(...)`。

## 约定

- **每个文件都从干净的数据库开始**；用到扩展的文件以 `require duckfn_quantstats` 开头。
- **期望的错误按子串匹配。** `statement error` 下写报文里一段有辨识度的片段就够了 —— 不必复刻 DuckDB
  的整条错误串，那样会把测试绑死在一句随时可能改的文案上。
- **留意值怎么打印。** `DOUBLE` 会渲染成 `7.0`；测试关心的是数字而不是类型时，加一个 cast，
  这样类型变了期望值也不会跟着变。
- **按关注点分文件**，不要按函数个数分：行为一个文件、错误路径一个文件，而需要社区扩展（比如解析 HTML）
  的那份单独放，因为它首次运行要联网。

新增函数时至少覆盖：正常值、`NULL`、边界值、错误路径（`statement error`）。提交前：
`just lint`（`cargo clippy --all-targets -- -D warnings`）。
