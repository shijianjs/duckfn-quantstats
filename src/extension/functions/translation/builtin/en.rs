// ============================================================================
// 内置数据：英文说明（`en` 的整张表）
//
// `en` 只给说明：它的 `show` 就是目录里的英文原文（`keys::all()`），由 `builtin::table()` 装配 ——
// 抄一份英文原文进来只会有第二份会过期的真相，而且一不小心抄错就会让英文报告的文字**变了样**。
//
// 说明里允许用 emoji（用户明确要的「生动些」），但只用主流系统都自带的那几个：表格、涨跌、钱、时钟、
// 盾牌、水滴这类。它们只是文字，不参与任何逻辑。
//
// Built-in data: the English notes (the whole `en` table).
//
// `en` only carries notes: its `show` values are the catalog's English sources (`keys::all()`), assembled by
// `builtin::table()` — copying the English text in here would be a second source of truth that goes stale,
// and a typo in it would visibly **change** the English report.
//
// The notes may use emoji (the "make it vivid" the user asked for), but only ones every mainstream system
// ships: charts, ups and downs, money, clocks, shields, water. They are text only and take part in nothing.
// ============================================================================

/// 每个 key 的英文说明；顺序与 `keys.rs` 的分组一致（指标在前、月份最后），只为了读起来能对照。
///
/// 完整性由 `table.rs::install_builtin` 在加载期校验：少一个 key 就报错，不会静默少一条说明。
///
/// The English note of every key; grouped the way `keys.rs` groups them (metrics first, months last) purely
/// so the two read side by side. Completeness is checked at load time by `table.rs::install_builtin`: a
/// missing key is an error rather than a silently absent note.
pub(super) const DESCRIPTIONS: &[(&str, &str)] = &[
    // ---- 报告抬头与文档 ----
    ("text.title", "Browser tab title of the report 📄"),
    (
        "text.benchmark_is",
        "Names the benchmark this report is compared against 🎯",
    ),
    (
        "text.generated_by",
        "Which tool rendered the report, and its version 🛠️",
    ),
    (
        "text.no_eoy_data",
        "Shown when there is not a full calendar year of returns yet 📅",
    ),
    (
        "date.range",
        "First and last date the report covers: {d1} {m1} {y1} start, {d2} {m2} {y2} end 📅",
    ),
    // ---- 分节标题 ----
    (
        "section.key_performance_metrics",
        "The metric table: strategy next to its benchmark 📊",
    ),
    (
        "section.worst_drawdowns",
        "The ten deepest drawdowns, worst first 🕳️",
    ),
    ("section.eoy_returns", "Year-by-year compounded returns 📅"),
    (
        "section.eoy_returns_vs_benchmark",
        "Year-by-year returns, strategy next to benchmark 📅",
    ),
    // ---- 表头 ----
    (
        "table.metric",
        "Column of metric names; the other columns are the two series 📋",
    ),
    ("table.eoy_year", "Calendar year ⏱️"),
    ("table.eoy_benchmark", "That year's benchmark return 🎯"),
    ("table.eoy_strategy", "That year's strategy return 📈"),
    (
        "table.eoy_multiplier",
        "Strategy divided by benchmark — above 1 means it won ➗",
    ),
    (
        "table.eoy_won",
        "+ when the strategy beat the benchmark that year 🏆",
    ),
    ("table.dd_started", "Date the drawdown began 🕳️"),
    ("table.dd_recovered", "Date the previous peak was regained ⤴️"),
    (
        "table.dd_drawdown",
        "Depth from peak to trough, in percent 📉",
    ),
    ("table.dd_days", "How long the drawdown lasted, in days ⏳"),
    // ---- 图例与 SVG 标签 ----
    ("label.strategy", "Legend entry of the strategy's own curve 📈"),
    ("label.benchmark", "Legend entry of the benchmark's curve 🎯"),
    (
        "label.mean",
        "Average of the plotted values, drawn as a dashed line 📊",
    ),
    ("dist.daily", "Daily returns 📆"),
    ("dist.weekly", "Weekly returns 🗓️"),
    ("dist.monthly", "Monthly returns 📅"),
    ("dist.quarterly", "Quarterly returns 🧭"),
    ("dist.yearly", "Yearly returns 🎆"),
    // ---- 图表标题 ----
    ("plot.cumulative_returns", "Growth of one unit over time 📈"),
    (
        "plot.cumulative_returns_vs_benchmark",
        "Strategy and benchmark growth on one axis 📈🎯",
    ),
    (
        "plot.cumulative_returns_log_scaled",
        "The same curve on a log scale, so equal ratios look equal 📐",
    ),
    (
        "plot.cumulative_returns_vs_benchmark_log_scaled",
        "Both curves on a log scale 📐",
    ),
    (
        "plot.cumulative_returns_volatility_matched",
        "Strategy rescaled to the benchmark's volatility ⚖️",
    ),
    (
        "plot.cumulative_returns_vs_benchmark_volatility_matched",
        "Volatility-matched comparison of the two curves ⚖️",
    ),
    ("plot.eoy_returns", "One bar per calendar year 📊"),
    (
        "plot.eoy_returns_vs_benchmark",
        "Yearly bars, strategy next to benchmark 📊",
    ),
    (
        "plot.daily_returns_cumulative_sum",
        "Daily returns added up, without compounding ➕",
    ),
    (
        "plot.return_quantiles",
        "How the returns are spread across quantiles 📊",
    ),
    (
        "plot.distribution_of_monthly_returns",
        "Spread of monthly returns 🌡️",
    ),
    (
        "plot.distribution_of_monthly_returns_vs_benchmark",
        "Monthly spread of both series 🌡️",
    ),
    (
        "plot.rolling_volatility_6_months",
        "Six-month rolling annualised volatility 🌊",
    ),
    (
        "plot.rolling_sharpe_6_months",
        "Six-month rolling Sharpe ratio ⚖️",
    ),
    (
        "plot.rolling_sortino_6_months",
        "Six-month rolling Sortino ratio ⚖️",
    ),
    (
        "plot.rolling_beta_to_benchmark",
        "Six-month rolling beta against the benchmark 🎯",
    ),
    (
        "plot.strategy_monthly_returns",
        "Monthly returns in a year-by-month grid 🗓️",
    ),
    ("plot.drawdown_underwater", "Distance below the running peak 🕳️"),
    (
        "plot.strategy_worst_5_drawdown_periods",
        "The five deepest drawdown stretches 🕳️",
    ),
    // ---- 指标 ----
    (
        "metric.risk_free_rate",
        "Annual rate of the risk-free alternative, used by Sharpe 💵",
    ),
    (
        "metric.time_in_market",
        "Share of periods with a non-zero position ⏱️",
    ),
    (
        "metric.cumulative_return",
        "Total compounded return over the whole period 📈",
    ),
    ("metric.cagr", "Compound annual growth rate 🚀"),
    ("metric.sharpe", "Excess return per unit of volatility ⚖️"),
    (
        "metric.prob_sharpe_ratio",
        "Probability that the true Sharpe ratio is above zero 🎲",
    ),
    (
        "metric.smart_sharpe",
        "Sharpe with a penalty for autocorrelated returns 🧠",
    ),
    (
        "metric.sortino",
        "Excess return per unit of downside risk 🛡️",
    ),
    (
        "metric.smart_sortino",
        "Sortino with a penalty for autocorrelated returns 🧠",
    ),
    (
        "metric.sortino_sqrt2",
        "Sortino divided by the square root of two 📐",
    ),
    (
        "metric.smart_sortino_sqrt2",
        "Smart Sortino divided by the square root of two 📐",
    ),
    (
        "metric.omega",
        "Gains above a threshold over losses below it 🎯",
    ),
    ("metric.max_drawdown", "Deepest peak-to-trough fall 📉"),
    ("metric.max_dd_date", "When that trough happened 📅"),
    (
        "metric.max_dd_period_start",
        "The peak the worst drawdown started from 🕳️",
    ),
    (
        "metric.max_dd_period_end",
        "The trough the worst drawdown ended at 🕳️",
    ),
    (
        "metric.longest_dd_days",
        "Longest stretch spent below a previous peak ⏳",
    ),
    (
        "metric.volatility_ann",
        "Annualised standard deviation of returns 🌊",
    ),
    (
        "metric.r_squared",
        "Share of the strategy's moves explained by the benchmark 🎯",
    ),
    (
        "metric.information_ratio",
        "Active return per unit of tracking error 🧭",
    ),
    (
        "metric.calmar",
        "Annualised return divided by the worst drawdown ⚖️",
    ),
    (
        "metric.skew",
        "Asymmetry of returns — negative means a fatter left tail 🪞",
    ),
    (
        "metric.kurtosis",
        "Fat-tailedness compared with a normal distribution 🐘",
    ),
    ("metric.expected_daily", "Geometric mean daily return 📆"),
    ("metric.expected_monthly", "Geometric mean monthly return 📅"),
    ("metric.expected_yearly", "Geometric mean yearly return 🎆"),
    (
        "metric.kelly_criterion",
        "Position size that maximises long-run growth 🎯",
    ),
    ("metric.risk_of_ruin", "Chance of losing the whole stake 💀"),
    (
        "metric.daily_value_at_risk",
        "Loss that the worst 5% of days go beyond ⚠️",
    ),
    (
        "metric.expected_shortfall",
        "Average loss on those worst days 🧊",
    ),
    (
        "metric.max_consecutive_wins",
        "Longest run of winning periods 🔥",
    ),
    (
        "metric.max_consecutive_losses",
        "Longest run of losing periods 🧊",
    ),
    (
        "metric.gain_pain_ratio",
        "Sum of gains divided by sum of losses ⚖️",
    ),
    (
        "metric.gain_pain_1m",
        "Gain/pain computed on monthly sums 🗓️",
    ),
    ("metric.payoff_ratio", "Average win divided by average loss ⚖️"),
    ("metric.profit_factor", "Gross profit over gross loss 💰"),
    ("metric.common_sense_ratio", "Profit factor times tail ratio 🧮"),
    (
        "metric.tail_ratio",
        "95th percentile divided by the 5th percentile 📐",
    ),
    (
        "metric.cpc_index",
        "Profit factor times win rate times payoff ratio 🧮",
    ),
    (
        "metric.outlier_win_ratio",
        "Best 1% of days against the average up day 🎯",
    ),
    (
        "metric.outlier_loss_ratio",
        "Worst 1% of days against the average down day 🕳️",
    ),
    ("metric.mtd", "Month to date 📅"),
    ("metric.3m", "Trailing three months 🗓️"),
    ("metric.6m", "Trailing six months 🗓️"),
    ("metric.ytd", "Year to date 📅"),
    ("metric.1y", "Trailing twelve months 📅"),
    ("metric.3y_ann", "Annualised over three years 📈"),
    ("metric.5y_ann", "Annualised over five years 📈"),
    ("metric.10y_ann", "Annualised over ten years 📈"),
    ("metric.all_time_ann", "Annualised over the whole period 📈"),
    ("metric.best_day", "Single best day 📈"),
    ("metric.worst_day", "Single worst day 📉"),
    ("metric.best_month", "Best calendar month 📈"),
    ("metric.worst_month", "Worst calendar month 📉"),
    ("metric.best_year", "Best calendar year 🏆"),
    ("metric.worst_year", "Worst calendar year 📉"),
    ("metric.avg_drawdown", "Average depth across all drawdowns 🕳️"),
    ("metric.avg_drawdown_days", "Average length of one drawdown ⏳"),
    (
        "metric.recovery_factor",
        "Net return divided by the worst drawdown ⤴️",
    ),
    (
        "metric.ulcer_index",
        "Drawdown depth weighted by how long it lasted 🩹",
    ),
    (
        "metric.serenity_index",
        "Return per unit of drawdown pain 🧘",
    ),
    (
        "metric.avg_up_month",
        "Average return of the positive months 📈",
    ),
    (
        "metric.avg_down_month",
        "Average return of the negative months 📉",
    ),
    ("metric.win_days", "Share of days that closed up 🎯"),
    ("metric.win_month", "Share of months that closed up 🎯"),
    ("metric.win_quarter", "Share of quarters that closed up 🎯"),
    ("metric.win_year", "Share of years that closed up 🎯"),
    ("metric.beta", "Sensitivity to the benchmark's moves 🎯"),
    (
        "metric.alpha",
        "Annualised return that beta does not explain 🌟",
    ),
    ("metric.correlation", "How closely the two series move together 🔗"),
    ("metric.treynor_ratio", "Excess return per unit of beta ⚖️"),
    // ---- 月份 ----
    ("month.jan", "January ❄️"),
    ("month.feb", "February 🌨️"),
    ("month.mar", "March 🌱"),
    ("month.apr", "April 🌦️"),
    ("month.may", "May 🌸"),
    ("month.jun", "June ☀️"),
    ("month.jul", "July 🏖️"),
    ("month.aug", "August 🌾"),
    ("month.sep", "September 🍂"),
    ("month.oct", "October 🎃"),
    ("month.nov", "November 🍁"),
    ("month.dec", "December 🎄"),
];
