// 配置类型要被 `extension::functions` 子树里的聚合函数读到，所以这里得比同层的
// 其它 `mod` 放宽一档（模块本身仍然被私有的 `mod types;` 挡在 extension 内部）。
//
// The options type is read by the aggregates under `extension::functions`, so this module is one
// notch more visible than its siblings (the private `mod types;` still keeps the whole subtree
// inside `extension`).
pub(crate) mod html_report_options;
pub(crate) mod return_point;
