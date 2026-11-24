//! 演示种子数据（与 index.html 原型一致）
//! 仅在首次启动、无本地快照时注入，方便直接体验完整界面。

use super::types::{Category, SnapshotV2, Task, now_millis};
use std::time::{SystemTime, UNIX_EPOCH};

fn today_millis() -> i64 {
    now_millis()
}
fn days_ago(n: i64) -> i64 {
    today_millis() - n * 86_400_000
}

fn task(uid: &str, title: &str, category: &str, priority: u8) -> Task {
    let now = now_millis();
    Task {
        uid: uid.into(),
        parent_uid: None,
        title: title.into(),
        category: category.into(),
        priority,
        done: false,
        completed_at: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        depth: 0,
        sort_order: 0,
    }
}

fn sub(parent: &str, uid: &str, title: &str, priority: u8, depth: u32) -> Task {
    let mut t = task(uid, title, "", priority);
    t.parent_uid = Some(parent.into());
    t.depth = depth;
    t
}

/// 构造演示快照
pub fn demo_snapshot() -> SnapshotV2 {
    let device_id = format!(
        "gpui-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            % 1_000_000
    );
    let categories = vec![
        Category { name: "工作".into(), color: "#5AA9E6".into(), sort_order: 0, updated_at: now_millis(), deleted_at: None },
        Category { name: "生活".into(), color: "#34C7A8".into(), sort_order: 1, updated_at: now_millis(), deleted_at: None },
        Category { name: "学习".into(), color: "#A78BFA".into(), sort_order: 2, updated_at: now_millis(), deleted_at: None },
        Category { name: "采购".into(), color: "#F2A65A".into(), sort_order: 3, updated_at: now_millis(), deleted_at: None },
    ];

    // t1 完成 Q4 季度复盘报告（含多级子任务）
    let t1 = task("t1", "完成 Q4 季度复盘报告", "工作", 2);
    let mut t1a = sub("t1", "t1a", "收集各部门数据", 1, 1);
    t1a.done = true;
    t1a.completed_at = Some(days_ago(2));
    let t1b = sub("t1", "t1b", "撰写执行摘要", 1, 1);
    let t1b1 = sub("t1b", "t1b1", "列出核心要点", 1, 2);
    let t1b2 = sub("t1b", "t1b2", "起草段落", 1, 2);
    let t1c = sub("t1", "t1c", "制作汇报 PPT", 1, 1);

    // t2 已完成
    let mut t2 = task("t2", "预约周末体检", "生活", 1);
    t2.done = true;
    t2.completed_at = Some(today_millis());

    // t3 含子任务
    let t3 = task("t3", "读完《设计心理学》第 3 章", "学习", 1);
    let t3a = sub("t3", "t3a", "做章节笔记", 1, 1);

    // t4 采购
    let t4 = task("t4", "采购冬季保暖用品", "采购", 0);
    let t4a = sub("t4", "t4a", "羽绒服", 1, 1);
    let t4b = sub("t4", "t4b", "暖手宝", 1, 1);
    let t4c = sub("t4", "t4c", "厚袜子", 1, 1);

    let t5 = task("t5", "给妈妈回电话", "生活", 2);

    // t6 已完成（昨日），子任务也完成
    let mut t6 = task("t6", "准备技术分享：WebDAV 同步原理", "工作", 1);
    t6.done = true;
    t6.completed_at = Some(days_ago(1));
    let mut t6a = sub("t6", "t6a", "画流程图", 1, 1);
    t6a.done = true;
    t6a.completed_at = Some(days_ago(1));
    let mut t6b = sub("t6", "t6b", "写讲稿", 1, 1);
    t6b.done = true;
    t6b.completed_at = Some(days_ago(1));

    // 组装（顺序即显示顺序）
    let tasks = vec![t1, t1a, t1b, t1b1, t1b2, t1c, t2, t3, t3a, t4, t4a, t4b, t4c, t5, t6, t6a, t6b];
    // 注意：Task 无 subs 字段，子任务关系由 build_tree 按 parent_uid 重建
    SnapshotV2 {
        device_id,
        categories,
        tasks,
        ..SnapshotV2::default()
    }
}
