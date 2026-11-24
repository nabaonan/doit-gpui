//! UI 组件层（1:1 复刻 doit-desktop 的视觉与交互）

pub mod icons;
pub mod topbar;
pub mod task_card;
pub mod today;
pub mod quick_add;
pub mod stats;
pub mod calendar;
pub mod settings;
pub mod ctx_menu;
pub mod cat_dialog;

/// 把同步状态映射为 (展示文案, 是否告警色)。文案与 doit-desktop StatusPills 对齐。
/// 供顶栏与设置页共用。
pub fn sync_badge(status: &str) -> (&'static str, bool) {
    match status {
        "syncing" => ("同步中…", false),
        "error" => ("同步异常", true),
        "off" => ("同步已关闭", false),
        "ready" => ("已开启暂未同步", false),
        "dirty" => ("有未同步更改 · 点击同步", true),
        _ => ("已与远程同步", false),
    }
}

/// 同步状态对应的云图标（与桌面端 cloud / cloudOk / cloudDirty 对齐）。
pub fn sync_icon(status: &str) -> fn() -> Vec<u8> {
    match status {
        "synced" => icons::cloud_ok,
        "dirty" => icons::cloud_dirty,
        _ => icons::cloud,
    }
}
