---
title: Testing
sidebar_position: 2
description: The four SQLLogicTest files, what each one pins down, how to run them, and the faster loop that skips make.
---

# Testing

Tests are written in the [SQLLogicTest](https://duckdb.org/dev/sqllogictest/intro) format under
`test/sql/`:

```shell
make debug && make test    # make test does not rebuild; run make debug after editing Rust
```

They come in two kinds, and the split is deliberate:

| File | Covers | Extra dependency |
| --- | --- | --- |
| `test/sql/quantstats/html_reports.test` | **Return-path behaviour**: the registration surface (two names, one signature each), the result shape and its ordering (symbols ascending, benchmarks in list order inside one symbol), `benchmark` NULL with none configured, display names falling back to the symbol, `NULL` rows skipped and instruments dropped with them, empty input → `NULL`, multi-threaded `combine` consistency (single vs 4 threads, md5-equal), `output_dir` auto-naming and path filling, two calls never overwriting each other | none |
| `test/sql/quantstats/html_reports_by_prices.test` | **Price-path behaviour**: byte-identical to `lag()`-derived returns, each of several benchmarks byte-identical to its own differenced reference, the benchmark side differenced too, points with a zero predecessor skipped, single-point instruments dropped, benchmark symbol missing / too few points | none |
| `test/sql/quantstats/html_reports_errors.test` | **Error paths**: the four ways a benchmark list can be wrong (symbol missing / empty string / NULL element / duplicate), the list disagreeing across symbols (order included), extra `benchmark_title` entries being ignored (no error), `periods_per_year = 0`, `output_dir = ''`, a NUL path, `open_in_browser` with a non-local path, an uncast options literal, the old API being gone | none |
| `test/sql/quantstats/html_reports_values.test` | **Output content**: parses the generated HTML with [webbed](https://duckdb.org/community_extensions/extensions/webbed)'s XPath and asserts the title, the date range, the `rf` echo, per-row metric numbers, the chart/table counts, the extra benchmark column, **each report carrying its own benchmark column when there are several**, **benchmark display names paired by index (missing / NULL / empty falling back to the symbol)**, and each symbol's own title and file name | the `webbed` community extension |

`webbed` is installed from inside the test file (`INSTALL webbed FROM community;`), which needs
**network on the first run** and then goes through the local DuckDB extension cache. Delete that file
if you do not want the dependency; the other files are unaffected.

## The fast loop

DuckDB's test runner can drive the artifact directly, which skips `make` entirely. The recipe needs a
Python environment with `duckdb_sqllogictest` installed — `make configure` creates one under
`configure/venv`, or you can use any Python 3 environment that has it:

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

`--test-dir` is required: it is also the value of `__TEST_DIR__`, the directory a test writing files
is given. To run a single file, add `--file-path test/sql/quantstats/html_reports.test`.

## Asserting on the HTML

Asserting on long HTML via XPath beats `length(...) > N` and localises failures far better than an
md5 comparison:

```sql
-- Report title and date range
SELECT html_extract_text(html, '//h1')[1] FROM report;
-- -> My Fund 2 Jan, 2024 - 12 Jan, 2024

-- The strategy column of one row of the metrics table (benchmark column comes first)
SELECT html_extract_text(html, '//div[@id="right"]/table[1]//tr[td[1]="Sharpe"]/td[2]')[1] FROM report;
-- -> 4.93
```

Note that webbed's `html_extract_text(html, xpath)` returns `VARCHAR[]` (**all** matches): index with
`[1]` for a single value, or use `array_to_string(..., ' | ')` / `array_length(...)` to assert a whole
group.

## Conventions

- **Every file starts from a clean database**, and a file that uses the extension starts with
  `require duckfn_quantstats`.
- **Expected errors are matched as substrings.** Under `statement error`, a distinctive fragment of
  the message is enough — there is no need to reproduce DuckDB's whole error string, and doing so ties
  the test to a message that may well change.
- **Watch how values print.** A `DOUBLE` renders as `7.0`; when a test is about a number rather than a
  type, cast it so the expectation stays stable if the type changes.
- **Divide the files by concern**, not by function count: behaviour in one file, error paths in
  another, and a `.test` that reaches for a community extension kept separate, because it needs the
  network the first time.

When adding a function, cover at least: normal values, `NULL`, boundary values and error paths
(`statement error`). Before committing: `just lint` (`cargo clippy --all-targets -- -D warnings`).
