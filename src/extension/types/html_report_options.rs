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
// ============================================================================

/// 报告配置，字段名即 SQL 里的键名。
///
/// 配置是**逐行求值的一列**，但每个 symbol 只用它第一行那份（见 slots.rs）：想给不同标的不同的标题 /
/// 显示名 / 落盘目录，就用 `symbol` 列把配置拼出来。基准维度上的差异则来自 `benchmark` 列表本身 ——
/// 一个标的对几个基准就出几份报告，每份的基准名各取自列表里的那一个。
///
/// The report options; the Rust field names are the keys used in SQL.
///
/// The options are a **per-row column**, of which each symbol only uses its first row's copy (see
/// slots.rs): to give different instruments their own titles, display names or output directory, build the
/// struct out of the `symbol` column. Differences along the benchmark dimension come from the `benchmark`
/// list itself — one instrument against several benchmarks produces one report per benchmark, each taking
/// its benchmark name from the corresponding list entry.
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

    /// 策略的显示名。缺省是**该 symbol 本身**（见 [`Self::strategy_title_or`]）。
    ///
    /// 报告指标表的表头、`open_in_browser` 的临时文件名前缀与返回行里的 `strategy_title` 都用它，
    /// 所以三处永远一致。
    ///
    /// Display name of the strategy; defaults to **the symbol itself** (see
    /// [`Self::strategy_title_or`]).
    ///
    /// The metrics table heading, the temporary file name prefix used by `open_in_browser` and the
    /// `strategy_title` in the returned rows all use it, so all three always agree.
    pub strategy_title: Option<String>,

    /// 基准的显示名，纯展示用。缺省是**那一份报告所用的基准 symbol**（见
    /// [`Self::benchmark_title_or`]）。
    ///
    /// 多个基准时**不能**写这个键（那时它没有单一答案，见 [`Self::benchmark_names`]）：显示名会各自
    /// 退回对应的基准 symbol。
    ///
    /// Display name of the benchmark, presentation only; defaults to **the benchmark symbol that
    /// report uses** (see [`Self::benchmark_title_or`]).
    ///
    /// It must **not** be set when there is more than one benchmark (it would have no single answer,
    /// see [`Self::benchmark_names`]): each display name then falls back to its own benchmark
    /// symbol.
    pub benchmark_title: Option<String>,

    /// 哪个（哪些）**symbol** 当基准：表里 `symbol` 列的取值，**列表**，顺序有意义。
    ///
    /// 每个基准各出一份报告：`['SPX', 'NDX']` 就是「该标的 vs SPX」与「该标的 vs NDX」两份，返回
    /// 清单里同一 symbol 会出现两行、由 `benchmark` 字段区分。被指到的 symbol 只作输入、不出报告。
    ///
    /// 整次调用里所有标的必须给**同一个列表**（顺序也要一致）：不一致时「谁把谁当基准」就没有单一答案，
    /// 所以直接报错。列表里出现空串、NULL 元素或重复项同样是配置错误。缺省（或空列表）表示不带基准，
    /// 每个标的一份单序列报告。
    ///
    /// 字段类型是 `VARCHAR[]`，所以一个基准也得写成 `['SPX']` 而不是 `'SPX'`。
    ///
    /// Which **symbol(s)** serve as the benchmark: values of the table's `symbol` column, a **list**
    /// whose order matters.
    ///
    /// One report per benchmark: `['SPX', 'NDX']` means "this instrument vs SPX" and "this
    /// instrument vs NDX", so the returned list holds two rows for the same symbol, told apart by the
    /// `benchmark` field. Every symbol named here is input only and gets no report of its own.
    ///
    /// Every instrument in one call has to supply the **same list** (same order too): otherwise
    /// "which one is the benchmark" would have no single answer, so a disagreement is an error. An
    /// empty string, a NULL element or a duplicate inside the list is a configuration error as well.
    /// Unset (or an empty list) means no benchmark and one single-series report per instrument.
    ///
    /// The field type is `VARCHAR[]`, so even a single benchmark is written `['SPX']`, not `'SPX'`.
    pub benchmark: Option<Vec<Option<String>>>,

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

    /// 把每份报告落盘到该**目录**下，文件名由函数自己生成。缺省不落盘。
    ///
    /// 只给目录、不给文件名：文件名要带上「哪个标的、对哪个基准、什么时候」这些只有函数知道的信息，
    /// 让调用方拼既啰嗦又容易在逻辑变动后失配；真正的路径随后在返回行的 `file_path` 里给出。
    /// 命名规则见 `naming.rs`（`<时间>-<策略名>-<基准名>-<随机尾缀>.html`）。
    ///
    /// 目录必须已经存在（不会替你创建），且不同标的/基准写的文件互不覆盖。
    ///
    /// 写文件走 **DuckDB 的 VFS**（duckfn 的 `duck_vfs` 便捷层），不是 `std::fs`：本地磁盘、内存
    /// 文件系统、wasm 构建里宿主真正能读的那个文件系统，以及装了 httpfs 后的 `s3://` / `http(s)://`
    /// 都是同一条通路、同一套语义。也因此聚合函数（C API 不给它客户端上下文）才写得进去。
    ///
    /// Also write every report into this **directory**, with file names generated by the function.
    /// Defaults to not writing anything.
    ///
    /// Only the directory is given, not the file name: a name has to carry "which instrument, against
    /// which benchmark, at what time", which only the function knows — building it in the caller is
    /// both wordy and easy to break whenever that logic changes; the actual path comes back in the
    /// `file_path` column. The naming rules live in `naming.rs`
    /// (`<time>-<strategy>-<benchmark>-<random>.html`).
    ///
    /// The directory has to exist already (it is not created for you), and files belonging to
    /// different instruments or benchmarks never overwrite each other.
    ///
    /// The write goes through **DuckDB's VFS** (duckfn's `duck_vfs` convenience layer) rather than
    /// `std::fs`: local disk, in-memory file systems, whatever file system the wasm build actually
    /// exposes, and `s3://` / `http(s)://` once httpfs is loaded are all the same path with the same
    /// semantics — which is also what lets an aggregate (no client context from the C API) write at
    /// all.
    pub output_dir: Option<String>,

    /// 生成后直接用**系统默认浏览器**打开这些报告。缺省 false（不打开）。
    ///
    /// 浏览器要的是一个真实存在的本地文件，而报告在这里只是一个字符串，所以：
    ///
    /// - 写了 `output_dir`：先落盘，再打开那些文件（每个标的 × 每个基准一份）；
    /// - 没写：每份报告先落到系统临时目录里的
    ///   `<临时目录>/<时间>-<策略名>-<基准名>-<随机尾缀>.html` 文件（名字里出现不了的字符换成 `_`），
    ///   再打开它；
    /// - `output_dir` 指向非本地路径（`s3://` 之类）时报错 —— 系统浏览器打不开它。
    ///
    /// 打开的时机是报告生成之后，且只负责把浏览器叫起来（不等它、也不看它怎么处理文件）。
    ///
    /// **wasm 构建下忽略**：那里没有可以启动的浏览器进程，报告字符串原样返回给宿主，展示是宿主页面的事
    /// （因此也不会为了打开而写临时文件）。
    ///
    /// Open the reports in the **system default browser** once they have been generated. Defaults to
    /// false.
    ///
    /// A browser needs a local file that actually exists, while all we have here is a string, hence:
    ///
    /// - with `output_dir` set, those files are written and then opened (one per instrument ×
    ///   benchmark);
    /// - without it, every report is written to `<temp>/<time>-<strategy>-<benchmark>-<random>.html`
    ///   first (characters that cannot appear in a file name become `_`) and that file is opened;
    /// - an `output_dir` pointing somewhere non-local such as `s3://` is an error, since no browser can
    ///   open it.
    ///
    /// It happens after the reports have been generated, and all it does is get the browser started (it
    /// neither waits for it nor looks at what it does with the file).
    ///
    /// **Ignored in the wasm build**: there is no browser process to launch there, so the report string
    /// goes back to the host untouched and displaying it is the host page's business (which is also why
    /// no temporary file is written just to open it).
    pub open_in_browser: Option<bool>,
}

impl QuantstatsHtmlOptions {
    /// 转成 quantstats-rs 的 [`ReportOptions`]，做法是「从它的 `default()` 出发、逐字段覆盖
    /// 用户显式写了的那些」—— 默认值因此只有一份真相，不会在这里再抄一套。
    ///
    /// 两个参数只服务显示名的退回：`strategy_title` 缺省用 `symbol`、`benchmark_title` 缺省用这一份
    /// 报告所用的基准 symbol。这不是美化 —— 一次调用会出几十份报告，默认的 `'Strategy'` 对每份都一样，
    /// 图例、文件名与返回行里的显示名都会失去区分度。
    ///
    /// 生命周期是泛型的：调用方要往返回值上挂基准（`with_benchmark`），由它自己决定借用多久。
    ///
    /// Convert to quantstats-rs' [`ReportOptions`] by starting from its `default()` and overriding
    /// only the fields the user actually wrote, so defaults have a single source of truth.
    ///
    /// The two arguments only serve the display-name fallbacks: `strategy_title` defaults to the
    /// `symbol` and `benchmark_title` to the benchmark symbol this very report uses. That is not
    /// cosmetics — one call produces dozens of reports, and the default `'Strategy'` is identical for
    /// every one of them, so the legend, the file names and the returned display names would all lose
    /// their distinguishing power.
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
        options.strategy_title = Some(self.strategy_title_or(symbol));
        if let Some(benchmark) = benchmark {
            options.benchmark_title = Some(self.benchmark_title_or(benchmark));
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

        // `output_dir` 刻意不在这里处理：quantstats-rs 落盘用的是 `std::fs`，而本扩展要的是 DuckDB 的
        // VFS（本地磁盘、内存文件系统、wasm 上的文件系统走同一条通路）。目录交给调用方
        // （report.rs）在拿到渲染结果后自己写，见 [`Self::output_dir_path`]。
        //
        // `output_dir` is deliberately not handled here: quantstats-rs writes with `std::fs`, while this
        // extension wants DuckDB's VFS (local disk, in-memory file systems and the wasm build's file
        // system all go through one path). The directory is the caller's business (report.rs), which
        // writes itself after the report has been rendered — see [`Self::output_dir_path`].
        Ok(options)
    }

    /// 这个 symbol 的显示名：写了 `strategy_title` 就用它，否则退回 symbol 本身。
    ///
    /// This symbol's display name: the configured `strategy_title` when there is one, the symbol
    /// itself otherwise.
    pub(crate) fn strategy_title_or(&self, symbol: &str) -> String {
        self.strategy_title
            .as_deref()
            .unwrap_or(symbol)
            .to_string()
    }

    /// 这一份报告所用基准的显示名：写了 `benchmark_title` 就用它，否则退回基准 symbol 本身。
    ///
    /// The display name of the benchmark this report uses: the configured `benchmark_title` when
    /// there is one, the benchmark symbol itself otherwise.
    pub(crate) fn benchmark_title_or(&self, benchmark: &str) -> String {
        self.benchmark_title
            .as_deref()
            .unwrap_or(benchmark)
            .to_string()
    }

    /// 配置里的基准 symbol 列表（按写入顺序）；没配基准时是空数组。
    ///
    /// 逐条校验，任何一条不成立都是配置错误：元素不能是 NULL（`['SPX', NULL]`）、不能是空串、不能在
    /// 同一个列表里重复。另外，`benchmark_title` 在列表长度大于 1 时也必须为空 —— 那时它没有单一答案，
    /// 显示名只能用各自的基准 symbol。
    ///
    /// The configured benchmark symbols, in the order they were written; empty when no benchmark was
    /// configured.
    ///
    /// Every entry is validated and any failure is a configuration error: an element may not be NULL
    /// (`['SPX', NULL]`), may not be an empty string and may not repeat inside one list. On top of
    /// that, `benchmark_title` has to be unset as soon as the list holds more than one benchmark — it
    /// would have no single answer there, and the display names can only come from the benchmark
    /// symbols themselves.
    pub(crate) fn benchmark_names(&self) -> DuckResult<Vec<&str>> {
        let Some(benchmarks) = &self.benchmark else {
            return Ok(Vec::new());
        };

        let mut names: Vec<&str> = Vec::with_capacity(benchmarks.len());
        for benchmark in benchmarks {
            let Some(name) = benchmark.as_deref() else {
                return Err(duck_error(
                    "qs_html_report_options.benchmark must not contain a NULL element",
                ));
            };
            if name.is_empty() {
                return Err(duck_error(
                    "qs_html_report_options.benchmark must not contain an empty string",
                ));
            }
            if names.contains(&name) {
                return Err(duck_error(format!(
                    "qs_html_report_options.benchmark lists '{name}' twice — a symbol can only be used as \
                     one benchmark"
                )));
            }
            names.push(name);
        }

        if names.len() > 1 && self.benchmark_title.is_some() {
            return Err(duck_error(format!(
                "qs_html_report_options.benchmark_title cannot be set with {} benchmarks — the display name \
                 comes from each benchmark symbol instead",
                names.len()
            )));
        }

        Ok(names)
    }

    /// 报告落盘目录；没配就是 `None`。
    ///
    /// 空字符串是配置错误，在这里报掉（否则会拿一个空目录名去拼路径）。
    ///
    /// The directory reports are written into; `None` when unset.
    ///
    /// An empty string is a configuration error and is reported here (otherwise an empty directory
    /// name would be used to build a path).
    pub(crate) fn output_dir_path(&self) -> DuckResult<Option<&str>> {
        let Some(output_dir) = self.output_dir.as_deref() else {
            return Ok(None);
        };
        if output_dir.is_empty() {
            return Err(duck_error(
                "qs_html_report_options.output_dir must not be an empty string",
            ));
        }
        Ok(Some(output_dir))
    }
}
