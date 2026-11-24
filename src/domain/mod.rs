//! 领域层：类型、任务树、持久化（对齐 doit-desktop 的 domain/ + storage/）

pub mod types;
pub mod color;
pub mod task_tree;
pub mod snapshot;
pub mod seed;

#[allow(unused_imports)]
pub use color::{normalize_category_color, parse_color_or_default, DEFAULT_CATEGORY_COLOR};
#[allow(unused_imports)]
pub use types::{AppSettings, Category, SnapshotV2, Task, TaskNode, WebDavSettings};
