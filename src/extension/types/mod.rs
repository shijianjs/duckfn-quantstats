// 配置类型要被 `extension::functions` 子树里的聚合函数读到，报告结果行类型则是聚合的输出，
// 翻译类型（写入口与读出口）由 `translation` 子树用到，所以这里得比同层的其它 `mod` 放宽一档
// （模块本身仍然被私有的 `mod types;` 挡在 extension 内部）。
//
// The options type is read by the aggregates under `extension::functions` and the report row type is
// one of their outputs; the translation types (the write-side entry and the read-side row) are used by the
// `translation` subtree, so this module is one notch more visible than its siblings (the private
// `mod types;` still keeps the whole subtree inside `extension`).
pub(crate) mod html_report;
pub(crate) mod html_report_options;
pub(crate) mod translation;
