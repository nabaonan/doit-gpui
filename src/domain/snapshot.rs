//! 快照持久化（对齐 doit-desktop 的 storage/snapshot.rs）
//!
//! - 主文件 doit-backup.json（与手机端 WebDAV 共用同名）
//! - 原子写：写临时文件 → fsync → rename → fsync 目录
//! - 写前备份轮转：保留 .bak.1 ~ .bak.3
//! - 读取：主文件损坏则依次回退备份

use super::types::SnapshotV2;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const BACKUP_COUNT: u8 = 3;

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
                Ok(s) => return Ok(s),
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
