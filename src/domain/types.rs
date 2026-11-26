//! 领域类型（1:1 对齐 doit-desktop 的 domain/types.ts 与数据契约 v2）
//!
//! 与手机端 doit-apk、桌面端 doit-desktop 共用同一份快照结构，
//! 保证 WebDAV 同步时三方数据结构一致。

use chrono::Datelike;
use serde::{Deserialize, Serialize};

/// 优先级：0 不设 / 1 低 / 2 普通 / 3 高 / 4 紧急
pub type Priority = u8;

/// 任务（扁平存储，parent_uid 表达层级）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub uid: String,
    pub parent_uid: Option<String>,
    pub title: String,
    /// 分类名（引用 Category.name）
    pub category: String,
    pub priority: Priority,
    pub done: bool,
    /// 完成日期（当日 00:00 时间戳）；未完成为 None
    pub completed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// 墓碑删除时间；None 表示未删除
    pub deleted_at: Option<i64>,
    /// 子任务层级，顶级为 0
    pub depth: u32,
    /// 排序权重，-1 表示新建置顶
    pub sort_order: i64,
}

/// 分类（扁平列表，颜色为 hex 字符串如 #3FBB9C）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    pub color: String,
    pub sort_order: i64,
    pub updated_at: i64,
    /// 删除墓碑（与手机端/桌面端契约一致）：同步合并据此识别删除，避免被旧快照复活。
    /// 旧快照没有该字段，用 default 兼容。
    #[serde(default)]
    pub deleted_at: Option<i64>,
}

/// 快照 v2（与手机/桌面端共用的 doit-backup.json 结构）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotV2 {
    pub version: u32,
    pub revision: u64,
    pub exported_at: i64,
    pub device_id: String,
    pub last_sync_at: Option<i64>,
    pub categories: Vec<Category>,
    pub tasks: Vec<Task>,
}

impl Default for SnapshotV2 {
    fn default() -> Self {
        Self {
            version: 2,
            revision: 0,
            exported_at: now_millis(),
            device_id: "doit-gpui".to_string(),
            last_sync_at: None,
            categories: Vec::new(),
            tasks: Vec::new(),
        }
    }
}

/// 内存中的任务树节点（由扁平 tasks 构建，供 UI 渲染）
#[derive(Debug, Clone)]
pub struct TaskNode {
    pub task: Task,
    pub subs: Vec<TaskNode>,
}

impl TaskNode {
    /// 是否含仍未删除的子任务
    pub fn has_live_subs(&self) -> bool {
        self.subs.iter().any(|s| s.task.deleted_at.is_none())
    }
    /// 有效叶子子任务数量与完成数（递归只数叶子）
    pub fn sub_leaves(&self) -> (usize, usize) {
        let mut total = 0;
        let mut done = 0;
        for sub in &self.subs {
            if sub.task.deleted_at.is_some() {
                continue;
            }
            if sub.has_live_subs() {
                let (t, d) = sub.sub_leaves();
                total += t;
                done += d;
            } else {
                total += 1;
                if sub.task.done {
                    done += 1;
                }
            }
        }
        (total, done)
    }
    /// 子任务完成百分比（0-100）；无子任务返回 None
    pub fn sub_progress(&self) -> Option<u32> {
        if !self.has_live_subs() {
            return None;
        }
        let (total, done) = self.sub_leaves();
        if total == 0 {
            return None;
        }
        Some(((done as f32 / total as f32) * 100.0).round() as u32)
    }
}

/// 应用设置（对应 AppSettings）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,           // "light" | "dark"
    pub skin: String,            // "mint" | "soft"
    pub progress_color: String,  // hex，如 #3FBB9C
    pub webdav: WebDavSettings,
    /// 今日待办「紧急优先」：只重排顶级任务（4→0），子任务顺序不变。
    /// 本地 UI 偏好，不进同步快照。
    #[serde(default)]
    pub sort_by_priority: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            skin: "mint".into(),
            progress_color: "#3FBB9C".into(),
            webdav: WebDavSettings::default(),
            sort_by_priority: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebDavSettings {
    pub enabled: bool,
    pub url: String,
    pub username: String,
    /// 仅存凭据引用（如 doit-webdav:{username}），明文存系统钥匙串
    pub password_ref: Option<String>,
    pub root_path: Option<String>,
}

/// 当前毫秒时间戳
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// 当日 00:00 的时间戳（本地时区，与 doit-desktop 的 startOfDay 一致）
pub fn start_of_today() -> i64 {
    day_start_ts(chrono::Local::now().date_naive())
}

/// 本地日期当天的 00:00 时间戳
pub fn day_start_ts(d: chrono::NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0)
        .and_then(|n| n.and_local_timezone(chrono::Local).single())
        .map_or_else(now_millis, |d| d.timestamp_millis())
}

/// 时间戳 → 本地日期
pub fn local_date(ts: i64) -> chrono::NaiveDate {
    chrono::DateTime::from_timestamp_millis(ts)
        .map_or_else(|| chrono::Local::now().date_naive(), |dt| {
            dt.with_timezone(&chrono::Local).date_naive()
        })
}

/// 本地日期 + n 天
pub fn add_days(d: chrono::NaiveDate, n: i64) -> chrono::NaiveDate {
    d + chrono::Duration::days(n)
}

/// 年月偏移（delta: -1 上一月 / +1 下一月），自动跨年
pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let mut y = year;
    let mut m = month as i32 + delta;
    while m < 1 {
        m += 12;
        y -= 1;
    }
    while m > 12 {
        m -= 12;
        y += 1;
    }
    (y, m as u32)
}

/// 某年某月的天数
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default();
    let next = match month {
        12 => chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1),
        m => chrono::NaiveDate::from_ymd_opt(year, m + 1, 1),
    }
    .unwrap_or(first);
    (next - first).num_days() as u32
}

/// 某年某月的最后一天
pub fn last_day_of_month(year: i32, month: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(year, month, days_in_month(year, month)).unwrap_or_default()
}

/// 周一=0 … 周日=6
pub fn weekday_mon0(d: chrono::NaiveDate) -> u32 {
    d.weekday().num_days_from_monday()
}

/// 所在周的周一（周一为一周之首）
pub fn start_of_week(d: chrono::NaiveDate) -> chrono::NaiveDate {
    d - chrono::Duration::days(i64::from(weekday_mon0(d)))
}

/// 两个日期之间相差的天数（b - a）
pub fn days_between(a: chrono::NaiveDate, b: chrono::NaiveDate) -> i64 {
    (b - a).num_days()
}

/// 月历网格：返回当月每一天的本地日期（按周日开头排列，空位为 None）
pub fn month_grid(year: i32, month: u32) -> Vec<Option<chrono::NaiveDate>> {
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default();
    // chrono weekday: Mon=0..Sun=6；日历首列为周日，故 (weekday+1)%7
    let lead = ((first.weekday().num_days_from_monday() + 1) % 7) as usize;
    let total = days_in_month(year, month);
    (0..42i32)
        .map(|i| {
            let day = i - lead as i32 + 1;
            if day < 1 || day > total as i32 {
                None
            } else {
                chrono::NaiveDate::from_ymd_opt(year, month, day as u32)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_month_crosses_year_boundary() {
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 11, 1), (2026, 12));
        assert_eq!(shift_month(2026, 11, -1), (2026, 10));
    }

    #[test]
    fn days_in_month_handles_leap_year() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 11), 30);
    }

    #[test]
    fn month_grid_starts_on_sunday() {
        // 2026-11-01 是星期日 → 周日列为第一格
        let grid = month_grid(2026, 11);
        assert_eq!(
            grid[0],
            Some(chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap())
        );
        assert_eq!(grid.iter().filter(|c| c.is_some()).count(), 30);
        assert_eq!(grid.iter().filter(|c| c.is_none()).count(), 12);
    }

    #[test]
    fn month_grid_leading_blanks_for_monday_start() {
        // 2028-02-01 是星期二 → 周日、周一两格留空
        let grid = month_grid(2028, 2);
        assert_eq!(grid[0], None);
        assert_eq!(grid[1], None);
        assert_eq!(
            grid[2],
            Some(chrono::NaiveDate::from_ymd_opt(2028, 2, 1).unwrap())
        );
        assert_eq!(grid.iter().filter(|c| c.is_some()).count(), 29);
    }

    #[test]
    fn day_start_roundtrip() {
        let today = chrono::Local::now().date_naive();
        assert_eq!(local_date(day_start_ts(today)), today);
        assert_eq!(local_date(start_of_today()), today);
    }

    #[test]
    fn add_days_moves_across_month() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 30).unwrap();
        assert_eq!(add_days(d, 1), chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(add_days(d, -30), chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    }

    #[test]
    fn start_of_week_returns_monday() {
        // 2026-11-22 是星期日 → 所在周的周一为 11-16
        let sun = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        assert_eq!(start_of_week(sun), chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap());
        // 2026-11-16 本身是周一
        let mon = chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        assert_eq!(start_of_week(mon), mon);
        assert_eq!(weekday_mon0(mon), 0);
        assert_eq!(weekday_mon0(sun), 6);
    }

    #[test]
    fn last_day_of_month_handles_leap_year() {
        assert_eq!(
            last_day_of_month(2028, 2),
            chrono::NaiveDate::from_ymd_opt(2028, 2, 29).unwrap()
        );
        assert_eq!(
            last_day_of_month(2026, 2),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
        assert_eq!(
            last_day_of_month(2026, 12),
            chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()
        );
    }

    #[test]
    fn days_between_counts_calendar_days() {
        let a = chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let b = chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(days_between(a, b), 30);
        assert_eq!(days_between(b, a), -30);
    }
}
