//! duckfn 命令行工具的入口：`cargo run --bin duckfn -- function_descriptions`
//!
//! 只干两件事：把插件本体挂进来，把项目根目录传给 duckfn 的 CLI（工具本身的实现都在
//! `duckfn::cli`，需要 `Cargo.toml` 里 duckfn 的 `cli` feature）。输出固定为
//! `target/function_descriptions.csv` —— 社区扩展文档页 `Added Functions` 那张表的数据来源，
//! 因为 DuckDB 的 C 扩展 API 没有设置函数描述与示例的接口。描述写在 `#[duck_*]` 属性上
//! （见 functions/aggregate_html/html_returns.rs、html_prices.rs），这里不重复一份。
//!
//! 为什么用 `#[path]` 把 `extension` 再编一遍，而不是 `use duckfn_quantstats::...`：
//! `#[duck_*]` 的文档元数据靠 `inventory` 的静态构造器收集，只有**真正被链接进最终二进制**的目标
//! 文件才会生效。直接依赖 rlib 时，链接器可能因为没人引用那些模块而把它们整块丢掉，导出的 CSV
//! 就会是空的（而且是静默的）。让 bin 自己把同一份源码编一遍，注册项就落在本 crate 里，一定齐全。
//! 这也是 duckfn 骨架里 `src/bin/duckfn.rs` 的标准写法。
//!
//! The entry point of the duckfn command-line tool: `cargo run --bin duckfn -- function_descriptions`.
//! It only pulls the extension in and passes the project root to duckfn's CLI (the tool itself lives
//! in `duckfn::cli`, which needs duckfn's `cli` feature). The output is always
//! `target/function_descriptions.csv` — the data behind the `Added Functions` table of the
//! community-extension doc page, because DuckDB's C extension API has no way to set a function's
//! description or examples. That text lives on the `#[duck_*]` attributes (see
//! functions/aggregate_html/html_returns.rs and html_prices.rs); it is not repeated here.
//!
//! `#[path]` compiles `extension` a second time instead of using `use duckfn_quantstats::...`
//! because the documentation metadata behind `#[duck_*]` is collected by `inventory`'s static
//! constructors, which only fire for object files that are really linked into the final binary: when
//! merely depending on an rlib, the linker may drop those modules entirely and the exported CSV
//! would come out empty — silently. Letting the bin compile the same sources keeps the registrations
//! in this crate. This is also how the duckfn skeleton's `src/bin/duckfn.rs` is written.

#[path = "../extension/mod.rs"]
mod extension;

fn main() -> std::process::ExitCode {
    duckfn::cli::run(env!("CARGO_MANIFEST_DIR"))
}
