//! 快照持久化（对齐 doit-desktop 的 storage/snapshot.rs）
//!
//! - 主文件 doit-backup.json（与手机端 WebDAV 共用同名）
//! - 原子写：写临时文件 → fsync → rename → fsync 目录
//! - 写前备份轮转：保留 .bak.1 ~ .bak.3
//! - 读取：主文件损坏则依次回退备份

use super::color::normalize_category_color;
use super::types::{AppSettings, SnapshotV2, now_millis};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const BACKUP_COUNT: u8 = 3;
const SETTINGS_FILE: &str = "settings.json";
/// 上次成功同步的快照快照基线（只用于三方合并，不进入业务读取路径）
const SYNC_BASE_FILE: &str = "sync-base.json";
/// 空快照的默认设备标识（对应桌面端 `doit-desktop`）
pub const DEFAULT_DEVICE_ID: &str = "doit-gpui";

/// 对齐桌面端 `domain/snapshot.ts` 的 `normalizeSnapshot`：
/// uid 去重、priority 钳制、完成态自洽、`parentUid` 空串归 `None`、
/// 分类色归一化、非法时间/排序回退、空 `deviceId` 补默认值。
///
/// Rust 的快照是强类型结构（serde 解析失败即整份拒绝），因此这里主要处理
/// 能通过解析但语义不合法的历史值：重复 uid、越界枚举、空串与负数。
#[must_use]
pub fn normalize_snapshot(s: SnapshotV2) -> SnapshotV2 {
    let now = now_millis();
    let mut seen = HashSet::new();
    let mut tasks = Vec::with_capacity(s.tasks.len());
    for (i, mut t) in s.tasks.into_iter().enumerate() {
        if !seen.insert(t.uid.clone()) {
            continue;
        }
        if t.priority > 4 {
            t.priority = 0;
        }
        if !t.done || t.completed_at.is_some_and(|c| c <= 0) {
            t.completed_at = None;
        }
        if t.parent_uid.as_deref().is_some_and(str::is_empty) {
            t.parent_uid = None;
        }
        if t.created_at <= 0 {
            t.created_at = now;
        }
        if t.updated_at <= 0 {
            t.updated_at = now;
        }
        if t.deleted_at.is_some_and(|d| d <= 0) {
            t.deleted_at = None;
        }
        if t.sort_order < 0 {
            t.sort_order = i as i64;
        }
        tasks.push(t);
    }

    let categories = s
        .categories
        .into_iter()
        .enumerate()
        .map(|(i, mut c)| {
            c.color = normalize_category_color(&c.color);
            if c.sort_order < 0 {
                c.sort_order = i as i64;
            }
            if c.updated_at <= 0 {
                c.updated_at = now;
            }
            if c.deleted_at.is_some_and(|d| d <= 0) {
                c.deleted_at = None;
            }
            c
        })
        .collect();

    SnapshotV2 {
        version: 2,
        revision: s.revision,
        exported_at: if s.exported_at <= 0 { now } else { s.exported_at },
        device_id: if s.device_id.trim().is_empty() {
            DEFAULT_DEVICE_ID.to_string()
        } else {
            s.device_id
        },
        last_sync_at: s.last_sync_at.filter(|v| *v > 0),
        categories,
        tasks,
    }
}

/// 快照存储
pub struct SnapshotStore {
    dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn main_path(&self) -> PathBuf {
        self.dir.join("doit-backup.json")
    }
    fn backup_path(&self, n: u8) -> PathBuf {
        self.dir.join(format!("doit-backup.json.bak.{n}"))
    }
    fn settings_path(&self) -> PathBuf {
        self.dir.join(SETTINGS_FILE)
    }
    fn sync_base_path(&self) -> PathBuf {
        self.dir.join(SYNC_BASE_FILE)
    }

    /// 读取快照：主文件优先，损坏则回退备份，全损坏返回 Err
    ///
    /// # Errors
    /// 当主文件与全部备份都无法解析为合法快照时返回错误描述。
    pub fn load(&self) -> Result<SnapshotV2, String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let candidates = std::iter::once(self.main_path())
            .chain((1..=BACKUP_COUNT).map(|n| self.backup_path(n)));
        let mut last_err = String::new();
        for path in candidates {
            if !path.exists() {
                continue;
            }
            match read_json::<SnapshotV2>(&path) {
                Ok(s) => return Ok(normalize_snapshot(s)),
                Err(e) => last_err = format!("{}: {}", path.display(), e),
            }
        }
        // 没有任何文件：返回空快照（首次启动）
        if last_err.is_empty() {
            return Ok(SnapshotV2::default());
        }
        Err(format!("快照及备份均损坏，最后错误：{last_err}"))
    }

    /// 原子保存（含备份轮转）
    ///
    /// # Errors
    /// 当目录不可创建、序列化失败或临时文件写入失败时返回错误描述。
    pub fn save(&self, snapshot: &SnapshotV2) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let main = self.main_path();

        // 1) 备份轮转：.bak.3 删除，.bak.2→.bak.3，.bak.1→.bak.2，当前→.bak.1
        if self.backup_path(BACKUP_COUNT).exists() {
            let _ = fs::remove_file(self.backup_path(BACKUP_COUNT));
        }
        for n in (1..BACKUP_COUNT).rev() {
            let from = self.backup_path(n);
            let to = self.backup_path(n + 1);
            if from.exists() {
                let _ = fs::rename(&from, &to);
            }
        }
        if main.exists() {
            let _ = fs::copy(&main, self.backup_path(1));
        }

        // 2) 序列化
        let json = serde_json::to_vec_pretty(snapshot).map_err(|e| e.to_string())?;

        // 3) 原子写临时文件
        let tmp = self.dir.join(format!(
            "doit-backup.json.{}.{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        {
            let mut f = File::create(&tmp).map_err(|e| e.to_string())?;
            f.write_all(&json).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        // 4) rename 覆盖主文件
        fs::rename(&tmp, &main).map_err(|e| e.to_string())?;

        // 5) fsync 目录（Unix）确保 rename 落盘
        #[cfg(unix)]
        {
            if let Ok(dir) = File::open(&self.dir) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    }

    /// 读取应用设置：文件不存在时回退到默认值。
    ///
    /// # Errors
    /// 当设置文件存在但无法解析为合法设置时返回错误描述。
    pub fn load_settings(&self) -> Result<AppSettings, String> {
        let path = self.settings_path();
        if !path.exists() {
            return Ok(AppSettings::default());
        }
        read_json::<AppSettings>(&path)
    }

    /// 写入应用设置（原子写：临时文件 → rename 覆盖）。
    ///
    /// # Errors
    /// 当目录不可创建、序列化失败或写入失败时返回错误描述。
    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        let tmp = self.dir.join(format!(
            "settings.json.{}.{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        {
            let mut f = File::create(&tmp).map_err(|e| e.to_string())?;
            f.write_all(&json).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp, self.settings_path()).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            if let Ok(dir) = File::open(&self.dir) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    }

    /// 读取同步基线：文件不存在或解析失败时返回 None（首次同步）。
    pub fn load_sync_base(&self) -> Option<SnapshotV2> {
        let path = self.sync_base_path();
        if !path.exists() {
            return None;
        }
        read_json::<SnapshotV2>(&path).ok().map(normalize_snapshot)
    }

    /// 写入同步基线（原子写），仅记录上次成功同步的快照。
    ///
    /// # Errors
    /// 当目录不可创建、序列化失败或写入失败时返回错误描述。
    pub fn save_sync_base(&self, snapshot: &SnapshotV2) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(snapshot).map_err(|e| e.to_string())?;
        let tmp = self.dir.join(format!(
            "sync-base.json.{}.{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        {
            let mut f = File::create(&tmp).map_err(|e| e.to_string())?;
            f.write_all(&json).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp, self.sync_base_path()).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            if let Ok(dir) = File::open(&self.dir) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    }

    /// 主文件路径（供 UI 显示）
    pub fn path(&self) -> PathBuf {
        self.main_path()
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let mut s = String::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// 默认存储目录：~/.local/share/doit-gpui（macOS 用 Application Support）
pub fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = dirs::home_dir().map(|h| h.join("Library/Application Support/doit-gpui"));
    #[cfg(target_os = "linux")]
    let base = dirs::data_local_dir().map(|d| d.join("doit-gpui"));
    #[cfg(target_os = "windows")]
    let base = dirs::data_local_dir().map(|d| d.join("doit-gpui"));
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let base: Option<PathBuf> = None;

    base.unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("doit-gpui-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn settings_roundtrip_persists_webdav_config() {
        let dir = temp_dir("settings-roundtrip");
        let store = SnapshotStore::new(&dir);

        // 无文件时回退默认值
        let mut s = store.load_settings().unwrap();
        assert_eq!(s.theme, "light");

        s.webdav.enabled = true;
        s.webdav.url = "https://dav.example.com/doit/".into();
        s.webdav.username = "alice".into();
        s.webdav.password_ref = Some("app-password".into());
        store.save_settings(&s).unwrap();

        // 新实例重新读取，验证确实落盘
        let store2 = SnapshotStore::new(&dir);
        let loaded = store2.load_settings().unwrap();
        assert_eq!(loaded.webdav.enabled, true);
        assert_eq!(loaded.webdav.url, "https://dav.example.com/doit/");
        assert_eq!(loaded.webdav.username, "alice");
        assert_eq!(loaded.webdav.password_ref.as_deref(), Some("app-password"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_settings_missing_file_returns_default() {
        let dir = temp_dir("settings-missing");
        let store = SnapshotStore::new(&dir);
        let s = store.load_settings().unwrap();
        assert_eq!(s, AppSettings::default());
    }

    #[test]
    fn sync_base_roundtrip() {
        let dir = temp_dir("sync-base");
        let store = SnapshotStore::new(&dir);
        assert!(store.load_sync_base().is_none());
        let mut snap = SnapshotV2::default();
        snap.revision = 7;
        snap.last_sync_at = Some(1234);
        store.save_sync_base(&snap).unwrap();

        let store2 = SnapshotStore::new(&dir);
        let loaded = store2.load_sync_base().unwrap();
        assert_eq!(loaded.revision, 7);
        assert_eq!(loaded.last_sync_at, Some(1234));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn normalize_snapshot_sanitizes_legacy_values() {
        use super::super::types::{Category, Task};

        let mut snap = SnapshotV2 {
            device_id: String::new(),
            ..SnapshotV2::default()
        };
        snap.categories.push(Category {
            name: "工作".into(),
            color: "5b9bd5".into(),
            sort_order: -1,
            updated_at: 0,
            deleted_at: Some(0),
        });
        let base_task = Task {
            uid: "dup".into(),
            parent_uid: Some(String::new()),
            title: "t".into(),
            category: "工作".into(),
            priority: 9,
            done: false,
            completed_at: Some(100),
            created_at: 0,
            updated_at: 0,
            deleted_at: Some(-1),
            depth: 0,
            sort_order: -1,
        };
        let mut dup = base_task.clone();
        dup.title = "重复".into();
        snap.tasks = vec![base_task, dup];

        let n = normalize_snapshot(snap);
        assert_eq!(n.device_id, DEFAULT_DEVICE_ID);
        assert_eq!(n.categories[0].color, "#5B9BD5");
        assert_eq!(n.categories[0].sort_order, 0);
        assert_eq!(n.categories[0].deleted_at, None);
        assert!(n.categories[0].updated_at > 0);
        assert_eq!(n.tasks.len(), 1, "重复 uid 应去重");
        assert_eq!(n.tasks[0].priority, 0);
        assert_eq!(n.tasks[0].parent_uid, None);
        assert_eq!(n.tasks[0].completed_at, None);
        assert_eq!(n.tasks[0].deleted_at, None);
        assert!(n.tasks[0].created_at > 0 && n.tasks[0].updated_at > 0);
    }
}
