---
title: Function descriptions
sidebar_position: 4
description: How the description / comment / example text on the attributes becomes the function table of the community-extension page.
---

# Function descriptions

DuckDB's C extension API has **no** way to set a function's description or examples:
`duckdb_scalar_function_set_name`, `_set_return_type`, `_set_varargs`, `_set_volatile` … and that is
it. There is no `_set_description` and no `_add_example`. So the `Added Functions` table on a
community extension's page
([the list of extensions](https://duckdb.org/community_extensions/list_of_extensions)) would be a bare
list of names without help from somewhere else.

That text sits next to the function it describes, on the `#[duck_*]` attribute (here: the two places
in `functions/aggregate_html/html_returns.rs` and `html_prices.rs`):

```rust
#[duck_aggregate_function(
    description = "Renders one quantstats HTML report per symbol from a long table of periodic returns",
    comment = "Groups by symbol internally, so the SQL needs no GROUP BY …",
    examples = ["SELECT unnest(qs_html_reports(…)) FROM daily_returns", "…"]
)]
```

All three keys are optional (`example` for one, `examples` for several; the two are mutually
exclusive) and take **no part in registration** — the macro only records them, along with the
registered name, in an inventory entry. To export:

```shell
just docs_csv                                           # -> target/function_descriptions.csv
cargo run --bin duckfn -- function_descriptions --all    # -> target/function_descriptions_all.csv
                                                         #    (includes undocumented functions, as a checklist)
```

No extension is loaded, the catalog is never queried and DuckDB need not be around: this reads what
the macros recorded at compile time, and the path is always the project's `target/`. The
`#[path = "../extension/mod.rs"] mod extension;` in `src/bin/duckfn.rs` is essential — `inventory`'s
static constructors only fire for object files that are really linked into the final binary, so
switching to `use duckfn_quantstats::…` would make the CSV come out empty, silently.

Three rules apply to the text itself: several examples are joined with `"; "` and lose their trailing
semicolons on export; line breaks collapse to a single space (the generated page is a Markdown table,
where a newline inside a cell ends the row); commas, quotes and non-ASCII text pass through unchanged.
So write one full statement per entry. The text is English because it is pasted onto that page as it
is.

When the extension goes to the community repository, drop this CSV at
`extensions/duckfn_quantstats/docs/function_descriptions.csv` in `community-extensions` (its
`generate_md.sh` LEFT JOINs it on `function_name` to override the function tables). No copy needs to
live in this repository — regenerate it after changing the code. See
[Community extensions](../community-extension.md).
