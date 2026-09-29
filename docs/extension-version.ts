// 文档站展示的扩展版本号 = 最近一次正式发布（git tag）的版本。
//
// 这是文档里版本号的唯一来源：正文（含本目录的 README）里都写占位符
// {{EXTENSION_VERSION}}，由 duckfn-docs-kit 的 remark 插件在构建时替换
// （注册处见 docusaurus.config.ts 的 remarkPlugins）。
// 发版时 `just release_bump X.Y.Z` 会连同它一起更新（scripts/release.sh 单独改这个文件）。
export const EXTENSION_VERSION = '0.1.0';
