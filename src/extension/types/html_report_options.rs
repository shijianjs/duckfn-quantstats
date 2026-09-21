use duckfn::{DuckResult, DuckStruct, duck_error};
use quantstats_rs::HtmlReportOptions as ReportOptions;

// ============================================================================
// 报告配置：一个具名 STRUCT 类型，可在 SQL 里直接 cast
//
// `#[duck(create_type = true)]` 让 duckfn 在扩展加载期执行
//   CREATE TYPE IF NOT EXISTS "duckfn_quantstats_html_options" AS STRUCT(...);
// 之后 SQL 里可以直接写 `{'title': 'x'}::duckfn_quantstats_html_options`，
// 也可以把 JSON 字符串转成它（`'{"title": "x"}'::JSON::duckfn_quantstats_html_options`）。
//
// 字段**全部**是 `Option<T>`，这是硬要求：DuckDB 的 struct 字面量缺字段时会补 NULL，
// 而 duckfn 读到「非 Option 字段为 NULL」时会让**整个 struct** 变成 NULL。那样用户写的
// `{'rf': 0.1}` 会整体退化成默认值，他设的 rf 被静默丢掉。逐个字段 unwrap_or 没有这个问题。
//
// Report options: a named STRUCT type that SQL can cast to directly.
//
// `#[duck(create_type = true)]` makes duckfn run
//   `CREATE TYPE IF NOT EXISTS "duckfn_quantstats_html_options" AS STRUCT(...)`
// at load time, so SQL can write `{'title': 'x'}::duckfn_quantstats_html_options`, or cast a JSON
// string to it.
//
// Every field is an `Option<T>` on purpose: DuckDB fills missing keys of a struct literal with NULL,
// and duckfn turns the **whole struct** into NULL when a non-Option field reads NULL — so a user's
// `{'rf': 0.1}` would silently fall back to all defaults and their `rf` would be dropped.
// ============================================================================

/// 报告配置，字段名即 SQL 里的键名。
///
/// The report options; the Rust field names are the keys used in SQL.
#[derive(Clone, Debug, Default, DuckStruct)]
#[duck(
    sql_name = "duckfn_quantstats_html_options",
    create_type = true
)]
pub(crate) struct QuantstatsHtmlOptions {
    /// 报告标题。缺省沿用 [`ReportOptions::default`]。
    ///
    /// Report title; defaults to [`ReportOptions::default`].
    pub title: Option<String>,

    /// 策略的显示名。缺省沿用 [`ReportOptions::default`]。
    ///
    /// Display name of the strategy; defaults to [`ReportOptions::default`].
    pub strategy_title: Option<String>,

    /// 基准的显示名，纯展示用。缺省沿用 [`ReportOptions::default`]（即不显示基准名）。
    ///
    /// Display name of the benchmark, presentation only; defaults to [`ReportOptions::default`].
    pub benchmark_title: Option<String>,

    /// 无风险利率，**年化**（`0.04` 表示 4%），与 Python quantstats 的 `rf` 口径一致。缺省 0.0。
    ///
    /// 报告内部会把它换算成周期利率，而 crate 里有两套换算：Sharpe（含滚动 Sharpe / Sortino）
    /// 走 `(1 + rf)^(1/periods_per_year) - 1`，指标表里的 PSR / Sortino 走 `rf / periods_per_year`。
    /// 两者略有差异，`rf = 0` 时都退化为 0。
    ///
    /// Annualized risk-free rate (`0.04` means 4%), matching the convention of Python quantstats'
    /// `rf`; defaults to 0.0.
    ///
    /// The report converts it to a per-period rate internally, and the crate has two conversions:
    /// Sharpe (and rolling Sharpe / Sortino) uses `(1 + rf)^(1/periods_per_year) - 1`, while PSR /
    /// Sortino in the metrics table use `rf / periods_per_year`. They differ slightly and both
    /// collapse to 0 when `rf = 0`.
    pub rf: Option<f64>,

    /// 年化周期数：日频 252、周频 52、月频 12。必须大于 0。缺省 252。
    ///
    /// Periods per year: 252 daily, 52 weekly, 12 monthly. Must be greater than 0. Defaults to 252.
    pub periods_per_year: Option<u32>,

    /// 是否把策略与基准的起始日对齐（两侧各自跳过起始处的零收益）。缺省 true。
    ///
    /// Whether to align the start dates of strategy and benchmark (each side skips leading zero
    /// returns); defaults to true.
    pub match_dates: Option<bool>,

    /// 生成报告的同时把 HTML 落盘到该路径。缺省不落盘。
    ///
    /// 写文件走 **DuckDB 的 VFS**（`duckfn::with_file_system`），不是 `std::fs`：本地磁盘、内存
    /// 文件系统、wasm 构建里宿主真正能读的那个文件系统，以及装了 httpfs 后的 `s3://` / `http(s)://`
    /// 都是同一条通路、同一套语义。也因此聚合函数（C API 不给它客户端上下文）才写得进去。
    ///
    /// Also write the HTML to this path. Defaults to not writing anything.
    ///
    /// The write goes through **DuckDB's VFS** (`duckfn::with_file_system`) rather than `std::fs`:
    /// local disk, in-memory file systems, whatever file system the wasm build actually exposes, and
    /// `s3://` / `http(s)://` once httpfs is loaded are all the same path with the same semantics —
    /// which is also what lets an aggregate (no client context from the C API) write at all.
    pub output: Option<String>,
}

impl QuantstatsHtmlOptions {
    /// 转成 quantstats-rs 的 [`ReportOptions`]，做法是「从它的 `default()` 出发、逐字段覆盖
    /// 用户显式写了的那些」—— 默认值因此只有一份真相，不会在这里再抄一套。
    ///
    /// 生命周期是泛型的：调用方要往返回值上挂基准（`with_benchmark`），由它自己决定借用多久。
    ///
    /// Convert to quantstats-rs' [`ReportOptions`] by starting from its `default()` and overriding
    /// only the fields the user actually wrote, so defaults have a single source of truth.
    ///
    /// The lifetime is generic: callers that want to attach a benchmark (`with_benchmark`) pick how
    /// long the returned value borrows for.
    pub(crate) fn to_report_options<'a>(&self) -> DuckResult<ReportOptions<'a>> {
        let mut options = ReportOptions::default();

        if let Some(title) = &self.title {
            options.title = title.clone();
        }
        if let Some(strategy_title) = &self.strategy_title {
            options = options.with_strategy_title(strategy_title.clone());
        }
        if let Some(benchmark_title) = &self.benchmark_title {
            options = options.with_benchmark_title(benchmark_title.clone());
        }
        if let Some(rf) = self.rf {
            options.rf = rf;
        }
        if let Some(periods_per_year) = self.periods_per_year {
            if periods_per_year == 0 {
                return Err(duck_error(
                    "duckfn_quantstats_html_options.periods_per_year must be greater than 0",
                ));
            }
            options.periods_per_year = periods_per_year;
        }
        if let Some(match_dates) = self.match_dates {
            options.match_dates = match_dates;
        }

        // `output` 刻意不在这里处理：quantstats-rs 落盘用的是 `std::fs`，而本扩展要的是 DuckDB 的
        // VFS（本地磁盘、内存文件系统、wasm 上的文件系统走同一条通路）。路径交给调用方
        // （report.rs）在拿到渲染结果后自己写，见 [`Self::output_path`]。
        //
        // `output` is deliberately not handled here: quantstats-rs writes with `std::fs`, while this
        // extension wants DuckDB's VFS (local disk, in-memory file systems and the wasm build's file
        // system all go through one path). The caller (report.rs) writes the rendered report itself —
        // see [`Self::output_path`].
        Ok(options)
    }

    /// 报告落盘路径；没配就是 `None`。
    ///
    /// 空字符串是配置错误，在这里报掉（否则会拿一个空路径去开文件）。
    ///
    /// The path the report should be written to; `None` when unset.
    ///
    /// An empty string is a configuration error and is reported here (otherwise an empty path would be
    /// handed to the file system).
    pub(crate) fn output_path(&self) -> DuckResult<Option<&str>> {
        let Some(output) = self.output.as_deref() else {
            return Ok(None);
        };
        if output.is_empty() {
            return Err(duck_error(
                "duckfn_quantstats_html_options.output must not be an empty string",
            ));
        }
        Ok(Some(output))
    }
}
