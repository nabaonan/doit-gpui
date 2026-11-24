//! doit-gpui —— 基于 GPUI Kit 的 doit 桌面待办纯原生复刻
//!
//! 目标效果见项目根目录 index.html（1:1 高保真原型）。
//!
//! 本文件是库入口，把四个层次对外暴露为公开 API：
//! - [`app`]：全局状态（Entity）与根视图编排
//! - [`domain`]：数据契约 v2、任务树运算、原子持久化
//! - [`theme`]：doit 设计令牌（亮/暗双主题）
//! - [`ui`]：四页 UI 组件
//!
//! 拆成 lib + bin 的原因：领域层与设计令牌层是稳定的对外接口，
//! 单独成库后 `pub` 项不会被编译器判为死代码，同时便于后续补集成测试。

// ---------------------------------------------------------------------------
// Lint 策略
// ---------------------------------------------------------------------------
// 目标：`cargo clippy --all-targets` 零告警，且不牺牲代码可读性。
//
// 逐项说明：
// - `pedantic`：整体开启，逐条豁免下面那些与本项目风格冲突的项。
// - `unreadable_literal`：色值一律写成 `0x3FBB9C` 形式，与 CSS 变量一一对应，
//   加 `_` 分隔反而看不出对应哪个色板，故豁免。
// - `must_use_candidate`：98 处纯 getter 加 `#[must_use]` 噪音大于收益，
//   且 UI 渲染函数返回值本就不可忽略，故豁免。
// - `doc_markdown`：文档里大量中文与 `doit-backup.json` 这类文件名，
//   强制加反引号没有实际收益，故豁免。
// - `cast_*`：数值转换处均有显式边界控制，pedantic 的截断/精度告警为噪音，故豁免。
// - `wildcard_imports`：GPUI Kit 只提供 `gpui_kit::*` 统一入口，
//   拆成逐项导入反而易与上游版本脱节，故豁免。
// - `too_many_lines`：UI 渲染函数天然较长，拆分反而割裂视觉结构，故豁免。
// - `many_single_char_names`：`p`（palette）/`t`（task）是本项目刻意统一的短名约定。
#![warn(clippy::pedantic)]
#![allow(
    clippy::unreadable_literal,
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::wildcard_imports,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]

pub mod app;
pub mod domain;
pub mod sync;
pub mod theme;
pub mod ui;
