---
title: Error paths
sidebar_position: 5
description: What each failure looks like — a NULL result, a skipped instrument, or a query-failing error with the exact message.
---

# Error paths

| Situation | Behaviour |
| --- | --- |
| Not a single row, or no instrument able to produce a report | `NULL` |
| An instrument has no valid point left after conversion | that instrument is left out of the result |
| A `benchmark` symbol has no row in the table | Error `no row for the benchmark symbol '…'` |
| The `benchmark` list disagrees between instruments (entries or order) | Error `every symbol must use the same benchmark list` |
| The `benchmark` list holds an empty string / a NULL element / a duplicate | Error `must not contain an empty string` / `… a NULL element` / `lists '…' twice` |
| `benchmark_title` entry missing / empty / NULL / extra | **Not an error**: a missing entry falls back to that benchmark's symbol, extra entries are ignored |
| Price branch: the benchmark yields no return (fewer than two valid points) | Error `produced no returns` |
| `periods_per_year = 0` | Error `periods_per_year must be greater than 0` |
| `output_dir = ''` | Error `output_dir must not be an empty string` |
| The `output_dir` contains a NUL byte | Error `contains a NUL byte` |
| The `output_dir` cannot be written (missing directory, unwritable remote, …) | Error from `duckfn::duck_vfs::write` naming the path |
| `open_in_browser` with an `output_dir` that is not a local path (`s3://…`, `memory://…`) | Error `only local file paths can be opened in a browser` |

Every error message starts with the registered function name (`qs_html_reports: …`), so it is obvious
which function reported it.
