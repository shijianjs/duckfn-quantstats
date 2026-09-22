// ============================================================================
// 参数槽：按 symbol 各留一份配置，外加一整张 symbol 表
//
// # 为什么是「一张 symbol 表」而不是「一个参数一个槽」
//
// 报告函数一次处理整张长表（SQL 里不写 GROUP BY），所以它必须自己把行按 symbol 分开 —— 这张表
// 就是那个分组：symbol -> { 该 symbol 的配置, 该 symbol 的点 }。调用方本来要写的 GROUP BY 由函数
// 内部完成，于是基准（表里另一个 symbol）也在同一个聚合状态里，随时可以取来配对。
//
// # 配置为什么也按 symbol 分槽
//
// 配置是逐行求值的一列（`DuckLazy<T>`），而报告是按 symbol 一份（标题、显示名、落盘路径都各不
// 相同），所以「解析一次」这条路径的粒度从「整个聚合」下沉到「每个 symbol」：某个 symbol 第一次
// 出现时把它的配置解析出来，之后同一个 symbol 的行不再碰这一列。代价是配置解析次数 = distinct
// symbol 数而不是 1 —— 这正是「每个标的一套配置」要付的钱，与行数无关。
//
// 槽本身仍是 `DuckLazySlot<T>`（0.0.6 起由 duckfn 提供，本文件不再自己写三态枚举）：
//
//   update 回调    slot.resolve_optional(arg.as_ref())?   该 symbol 首次出现时解析一次
//   simple_combine slot.combine(&other.slot)              搬运已解析的值，不重新解析
//   result         slot.get()                             `Option<Arc<T>>`；未解析与「解析为 NULL」都是 None
//
// 「这个 symbol 是不是第一次出现」不需要额外的标志位：表的键本身就是那个标记 —— 只有 insert
// 新槽位的那条路径会读配置列，命中已有槽位的那条路径只追加点（见 `push`）。
//
// # 为什么基准是表里的一个 symbol，而不是列表参数
//
// 聚合函数只看得到自己组内的行。把基准做成一次性传入的列表参数（`list(...)` + cross join）确实能
// 让基准只求值一次，但代价是 SQL 侧的三步走（CTE 聚成单行 → cross join → GROUP BY），而且「带基准」
// 这个常态反而比不带基准多一个参数、多一个重载。
//
// 基准本来就在同一张长表里（它也是一个 symbol），所以让函数自己按 symbol 分组、自己去表里取基准更
// 直接：SQL 一句 SELECT，没有 GROUP BY、没有 cross join，带不带基准只差配置里一个 `benchmark` 键。
// 代价是聚合状态持有全表点（与「每组一份点数组」同阶），换来的是「一次调用出全套报告」。被指为基准
// 的 symbol 只作输入、不出报告 —— 「N 个策略 × M 个基准」那种笛卡尔积不是插件要做的事。
//
// # 确定性与 combine
//
// DuckDB 会并行/分块执行，`combine` 的调用顺序不保证，所以：`combine` 只做合并（同 symbol 合并槽
// 与点、新 symbol 插入），点的排序仍交给 `ReturnSeries::new` / `prices_to_returns`，而**输出顺序**
// 由 `result()` 里按 symbol 排序的遍历定下（`iter_sorted`），不依赖 HashMap 的迭代顺序。
//
// 同一个 symbol 的配置取「该份状态里首次出现的那一行」；并行下是哪一行不保证，所以调用方要保证同一个
// symbol 的配置逐行一致（文档与测试都按这个前提写）。
//
// One slot per symbol, plus one table of all symbols.
//
// # Why a table of symbols rather than one slot per argument
//
// The report function handles a whole long table in one call (no GROUP BY in SQL), so it has to split
// the rows by symbol itself — that split *is* this table: symbol -> { its options, its points }. The
// GROUP BY an SQL caller would have written happens inside the function, so the benchmark (another
// symbol of the same table) is right there in the same aggregate state, ready to be paired up.
//
// # Why the options are per symbol too
//
// The options are a per-row column (`DuckLazy<T>`) while the reports are per symbol (each with its own
// title, display name and output path), so "parse it once" moves from the whole aggregate down to each
// symbol: the first time a symbol shows up its options are parsed and the rows after that never touch
// that column again. The price is options parses = distinct symbols instead of 1 — exactly what
// per-instrument options cost, and it does not grow with the row count.
//
// The slot itself is still `DuckLazySlot<T>` (duckfn's since 0.0.6, so this file no longer hand-rolls
// the three-state enum):
//
//   update callback   slot.resolve_optional(arg.as_ref())?   parsed once, on the symbol's first row
//   simple_combine    slot.combine(&other.slot)              carries the parsed value, never re-parses
//   result            slot.get()                             `Option<Arc<T>>`; "never parsed" and "parsed as NULL" are both None
//
// "Is this the symbol's first row?" needs no extra flag: the key of the table *is* that marker — only
// the path that inserts a fresh slot reads the options column, while a hit on an existing slot merely
// appends a point (see `push`).
//
// # Why the benchmark is a symbol in the table rather than a list argument
//
// An aggregate only sees the rows of its own group. Making the benchmark a list passed in once
// (`list(...)` plus a cross join) does evaluate it exactly once, but the price is a three-step SQL
// dance (CTE into one row → cross join → GROUP BY) and a "with a benchmark" case that — as the norm
// rather than the exception — needs an extra argument and an extra overload.
//
// The benchmark is already in the same long table (it is a symbol like any other), so having the
// function group by symbol itself and look the benchmark up in that same table is more direct: one
// SELECT, no GROUP BY, no cross join, and with/without a benchmark differs by a single `benchmark` key
// in the options. The price is an aggregate state holding the whole table's points (the same order as
// one point array per group); what it buys is "one call, the whole set of reports". The symbol named as
// the benchmark is input only and gets no report — the "N strategies × M benchmarks" cartesian product
// is not this extension's job.
//
// # Determinism and combine
//
// DuckDB runs in parallel and in chunks and does not promise a combine order, so: `combine` only merges
// (same symbol → merge slots and points, new symbol → insert), the point ordering stays with
// `ReturnSeries::new` / `prices_to_returns`, and the **output order** is settled in `result()` by
// iterating symbols in ascending order (`iter_sorted`) instead of following HashMap iteration order.
//
// One symbol's options come from "the first row of that symbol seen by that state", which under
// parallelism is whichever row a thread got first, so callers must keep one symbol's options identical
// row by row (the documentation and the tests assume exactly that).
// ============================================================================

use std::collections::HashMap;
use std::sync::Arc;

use duckfn::{DuckLazy, DuckLazySlot, DuckResult};

use crate::extension::types::html_report_options::QuantstatsHtmlOptions;

use super::series::SeriesPoint;

/// 一个 symbol 的槽位：它的配置，和它的点。
///
/// The slot of one symbol: its options and its points.
#[derive(Default, Debug, Clone)]
pub(super) struct SymbolSlot {
    /// 该 symbol 的配置：只在该 symbol 第一次出现时解析一次，之后 combine 只搬引用计数。
    ///
    /// This symbol's options: parsed once, on its first row, and carried over by a refcount bump in
    /// every later `combine`.
    options: DuckLazySlot<QuantstatsHtmlOptions>,

    /// 该 symbol 的点：收益率路径下就是收益率，价格路径下是价格/净值（差分留到 `result()`）。
    ///
    /// The points of this symbol: returns on the return branch, prices/NAVs on the price branch
    /// (differencing is left to `result()`).
    points: Vec<SeriesPoint>,
}

impl SymbolSlot {
    /// 该 symbol 的配置；没有解析结果（整列是 NULL，或这份状态没见过它）时给全默认。
    ///
    /// 返回 `Arc` 而不是引用：调用方（report.rs）要把配置和这份状态活着一样久地一起拿着，而
    /// `DuckLazySlot` 只借得出 `&T`。克隆 `Arc` 只是引用计数，不复制配置。
    ///
    /// This symbol's options, or all defaults when there is no parse result (the column was NULL
    /// throughout, or this state never saw the symbol).
    ///
    /// It hands out an `Arc` rather than a reference because the caller (report.rs) has to hold the
    /// options for as long as the state lives, and `DuckLazySlot` only lends `&T`. Cloning the `Arc`
    /// is a refcount bump, not a copy of the options.
    pub(super) fn options_or_default(&self) -> Arc<QuantstatsHtmlOptions> {
        self.options.get().unwrap_or_default()
    }

    /// 该 symbol 的点。
    ///
    /// The points of this symbol.
    pub(super) fn points(&self) -> &[SeriesPoint] {
        &self.points
    }
}

/// 整张长表按 symbol 分好的组。
///
/// The whole long table, already split by symbol.
#[derive(Default, Debug, Clone)]
pub(super) struct SymbolTable {
    slots: HashMap<String, SymbolSlot>,
}

impl SymbolTable {
    /// 一行数据进表：symbol 第一次出现时把它的配置解析出来，点则一直往后追加。
    ///
    /// 空 symbol 直接丢掉：它既当不了文件名、也当不了显示名（显示名缺省时就退回它），属于「这一行
    /// 没有标的」，与 `symbol` 列本身是 NULL 时 duckfn 整行跳过的既有语义一致。
    ///
    /// Push one data row: the first time a symbol appears its options are parsed, and its points keep
    /// accumulating afterwards.
    ///
    /// An empty symbol is dropped outright: it can be neither a file name nor a display name (the
    /// display name falls back to it), so such a row has no instrument — consistent with duckfn's
    /// existing "skip the whole row" semantics for a NULL `symbol` column.
    pub(super) fn push(
        &mut self,
        symbol: &str,
        options: Option<&DuckLazy<QuantstatsHtmlOptions>>,
        point: SeriesPoint,
    ) -> DuckResult<()> {
        if symbol.is_empty() {
            return Ok(());
        }

        match self.slots.get_mut(symbol) {
            // 已经见过这个 symbol：配置不动（首行那份说了算），只追加点。
            //
            // Seen before: the options stay put (the first row's copy wins) and only a point is added.
            Some(slot) => slot.points.push(point),
            None => {
                let mut slot = SymbolSlot::default();
                slot.options.resolve_optional(options)?;
                slot.points.push(point);
                self.slots.insert(symbol.to_owned(), slot);
            }
        }

        Ok(())
    }

    /// 合并两份状态（DuckDB 并行/分块时会各建一份，最后合起来）。
    ///
    /// 同 symbol 的两份槽位合并：配置走 `DuckLazySlot::combine`（搬运已解析的值，不重新解析），
    /// 点则拼接 —— 真正的按日期排序不在这里做，也不需要（见模块头）。
    ///
    /// Merge two states (DuckDB creates one per thread/chunk and merges them at the end).
    ///
    /// A symbol present on both sides merges its slots: the options go through
    /// `DuckLazySlot::combine` (carrying the parsed value over rather than parsing again) and the
    /// points are concatenated — the real date ordering neither happens here nor is needed (see the
    /// module header).
    pub(super) fn combine(&mut self, other: &Self) {
        for (symbol, other_slot) in &other.slots {
            match self.slots.get_mut(symbol.as_str()) {
                Some(slot) => {
                    slot.options.combine(&other_slot.options);
                    slot.points.extend_from_slice(&other_slot.points);
                }
                None => {
                    self.slots.insert(symbol.clone(), other_slot.clone());
                }
            }
        }
    }

    /// 表里一个 symbol 都没有（`result()` 据此返回 SQL NULL）。
    ///
    /// Whether the table holds no symbol at all (`result()` turns that into SQL NULL).
    pub(super) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// 按名字取一个 symbol 的槽位（取基准用它）。
    ///
    /// Look one symbol up by name (that is how the benchmark is fetched).
    pub(super) fn get(&self, symbol: &str) -> Option<&SymbolSlot> {
        self.slots.get(symbol)
    }

    /// 按 symbol 升序遍历：结果 LIST 的顺序由它定下，不依赖 HashMap 的迭代顺序与 combine 的顺序。
    ///
    /// Iterate in ascending symbol order: this is what fixes the order of the returned LIST instead of
    /// leaving it to HashMap iteration or the combine order.
    pub(super) fn iter_sorted(&self) -> impl Iterator<Item = (&str, &SymbolSlot)> {
        let mut symbols: Vec<&str> = self.slots.keys().map(String::as_str).collect();
        symbols.sort_unstable();

        // 下标取槽位不可能失败：这些键就是从这张表里取的。
        //
        // Indexing cannot fail here: those keys were taken from this very table.
        symbols.into_iter().map(move |symbol| (symbol, &self.slots[symbol]))
    }
}
