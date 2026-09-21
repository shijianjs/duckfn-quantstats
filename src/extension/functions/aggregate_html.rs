use chrono::{NaiveDate, TimeDelta};
use duckfn::{
    DuckAggregateState, DuckDate, DuckOptionResult, DuckResult, duck_aggregate_function, duck_error,
};
use quantstats_rs::{HtmlReportOptions, ReturnSeries, html};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

// ============================================================================
// 两个聚合函数，把「按日期排列的收益」归约成一份 quantstats HTML 报告
//
//   duckfn_quantstats_html(opt, dt, ret)
//     单序列：一行 = 一天，直接出报告。
//   duckfn_quantstats_html_benchmark(opt, name, dt, ret)
//     长表：`name` 是标签，等于 `opt.benchmark_name` 的行是基准、其余是策略，出带基准对比的报告。
//
// 为什么是聚合而不是标量/表函数：报告天然是「按组归约」，与 SQL 的 GROUP BY 一一对应；
// 标量函数做不到跨行归约，表函数还得自己解析表名、扫表。
//
// 报告只在 result() 里生成一次 —— 即每个分组一次。GROUP BY 100 个标的就会渲染 100 份完整报告
// （每份内含十几张 SVG），这是预期行为，不是性能 bug。
//
// 排序问题：DuckDB 并行/分块执行时 combine 的调用顺序不保证，所以这里只做「拼接」，
// 真正的按日期排序交给 ReturnSeries::new（它内部会 sort）。因此 SQL 侧**不需要 ORDER BY**。
//
// Two aggregate functions folding a date-ordered return series into one quantstats HTML report.
//
//   duckfn_quantstats_html(opt, dt, ret)
//     Single series: one row per day.
//   duckfn_quantstats_html_benchmark(opt, name, dt, ret)
//     Long table: `name` is a label; rows whose label equals `opt.benchmark_name` are the benchmark
//     and the rest are the strategy, producing a report with benchmark comparison.
//
// Why aggregates rather than a scalar or table function: a report is a per-group reduction, which
// maps 1:1 onto GROUP BY; a scalar function cannot reduce across rows, and a table function would
// have to parse table names and scan tables itself.
//
// The report is rendered once in result(), i.e. once per group. GROUP BY over 100 instruments renders
// 100 full reports (each with a dozen inline SVGs); that is expected, not a performance bug.
//
// Ordering: DuckDB may call combine in any order when running in parallel or in chunks, so combine
// only concatenates; the real date ordering is done by ReturnSeries::new (which sorts internally).
// SQL therefore does **not** need an ORDER BY.
// ============================================================================

/// 把 `DuckDate`（自 1970-01-01 起的天数）换成 chrono 的 `NaiveDate`。
///
/// `ReturnSeries` 要的是 `NaiveDate`，而 duckfn 的 `DuckDate` 只存天数，换算只能自己做。
/// 越界（例如 `DATE 'infinity'` 之类的极端值）返回查询错误，不用会 panic 的运算符。
///
/// Convert a `DuckDate` (days since 1970-01-01) into a chrono `NaiveDate`.
///
/// `ReturnSeries` wants `NaiveDate`, while duckfn's `DuckDate` only stores days, so the conversion
/// is on us. Out-of-range values (e.g. `DATE 'infinity'`) become a query error instead of a panic.
fn naive_date(days_since_epoch: i32) -> DuckResult<NaiveDate> {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|epoch| epoch.checked_add_signed(TimeDelta::days(i64::from(days_since_epoch))))
        .ok_or_else(|| {
            duck_error(format!(
                "cannot convert DuckDate {{ days_since_epoch: {days_since_epoch} }} into a calendar date"
            ))
        })
}

/// `(天数, 收益)` 列表 → `ReturnSeries`；`name` 只做标记，不参与计算。
///
/// A list of `(days, return)` pairs → `ReturnSeries`; `name` is a label only and is not used in any
/// computation.
fn build_series(points: &[(i32, f64)], name: Option<String>) -> DuckResult<ReturnSeries> {
    let mut dates = Vec::with_capacity(points.len());
    let mut values = Vec::with_capacity(points.len());
    for &(days_since_epoch, value) in points {
        dates.push(naive_date(days_since_epoch)?);
        values.push(value);
    }

    ReturnSeries::new(dates, values, name)
        .map_err(|err| duck_error(format!("cannot build the returns series: {err}")))
}

/// 渲染报告，并把 quantstats-rs 的错误转成 DuckDB 查询错误（`what` 用来指明是哪个函数）。
///
/// Render the report, turning quantstats-rs errors into DuckDB query errors (`what` names the
/// function the error came from).
fn render(
    what: &str,
    series: &ReturnSeries,
    options: HtmlReportOptions<'_>,
) -> DuckResult<String> {
    html(series, options)
        .map_err(|err| duck_error(format!("{what}: cannot render the report: {err}")))
}

/// 两个聚合共用的配置槽：配置参数多行不变，只在第一行取到后存进状态，之后所有行复用，
/// 免得每行都克隆一份 struct。
///
/// A configuration slot shared by both aggregates: the config argument is constant across rows, so it
/// is kept from the first row that carries one and reused afterwards instead of being cloned per row.
#[derive(Default, Debug, Clone)]
struct ConfigSlot(Option<QuantstatsHtmlOptions>);

impl ConfigSlot {
    /// 记录首个非 NULL 的配置；整列都是 NULL 时保持 `None`（`result()` 里退化成默认配置）。
    ///
    /// Keep the first non-NULL config; when the whole column is NULL it stays `None` and `result()`
    /// falls back to the default config.
    fn absorb(&mut self, config: Option<QuantstatsHtmlOptions>) {
        if self.0.is_none() {
            self.0 = config;
        }
    }

    /// 取配置：整列没有配置时返回 `QuantstatsHtmlOptions::default()`（所有字段为 `None`），
    /// 逐字段覆盖后等价于 quantstats-rs 的全默认。
    ///
    /// Get the config: with no config at all it returns `QuantstatsHtmlOptions::default()` (all
    /// fields `None`), which after per-field overriding equals quantstats-rs' own defaults.
    fn get(&self) -> QuantstatsHtmlOptions {
        self.0.clone().unwrap_or_default()
    }
}

// ============================================================================
// duckfn_quantstats_html：单序列报告
// ============================================================================

/// 单序列报告聚合状态：累积 `(自 1970-01-01 起的天数, 收益)`。
///
/// Aggregate state for the single-series report: accumulates `(days since 1970-01-01, return)`.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlReportState {
    config: ConfigSlot,
    points: Vec<(i32, f64)>,
}

/// 单序列报告：对 `dt` 与 `ret` 两列做聚合，输出该组的完整 HTML 报告字符串。
///
/// 参数顺序即 SQL 参数顺序：配置（可空，`NULL` 表示全默认）、日期、收益。
/// `dt` 或 `ret` 为 NULL 的行会被整行跳过（duckfn 对非 `Option` 入参的既有语义，与 SQL 聚合惯例一致）。
///
/// ```sql
/// SELECT duckfn_quantstats_html(NULL, dt, ret) FROM daily_returns;
/// SELECT symbol, duckfn_quantstats_html({'title': 'My Fund'}, dt, ret) FROM daily_returns GROUP BY symbol;
/// ```
///
/// Single-series report: aggregates the `dt` and `ret` columns into the full HTML report string of
/// that group.
///
/// Argument order is the SQL argument order: config (nullable, `NULL` means all defaults), date,
/// return. Rows whose `dt` or `ret` is NULL are skipped entirely — duckfn's existing semantics for
/// non-`Option` arguments, and the usual SQL aggregate behaviour.
#[duck_aggregate_function]
fn duckfn_quantstats_html(
    config: Option<QuantstatsHtmlOptions>,
    dt: DuckDate,
    ret: f64,
    state: &mut HtmlReportState,
) {
    state.config.absorb(config);
    state.points.push((dt.days_since_epoch, ret));
}

impl DuckAggregateState for HtmlReportState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.config.absorb(other.config.0.clone());
        self.points.extend_from_slice(&other.points);
    }

    fn result(&self) -> DuckOptionResult<String> {
        // 一组没有任何有效行 → SQL NULL。不要交给 html()，那边会报 EmptySeries 错误，
        // 而「空输入给 NULL」才是聚合函数该有的行为。
        //
        // A group without any valid row → SQL NULL. Do not hand it to html(), which would fail with
        // EmptySeries; returning NULL is the behaviour SQL users expect from an aggregate.
        if self.points.is_empty() {
            return Ok(None);
        }

        let config = self.config.get();
        let series = build_series(&self.points, None)?;
        let options = config.to_report_options()?;

        Ok(Some(render("duckfn_quantstats_html", &series, options)?))
    }
}

// ============================================================================
// duckfn_quantstats_html_benchmark：长表（标签 + 日期 + 收益）带基准报告
// ============================================================================

/// 带基准报告聚合状态：累积 `(标签, 自 1970-01-01 起的天数, 收益)`。
///
/// Aggregate state for the benchmark report: accumulates `(label, days since 1970-01-01, return)`.
#[derive(Default, Debug, Clone)]
pub(crate) struct HtmlBenchmarkState {
    config: ConfigSlot,
    rows: Vec<(String, i32, f64)>,
}

/// 带基准报告：入参是长表式的一行一个标的，用 `name` 区分策略与基准。
///
/// `name` 等于 `opt.benchmark_name` 的行组成基准序列，其余行组成策略序列；两侧的起始对齐由
/// quantstats-rs 按 `match_dates`（默认 true）完成，这里不做额外处理。
///
/// 两条护栏（都会报错而不是产出误导性的报告）：
///   - 没给 `benchmark_name`（或给定的标签一行都没匹配上）→ 无法确定基准；
///   - 非基准行出现多个不同标签 → 一个分组里混了多个标的，应改用 `GROUP BY` 拆开。
///
/// ```sql
/// SELECT fund, duckfn_quantstats_html_benchmark(
///     {'title': 'My Fund', 'benchmark_name': 'SPY', 'benchmark_title': 'S&P 500'}, name, dt, ret)
/// FROM returns GROUP BY fund;
/// ```
///
/// Benchmark report: the input is a long table, one row per instrument, with `name` telling strategy
/// from benchmark.
///
/// Rows whose `name` equals `opt.benchmark_name` form the benchmark series and the remaining rows
/// form the strategy series; start-date alignment is done inside quantstats-rs according to
/// `match_dates` (true by default) and is not repeated here.
///
/// Two guards, both erroring instead of producing a misleading report:
///   - `benchmark_name` is missing (or matches no row at all) → the benchmark is undecidable;
///   - the non-benchmark rows carry more than one distinct label → several instruments are mixed
///     into one group; use `GROUP BY` to split them.
#[duck_aggregate_function]
fn duckfn_quantstats_html_benchmark(
    config: Option<QuantstatsHtmlOptions>,
    name: String,
    dt: DuckDate,
    ret: f64,
    state: &mut HtmlBenchmarkState,
) {
    state.config.absorb(config);
    state.rows.push((name, dt.days_since_epoch, ret));
}

impl DuckAggregateState for HtmlBenchmarkState {
    type Output = String;

    fn simple_combine(&mut self, other: &Self) {
        self.config.absorb(other.config.0.clone());
        self.rows.extend_from_slice(&other.rows);
    }

    fn result(&self) -> DuckOptionResult<String> {
        const WHAT: &str = "duckfn_quantstats_html_benchmark";

        if self.rows.is_empty() {
            return Ok(None);
        }

        let config = self.config.get();

        // 护栏 1：没有基准标签就无法拆分策略与基准。
        //
        // Guard 1: without a benchmark label there is no way to tell strategy from benchmark.
        let Some(benchmark_name) = config.benchmark_name.clone() else {
            return Err(duck_error(format!(
                "{WHAT}: 'benchmark_name' is required — it must name the label that marks the \
                 benchmark rows, e.g. {{'benchmark_name': 'SPY'}}"
            )));
        };

        let mut strategy_points: Vec<(i32, f64)> = Vec::new();
        let mut benchmark_points: Vec<(i32, f64)> = Vec::new();
        // 只记标签本身（去重），用于护栏 2 的报错文案。
        //
        // Only the distinct strategy labels are kept, for guard 2's error message.
        let mut strategy_labels: Vec<&str> = Vec::new();

        for (name, days_since_epoch, value) in &self.rows {
            if *name == benchmark_name {
                benchmark_points.push((*days_since_epoch, *value));
            } else {
                if !strategy_labels.iter().any(|label| *label == name.as_str()) {
                    strategy_labels.push(name);
                }
                strategy_points.push((*days_since_epoch, *value));
            }
        }

        if benchmark_points.is_empty() {
            return Err(duck_error(format!(
                "{WHAT}: no row matches benchmark_name = '{benchmark_name}' in this group"
            )));
        }

        // 护栏 2：一个分组里混了多个策略标签时，报告会把它们合成一条收益曲线，必须拦下。
        //
        // Guard 2: with several strategy labels in one group the report would silently merge them
        // into a single return curve, so this is rejected.
        if strategy_labels.len() > 1 {
            return Err(duck_error(format!(
                "{WHAT}: this group holds {} distinct strategy labels ({}); group by the instrument \
                 so that each group has exactly one strategy plus the benchmark \
                 (benchmark_name = '{benchmark_name}')",
                strategy_labels.len(),
                strategy_labels.join(", "),
            )));
        }

        if strategy_points.is_empty() {
            return Err(duck_error(format!(
                "{WHAT}: this group holds only rows labelled '{benchmark_name}', so there is no \
                 strategy to report on"
            )));
        }

        let strategy =
            build_series(&strategy_points, strategy_labels.first().map(|s| (*s).to_owned()))?;
        let benchmark = build_series(&benchmark_points, Some(benchmark_name.clone()))?;

        let options = config.to_report_options()?.with_benchmark(&benchmark);

        Ok(Some(render(WHAT, &strategy, options)?))
    }
}
