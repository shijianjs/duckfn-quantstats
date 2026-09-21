// ============================================================================
// 两个 SQL 名字、四个聚合重载
//
// 收益率序列（一行 = 一个周期的收益率）：
//
//   duckfn_quantstats_html(date, period_return, options)                  单序列报告
//   duckfn_quantstats_html(date, period_return, benchmark, options)       带基准报告
//
// 价格/净值序列（一行 = 一天的价格或净值，函数内部换算成收益率）：
//
//   duckfn_quantstats_html_prices(date, price, options)                   单序列报告
//   duckfn_quantstats_html_prices(date, price, benchmark, options)        带基准报告
//
// 每个名字下两个签名只差一个 `benchmark` 参数，所以用 `overloads_name` 各注册成一个**函数集**，
// 按参数个数分派（`register_all_aggregate_overload` 会按名字分组，每个重载各自带参数表与返回类型）。
// 参数顺序固定为「数据列在前、配置在后」。
//
// 价格那一支**必须**另起一个名字：`(date, price, options)` 与 `(date, period_return, options)`
// 的类型序列完全一样（都是 `DATE, DOUBLE, STRUCT`），同一个名字下无法按类型分派。
//
// 报告只在 result() 里生成一次 —— 即每个分组一次。GROUP BY 100 个标的就会渲染 100 份完整报告
// （每份内含十几张 SVG），这是预期行为，不是性能 bug。同理，每个分组仍会各自持有一份解析好的基准点：
// 聚合状态不跨分组共享，这部分开销消不掉，能省掉的是 DuckDB 层的行展开与扫描。
//
// 文件分工（按「路径 → 设施」的顺序读）：
//
//   html_returns.rs   路径 1/2 —— 收益率序列（单序列、带基准）
//   html_prices.rs    路径 3/4 —— 价格/净值序列（单序列、带基准）
//   kind.rs           一个分支的 SQL 侧名字（函数名、值列名）
//   series.rs         内部点表示、日期换算、序列构造、价格差分
//   slots.rs          配置槽与基准槽（只缓存解析结果，不缓存 DuckLazy 凭证）
//   report.rs         收尾：点 → ReturnSeries → 渲染 HTML
//
// Two SQL names, four aggregate overloads.
//
// Return series (one row per period's return):
//
//   duckfn_quantstats_html(date, period_return, options)                  single series
//   duckfn_quantstats_html(date, period_return, benchmark, options)       with a benchmark
//
// Price/NAV series (one row per day's price, converted to returns inside the function):
//
//   duckfn_quantstats_html_prices(date, price, options)                   single series
//   duckfn_quantstats_html_prices(date, price, benchmark, options)        with a benchmark
//
// The two signatures under each name differ only by the `benchmark` argument, so `overloads_name`
// registers each pair as **one function set**, dispatched by argument count
// (`register_all_aggregate_overload` groups by name and every overload keeps its own parameter list and
// return type). The argument order is always "data columns first, options last".
//
// The price branch **needs its own name**: `(date, price, options)` and `(date, period_return, options)`
// have exactly the same type sequence (`DATE, DOUBLE, STRUCT`), so one name could not dispatch them.
//
// The report is rendered once in result(), i.e. once per group. GROUP BY over 100 instruments renders 100
// full reports (each with a dozen inline SVGs); that is expected, not a performance bug. For the same
// reason every group holds its own parsed copy of the benchmark points: aggregate states are not shared
// across groups, so that part cannot be avoided — what this design removes is DuckDB's row expansion and
// scanning.
//
// File layout (read "paths" first, then the shared pieces):
//
//   html_returns.rs   paths 1/2 — return series (single, with a benchmark)
//   html_prices.rs    paths 3/4 — price/NAV series (single, with a benchmark)
//   kind.rs           the SQL-side names of one branch (function name, value field)
//   series.rs         the internal point type, date conversion, series building, price differencing
//   slots.rs          the options slot and the benchmark slot (parsed values only, never a DuckLazy token)
//   report.rs         the tail: points → ReturnSeries → rendered HTML
// ============================================================================

mod html_prices;
mod html_returns;

mod kind;
mod report;
mod series;
mod slots;
