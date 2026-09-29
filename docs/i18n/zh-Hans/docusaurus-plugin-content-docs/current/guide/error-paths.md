---
title: 错误路径
sidebar_position: 5
description: 每种失败长什么样 —— 返回 NULL、略过某个标的，还是带确切报文让查询失败。
---

# 错误路径

| 情况 | 行为 |
| --- | --- |
| 一行都没有，或没有任何标的能出报告 | 返回 `NULL` |
| 某个标的差分/构造后没有有效点 | 该标的从结果里略过 |
| `benchmark` 指的 symbol 在表里没有行 | 报错 `no row for the benchmark symbol '…'` |
| `benchmark` 列表在各标的之间不一致（元素或顺序不同） | 报错 `every symbol must use the same benchmark list` |
| `benchmark` 列表里有空串 / NULL 元素 / 重复项 | 报错 `must not contain an empty string` / `… a NULL element` / `lists '…' twice` |
| `benchmark_title` 缺项 / 空串 / NULL / 多出来 | **不算错误**：缺项退回对应的基准 symbol，多余项忽略 |
| 价格路径：基准差分不出收益率（有效点不足两个） | 报错 `produced no returns` |
| `periods_per_year = 0` | 报错 `periods_per_year must be greater than 0` |
| `output_dir = ''` | 报错 `output_dir must not be an empty string` |
| `output_dir` 里有 NUL 字节 | 报错 `contains a NUL byte` |
| `output_dir` 写不进去（目录不存在、远端不可写等） | 报错里带 `duckfn::duck_vfs::write` 与路径 |
| `open_in_browser` 配的 `output_dir` 不是本地路径（`s3://…`、`memory://…`） | 报错 `only local file paths can be opened in a browser` |

报错信息一律以注册的函数名开头（`qs_html_reports: …`），所以一眼能看出是哪个函数的问题。
