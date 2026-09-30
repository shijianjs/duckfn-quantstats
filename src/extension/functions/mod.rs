mod aggregate_html;

// 翻译子树的可见性比同层高一点：`extension/mod.rs` 的加载期初始化（`#[duck_custom_register]`）要调
// `translation::table::install_builtin`，而那是本模块的**父**模块，`pub(super)` 到不了那儿。
//
// The translation subtree is one notch more visible than its siblings: the load-time initialisation in
// `extension/mod.rs` (the `#[duck_custom_register]`) calls `translation::table::install_builtin`, and that is
// this module's **parent**, which `pub(super)` cannot reach.
pub(crate) mod translation;
