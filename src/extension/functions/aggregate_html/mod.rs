// ============================================================================
// 两个 SQL 名字、各一个签名，一次调用出整套报告
//
// 收益率序列（一行 = 一个周期的收益率）：
//
//   qs_html_reports(symbol, date, period_return, options)
//
// 价格/净值序列（一行 = 一天的价格或净值，函数内部换算成收益率）：
//
//   qs_html_reports_by_prices(symbol, date, price, options)
//
// 两者都返回 `list<struct{symbol, strategy_title, html, file_path}>`：**整张长表一次喂进来**（带
// `symbol` 列，SQL 里不写 GROUP BY），函数内部按 symbol 分组，每个 symbol 一份完整报告。基准就是同一张
// 表里的一个 symbol（配置里的 `benchmark` 键），它只作输入、不出报告；因此「带基准」不需要额外参数、
// 不需要 `list(...)`、也不需要 cross join。
//
// 价格那一支**必须**另起一个名字：`(symbol, date, price, options)` 与
// `(symbol, date, period_return, options)` 的类型序列完全一样（`VARCHAR, DATE, DOUBLE, STRUCT`），
// 同一个名字下无法按类型分派。
//
// 报告只在 result() 里生成 —— 一次调用每个标的渲染一份。100 个标的就是 100 份完整报告（每份内含十几张
// SVG），这是预期行为，不是性能 bug；聚合状态因此持有整张表的点，与「每组一份点数组」同阶。
//
// 文件分工（按「路径 → 设施」的顺序读）：
//
//   html_returns.rs   路径 1/2 —— 收益率序列
//   html_prices.rs    路径 2/2 —— 价格/净值序列（收尾里先差分）
//   kind.rs           一条路径在 SQL 侧的名字（取自宏生成的 `SQL_NAME`）
//   slots.rs          参数槽：symbol 表与「每个 symbol 的配置只解析一次」
//   series.rs         内部点表示、序列构造、价格差分（日期换算交给 duckfn 的 chrono 桥）
//   report.rs         收尾：按 symbol 逐份渲染、落盘、按需打开浏览器，回填每份的路径
//   browser.rs        用系统默认浏览器打开报告（tempfile 建临时文件、sanitize-filename 管合法文件名、
//                     open 负责启动；wasm 下整个功能被忽略）
//
// Two SQL names, one signature each, and a whole set of reports per call.
//
// Return series (one row per period's return):
//
//   qs_html_reports(symbol, date, period_return, options)
//
// Price/NAV series (one row per day's price, converted to returns inside the function):
//
//   qs_html_reports_by_prices(symbol, date, price, options)
//
// Both return `list<struct{symbol, strategy_title, html, file_path}>`: a **whole long table per call**
// (with a `symbol` column, no GROUP BY in SQL), grouped by symbol inside the function, one full report
// per symbol. The benchmark is a symbol of that same table (the `benchmark` key in the options), input
// only and never reported on — so "with a benchmark" needs no extra argument, no `list(...)` and no
// cross join.
//
// The price branch **needs its own name**: `(symbol, date, price, options)` and
// `(symbol, date, period_return, options)` have exactly the same type sequence
// (`VARCHAR, DATE, DOUBLE, STRUCT`), so one name could not dispatch them.
//
// Reports are rendered in result() only — one per symbol per call. A hundred instruments means a hundred
// full reports (each with a dozen inline SVGs); that is expected, not a performance bug, and the
// aggregate state therefore holds the whole table's points — the same order as one point array per group.
//
// File layout (read "paths" first, then the shared pieces):
//
//   html_returns.rs   path 1/2 — the return series
//   html_prices.rs    path 2/2 — the price/NAV series (differenced in the tail first)
//   kind.rs           the SQL-side name of one path (read from the macro-generated `SQL_NAME`)
//   slots.rs          the argument slots: the symbol table and "parse each symbol's options once"
//   series.rs         the internal point type, series building and price differencing (the date
//                     conversion is duckfn's chrono bridge)
//   report.rs         the tail: render one report per symbol, persist it, open a browser when asked,
//                     and fill every report's path in
//   browser.rs        opening the report in the system default browser (tempfile creates the temporary
//                     file, sanitize-filename owns the legal-file-name rules, open starts the browser;
//                     the whole feature is ignored on wasm)
// ============================================================================

mod html_prices;
mod html_returns;

mod browser;
mod kind;
mod report;
mod series;
mod slots;
