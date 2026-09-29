---
title: Design notes
sidebar_position: 1
description: Why the function groups by symbol itself, how the benchmark pairing works, why the options type is all-Option, and how the reports are persisted and opened.
---

# Design notes

This page is the "why" behind the code: the shapes that were chosen, and the alternatives that were
rejected. The user-facing behaviour is in [Functions](../guide/functions.md) and
[Options](../guide/options.md); this is what users do not need.

The project started from DuckDB's official
[extension-template-rs](https://github.com/duckdb/extension-template-rs) and has been reshaped to
follow duckfn's skeleton conventions (entry module, `EXTENSION_NAME`, dependency list).

## Two SQL names, one signature each

The `symbol` column is the grouping key, so there is **no `GROUP BY` in SQL**: the function groups
inside the aggregate state itself (see "the symbol table" below) and a single call produces the whole
set of reports. Each name therefore has exactly one signature, with the argument order fixed to "data
columns first (`symbol`, `date`, value), options last".

The price branch **needs its own name**: `(symbol, date, price, options)` and
`(symbol, date, period_return, options)` have exactly the same type sequence
(`VARCHAR, DATE, DOUBLE, STRUCT`), so one name could not dispatch them.

The registered name is never written by hand: the attribute macro generates a `SQL_NAME` constant per
signature (the function name itself, now that there is a single signature), and error prefixes read it
— `kind.rs` points at it instead of repeating the literal. The price of that is that such functions
have to be `pub(super)`, because the generated module inherits the function's visibility.

## The symbol table: the function does its own grouping

The aggregate state is not "one point array" but a `HashMap<String, SymbolSlot>`, one slot per symbol:
that symbol's points plus that symbol's copy of the options. Three things come out of that:

- **the caller writes no `GROUP BY`** — one `SELECT` yields the whole set of reports;
- **the benchmark is in the same state** — it is another symbol of the table, so it is right there to
  pair up (see below);
- **the options' granularity drops to the symbol** — reports are per instrument (each with its own
  title and display name), so the options are too.

The price is an aggregate state holding the whole table's points (the same order as one point array
per group), while the number of reports rendered in `result()` is still the number of instruments.

## The argument slots: options are lazy per symbol

`options` is read through `DuckLazy`: every row only builds an O(1) token, and the single real parse
happens the **first time a symbol appears** (once the table holds it, later rows merely append a
point). This is not a nicety — duckfn's adapter reads arguments per row, so parsing the struct on
every row would be O(rows) parses, whereas parses = number of symbols is what this API should cost.

The parse result is cached in the slot through duckfn's `DuckLazySlot`: a `DuckLazy` token is only
valid inside the callback that produced it, so the state can hold the parse result and nothing else.
"Is this the symbol's first row?" needs no extra flag — the key of the table *is* that marker, because
only the path that inserts a fresh slot reads the options column.

## One report per symbol

The report is rendered in `result()`, one per instrument per call. 100 instruments render 100 full
reports (each with a dozen inline SVGs), so time and memory grow linearly with the number of
instruments — the same order as the old "one report per `GROUP BY` group", only with the grouping
moved from SQL into the function. The result order is settled in `result()` by sorting on the symbol;
it does not follow HashMap iteration or DuckDB's merge order.

No `ORDER BY` is needed: the aggregate only concatenates and lets `ReturnSeries::new` sort by date
(the price branch sorts by date first, to difference).

## Why the benchmark is a symbol in the table (a list of them, in fact)

The benchmark is already in the same long table (it is a symbol like any other), so naming it in the
`benchmark` option and letting the function look it up is the shortest path: one `SELECT`, no
`GROUP BY`, no `cross join`, and with/without a benchmark differs by a single key.

The earlier design passed the benchmark in as a **list argument** (`list(...)` plus `cross join` plus
`GROUP BY`, a three-step dance). It did evaluate the benchmark exactly once, but the price was a "with
a benchmark" case that — the norm rather than the exception — needed an extra argument, an extra
overload and two extra SQL steps.

`benchmark` is a **list** (`['SPX', 'NDX']`) because the real scenario is "one instrument against
several benchmark series": quantstats-rs' `HtmlReportOptions` holds exactly one
`Option<&ReturnSeries>` (the metrics column, the benchmark line in the plots and rolling beta are all
built around it), so several benchmarks can only become **several reports** — 1 instrument × M
benchmarks = M rows, told apart by the `benchmark` field, with the list order being the report order.

Note that "several instruments against one benchmark" is a different thing: that is one report per
instrument and needs no list. A "N strategies × M benchmarks" cartesian product is left out of the API
on purpose — whoever wants it groups/filters and calls again.

The symbols named as benchmarks are **input only and get no report**; each benchmark's series is
converted once (differenced first, on the price branch) and shared by every instrument. The
`benchmark` option additionally has to be the same list across the whole call (entries and order) —
otherwise "which one is the benchmark" would have no single answer, so a disagreement is an error.
Problems inside the list itself (an empty string, a NULL element, a duplicate) are caught in the
options type, see below.

The benchmark's display name (`benchmark_title`) is a list too, **paired by index** with `benchmark`.
It is presentation only, hence lenient: a missing entry (shorter list, NULL, empty string) falls back
to that report's benchmark symbol and extra entries are ignored — neither is an error.

## The result row type registers no named type

`QuantstatsHtmlReport` in `html_report.rs` deliberately leaves `create_type` off: the aggregate's
return type already carries the full anonymous `STRUCT(symbol VARCHAR, benchmark VARCHAR,
strategy_title VARCHAR, benchmark_title VARCHAR, html VARCHAR, file_path VARCHAR)[]`, so SQL can read
it by field name (`unnest` / `list_transform` / `[1].html`) — a type name on top would only add another
surface to maintain.

The field names are the Rust field names verbatim (duckfn's `DuckStruct` derive has no field-level
renaming) and none of the six is an SQL keyword, so DuckDB renders `typeof` without quotes. There are
only three `Option`s: `benchmark` is NULL for a single-series report (none configured),
`benchmark_title` goes with it (it is that report's benchmark's display name), and `file_path` is NULL
when nothing was written — the other three are always there.

Both display names are echoed into the row (`strategy_title` / `benchmark_title`): they are the very
ones the legend and the file name used (each falling back to its own symbol), so printing or comparing
them needs no HTML parsing.

The same type doubles as `DuckAggregateState::Output = Vec<QuantstatsHtmlReport>` — duckfn's list write
path (`create_writer_batch` / `write_valid` / `write_finish` in `duck_list.rs`) attaches a child writer
and the elements go into the child vector through the write path `#[derive(DuckStruct)]` generates, so
"an aggregate returning an array of structs" needs no extra machinery at all.

## The options type

`#[duck(create_type = true)]` makes duckfn run
`CREATE TYPE IF NOT EXISTS "qs_html_report_options" AS STRUCT(...)` at load time, which is what makes
`{'title': 'x'}::qs_html_report_options` (and the JSON form) possible in SQL.

Every field is an `Option<T>` on purpose: DuckDB fills the missing keys of a struct literal with NULL,
and duckfn turns the **whole struct** into NULL when a non-Option field reads NULL — so a user's
`{'rf': 0.1}` would silently fall back to all defaults and their `rf` would be dropped.

`to_report_options(strategy_title, benchmark_title)` converts by starting from quantstats-rs'
`HtmlReportOptions::default()` and overriding only the fields the user actually wrote, so the defaults
have a single source of truth. The two arguments are **the already-resolved display names**
(`report.rs` gets them from `strategy_title_or` / `benchmark_title_or`): the same names also go into the
file name, so resolving once and using them twice is what keeps the report legend and the file on disk
in agreement. The fallback rules themselves are simple — `strategy_title` defaults to the symbol,
`benchmark_title` is taken by index and falls back to that benchmark's symbol — but they cannot be
dropped: with dozens of reports out of one call, the default `'Strategy'` is identical for every one of
them, so the legend, the temporary file name and the returned display name would all lose their
distinguishing power.

`output_dir` is deliberately not forwarded: quantstats-rs writes with `std::fs`, while this extension
wants DuckDB's VFS (see below) — the directory is handled by `report.rs` after the report has been
rendered.

`benchmark` and `benchmark_title` are both list fields (`Option<Vec<Option<String>>>`), but **only
`benchmark` is validated**, in one place, `QuantstatsHtmlOptions::benchmark_names()`. `periods_per_year
= 0` and `output_dir = ''` are configuration errors reported right here as well — all of them before
anything is rendered or any file-system call happens.

## Persisting the reports

`output_dir` takes a **directory** and `naming.rs` builds the file name:
`<time>-<strategy>-<benchmark>-<random>.html` (the benchmark part is absent when none is configured).
Naming lives in the function for three reasons:

- a name has to carry "which instrument, against which benchmark, at what time", which only the
  function knows — and with one instrument against several benchmarks, a path built from the instrument
  alone would necessarily overwrite itself, which is exactly where the old "the caller writes the full
  path" design broke down;
- the last two parts are the display names the report itself uses (`strategy_title` and the benchmark's),
  so a directory full of reports still says which is which;
- the random suffix (`fastrand`) plus an existence check through `duck_vfs::exists` before writing
  (retrying with another suffix on a collision — see `report_path`) makes "nothing existing is
  overwritten" a guarantee rather than a probability: two calls each write their own files.

Legality and randomness are both delegated: `sanitize-filename` owns the illegal/control characters,
the Windows reserved device names and the trailing dots and spaces, `fastrand` the random suffix (the
very random source tempfile uses internally). This file only adds two policies of its own: spaces
become `_`, and a part is capped at 32 characters.

The write itself goes through duckfn's convenience layer `duck_vfs::write_string`, i.e. through
**DuckDB's VFS** rather than `std::fs`:

- local disk, in-memory file systems, whatever file system the wasm build exposes, and `s3://` /
  `http(s)://` once `httpfs` is loaded all go through the same path with the same semantics;
- it is also the only way an aggregate can write at all. DuckDB's C API gives aggregate functions no
  client context (no bind callback, no `duckdb_aggregate_function_get_client_context`), so duckfn keeps
  an owned long-lived connection from registration time and hands out a fresh `ClientContext` →
  `FileSystem` from it;
- the directory is joined with `/` (`report.rs::join`), deliberately not with `Path::join`: the latter
  joins with the platform separator, so on Windows `s3://bucket/reports` would come out as
  `s3://bucket/reports\name.html`.

`write_string` **replaces** its target: afterwards the file holds exactly the report, even when it
previously held something longer (the C API's missing truncate is handled inside duckfn's `duck_vfs`
layer, which zeroes a longer file before writing). With never-colliding names this extension does not
actually hit that path, but `read_text()` returning exactly what the function returned still holds and
the test suite pins it with `md5`.

One call may write several files (instruments × benchmarks). The tail in `report.rs` runs in two
passes: it first settles every (symbol, benchmark) pair's `ReportTarget` (validating `output_dir` and
`open_in_browser` on the way), and only then renders, writes and opens each one, filling the path that
was actually written into the result row.

## Opening the report in a browser

`open_in_browser` hands the report to the system default browser once it has been generated. A browser
needs a local file that actually exists, which decides the rest:

- with `output_dir` set, those files are written there and then opened;
- without it, every report is written to a temporary file first —
  `<temp dir>/<time>-<strategy>-<benchmark>-<random>.html`, created by
  [tempfile](https://crates.io/crates/tempfile). The stem (time + the two display names) is shared with
  `naming.rs`; tempfile picks the random suffix, appends the `.html` (what makes the browser render the
  file instead of downloading it) and guarantees the name was free at that moment, so nothing existing
  is overwritten and two reports from the same second cannot collide. The time comes first so that
  sorting the temp directory by name sorts it by time;
- an `output_dir` that is not a local path (`s3://…`, `memory://…`) is an error rather than a silent
  no-op, since no browser can open it. That is checked **before** anything is rendered.

Launching is [open](https://crates.io/crates/open)'s job, and it is the non-blocking `that_detached`
variant: the reports are already on disk, so the query neither waits for the browser nor looks at what
the browser does with the files. On Windows that is a single `ShellExecute` call (the
`shellexecute-on-windows` feature, rather than the crate's PowerShell-based default); on macOS and
elsewhere it is `open` / `xdg-open` plus the crate's fallback list. The only failure reported is the
launcher itself not starting.

## WebAssembly

`output_dir` goes through DuckDB's VFS, so the wasm build uses exactly the same code path as the native
one and the files land wherever DuckDB's own file system points in that environment. That replaces the
earlier behaviour, where the path was dropped on `wasm32-unknown-emscripten` because `std::fs` has no
writable file system there.

The naming logic (`naming.rs`) is therefore **shared**: a wasm build names its report files too, which
is why `sanitize-filename` and `fastrand` are ordinary dependencies rather than non-wasm ones.

`open_in_browser` is the one deliberate exception, and it is also the only platform-specific code left
in the extension (`browser.rs`): a wasm build has no browser process to launch, so the option is
ignored there — no browser, and no temporary file either. The report string comes back to the host as
it is, and showing it is the host page's job. The two crates behind that option (`open`, `tempfile`)
are declared under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, so a wasm build does not
compile them at all — a hard requirement, in fact, since `open` has no emscripten implementation and
would not build.

`just build_wasm` (`cargo build --release --target wasm32-unknown-emscripten --example
duckfn_quantstats`) compiles fine; the runtime behaviour is DuckDB's VFS's, not ours.
