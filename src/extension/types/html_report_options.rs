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

    /// 基准的显示名，纯展示用：**列表**，与 `benchmark` 按下标一一对应。
    ///
    /// 这一项没给（列表短了、这一项是 NULL 或空串、或者整个键没写）时，退回那一份报告所用的基准
    /// symbol（见 [`Self::benchmark_title_or`]）。它只是展示，所以**宽松**：多余的表项直接忽略，
    /// 不做 `benchmark` 那套校验。
    ///
    /// Display name of the benchmark, presentation only: a **list**, paired with `benchmark` by index.
    ///
    /// An entry that was not given (the list is shorter, the entry is NULL or an empty string, or the
    /// key is absent) falls back to the benchmark symbol that report uses (see
    /// [`Self::benchmark_title_or`]). It is presentation only, hence **lenient**: extra entries are
    /// ignored rather than validated the way `benchmark` is.
    pub benchmark_title: Option<Vec<Option<String>>>,

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

    /// 报告的语言（BCP-47 标签，如 `zh-CN`、`ja`、`de`）。缺省**不做任何翻译、也不加任何说明**。
    ///
    /// 取值必须与**当前翻译表**里的某个语言一致，否则报错（提示用 `qs_list_translations()` 查有哪些语言）
    /// —— 语言写错时宁可失败，也不要悄悄出一份没翻译的报告。内置 `en`、`zh-CN`、`ja`、`de`、`fr`、`es`
    /// 六种；`qs_set_translation()` 能在运行时增删（增删只影响当前进程，见 `functions/translation`）。
    ///
    /// 两个特别的取值：
    ///
    /// - `en`：**文字保持不变**，只补**英文**说明（挂成 `title`）。内置的 `en` 就是这么一份「原文 + 说明」
    ///   的表，所以想给英文报告加浮出提示、又不想改字，用它；
    /// - 其它语言：标题、指标名、月份等换成该语言的文本，并把同语言的说明挂上去。
    ///
    /// 翻译是渲染之后的**静态改写**：报告里不注入任何脚本，HTML 依然自包含。改写在落盘与开浏览器之前完成，
    /// 所以磁盘上的文件、`html` 列与浏览器里看到的完全一致。
    ///
    /// 每个 symbol 各自解析自己的语言（跟其它选项一样），所以一张表里可以同时出中英两份报告。
    ///
    /// The language of the report (a BCP-47 tag such as `zh-CN`, `ja`, `de`). Unset means **no translation and no
    /// notes at all**.
    ///
    /// The value has to be one of the languages in the **current translation table**, otherwise it is an error
    /// (`qs_list_translations()` tells which languages have entries) — a mistyped language should fail rather
    /// than quietly produce an untranslated report. Six are built in: `en`, `zh-CN`, `ja`, `de`, `fr`, `es`;
    /// `qs_set_translation()` can add and remove languages at run time (which affects this process only, see
    /// `functions/translation`).
    ///
    /// Two values are special:
    ///
    /// - `en` **leaves the text alone** and only adds **English** notes (as `title`s). The built-in `en` is
    ///   exactly such an "original text + notes" table, so it is what to use to get tooltips on an English
    ///   report without changing a word;
    /// - any other language replaces the titles, metric names, months and so on with that language's text and
    ///   attaches its notes.
    ///
    /// The translation is a **static rewrite** after rendering: no script goes into the report and the HTML
    /// stays self-contained. It happens before persistence and before the browser opens, so the file on disk,
    /// the `html` column and what the browser shows are the same thing.
    ///
    /// Every symbol resolves its own language (like the other options), so one table can produce Chinese and
    /// English reports side by side.
    pub language: Option<String>,

    /// 把每份报告落盘到该**目录**下，文件名由函数自己生成。缺省不落盘。
    ///
    /// 只给目录、不给文件名：文件名要带上「哪个标的、对哪个基准、什么时候」这些只有函数知道的信息，
    /// 让调用方拼既啰嗦又容易在逻辑变动后失配；真正的路径随后在返回行的 `file_path` 里给出。
    /// 命名规则见 `naming.rs`（`<时间>-<策略名>-<基准名>-<随机尾缀>.html`）。
    ///
    /// 目录必须是**本地路径**，且已经存在（不会替你创建）；不同标的/基准写的文件互不覆盖。
    /// 写文件用 `std::fs`，所以 `s3://…` 这类远端目标不支持（那种路径会以写入错误告终）。
    ///
    /// **wasm 构建下不落盘**：那边整个跳过文件操作 —— 不报错、不写文件，返回行里的 `file_path` 是
    /// NULL，报告本身照常渲染并返回（宿主页面自己展示）。原因见 `storage.rs`：DuckDB-Wasm 的文件系统
    /// 不忠实，连「这个文件名空着吗」都没有可靠答案。
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
    /// The directory has to be a **local** path and has to exist already (it is not created for you);
    /// files belonging to different instruments or benchmarks never overwrite each other. Writing uses
    /// `std::fs`, so a remote target such as `s3://…` is not supported (it ends in a write error).
    ///
    /// **A wasm build writes nothing**: the file operation is skipped there altogether — no error, no
    /// file, and a NULL `file_path` in the returned row, with the report rendered and returned as usual
    /// (the host page shows it). For the reason see `storage.rs`: DuckDB-Wasm's file system is not
    /// faithful enough to answer even "is this name free".
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
    /// 两个显示名由调用方**解析好再传进来**（[`Self::strategy_title_or`] 与
    /// [`Self::benchmark_title_or`]，见 report.rs）：同一份名字还要用于文件名，解析一次、两处使用，
    /// 报告图例与磁盘上的文件名才不会各说各话。
    ///
    /// 生命周期是泛型的：调用方要往返回值上挂基准（`with_benchmark`），由它自己决定借用多久。
    ///
    /// Convert to quantstats-rs' [`ReportOptions`] by starting from its `default()` and overriding
    /// only the fields the user actually wrote, so defaults have a single source of truth.
    ///
    /// The two display names are **resolved by the caller** ([`Self::strategy_title_or`] and
    /// [`Self::benchmark_title_or`], see report.rs): the same names also go into the file name, so
    /// resolving once and using them twice is what keeps the report legend and the file on disk in
    /// agreement.
    ///
    /// The lifetime is generic: callers that want to attach a benchmark (`with_benchmark`) pick how
    /// long the returned value borrows for.
    pub(crate) fn to_report_options<'a>(
        &self,
        strategy_title: &str,
        benchmark_title: Option<&str>,
    ) -> DuckResult<ReportOptions<'a>> {
        let mut options = ReportOptions::default();

        if let Some(title) = &self.title {
            options.title = title.clone();
        }
        options.strategy_title = Some(strategy_title.to_string());
        if let Some(benchmark_title) = benchmark_title {
            options.benchmark_title = Some(benchmark_title.to_string());
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

        // `output_dir` 刻意不在这里处理：文件由本扩展自己写（自己命名、wasm 上跳过），quantstats-rs
        // 的落盘是它自己那套命名，两者不能同时有效。目录交给调用方（report.rs / storage.rs）在拿到
        // 渲染结果后自己写，见 [`Self::output_dir_path`]。
        //
        // `output_dir` is deliberately not handled here: this extension writes the file itself (its own
        // naming, skipped on wasm), while quantstats-rs' own persistence names files its own way — the two
        // cannot be in effect at once. The directory is the caller's business (report.rs / storage.rs),
        // which writes after the report has been rendered — see [`Self::output_dir_path`].
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

    /// 第 `index` 个基准（也就是 `benchmark[index]`）在报告里的显示名：取 `benchmark_title[index]`，
    /// 那一项没给就退回基准 symbol 本身。
    ///
    /// 显示名与基准按下标对齐，是因为它俩本来就是同一份列表的两列；缺项退回 symbol 而不是报错 —— 它只是
    /// 展开展示，`benchmark` 那套校验（空串、NULL、重复）在这儿不适用。
    ///
    /// The display name of the `index`-th benchmark (i.e. `benchmark[index]`): `benchmark_title[index]`
    /// when it was given, the benchmark symbol itself otherwise.
    ///
    /// The display names are paired with the benchmarks by index because they are two columns of the
    /// same list; a missing entry falls back to the symbol rather than erroring — this is presentation
    /// only, so the validation `benchmark` gets (empty strings, NULLs, duplicates) does not apply here.
    pub(crate) fn benchmark_title_or(&self, index: usize, benchmark: &str) -> String {
        self.benchmark_title_at(index).unwrap_or(benchmark).to_string()
    }

    /// `benchmark_title` 里的第 `index` 项；没有这一项、它是 NULL、或是空串时返回 `None`。
    ///
    /// 空串按「没写」处理：空着的图例（`Benchmark is `）不如退回 symbol 有用。
    ///
    /// The `index`-th entry of `benchmark_title`; `None` when there is no such entry, it is NULL or it
    /// is an empty string.
    ///
    /// An empty string counts as "not written": an empty legend entry (`Benchmark is `) is less useful
    /// than falling back to the symbol.
    fn benchmark_title_at(&self, index: usize) -> Option<&str> {
        let title = self.benchmark_title.as_ref()?.get(index)?.as_deref()?;
        if title.is_empty() { None } else { Some(title) }
    }

    /// 配置里的基准 symbol 列表（按写入顺序）；没配基准时是空数组。
    ///
    /// 逐条校验，任何一条不成立都是配置错误：元素不能是 NULL（`['SPX', NULL]`）、不能是空串、不能在
    /// 同一个列表里重复。`benchmark_title` 不参与校验 —— 它按下标对齐、缺项退回 symbol（见
    /// [`Self::benchmark_title_or`]）。
    ///
    /// The configured benchmark symbols, in the order they were written; empty when no benchmark was
    /// configured.
    ///
    /// Every entry is validated and any failure is a configuration error: an element may not be NULL
    /// (`['SPX', NULL]`), may not be an empty string and may not repeat inside one list.
    /// `benchmark_title` is not validated — it pairs up by index and falls back to the symbol (see
    /// [`Self::benchmark_title_or`]).
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

        Ok(names)
    }

    /// 报告落盘目录；没配就是 `None`。
    ///
    /// 空字符串与含 NUL 字节都是配置错误，在这里报掉：前者会拿一个空目录名去拼路径，后者没有任何文件
    /// 系统能接受（`std::fs` 会拒绝，早一点报出来还省掉一次渲染）。这里只管配置形状，落盘本身（含
    /// wasm 下跳过）在 storage.rs。
    ///
    /// The directory reports are written into; `None` when unset.
    ///
    /// An empty string and one carrying a NUL byte are both configuration errors and are reported here: the
    /// former would build a path out of an empty directory name, the latter no file system accepts (`std::fs`
    /// rejects it too, and saying so before a render saves the work). This only checks the shape of the
    /// configuration; the write itself — including skipping it on wasm — lives in storage.rs.
    pub(crate) fn output_dir_path(&self) -> DuckResult<Option<&str>> {
        let Some(output_dir) = self.output_dir.as_deref() else {
            return Ok(None);
        };
        if output_dir.is_empty() {
            return Err(duck_error(
                "qs_html_report_options.output_dir must not be an empty string",
            ));
        }
        if output_dir.contains('\0') {
            return Err(duck_error(
                "qs_html_report_options.output_dir contains a NUL byte",
            ));
        }
        Ok(Some(output_dir))
    }
}
