//! WebDAV 同步：网络层 + 三方合并 + 编排。
//!
//! 协议与手机端 / 桌面端完全一致：
//! - 基地址 = url 去尾斜杠 + rootPath，统一以 '/' 结尾；
//! - 远端快照文件名 `doit-backup.json`（与本地主文件同名）；
//! - PROPFIND 探测目录与鉴权，GET 拉取快照（404 视为远端无文件），PUT 上传快照；
//! - Basic Auth。
//!
//! 合并算法移植自桌面端 `domain/merge.ts` 的 merge3：
//! 并集保留、字段级合并、同字段冲突按 effectiveTime 取新（平局远程优先）、
//! 墓碑删除、陈旧远端检测。不依赖系统时间（now 由调用方注入）以便单测。

use crate::domain::snapshot::normalize_snapshot;
use crate::domain::types::{Category, SnapshotV2, Task, WebDavSettings};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

const BACKUP_NAME: &str = "doit-backup.json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8" ?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#;

/// 同步结果：返回合并后的新快照与是否拉取到远端数据。
#[derive(Debug, Clone)]
pub struct SyncOutcome {
    pub snapshot: SnapshotV2,
    /// true = 远端有文件并已拉取合并；false = 远端无文件，仅上传本地
    pub pulled: bool,
}

/// 归一化 WebDAV 基地址：url 去尾斜杠 + rootPath 子路径，并保证以 '/' 结尾。
///
/// # Errors
/// 地址为空或不以 http(s):// 开头时返回错误描述。
pub fn normalize_base(settings: &WebDavSettings) -> Result<String, String> {
    let url = settings.url.trim();
    if url.is_empty() {
        return Err("未配置 WebDAV 地址".into());
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("WebDAV 地址必须以 http(s):// 开头".into());
    }
    let mut base = url.trim_end_matches('/').to_string();
    let root = settings
        .root_path
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_matches('/');
    if !root.is_empty() {
        base.push('/');
        base.push_str(root);
    }
    base.push('/');
    Ok(base)
}

fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(REQUEST_TIMEOUT)
        .timeout_connect(CONNECT_TIMEOUT)
        .build()
}

/// 构造 Basic Auth 请求头（ureq 2.x 无内置 auth 辅助方法）。
fn auth_header(user: &str, pass: &str) -> String {
    use base64::Engine;
    let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
    format!("Basic {token}")
}

/// 把底层网络错误分类成安全文案：绝不使用 e.to_string()（可能含 URL / userinfo）。
fn classify(e: &ureq::Error) -> String {
    if matches!(e, ureq::Error::Status(401 | 403, _)) {
        "鉴权失败，请检查账号和密码".into()
    } else if matches!(e, ureq::Error::Status(404, _)) {
        "目录不存在 (HTTP 404)".into()
    } else if let ureq::Error::Status(code, _) = e {
        format!("WebDAV 请求失败 (HTTP {code})")
    } else {
        match e.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::ProxyConnect => {
                "无法连接 WebDAV 服务器".into()
            }
            _ => "WebDAV 网络请求失败（可能超时）".into(),
        }
    }
}

/// PROPFIND 目录探测 + 鉴权探测。
fn probe_root(agent: &ureq::Agent, base: &str, user: &str, pass: &str) -> Result<(), String> {
    agent
        .request("PROPFIND", base)
        .set("Depth", "0")
        .set("Content-Type", "application/xml; charset=utf-8")
        .set("Authorization", &auth_header(user, pass))
        .send_string(PROPFIND_BODY)
        .map(|_| ())
        .map_err(|e| classify(&e))
}

/// GET 远端快照；404 表示服务器尚无文件（返回 None）。
fn get_remote(
    agent: &ureq::Agent,
    base: &str,
    user: &str,
    pass: &str,
) -> Result<Option<SnapshotV2>, String> {
    let url = format!("{base}{BACKUP_NAME}");
    match agent
        .get(&url)
        .set("Authorization", &auth_header(user, pass))
        .call()
    {
        Ok(resp) => {
            let text = resp.into_string().map_err(|_| "读取远端快照失败".to_string())?;
            let snap = serde_json::from_str::<SnapshotV2>(&text)
                .map_err(|_| "远端快照不是合法的 doit-backup.json".to_string())?;
            Ok(Some(snap))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(classify(&e)),
    }
}

/// PUT 快照到 WebDAV（幂等覆盖）。
fn put_remote(
    agent: &ureq::Agent,
    base: &str,
    user: &str,
    pass: &str,
    snapshot: &SnapshotV2,
) -> Result<(), String> {
    let url = format!("{base}{BACKUP_NAME}");
    let bytes = serde_json::to_vec(snapshot).map_err(|_| "快照序列化失败".to_string())?;
    agent
        .put(&url)
        .set("Authorization", &auth_header(user, pass))
        .set("Content-Type", "application/json; charset=utf-8")
        .send(&bytes[..])
        .map(|_| ())
        .map_err(|e| classify(&e))
}

/// 真实的「测试连接」：PROPFIND 探测目录 + 鉴权，返回延迟毫秒。
///
/// # Errors
/// 配置缺失、鉴权失败、网络失败时返回安全文案。
pub fn test_connection(settings: &WebDavSettings, password: &str) -> Result<u128, String> {
    if settings.url.trim().is_empty() {
        return Err("请先填写服务器地址".into());
    }
    if settings.username.trim().is_empty() {
        return Err("请先填写账号".into());
    }
    if password.trim().is_empty() {
        return Err("请先填写密码 / 令牌".into());
    }
    let agent = build_agent();
    let base = normalize_base(settings)?;
    let start = std::time::Instant::now();
    probe_root(&agent, &base, settings.username.trim(), password.trim())?;
    Ok(start.elapsed().as_millis())
}

/// 执行一次完整同步：探测 → 拉取 → 合并 → 上传 → 返回结果快照。
/// 本地快照在合并后由调用方写盘（本函数不碰文件系统，便于测试）。
///
/// # Errors
/// 配置缺失、鉴权失败、网络失败、远端快照非法时返回安全文案。
pub fn sync_once(
    settings: &WebDavSettings,
    password: &str,
    local: &SnapshotV2,
    base_snapshot: Option<&SnapshotV2>,
    now: i64,
) -> Result<SyncOutcome, String> {
    if !settings.enabled {
        return Err("同步未启用".into());
    }
    let agent = build_agent();
    let base = normalize_base(settings)?;
    let user = settings.username.trim();
    let pass = password.trim();
    if user.is_empty() || pass.is_empty() {
        return Err("请先填写账号和密码".into());
    }

    probe_root(&agent, &base, user, pass)?;
    let remote = get_remote(&agent, &base, user, pass)?;
    let pulled = remote.is_some();

    // 合并前统一归一化本地 / 远端 / 基线（对齐桌面端 normalizeSnapshot）：
    // 分类色补 # 转大写、重复 uid、priority 越界等历史脏数据不进入合并。
    let local_norm = normalize_snapshot(local.clone());
    let remote_norm = remote.as_ref().map(|s| normalize_snapshot(s.clone()));
    let base_norm = base_snapshot.map(|s| normalize_snapshot(s.clone()));

    let merged = if let Some(remote_snap) = &remote_norm {
        // 陈旧远端短路由 merge3 首行 is_stale_remote 完成：跳过合并，直接以本地为准
        merge3(&local_norm, remote_snap, base_norm.as_ref(), now, None)
    } else {
        // 远端无文件：首次同步，直接以本地为准上传
        let mut s = local_norm.clone();
        s.last_sync_at = Some(now);
        s.exported_at = now;
        s
    };

    put_remote(&agent, &base, user, pass, &merged)?;
    Ok(SyncOutcome {
        snapshot: merged,
        pulled,
    })
}

// ---------------- 三方合并（移植桌面端 merge.ts） ----------------

/// 任务的有效时间戳：取更新 / 完成 / 删除 / 创建的最大值。
pub fn effective_time(t: &Task) -> i64 {
    t.updated_at
        .max(t.completed_at.unwrap_or(0))
        .max(t.deleted_at.unwrap_or(0))
        .max(t.created_at)
}

/// 陈旧远端检测：远端导出时间更早且 revision 不高于本地。
pub fn is_stale_remote(remote: &SnapshotV2, local: &SnapshotV2) -> bool {
    remote.exported_at < local.exported_at && remote.revision <= local.revision
}

/// 字段级合并：仅一边相对 base 变化取变化方；两边都变且不同取 effectiveTime 较新者（平局远程）。
fn merge_field<T: PartialEq + Clone>(
    lv: T,
    rv: T,
    bv: Option<T>,
    local_eff: i64,
    remote_eff: i64,
) -> T {
    let (local_changed, remote_changed) = match &bv {
        Some(b) => (lv != *b, rv != *b),
        None => (true, true),
    };
    if local_changed && !remote_changed {
        lv
    } else if !local_changed && remote_changed {
        rv
    } else if local_changed && remote_changed && lv != rv {
        if local_eff > remote_eff {
            lv
        } else {
            rv
        }
    } else {
        bv.unwrap_or(lv)
    }
}

fn merge_task(local: &Task, remote: &Task, base: Option<&Task>) -> Task {
    let le = effective_time(local);
    let re = effective_time(remote);
    let mut out = local.clone();

    macro_rules! f {
        ($field:ident) => {
            out.$field = merge_field(
                local.$field.clone(),
                remote.$field.clone(),
                base.map(|b| b.$field.clone()),
                le,
                re,
            );
        };
    }
    f!(parent_uid);
    f!(title);
    f!(category);
    f!(priority);
    f!(done);
    f!(completed_at);
    f!(created_at);
    f!(deleted_at);
    f!(depth);
    f!(sort_order);

    // 墓碑 vs 编辑：删除之后对方又编辑过 → 撤销删除（编辑胜），否则删除生效。
    let (ld, rd) = (local.deleted_at, remote.deleted_at);
    if ld.is_some() || rd.is_some() {
        let edited_after_delete = (ld.is_some() && rd.is_none() && re > ld.unwrap_or(0))
            || (rd.is_some() && ld.is_none() && le > rd.unwrap_or(0));
        out.deleted_at = if edited_after_delete {
            None
        } else {
            ld.max(rd)
        };
    }

    // 完成态与完成时间必须自洽
    if !out.done {
        out.completed_at = None;
    } else if out.completed_at.is_none() {
        out.completed_at = Some(le.max(re));
    }
    out.updated_at = le.max(re);
    out
}

fn merge_category(local: &Category, remote: &Category, base: Option<&Category>) -> Category {
    let (ld, rd) = (local.deleted_at, remote.deleted_at);
    if ld.is_some() || rd.is_some() {
        if ld.is_some() && rd.is_some() {
            let newer = if ld.unwrap_or(0) >= rd.unwrap_or(0) { local } else { remote };
            let mut c = newer.clone();
            c.deleted_at = ld.max(rd);
            c.updated_at = local.updated_at.max(remote.updated_at);
            return c;
        }
        let dead = if ld.is_some() { local } else { remote };
        let live = if ld.is_some() { remote } else { local };
        let dead_at = dead.deleted_at.unwrap_or(0);
        // 本地是否有真实内容编辑（颜色/排序相对 base 变化）——不看 updatedAt
        let live_edited = base.is_some_and(|b| {
            live.color != b.color || live.sort_order != b.sort_order
        });
        let delete_wins = !live_edited || live.updated_at <= dead_at;
        if delete_wins {
            let mut c = dead.clone();
            c.deleted_at = Some(dead_at);
            c.updated_at = local.updated_at.max(remote.updated_at);
            return c;
        }
        // 删除之后真正编辑过 → 编辑胜，恢复活记录
        let mut c = live.clone();
        c.deleted_at = None;
        c.updated_at = local.updated_at.max(remote.updated_at);
        return c;
    }

    let le = local.updated_at;
    let re = remote.updated_at;
    let mut out = local.clone();
    out.color = merge_field(
        local.color.clone(),
        remote.color.clone(),
        base.map(|b| b.color.clone()),
        le,
        re,
    );
    out.sort_order = merge_field(
        local.sort_order,
        remote.sort_order,
        base.map(|b| b.sort_order),
        le,
        re,
    );
    out.updated_at = le.max(re);
    out.deleted_at = None;
    out
}

/// 手机端同款三方合并：并集保留、字段级合并、冲突按 effectiveTime 取新（平局远程优先）、
/// 墓碑删除、陈旧远端检测。纯函数，不依赖系统时间。
pub fn merge3(
    local: &SnapshotV2,
    remote: &SnapshotV2,
    base: Option<&SnapshotV2>,
    now: i64,
    device_id: Option<&str>,
) -> SnapshotV2 {
    // 陈旧远端：远端明显更旧时直接以本地为准，避免把旧数据合回来
    if is_stale_remote(remote, local) {
        let mut s = local.clone();
        s.revision = local.revision.max(remote.revision) + 1;
        s.exported_at = now;
        s.last_sync_at = Some(now);
        return s;
    }

    let lt: BTreeMap<&str, &Task> = local.tasks.iter().map(|t| (t.uid.as_str(), t)).collect();
    let rt: BTreeMap<&str, &Task> = remote.tasks.iter().map(|t| (t.uid.as_str(), t)).collect();
    let bt: BTreeMap<&str, &Task> = base
        .map(|b| b.tasks.iter().map(|t| (t.uid.as_str(), t)).collect())
        .unwrap_or_default();

    let mut tasks = Vec::new();
    let uids: BTreeSet<&str> = lt.keys().chain(rt.keys()).copied().collect();
    for uid in uids {
        let l = lt.get(uid);
        let r = rt.get(uid);
        let b = bt.get(uid).copied();
        match (l, r) {
            (Some(l), Some(r)) => tasks.push(merge_task(l, r, b)),
            (Some(l), None) => tasks.push((*l).clone()),
            (None, Some(r)) => tasks.push((*r).clone()),
            (None, None) => {}
        }
    }
    tasks.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then(a.sort_order.cmp(&b.sort_order))
            .then(a.uid.cmp(&b.uid))
    });

    let lc: BTreeMap<&str, &Category> = local
        .categories
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let rc: BTreeMap<&str, &Category> = remote
        .categories
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let bc: BTreeMap<&str, &Category> = base
        .map(|b| b.categories.iter().map(|c| (c.name.as_str(), c)).collect())
        .unwrap_or_default();

    let mut categories = Vec::new();
    let names: BTreeSet<&str> = lc.keys().chain(rc.keys()).chain(bc.keys()).copied().collect();
    for name in names {
        let l = lc.get(name);
        let r = rc.get(name);
        let b = bc.get(name).copied();
        match (l, r) {
            (Some(l), Some(r)) => categories.push(merge_category(l, r, b)),
            (Some(l), None) => categories.push((*l).clone()),
            (None, Some(r)) => {
                // base 有、本地无 → 本地删除了该分类：
                // 远端相对 base 未更新则删除生效；更新过（改色/改排序）则远端编辑胜。
                let keep = match b {
                    Some(b) => r.updated_at > b.updated_at,
                    None => true,
                };
                if keep {
                    categories.push((*r).clone());
                }
            }
            (None, None) => {}
        }
    }
    categories.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));

    SnapshotV2 {
        version: 2,
        revision: local.revision.max(remote.revision) + 1,
        exported_at: now,
        device_id: device_id.unwrap_or(&local.device_id).to_string(),
        last_sync_at: Some(now),
        categories,
        tasks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::now_millis;

    fn settings(url: &str, enabled: bool) -> WebDavSettings {
        WebDavSettings {
            enabled,
            url: url.into(),
            username: "alice".into(),
            password_ref: Some("pw".into()),
            root_path: None,
        }
    }

    fn task(uid: &str, title: &str, updated: i64) -> Task {
        Task {
            uid: uid.into(),
            parent_uid: None,
            title: title.into(),
            category: "工作".into(),
            priority: 2,
            done: false,
            completed_at: None,
            created_at: 100,
            updated_at: updated,
            deleted_at: None,
            depth: 0,
            sort_order: 0,
        }
    }

    #[test]
    fn normalize_base_appends_slash_and_root() {
        let mut s = settings("https://dav.example.com/doit/", true);
        assert_eq!(normalize_base(&s).unwrap(), "https://dav.example.com/doit/");
        s.root_path = Some("/sub/dir/".into());
        assert_eq!(
            normalize_base(&s).unwrap(),
            "https://dav.example.com/doit/sub/dir/"
        );
        s.url = "ftp://x".into();
        assert!(normalize_base(&s).is_err());
    }

    #[test]
    fn merge3_union_of_local_and_remote() {
        let mut local = SnapshotV2::default();
        local.tasks.push(task("a", "本地任务", 200));
        let mut remote = SnapshotV2::default();
        remote.tasks.push(task("b", "远端任务", 200));
        let merged = merge3(&local, &remote, None, 1000, None);
        assert_eq!(merged.tasks.len(), 2);
        assert!(merged.tasks.iter().any(|t| t.uid == "a"));
        assert!(merged.tasks.iter().any(|t| t.uid == "b"));
        assert_eq!(merged.revision, 1);
        assert_eq!(merged.last_sync_at, Some(1000));
    }

    #[test]
    fn merge3_same_uid_newer_wins() {
        let mut local = SnapshotV2::default();
        local.tasks.push(task("a", "本地旧", 100));
        let mut remote = SnapshotV2::default();
        remote.tasks.push(task("a", "远端新", 500));
        let merged = merge3(&local, &remote, None, 1000, None);
        let t = merged.tasks.iter().find(|t| t.uid == "a").unwrap();
        assert_eq!(t.title, "远端新");
    }

    #[test]
    fn merge3_same_uid_tie_prefers_remote() {
        let mut local = SnapshotV2::default();
        local.tasks.push(task("a", "本地", 300));
        let mut remote = SnapshotV2::default();
        remote.tasks.push(task("a", "远端", 300));
        let merged = merge3(&local, &remote, None, 1000, None);
        assert_eq!(merged.tasks[0].title, "远端");
    }

    #[test]
    fn merge3_tombstone_wins_when_no_edit_after_delete() {
        let mut base = SnapshotV2::default();
        base.tasks.push(task("a", "任务", 100));
        let mut local = base.clone();
        local.tasks[0].deleted_at = Some(200);
        local.tasks[0].updated_at = 200;
        let mut remote = base.clone();
        remote.tasks[0].title = "远端编辑".into();
        remote.tasks[0].updated_at = 250; // 删除后又编辑 → 编辑胜
        let merged = merge3(&local, &remote, Some(&base), 1000, None);
        assert!(merged.tasks[0].deleted_at.is_none(), "删除后编辑应复活");
    }

    #[test]
    fn merge3_stale_remote_keeps_local() {
        let mut local = SnapshotV2::default();
        local.revision = 5;
        local.exported_at = now_millis();
        local.tasks.push(task("a", "本地", 100));
        let mut remote = SnapshotV2::default();
        remote.revision = 3;
        remote.exported_at = 1;
        remote.tasks.push(task("b", "远端旧", 100));
        assert!(is_stale_remote(&remote, &local));
        let merged = merge3(&local, &remote, None, 1000, None);
        assert_eq!(merged.tasks.len(), 1);
        assert_eq!(merged.tasks[0].uid, "a");
    }

    #[test]
    fn test_connection_validates_fields_without_network() {
        let s = settings("", true);
        assert!(test_connection(&s, "pw").is_err());
    }
}
