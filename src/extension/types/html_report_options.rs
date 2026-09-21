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
    /// 注意：WebAssembly 目标下该字段被**忽略**（那边没有可写的文件系统），报告照常返回、不报错。
    ///
    /// Also write the HTML to this path. Defaults to not writing anything.
    ///
    /// Note: on WebAssembly targets the field is **ignored** (there is no writable file system
    /// there); the report is still returned instead of failing.
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
        if let Some(output) = &self.output {
            if output.is_empty() {
                return Err(duck_error(
                    "duckfn_quantstats_html_options.output must not be an empty string",
                ));
            }
            // 原生目标：把路径交给 quantstats-rs，它会在拼完模板后 std::fs::write。
            //
            // Native: hand the path to quantstats-rs, which does a std::fs::write after rendering.
            #[cfg(not(target_arch = "wasm32"))]
            {
                options = options.with_output(output);
            }
            // wasm 目标：那边没有可写的文件系统，运行时写盘只会得到一个 IO 错误、把整条查询带崩，
            // 所以这里直接忽略路径，HTML 照常返回。
            //
            // WebAssembly: there is no writable file system there; writing at runtime would only
            // produce an IO error that kills the whole query, so the path is dropped and the HTML is
            // still returned.
            #[cfg(target_arch = "wasm32")]
            {
                let _ = output;
            }
        }

        Ok(options)
    }
}
