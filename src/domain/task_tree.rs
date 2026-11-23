//! 任务树操作（对齐 doit-desktop 的 domain/taskTree.ts）
//!
//! 核心规则：
//! - 父任务完成态由子任务推导（recalc_up）
//! - 今日可见性：顶级 + 未删除 + 分类匹配 + (!完成 || 完成于今天)
//! - 删除即墓碑（deleted_at），级联子树

use super::types::{SnapshotV2, Task, TaskNode, now_millis, start_of_today};
use std::collections::HashMap;

impl SnapshotV2 {
    /// 由扁平任务列表构建树（按 parent_uid 组装，递归）
    pub fn build_tree(&self, filter_category: Option<&str>) -> Vec<TaskNode> {
        // 递归辅助函数：parent 为 None 时取顶级任务，否则按 parent_uid 取子任务
        fn build(
            parent: Option<&str>,
            roots: &[&Task],
            by_parent: &HashMap<&str, Vec<&Task>>,
            filter: Option<&str>,
        ) -> Vec<TaskNode> {
            let children: &[&Task] = match parent {
                None => roots,
                Some(uid) => by_parent.get(uid).map_or(&[], Vec::as_slice),
            };
            let mut nodes = Vec::new();
            for t in children {
                // 分类筛选只作用于顶级任务；子任务跟随父任务
                if parent.is_none() {
                    if let Some(cat) = filter {
                        if cat != "all" && t.category != cat {
                            continue;
                        }
                    }
                }
                let subs = build(Some(&t.uid), roots, by_parent, filter);
                nodes.push(TaskNode { task: (*t).clone(), subs });
            }
            nodes
        }

        let mut roots: Vec<&Task> = Vec::new();
        let mut by_parent: HashMap<&str, Vec<&Task>> = HashMap::new();
        for t in &self.tasks {
            if t.deleted_at.is_some() {
                continue;
            }
            match &t.parent_uid {
                None => roots.push(t),
                Some(uid) => by_parent.entry(uid.as_str()).or_default().push(t),
            }
        }
        // 每个父节点下按 sort_order 排序，相同则按 created_at
        for v in by_parent.values_mut() {
            v.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.created_at.cmp(&b.created_at)));
        }
        roots.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.created_at.cmp(&b.created_at)));
        build(None, &roots, &by_parent, filter_category)
    }

    /// 今日可见的顶级任务树
    pub fn today_tree(&self, filter_category: Option<&str>) -> Vec<TaskNode> {
        let today = start_of_today();
        let tree = self.build_tree(filter_category);
        tree.into_iter()
            .filter(|node| {
                !node.task.done || node.task.completed_at.is_some_and(|c| c >= today)
            })
            .collect()
    }

    /// 查找任务（可变）
    pub fn find_mut(&mut self, uid: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.uid == uid)
    }
    /// 查找任务（只读）
    pub fn find(&self, uid: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.uid == uid)
    }

    /// 切换任务完成态（单向：已完成不可恢复；含子任务不可直接勾选）
    ///
    /// # Panics
    /// 不会 panic；若任务在查找后被并发删除，`find_mut` 的回退分支会跳过更新。
    pub fn toggle_done(&mut self, uid: &str) -> bool {
        let Some(t) = self.find(uid) else { return false };
        if t.done {
            return false;
        }
        // 含未删除子任务的任务不可直接完成
        if self.tasks.iter().any(|s| s.parent_uid.as_deref() == Some(uid) && s.deleted_at.is_none()) {
            return false;
        }
        {
            let t = self.find_mut(uid).unwrap();
            t.done = true;
            t.completed_at = Some(start_of_today());
            t.updated_at = now_millis();
        }
        self.recalc_up(uid);
        true
    }

    /// 逐级向上重算父任务完成态：所有有效子任务完成 → 父任务自动完成
    pub fn recalc_up(&mut self, child_uid: &str) {
        let mut current = self.find(child_uid).and_then(|t| t.parent_uid.clone());
        let mut guard = 0;
        while let Some(parent_uid) = current.take() {
            guard += 1;
            if guard > 100 {
                break;
            }
            let children: Vec<&Task> = self
                .tasks
                .iter()
                .filter(|s| s.parent_uid.as_deref() == Some(parent_uid.as_str()) && s.deleted_at.is_none())
                .collect();
            if children.is_empty() {
                break;
            }
            let all_done = children.iter().all(|c| c.done);
            if let Some(parent) = self.find_mut(&parent_uid) {
                parent.done = all_done;
                parent.completed_at = if all_done { Some(start_of_today()) } else { None };
                parent.updated_at = now_millis();
                current.clone_from(&parent.parent_uid);
            } else {
                break;
            }
        }
    }

    /// 标记删除（墓碑），级联子树
    pub fn mark_deleted(&mut self, uid: &str) {
        let now = now_millis();
        if let Some(t) = self.find_mut(uid) {
            t.deleted_at = Some(now);
            t.updated_at = now;
        }
        // 递归级联：收集所有后代 uid
        let mut targets = vec![uid.to_string()];
        let mut i = 0;
        while i < targets.len() {
            let p = targets[i].clone();
            for t in &mut self.tasks {
                if t.parent_uid.as_deref() == Some(p.as_str()) && t.deleted_at.is_none() {
                    t.deleted_at = Some(now);
                    t.updated_at = now;
                    targets.push(t.uid.clone());
                }
            }
            i += 1;
        }
    }

    /// 新建顶级任务
    pub fn create_top(&mut self, title: String, category: String, priority: u8) -> String {
        let now = now_millis();
        let uid = format!("u-{}-{:x}", now, now.wrapping_mul(2654435761));
        let task = Task {
            uid: uid.clone(),
            parent_uid: None,
            title,
            category,
            priority,
            done: false,
            completed_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            depth: 0,
            sort_order: -1, // 新建置顶
        };
        self.tasks.push(task);
        self.reindex();
        self.revision += 1;
        uid
    }

    /// 新建子任务（继承父任务分类；优先级由调用方指定，默认可传父任务优先级）
    pub fn create_sub(&mut self, parent_uid: &str, title: String, priority: u8) -> Option<String> {
        let (category, depth) = {
            let p = self.find(parent_uid)?;
            (p.category.clone(), p.depth + 1)
        };
        let now = now_millis();
        let uid = format!("u-{}-{:x}", now, now.wrapping_mul(2246822519));
        let task = Task {
            uid: uid.clone(),
            parent_uid: Some(parent_uid.to_string()),
            title,
            category,
            priority,
            done: false,
            completed_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            depth,
            sort_order: -1,
        };
        self.tasks.push(task);
        self.reindex();
        self.revision += 1;
        Some(uid)
    }

    /// 原地编辑标题
    pub fn rename(&mut self, uid: &str, title: String) {
        if let Some(t) = self.find_mut(uid) {
            t.title = title;
            t.updated_at = now_millis();
        }
    }

    /// 修改优先级
    pub fn set_priority(&mut self, uid: &str, priority: u8) {
        if let Some(t) = self.find_mut(uid) {
            t.priority = priority;
            t.updated_at = now_millis();
        }
    }

    /// 修改分类（仅顶级任务）
    pub fn set_category(&mut self, uid: &str, category: String) {
        if let Some(t) = self.find_mut(uid) {
            if t.parent_uid.is_none() {
                t.category = category;
                t.updated_at = now_millis();
            }
        }
    }

    /// 分类内置色卡（与 tui/手机端一致，新增分类轮转取色）
    pub const CAT_PALETTE: [&str; 8] = [
        "#4A86D6", "#4FBF9E", "#EF9F27", "#DE5B55", "#7F77DD", "#D4537E", "#1D9E75", "#3AA6C9",
    ];
    /// 删除分类时任务迁入的默认分类
    pub const DEFAULT_CATEGORY: &str = "默认";

    /// 新增多个分类（跳过空名与已存在），自动分配未占用的色卡颜色。
    /// 返回实际新增的分类个数。
    pub fn add_categories(&mut self, names: &[String]) -> usize {
        self.add_categories_with_colors(
            &names.iter().map(|n| (n.clone(), None)).collect::<Vec<_>>(),
        )
    }

    /// 新增多个分类；每项可选指定颜色（None 自动分配未占用色卡色）。
    /// 返回实际新增的分类个数。
    pub fn add_categories_with_colors(&mut self, rows: &[(String, Option<String>)]) -> usize {
        let mut added = 0usize;
        for (raw, picked_color) in rows {
            let name = raw.trim();
            if name.is_empty()
                || self.categories.iter().any(|c| c.name == name)
                || name == "全部"
            {
                continue;
            }
            // 显式指定颜色则用之；否则取第一个未被占用的色卡色
            let color = if let Some(c) = picked_color {
                c.clone()
            } else {
                let used: Vec<&str> = self.categories.iter().map(|c| c.color.as_str()).collect();
                Self::CAT_PALETTE
                    .iter()
                    .find(|c| !used.contains(c))
                    .unwrap_or(&Self::CAT_PALETTE[7])
                    .to_string()
            };
            let now = now_millis();
            self.categories.push(crate::domain::types::Category {
                name: name.to_string(),
                color,
                sort_order: (self.categories.len()) as i64,
                updated_at: now,
            });
            added += 1;
        }
        if added > 0 {
            self.revision += 1;
        }
        added
    }

    /// 重命名分类：分类表改名，并把所有引用该分类的任务一起改名。
    /// 返回重命名是否成功（新名非空、不与既有分类冲突、旧分类存在）。
    pub fn rename_category(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty()
            || new == "全部"
            || self.categories.iter().any(|c| c.name == new && c.name != old)
        {
            return false;
        }
        let Some(idx) = self.categories.iter().position(|c| c.name == old) else {
            return false;
        };
        let now = now_millis();
        self.categories[idx].name = new.to_string();
        self.categories[idx].updated_at = now;
        for t in &mut self.tasks {
            if t.category == old {
                t.category = new.to_string();
                t.updated_at = now;
            }
        }
        self.revision += 1;
        true
    }

    /// 重命名分类 + 可同步修改颜色（颜色为 None 表示不修改）。
    /// 返回是否成功（与 `rename_category` 一致；名称冲突/空名/不存在则失败）。
    pub fn rename_category_with_color(
        &mut self,
        old: &str,
        new: &str,
        color: Option<&str>,
    ) -> bool {
        if !self.rename_category(old, new) {
            return false;
        }
        if let Some(c) = color {
            let c = c.trim();
            if !c.is_empty() {
                let now = now_millis();
                if let Some(idx) = self.categories.iter().position(|x| x.name == new.trim()) {
                    self.categories[idx].color = c.to_string();
                    self.categories[idx].updated_at = now;
                }
            }
        }
        true
    }

    /// 删除分类：其下任务（含子任务归属）移入「默认」，任务 updated_at 刷新。
    /// 返回移除的任务数。
    pub fn delete_category(&mut self, name: &str) -> usize {
        let Some(idx) = self.categories.iter().position(|c| c.name == name) else {
            return 0;
        };
        self.categories.remove(idx);
        let now = now_millis();
        let mut moved = 0usize;
        for t in &mut self.tasks {
            if t.category == name {
                t.category = Self::DEFAULT_CATEGORY.to_string();
                t.updated_at = now;
                moved += 1;
            }
        }
        self.revision += 1;
        moved
    }

    /// 把 sort_order=-1 的新建项回退为索引，保证顺序稳定
    fn reindex(&mut self) {
        for t in &mut self.tasks {
            if t.sort_order < 0 {
                t.sort_order = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::{now_millis, start_of_today};

    fn mk(uid: &str, parent: Option<&str>, category: &str) -> Task {
        let now = now_millis();
        Task {
            uid: uid.into(),
            parent_uid: parent.map(Into::into),
            title: uid.into(),
            category: category.into(),
            priority: 2,
            done: false,
            completed_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            depth: u32::from(parent.is_some()),
            sort_order: 0,
        }
    }

    #[test]
    fn builds_tree_by_parent() {
        let s = SnapshotV2 {
            tasks: vec![
                mk("p", None, "工作"),
                mk("c1", Some("p"), "工作"),
                mk("c2", Some("p"), "工作"),
            ],
            ..SnapshotV2::default()
        };
        let tree = s.build_tree(None);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].subs.len(), 2);
    }

    #[test]
    fn completing_all_subs_completes_parent() {
        let s = SnapshotV2 {
            tasks: vec![
                mk("p", None, "工作"),
                mk("c1", Some("p"), "工作"),
                mk("c2", Some("p"), "工作"),
            ],
            ..SnapshotV2::default()
        };
        let mut s = s;
        assert!(!s.toggle_done("p")); // 含子任务不可直接完成
        assert!(s.toggle_done("c1"));
        assert!(!s.find("p").unwrap().done);
        assert!(s.toggle_done("c2"));
        assert!(s.find("p").unwrap().done, "全部子任务完成后父任务应自动完成");
        assert_eq!(s.find("p").unwrap().completed_at, Some(start_of_today()));
    }

    #[test]
    fn completed_task_cannot_be_reverted() {
        let mut s = SnapshotV2 {
            tasks: vec![mk("t", None, "生活")],
            ..SnapshotV2::default()
        };
        assert!(s.toggle_done("t"));
        assert!(!s.toggle_done("t"), "已完成不可恢复");
    }

    #[test]
    fn delete_is_tombstone_and_cascades() {
        let mut s = SnapshotV2 {
            tasks: vec![mk("p", None, "工作"), mk("c", Some("p"), "工作")],
            ..SnapshotV2::default()
        };
        s.mark_deleted("p");
        assert!(s.find("p").unwrap().deleted_at.is_some());
        assert!(s.find("c").unwrap().deleted_at.is_some(), "删除应级联子任务");
        assert!(s.build_tree(None).is_empty(), "墓碑任务不应出现在树中");
    }

    #[test]
    fn create_sub_inherits_parent_category_uses_given_priority() {
        let mut p = mk("p", None, "学习");
        p.priority = 4;
        let mut s = SnapshotV2 {
            tasks: vec![p],
            ..SnapshotV2::default()
        };
        // 分类继承父任务；优先级由调用方传入（不强制跟随父任务）
        let sub_uid = s.create_sub("p", "子任务".to_string(), 2).unwrap();
        let sub = s.find(&sub_uid).unwrap();
        assert_eq!(sub.category, "学习");
        assert_eq!(sub.priority, 2);
    }

    #[test]
    fn category_crud_add_rename_delete() {
        let mut s = SnapshotV2 { ..SnapshotV2::default() };
        // 添加：一次性多个，跳过空名
        let added = s.add_categories(&["旅行".to_string(), "  ".to_string(), "学习".to_string()]);
        assert_eq!(added, 2);
        assert!(s.categories.iter().any(|c| c.name == "旅行"));
        assert_eq!(s.categories.len(), 2);

        // 重命名：任务跟随改名，空名/冲突拒绝
        let t = mk("t", None, "旅行");
        s.tasks.push(t);
        assert!(s.rename_category("旅行", "出差"));
        assert!(s.categories.iter().any(|c| c.name == "出差"));
        assert_eq!(s.find("t").unwrap().category, "出差");
        assert!(!s.rename_category("出差", "  "));
        let _ = s.add_categories(&["生活".to_string()]);
        assert!(!s.rename_category("出差", "生活"));

        // 删除：任务移入默认
        let moved = s.delete_category("出差");
        assert_eq!(moved, 1);
        assert!(s.categories.iter().all(|c| c.name != "出差"));
        assert_eq!(s.find("t").unwrap().category, SnapshotV2::DEFAULT_CATEGORY);
    }

    #[test]
    fn today_visibility_excludes_old_completed() {
        let mut old = mk("old", None, "工作");
        old.done = true;
        old.completed_at = Some(start_of_today() - 5 * 86_400_000);
        let fresh = mk("fresh", None, "工作");
        let s = SnapshotV2 {
            tasks: vec![old, fresh],
            ..SnapshotV2::default()
        };
        let tree = s.today_tree(None);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].task.uid, "fresh");
    }

    #[test]
    fn menu_actions_rename_priority_category() {
        let mut s = SnapshotV2 {
            tasks: vec![mk("t", None, "工作")],
            ..SnapshotV2::default()
        };
        // 「编辑」→ rename
        s.rename("t", "新标题".to_string());
        assert_eq!(s.find("t").unwrap().title, "新标题");
        // 「改优先级」→ set_priority
        s.set_priority("t", 4);
        assert_eq!(s.find("t").unwrap().priority, 4);
        // 「改分类」→ set_category
        s.set_category("t", "学习".to_string());
        assert_eq!(s.find("t").unwrap().category, "学习");
    }
}
