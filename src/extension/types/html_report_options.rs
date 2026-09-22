use duckfn::{DuckResult, DuckStruct, duck_error};
use quantstats_rs::HtmlReportOptions as ReportOptions;

// ============================================================================
// 报告配置：一个具名 STRUCT 类型，可在 SQL 里直接 cast
//
// `#[duck(create_type = true)]` 让 duckfn 在扩展加载期执行
//   CREATE TYPE IF NOT EXISTS "qs_html_report_options" AS STRUCT(...);
// 之后 SQL 里可以直接写 `{'title': 'x'}::qs_html_report_options`，
// 也可以把 JSON 字符串转成它（`'{"title": "x"}'::JSON::qs_html_report_options`）。
//
// 字段**全部**是 `Option<T>`，这是硬要求：DuckDB 的 struct 字面量缺字段时会补 NULL，
// 而 duckfn 读到「非 Option 字段为 NULL」时会让**整个 struct** 变成 NULL。那样用户写的
// `{'rf': 0.1}` 会整体退化成默认值，他设的 rf 被静默丢掉。逐个字段 unwrap_or 没有这个问题。
//
// # 这是一个**逐行的参数**，每个 symbol 各留一份
//
// 报告函数一次处理整张长表、内部按 symbol 分组，而配置是逐行求值的一列 —— 这正是「每个标的各
// 有一套标题 / 显示名 / 落盘路径」的实现方式：SQL 侧用 symbol 列把配置拼出来
//
//   {'title': symbol, 'output': 'reports/' || symbol || '.html'}::qs_html_report_options
//
// 函数只对每个 symbol 求值一次（见 `slots.rs` 的 `SymbolTable::push`）。同一个 symbol 的配置
// 因此必须逐行一致 —— 只有该 symbol 第一行的那份会被用上。
//
// # 显示名没写就退回数据里的名字
//
// `strategy_title` 与 `benchmark_title` 都是纯展示字段，缺省时退回该 symbol / 基准 symbol 的
// 名字（见 [`Self::strategy_title_or`] 与 [`Self::benchmark_title_or`]，两条规则各只有一处）：
// 一次调用出几十份报告时，图例、浏览器临时文件名与返回行里的显示名都必须自解释。
//
// Report options: a named STRUCT type that SQL can cast to directly.
//
// `#[duck(create_type = true)]` makes duckfn run
//   `CREATE TYPE IF NOT EXISTS "qs_html_report_options" AS STRUCT(...)`
// at load time, so SQL can write `{'title': 'x'}::qs_html_report_options`, or cast a JSON
// string to it.
//
// Every field is an `Option<T>` on purpose: DuckDB fills missing keys of a struct literal with NULL,
// and duckfn turns the **whole struct** into NULL when a non-Option field reads NULL — so a user's
// `{'rf': 0.1}` would silently fall back to all defaults and their `rf` would be dropped.
//
// # This is a **per-row** argument, and every symbol keeps its own copy
//
// The report function handles a whole long table in one call and groups by symbol internally, while
// the options are a per-row column — that is exactly how "each instrument gets its own title,
// display name and output path" works: SQL builds the struct out of the symbol column
//
//   {'title': symbol, 'output': 'reports/' || symbol || '.html'}::qs_html_report_options
//
// and the function evaluates it once per symbol (see `SymbolTable::push` in `slots.rs`). The
// options of one symbol therefore have to agree row by row — only the first row's copy is used.
//
// # Display names fall back to names that come from the data
//
// `strategy_title` and `benchmark_title` are presentation only, and fall back to the symbol / the
// benchmark symbol when unset (see [`Self::strategy_title_or`] and [`Self::benchmark_title_or`],
// one place each): with dozens of reports coming out of one call, the legend, the temporary file
// name and the returned display name all have to be self-explanatory.
// ============================================================================

/// 报告配置，字段名即 SQL 里的键名。
///
/// The report options; the Rust field names are the keys used in SQL.
#[derive(Clone, Debug, Default, DuckStruct)]
#[duck(
    sql_name = "qs_html_report_options",
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

    /// 基准的显示名，纯展示用。缺省时退回基准那个 symbol 的名字（见 [`Self::strategy_title_or`]
    /// 的同类规则），报告图例与浏览器临时文件名因此不会出现一个没有名字的基准。
    ///
    /// Display name of the benchmark, presentation only. When unset it falls back to the benchmark
    /// symbol (the same kind of rule as [`Self::strategy_title_or`]), so the report legend and the
    /// temporary file name never show a nameless benchmark.
    pub benchmark_title: Option<String>,

    /// 哪个 **symbol** 当基准。缺省（NULL）表示每个 symbol 各出一份不带基准的报告。
    ///
    /// 它指的是输入表里 `symbol` 列的一个取值 —— 基准就是表里一个普通的 symbol，不需要另外
    /// 构造列表参数，也不需要 cross join。被它指到的那个 symbol **只作输入、不出报告**：一次
    /// 调用里 100 个标的指了 1 个基准，就返回 99 条结果。想给不同标的配不同基准，或者想让
    /// 基准自己也出一份报告，用户自己 `GROUP BY` / 过滤后分几次调用即可 —— 插件不把「一次调用
    /// 里 N 个策略 × M 个基准」这种笛卡尔积做进 API。
    ///
    /// 规则：一次调用里所有配置写下的非 NULL `benchmark` 必须一致，不一致报错（否则「谁把谁当
    /// 基准」会变成一句说不清的话）。
    ///
    /// Which **symbol** is the benchmark. Unset (NULL) means every symbol gets its own
    /// benchmark-less report.
    ///
    /// It names a value of the input's `symbol` column — the benchmark is an ordinary symbol in the
    /// table, so there is no list argument to build and no cross join to write. The symbol it names
    /// is **input only and gets no report**: point at one benchmark among 100 instruments and 99
    /// rows come back. Wanting different benchmarks per instrument, or a report for the benchmark
    /// itself, is expressed by grouping/filtering and calling again — the extension does not bake
    /// the N strategies × M benchmarks cartesian product into its API.
    ///
    /// Rule: every non-NULL `benchmark` written in one call must agree, otherwise it is an error
    /// (otherwise "which one is the benchmark" would have no single answer).
    pub benchmark: Option<String>,

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
    /// 写文件走 **DuckDB 的 VFS**（duckfn 的 `duck_vfs` 便捷层），不是 `std::fs`：本地磁盘、内存
    /// 文件系统、wasm 构建里宿主真正能读的那个文件系统，以及装了 httpfs 后的 `s3://` / `http(s)://`
    /// 都是同一条通路、同一套语义。也因此聚合函数（C API 不给它客户端上下文）才写得进去。
    /// **覆盖写就是覆盖写**：目标已存在时内容会被换成这一次的报告（旧文件更长也不会留下尾巴，
    /// C API 缺 truncate 这件事由便捷层内部处理）。
    ///
    /// Also write the HTML to this path. Defaults to not writing anything.
    ///
    /// The write goes through **DuckDB's VFS** (duckfn's `duck_vfs` convenience layer) rather than
    /// `std::fs`: local disk, in-memory file systems, whatever file system the wasm build actually
    /// exposes, and `s3://` / `http(s)://` once httpfs is loaded are all the same path with the same
    /// semantics — which is also what lets an aggregate (no client context from the C API) write at
    /// all. **Replace really replaces**: an existing target ends up holding exactly this report (a
    /// longer file leaves no tail; the C API's missing truncate is handled inside that layer).
    pub output: Option<String>,

    /// 生成后直接用**系统默认浏览器**打开这份报告。缺省 false（不打开）。
    ///
    /// 浏览器要的是一个真实存在的本地文件，而报告在这里只是一个字符串，所以：
    ///
    /// - 写了 `output`：先落盘，再打开那个文件；
    /// - 没写：报告先落到系统临时目录里一个形如 `<时间>-<策略名>-<基准名>-<随机尾缀>.html` 的文件（名字里
    ///   出现不了的字符换成 `_`），再打开它；
    /// - `output` 指向非本地路径（`s3://` 之类）时报错 —— 系统浏览器打不开它。
    ///
    /// 打开的时机是报告生成之后，且只负责把浏览器叫起来（不等它、也不看它怎么处理文件）。
    ///
    /// **wasm 构建下忽略**：那里没有可以启动的浏览器进程，报告字符串原样返回给宿主，展示是宿主页面的事
    /// （因此也不会为了打开而写临时文件）。
    ///
    /// Open the report in the **system default browser** once it has been generated. Defaults to false.
    ///
    /// A browser needs a local file that actually exists, while all we have here is a string, hence:
    ///
    /// - with `output` set, that file is written and then opened;
    /// - without it, the report is written to `<temp>/<time>-<strategy>-<benchmark>-<random>.html` first
    ///   (characters that cannot appear in a file name become `_`) and that file is opened;
    /// - an `output` pointing somewhere non-local such as `s3://` is an error, since no browser can open it.
    ///
    /// It happens after the report has been generated, and all it does is get the browser started (it neither
    /// waits for it nor looks at what it does with the file).
    ///
    /// **Ignored in the wasm build**: there is no browser process to launch there, so the report string goes
    /// back to the host untouched and displaying it is the host page's business (which is also why no
    /// temporary file is written just to open it).
    pub open_in_browser: Option<bool>,
}

impl QuantstatsHtmlOptions {
    /// 转成 quantstats-rs 的 [`ReportOptions`]，做法是「从它的 `default()` 出发、逐字段覆盖
    /// 用户显式写了的那些」—— 默认值因此只有一份真相，不会在这里再抄一套。
    ///
    /// 两个参数是显示名的退路，不是可选的装饰：`symbol` 是这份报告对应的标的，`benchmark` 是
    /// 它的基准（没有基准时传 `None`）。见 [`Self::strategy_title_or`] 与
    /// [`Self::benchmark_title_or`]。
    ///
    /// 生命周期是泛型的：调用方要往返回值上挂基准（`with_benchmark`），由它自己决定借用多久。
    ///
    /// Convert to quantstats-rs' [`ReportOptions`] by starting from its `default()` and overriding
    /// only the fields the user actually wrote, so defaults have a single source of truth.
    ///
    /// The two arguments are where the display names fall back to, not optional decoration:
    /// `symbol` is the instrument this report belongs to and `benchmark` its benchmark (`None` when
    /// there is none). See [`Self::strategy_title_or`] and [`Self::benchmark_title_or`].
    ///
    /// The lifetime is generic: callers that want to attach a benchmark (`with_benchmark`) pick how
    /// long the returned value borrows for.
    pub(crate) fn to_report_options<'a>(
        &self,
        symbol: &str,
        benchmark: Option<&str>,
    ) -> DuckResult<ReportOptions<'a>> {
        let mut options = ReportOptions::default();

        if let Some(title) = &self.title {
            options.title = title.clone();
        }
        // 显示名一律显式写进报告配置：没配置就退回 symbol（图例、浏览器临时文件名、返回行里的
        // `strategy_title` 于是都指向同一个名字）。
        //
        // The display name is always written into the report options: when it is not configured it
        // falls back to the symbol, so the legend, the temporary file name and the returned
        // `strategy_title` all name the same thing.
        options = options.with_strategy_title(self.strategy_title_or(symbol));
        if let Some(benchmark) = benchmark {
            options = options.with_benchmark_title(self.benchmark_title_or(benchmark));
        }
        if let Some(rf) = self.rf {
            options.rf = rf;
        }
        if let Some(periods_per_year) = self.periods_per_year {
            if periods_per_year == 0 {
                return Err(duck_error(
                    "qs_html_report_options.periods_per_year must be greater than 0",
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

    /// 这份报告的显示名：显式写了 `strategy_title` 就用它，否则退回 `symbol`。
    ///
    /// 退回不是「凑一个非空值」，而是这个函数一次要出几十份报告的前提：默认的 `'Strategy'`
    /// 对每一份都一样，图例里认不出谁是谁，浏览器临时文件名也会撞成同一串前缀。
    ///
    /// The display name of this report: the configured `strategy_title`, or the `symbol` itself.
    ///
    /// The fallback is what makes one call with dozens of reports usable rather than cosmetic: the
    /// default `'Strategy'` is identical for every one of them, so the legend could not tell them
    /// apart and the temporary file names would share one useless prefix.
    pub(crate) fn strategy_title_or(&self, symbol: &str) -> String {
        self.strategy_title
            .clone()
            .unwrap_or_else(|| symbol.to_owned())
    }

    /// 基准的显示名：显式写了 `benchmark_title` 就用它，否则退回基准的那个 symbol。
    ///
    /// 与 [`Self::strategy_title_or`] 同一条规则的两侧，理由也相同。
    ///
    /// The benchmark's display name: the configured `benchmark_title`, or the benchmark symbol.
    ///
    /// The other half of the same rule as [`Self::strategy_title_or`], for the same reason.
    pub(crate) fn benchmark_title_or(&self, benchmark: &str) -> String {
        self.benchmark_title
            .clone()
            .unwrap_or_else(|| benchmark.to_owned())
    }

    /// 当基准的那个 symbol 名；没配就是 `None`。
    ///
    /// 空字符串是配置错误，在这里报掉（与 [`Self::output_path`] 同样的处理）：空 symbol 在数据
    /// 里找不到，与其让它落进「基准 symbol 不存在」那条报错，不如直接说清是配置写坏了。
    ///
    /// The symbol that acts as the benchmark; `None` when unset.
    ///
    /// An empty string is a configuration error and is reported here (the same treatment as
    /// [`Self::output_path`]): it matches no symbol in the data, and saying so directly beats
    /// falling through to the "the benchmark symbol has no rows" error.
    pub(crate) fn benchmark_name(&self) -> DuckResult<Option<&str>> {
        let Some(benchmark) = self.benchmark.as_deref() else {
            return Ok(None);
        };
        if benchmark.is_empty() {
            return Err(duck_error(
                "qs_html_report_options.benchmark must not be an empty string",
            ));
        }
        Ok(Some(benchmark))
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
                "qs_html_report_options.output must not be an empty string",
            ));
        }
        Ok(Some(output))
    }
}
