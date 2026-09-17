use gpui_kit::gpui::SharedString;

/// Data types for the Doit application.

/// A single todo item.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: SharedString,
    pub completed: bool,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub order: u32,
    pub tag_id: Option<String>,
    pub cat_id: Option<String>,
    pub parent_id: Option<String>,
    pub remind_at: Option<String>,
}

/// A tag/label that can be attached to todos.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: SharedString,
    pub color: String,
}

/// A category for grouping todos.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Category {
    pub id: String,
    pub name: SharedString,
    pub color: String,
}

/// Shortcut key configuration.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ShortcutConfig {
    pub key: SharedString,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Schedule configuration for auto backup/restore.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScheduleConfig {
    pub enabled: bool,
    pub interval: u32,
    pub unit: SharedString,
}

/// Cloud sync configuration (WebDAV).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CloudSyncConfig {
    pub enabled: bool,
    /// Accept self-signed / privately-signed certificates (for intranet NAS).
    pub trust_self_signed: bool,
    pub provider: SharedString,
    pub webdav_url: SharedString,
    pub webdav_username: SharedString,
    pub webdav_password: SharedString,
    pub fetch_on_startup: bool,
    pub upload_on_exit: bool,
    pub keep_recent: u32,
}

/// Full application settings.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub completion_mode: SharedString,
    pub long_press_duration: u32,
    pub theme: SharedString,
    pub add_todo_shortcut: ShortcutConfig,
    pub tags: Vec<Tag>,
    pub categories: Vec<Category>,
    pub default_category_id: Option<String>,
    pub cloud_sync: CloudSyncConfig,
    pub auto_backup: ScheduleConfig,
    pub auto_restore: ScheduleConfig,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            completion_mode: "checkbox".into(),
            long_press_duration: 3,
            theme: "system".into(),
            add_todo_shortcut: ShortcutConfig {
                key: "Enter".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
            },
            tags: vec![
                Tag { id: "tag-w".into(), name: "工作".into(), color: "#3B82F6".into() },
                Tag { id: "tag-p".into(), name: "个人".into(), color: "#22C55E".into() },
            ],
            categories: vec![
                Category { id: "cat-work".into(), name: "工作".into(), color: "#3B82F6".into() },
                Category { id: "cat-life".into(), name: "生活".into(), color: "#22C55E".into() },
            ],
            default_category_id: Some("cat-work".into()),
            cloud_sync: CloudSyncConfig {
                enabled: false,
                trust_self_signed: false,
                provider: "webdav".into(),
                webdav_url: "".into(),
                webdav_username: "".into(),
                webdav_password: "".into(),
                fetch_on_startup: true,
                upload_on_exit: true,
                keep_recent: 3,
            },
            auto_backup: ScheduleConfig {
                enabled: false,
                interval: 30,
                unit: "minute".into(),
            },
            auto_restore: ScheduleConfig {
                enabled: false,
                interval: 30,
                unit: "minute".into(),
            },
        }
    }
}

/// The four main view modes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Today,
    Calendar,
    Timeline,
    Stats,
}

impl ViewMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Today => "今日待办",
            Self::Calendar => "日历浏览",
            Self::Timeline => "时间轴",
            Self::Stats => "统计",
        }
    }

    pub fn index(&self) -> u32 {
        match self {
            Self::Today => 0,
            Self::Calendar => 1,
            Self::Timeline => 2,
            Self::Stats => 3,
        }
    }
}

/// Filter for which category's todos to show.
#[derive(Clone, Debug, PartialEq)]
pub enum CatFilter {
    /// Show un-categorized items.
    None,
    /// Show items in a specific category.
    Id(String),
}

/// A portable snapshot of all user data, used for WebDAV sync (upload/download).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SyncSnapshot {
    /// Snapshot format version.
    pub version: u32,
    /// Local export time (ISO-8601).
    pub exported_at: String,
    pub todos: Vec<TodoItem>,
    pub settings: AppSettings,
}

/// The file WebDAV sync reads and writes.
pub const SYNC_FILE: &str = "doit-snapshot.json";

/// Built-in colour palette shared by tags & categories; defaults for newly
/// created labels and the fallback palette the original app offered.
pub const COLOR_PALETTE: &[(&str, &str)] = &[
    ("red", "#EF4444"),
    ("orange", "#F97316"),
    ("amber", "#F59E0B"),
    ("lime", "#84CC16"),
    ("green", "#22C55E"),
    ("teal", "#14B8A6"),
    ("cyan", "#06B6D4"),
    ("blue", "#3B82F6"),
    ("indigo", "#6366F1"),
    ("purple", "#A855F7"),
    ("fuchsia", "#D946EF"),
    ("pink", "#EC4899"),
];
