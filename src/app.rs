//! 应用状态与根视图编排
//!
//! 采用 GPUI 的 Entity 模式：AppState 为可变状态（Entity 持有），
//! AppView 实现 Render，通过 cx.listener / on_click 修改状态并 cx.notify() 触发重绘。

use crate::domain::snapshot::{SnapshotStore, default_data_dir};
use crate::domain::types::{AppSettings, SnapshotV2};
use crate::theme::palette::AppTheme;
use crate::ui::topbar::render_topbar;
use crate::ui::today::render_today;
use crate::ui::quick_add::{render_menus, render_quick_add};
use crate::ui::stats::render_stats;
use crate::ui::calendar::render_calendar;
use crate::ui::settings::render_settings;
use chrono::Datelike;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::prelude::*;
use gpui_kit::*;


/// 把 gpui-component 内置主题的「聚焦描边（ring）/ 输入框边框（input）/
/// 高亮（accent）」统一覆盖为应用主题色（薄荷绿），亮/暗两档各用对应色值。
/// 启动时（main.rs）与设置页切换主题时应调用，保证与 AppState.theme 联动。
pub fn apply_component_theme(is_dark: bool, cx: &mut App) {
    use gpui::Hsla;
    // 亮色薄荷绿 / 暗色薄荷绿（与 DoitPalette 两套一致）
    let mint_rgb = if is_dark { 0x46C7A6 } else { 0x3FBB9C };
    let mint = Hsla::from(rgb(mint_rgb));
    let foreground = Hsla::from(rgb(0xFFFFFF));
    // 确保全局 Theme 存在后再整体覆写
    Theme::change(if is_dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    let theme = Theme::global_mut(cx);
    let ring_tok = mint.into();
    theme.tokens.ring = ring_tok;
    theme.tokens.input = mint.alpha(0.55).into();
    theme.tokens.accent = mint.alpha(0.55).into();
    theme.tokens.accent_foreground = foreground.into();
    // 同步布局侧的颜色字段（组件取 tokens 为主，这里兜底）
    theme.colors.ring = mint;
    theme.colors.input = mint.alpha(0.55);
    theme.colors.accent = mint.alpha(0.55);
    theme.colors.accent_foreground = foreground;
}

/// 导航页面
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Today,
    Stats,
    Calendar,
    Settings,
}

/// 统计页区间（近一周 / 近一月 / 近一年）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatsRange {
    #[default]
    Week,
    Month,
    Year,
}

/// 快速添加栏当前展开的下拉菜单
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickMenu {
    Priority,
    Category,
}

/// 内联编辑模式（右键菜单「编辑」/「子任务」）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineMode {
    /// 原地改标题
    Rename,
    /// 新增子任务
    NewSub,
}

/// 全局应用状态（可变，由 Entity 持有）
pub struct AppState {
    pub snapshot: SnapshotV2,
    pub settings: AppSettings,
    pub page: Page,
    pub filter: String,              // 分类筛选，"all" 表示全部
    pub store: SnapshotStore,
    pub toast: Option<String>,
    /// 每次 notify_msg 自增；AppView 据此检测新 toast 并重新倒计时自动消失。
    pub toast_seq: u64,
    pub sync_status: String,         // synced / dirty / syncing / error / off / ready 等
    /// 是否成功验证过连接或完成过一次同步（对齐桌面端 sync.verified）：
    /// 未验证时顶栏显示「已开启暂未同步」(ready) 而不是误导性的「已同步」。
    pub synced_once: bool,
    /// 同步任务进行中（防止重复点击重复发起）
    pub syncing: bool,
    pub collapsed: std::collections::HashSet<String>, // 折叠的任务 uid
    pub quick_input: Entity<InputState>, // 底部快速添加输入框状态
    pub quick_add_priority: u8,      // 快速添加默认优先级
    pub quick_add_category: String,  // 快速添加目标分类，"全部" 表示跟随当前筛选
    pub open_menu: Option<QuickMenu>, // 快速添加栏展开的下拉
    /// 「添加分类」弹窗的输入行（gpui-kit Dialog 使用；打开时创建，关闭时清空）
    pub cat_add_rows: Option<Vec<(Entity<InputState>, usize)>>,
    /// 「编辑分类」弹窗：目标旧名 + 当前颜色索引 + 输入框（gpui-kit Dialog 使用）
    pub cat_edit: Option<(String, usize, Entity<InputState>)>,
    /// 「删除分类」确认的目标名（gpui-kit Dialog 使用）
    pub cat_delete: Option<String>,
    /// 「添加分类」弹框里当前展开颜色气泡的行号（None 全收起）
    pub cat_color_open: Option<usize>,
    pub stats_range: StatsRange,     // 统计页区间

    // ---- 内联编辑（右键菜单「编辑」/「子任务」）----
    /// 被内联编辑的目标任务 uid + 模式（None 表示没有进行中的内联编辑）
    pub inline_edit: Option<(String, InlineMode)>,
    /// 内联编辑共享输入框状态
    pub inline_input: Entity<InputState>,
    /// 新增子任务时选用的优先级（默认继承父任务，可在输入卡片上调整）
    pub new_sub_priority: u8,
    /// 子任务输入卡片的优先级下拉是否展开（绝对浮动，不影响布局）
    pub new_sub_prio_open: bool,

    // ---- 月历页状态 ----
    /// 当前展示的年月
    pub cal_year: i32,
    pub cal_month: u32,
    /// 当前选中的日期
    pub cal_sel: chrono::NaiveDate,

    // ---- WebDAV 设置输入框 ----
    pub webdav_url: Entity<InputState>,
    pub webdav_user: Entity<InputState>,
    pub webdav_pass: Entity<InputState>,
}

impl AppState {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let store = SnapshotStore::new(default_data_dir());
        let snapshot = store.load().unwrap_or_else(|_| SnapshotV2::default());
        // 首次启动注入演示数据（与原型一致），方便直接看到效果
        let snapshot = if snapshot.tasks.is_empty() {
            crate::domain::seed::demo_snapshot()
        } else {
            snapshot
        };
        // 从磁盘加载设置（主题/进度色/WebDAV 配置），首次启动或文件损坏时用默认值
        let settings = store.load_settings().unwrap_or_default();

        let quick_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("添加待办，回车快速创建…"));
        let inline_input =
            cx.new(|cx| InputState::new(window, cx));

        // WebDAV 三个输入框，初始值来自设置
        let url0 = settings.webdav.url.clone();
        let user0 = settings.webdav.username.clone();
        let pass0 = settings.webdav.password_ref.clone().unwrap_or_default();
        let webdav_url = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("https://dav.example.com/doit/");
            if !url0.is_empty() {
                s.set_value(url0, window, cx);
            }
            s
        });
        let webdav_user = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("WebDAV 用户名");
            if !user0.is_empty() {
                s.set_value(user0, window, cx);
            }
            s
        });
        let webdav_pass = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("应用专用密码或只读令牌");
            if !pass0.is_empty() {
                s.set_value(pass0, window, cx);
            }
            // 密码框加密展示（掩码），不显示明文
            s.set_masked(true, window, cx);
            s
        });

        let webdav_enabled = settings.webdav.enabled;
        let today = chrono::Local::now().date_naive();
        let first_cat = snapshot
            .categories
            .first()
            .map_or_else(|| "全部".to_string(), |c| c.name.clone());
        Self {
            snapshot,
            settings,
            page: Page::Today,
            filter: "all".into(),
            store,
            toast: None,
            toast_seq: 0,
            // 与桌面端一致：开启但未验证过连接时显示 ready（已开启暂未同步），未开启则 off。
            sync_status: if webdav_enabled { "ready".into() } else { "off".into() },
            synced_once: false,
            syncing: false,
            collapsed: std::collections::HashSet::new(),
            quick_input,
            quick_add_priority: crate::theme::palette::priority::NORMAL,
            quick_add_category: first_cat,
            open_menu: None,
            cat_add_rows: None,
            cat_edit: None,
            cat_delete: None,
            cat_color_open: None,
            stats_range: StatsRange::Week,
            inline_edit: None,
            inline_input,
            new_sub_priority: crate::theme::palette::priority::NORMAL,
            new_sub_prio_open: false,
            cal_year: today.year(),
            cal_month: today.month(),
            cal_sel: today,
            webdav_url,
            webdav_user,
            webdav_pass,
        }
    }

    pub fn theme(&self) -> AppTheme {
        match self.settings.theme.as_str() {
            "dark" => AppTheme::Dark,
            _ => AppTheme::Light,
        }
    }

    /// 保存快照（防抖的简化版：直接写）
    pub fn save(&mut self) {
        self.snapshot.exported_at = crate::domain::types::now_millis();
        let _ = self.store.save(&self.snapshot);
        // 已启用同步时，任何本地编辑都标记为「有未同步更改 · 点击同步」。
        // 同步完成回调里会在 save() 之后把状态重置为 synced。
        if self.settings.webdav.enabled && !self.syncing {
            self.sync_status = "dirty".into();
        }
    }

    /// 持久化应用设置（主题/进度色/WebDAV 配置）。
    /// 与快照分离存放，避免任务变更时的频繁写入覆盖设置。
    pub fn save_settings(&self) {
        let _ = self.store.save_settings(&self.settings);
    }

    pub fn notify_msg(&mut self, msg: impl Into<String>) {
        self.toast = Some(msg.into());
        self.toast_seq = self.toast_seq.wrapping_add(1);
    }

    // ---------- WebDAV 同步 ----------

    /// 「测试连接」：把输入框里的配置落到内存+磁盘，然后在后台线程真实探测 WebDAV 目录与鉴权。
    /// 结果通过 toast 反馈到 UI。
    pub fn test_webdav(&mut self, url: &str, user: &str, pass: &str, state: &Entity<AppState>, cx: &mut App) {
        if self.syncing {
            self.notify_msg("同步正在进行中，请稍候");
            cx.notify(state.entity_id());
            return;
        }
        self.settings.webdav.url = url.trim().to_string();
        self.settings.webdav.username = user.trim().to_string();
        self.settings.webdav.password_ref = if pass.trim().is_empty() {
            None
        } else {
            Some(pass.trim().to_string())
        };
        self.save_settings();

        let settings = self.settings.clone();
        let pass_owned = pass.to_string();
        let weak = state.downgrade();
        cx.spawn(async move |cx| {
            // ureq 是同步阻塞客户端，放到后台线程池执行，避免卡住 UI
            let result = cx
                .background_executor()
                .spawn(async move { crate::sync::test_connection(&settings.webdav, &pass_owned) })
                .await;
            let _ = weak.update(cx, |st, cx| {
                match result {
                    Ok(ms) => {
                        st.settings.webdav.enabled = true;
                        st.save_settings();
                        st.synced_once = true;
                        st.sync_status = "synced".into();
                        st.notify_msg(format!("连接成功（{ms} ms），配置已保存"));
                    }
                    Err(e) => {
                        st.notify_msg(format!("连接失败：{e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        self.notify_msg("正在测试连接…");
    }

    /// 顶栏同步胶囊点击入口：用已保存的 WebDAV 配置直接同步，
    /// 不需要先回到设置页重新填写/保存。
    pub fn sync_now_entity(state: &Entity<AppState>, cx: &mut App) {
        let target = state.clone();
        state.update(cx, move |st, cx| st.sync_now(&target, cx));
    }

    /// 与桌面端 StatusPills 的点击行为一致：点击即同步。
    /// 未开启或未配置时给出明确提示，不静默、也不进入 syncing 状态。
    pub fn sync_now(&mut self, state: &Entity<AppState>, cx: &mut App) {
        if self.syncing {
            self.notify_msg("同步正在进行中，请稍候");
            cx.notify(state.entity_id());
            return;
        }
        if !self.settings.webdav.enabled || self.settings.webdav.url.trim().is_empty() {
            self.notify_msg("尚未设置同步 · 请到设置页填写并开启 WebDAV 同步");
            cx.notify(state.entity_id());
            return;
        }
        let settings = self.settings.clone();
        let pass = settings.webdav.password_ref.clone().unwrap_or_default();
        self.run_sync(settings, pass, state, cx);
    }

    /// 后台执行完整同步（探测 → 拉取 → 三方合并 → 上传），成功后替换内存快照并落盘。
    /// `save_and_sync` 与顶栏 `sync_now` 共用这条路径，避免两处行为漂移。
    fn run_sync(
        &mut self,
        settings: AppSettings,
        pass: String,
        state: &Entity<AppState>,
        cx: &mut App,
    ) {
        let local = self.snapshot.clone();
        let base = self.store.load_sync_base();
        let now = crate::domain::types::now_millis();
        let weak = state.downgrade();

        self.syncing = true;
        self.sync_status = "syncing".into();
        cx.notify(state.entity_id());

        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::sync::sync_once(&settings.webdav, &pass, &local, base.as_ref(), now)
                })
                .await;
            let _ = weak.update(cx, |st, cx| {
                st.syncing = false;
                match result {
                    Ok(outcome) => {
                        st.snapshot = outcome.snapshot;
                        st.snapshot.exported_at = crate::domain::types::now_millis();
                        st.save();
                        // 基线只记录本次成功同步的结果，供下次三方合并
                        let _ = st.store.save_sync_base(&st.snapshot);
                        st.synced_once = true;
                        st.sync_status = "synced".into();
                        st.notify_msg(if outcome.pulled {
                            "同步完成，已拉取并合并远端数据"
                        } else {
                            "同步完成，本地快照已上传到服务器"
                        });
                    }
                    Err(e) => {
                        st.sync_status = "error".into();
                        st.notify_msg(format!("同步失败：{e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        self.notify_msg("正在同步…");
    }

    /// 「保存并同步」：保存配置后，在后台线程执行完整同步。
    pub fn save_and_sync(&mut self, url: &str, user: &str, pass: &str, state: &Entity<AppState>, cx: &mut App) {
        if self.syncing {
            self.notify_msg("同步正在进行中，请稍候");
            cx.notify(state.entity_id());
            return;
        }
        self.settings.webdav.url = url.trim().to_string();
        self.settings.webdav.username = user.trim().to_string();
        self.settings.webdav.password_ref = if pass.trim().is_empty() {
            None
        } else {
            Some(pass.trim().to_string())
        };
        self.settings.webdav.enabled = true;
        self.save_settings();

        let settings = self.settings.clone();
        let pass_owned = pass.to_string();
        self.run_sync(settings, pass_owned, state, cx);
    }

    // ---------- 快速添加栏 ----------

    /// 展开 / 收起下拉菜单（点同一个则收起）
    pub fn toggle_menu(&mut self, menu: QuickMenu) {
        self.open_menu = if self.open_menu == Some(menu) { None } else { Some(menu) };
    }

    /// 关闭所有下拉菜单
    pub fn close_menu(&mut self) {
        self.open_menu = None;
    }

    /// 选择顶部分类筛选，并同步快速添加栏的分类选项：
    /// - 选中具体分类 → 快速添加也切到该分类；
    /// - 选中「全部」→ 快速添加切回「全部」（跟随筛选）。
    pub fn select_filter(&mut self, filter: &str) {
        self.filter = filter.to_string();
        self.quick_add_category = filter.to_string();
    }

    /// 新建任务实际使用的分类：
    /// 优先取快速添加栏选中的分类；「全部」时跟随顶部分类筛选；再退回第一个分类。
    pub fn resolve_quick_category(&self) -> String {
        if self.quick_add_category != "全部" && !self.quick_add_category.is_empty() {
            return self.quick_add_category.clone();
        }
        if self.filter != "all" && !self.filter.is_empty() {
            return self.filter.clone();
        }
        self.snapshot
            .categories
            .first()
            .map_or_else(String::new, |c| c.name.clone())
    }

    // ---------- 右键菜单动作（菜单构建见 ui::ctx_menu）----------

    /// 「编辑」：进入内联改标题
    pub fn ctx_edit(&mut self, uid: String, window: &mut Window, cx: &mut App) {
        self.begin_inline_edit(uid, InlineMode::Rename, window, cx);
    }

    /// 「子任务」：进入内联新增子任务
    //
    // 以下删除/优先级/分类等方法接收 owned String（而非 &str），
    // 因为它们是右键菜单 `'static` 闭包的 on_click 回调，必须 move 捕获。
    #[allow(clippy::needless_pass_by_value)]
    pub fn ctx_add_sub(&mut self, uid: String, window: &mut Window, cx: &mut App) {
        self.begin_inline_edit(uid, InlineMode::NewSub, window, cx);
    }

    /// 「设置优先级」：直接改数据
    #[allow(clippy::needless_pass_by_value)]
    pub fn ctx_set_priority(&mut self, uid: String, v: u8) {
        self.snapshot.set_priority(&uid, v);
        self.save();
        self.notify_msg(format!("优先级：{}", crate::theme::palette::priority::name(v)));
    }

    /// 「改分类」：直接改数据
    #[allow(clippy::needless_pass_by_value)]
    pub fn ctx_set_category(&mut self, uid: String, name: String) {
        self.snapshot.set_category(&uid, name.clone());
        self.save();
        self.notify_msg(format!("已移动到 {name}"));
    }

    /// 「删除」：墓碑删除
    #[allow(clippy::needless_pass_by_value)]
    pub fn ctx_delete(&mut self, uid: String) {
        self.snapshot.mark_deleted(&uid);
        self.save();
        self.notify_msg("已删除");
    }

    // ---------- 分类管理弹窗（gpui-kit Dialog） ----------

    /// 打开「添加分类」Dialog：预建一行输入（颜色默认下一个未占用索引）
    #[allow(clippy::too_many_arguments)]
    pub fn open_add_categories(&mut self, window: &mut Window, cx: &mut App) {
        let color_ix = self.next_free_cat_color_ix();
        let input = new_cat_input(window, cx);
        input.update(cx, |i, cx| i.focus(window, cx));
        self.cat_add_rows = Some(vec![(input, color_ix)]);
    }

    /// 添加分类弹窗里追加一行输入框（支持一次性加多个）
    pub fn cat_add_input(&mut self, window: &mut Window, cx: &mut App) {
        let color_ix = self.next_free_cat_color_ix();
        let input = new_cat_input(window, cx);
        if let Some(rows) = &mut self.cat_add_rows {
            rows.push((input, color_ix));
        }
    }

    /// 设置某行的颜色索引（由颜色气泡选中时调用），并收起气泡
    pub fn cat_set_row_color(&mut self, row: usize, color_ix: usize) {
        if let Some(rows) = &mut self.cat_add_rows {
            if let Some((_, ix)) = rows.get_mut(row) {
                *ix = color_ix;
            }
        }
        self.cat_color_open = None;
    }

    /// 切换某行颜色气泡的展开/收起
    pub fn cat_toggle_color_bubble(&mut self, row: usize) {
        self.cat_color_open = if self.cat_color_open == Some(row) { None } else { Some(row) };
    }

    /// 下一个未被现有分类占用的色卡索引（默认新行取色）
    fn next_free_cat_color_ix(&self) -> usize {
        use crate::domain::types::SnapshotV2;
        // 历史色可能不带 #，归一化后再比对，避免重复占用同一色卡
        let used: Vec<String> = self
            .snapshot
            .categories
            .iter()
            .map(|c| crate::domain::color::normalize_category_color(&c.color))
            .collect();
        SnapshotV2::CAT_PALETTE
            .iter()
            .position(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c)))
            .unwrap_or(0)
    }

    /// 保存添加分类：逐行读取非空名称与所选颜色，批量新增。
    /// 返回 true 表示已保存（关闭 Dialog）。
    #[allow(clippy::needless_pass_by_value)]
    pub fn confirm_add_categories(&mut self, cx: &mut App) -> bool {
        let Some(rows) = &self.cat_add_rows else { return true };
        let entries: Vec<_> = rows
            .iter()
            .filter_map(|(i, color_ix)| {
                let t = i.read(cx).text().to_string().trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    let color =
                        crate::domain::types::SnapshotV2::CAT_PALETTE[*color_ix % 8].to_string();
                    Some((t, Some(color)))
                }
            })
            .collect();
        if entries.is_empty() {
            self.notify_msg("请输入至少一个分类名称");
            return false;
        }
        let added = self.snapshot.add_categories_with_colors(&entries);
        self.cat_add_rows = None;
        self.save();
        self.notify_msg(format!("已添加 {added} 个分类"));
        true
    }

    /// 打开「编辑分类」Dialog
    pub fn open_edit_category(&mut self, name: &str, window: &mut Window, cx: &mut App) {
        let color_ix = self
            .snapshot
            .categories
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| {
                use crate::domain::types::SnapshotV2;
                let normalized = crate::domain::color::normalize_category_color(&c.color);
                SnapshotV2::CAT_PALETTE
                    .iter()
                    .position(|h| h.eq_ignore_ascii_case(&normalized))
            })
            .unwrap_or(0);
        let input = new_cat_input(window, cx);
        input.update(cx, |i, cx| i.set_value(name, window, cx));
        self.cat_edit = Some((name.to_string(), color_ix, input));
    }

    /// 编辑分类弹窗里设置新颜色索引（由颜色气泡选中时调用）
    pub fn cat_set_edit_color(&mut self, color_ix: usize) {
        if let Some((_, ix, _)) = &mut self.cat_edit {
            *ix = color_ix;
        }
        self.cat_color_open = None;
    }

    /// 保存编辑分类（改名 + 可选换色）；返回 true 表示保存成功并关闭 Dialog。
    #[allow(clippy::needless_pass_by_value)]
    pub fn confirm_edit_category(&mut self, cx: &mut App) -> bool {
        let Some((old_name, color_ix, input)) = &self.cat_edit else { return true };
        let old_name = old_name.clone();
        let color_ix = *color_ix;
        let new_name = input.read(cx).text().to_string();
        let new_name_trim = new_name.trim().to_string();
        let color = crate::domain::types::SnapshotV2::CAT_PALETTE[color_ix % 8].to_string();
        let ok = self
            .snapshot
            .rename_category_with_color(&old_name, &new_name_trim, Some(&color));
        if ok {
            self.cat_edit = None;
            self.save();
            self.notify_msg("已保存分类");
            if self.filter == old_name {
                self.filter = "all".into();
            }
            if self.quick_add_category == old_name {
                self.quick_add_category = new_name_trim;
            }
            true
        } else {
            self.notify_msg("重命名失败：名称无效或已存在");
            false
        }
    }

    /// 打开「删除分类」确认 Dialog
    pub fn open_delete_category(&mut self, name: &str) {
        self.cat_delete = Some(name.to_string());
    }

    /// 删除分类；返回 true（关闭 Dialog）
    #[allow(clippy::needless_pass_by_value)]
    pub fn confirm_delete_category(&mut self, name: &str) -> bool {
        let moved = self.snapshot.delete_category(name);
        self.cat_delete = None;
        self.save();
        if moved > 0 {
            self.notify_msg(format!("已删除「{name}」· {moved} 个任务移入默认"));
        } else {
            self.notify_msg(format!("已删除分类：{name}"));
        }
        if self.filter == name {
            self.filter = "all".into();
        }
        true
    }

    /// 开始内联编辑：把输入框预填当前标题 / 清空，并记录目标
    pub fn begin_inline_edit(
        &mut self,
        uid: String,
        mode: InlineMode,
        window: &mut Window,
        cx: &mut App,
    ) {
        // 新增子任务：先把父任务展开，保证其后的输入卡片立即可见；
        // 默认优先级继承父任务。
        if mode == InlineMode::NewSub {
            self.collapsed.remove(&uid);
            self.new_sub_priority =
                self.snapshot.find(&uid).map(|t| t.priority).unwrap_or_default();
        }
        // 先按模式准备输入框内容
        let initial = if mode == InlineMode::Rename {
            self.snapshot.find(&uid).map(|t| t.title.clone()).unwrap_or_default()
        } else {
            String::new()
        };
        let input = self.inline_input.clone();
        input.update(cx, |i, cx| i.set_value(initial, window, cx));
        // 立即聚焦，方便直接输入
        input.update(cx, |i, cx| i.focus(window, cx));
        self.inline_edit = Some((uid, mode));
    }

    /// 提交内联编辑（由回车事件触发）。
    /// 返回 true 表示已提交并结束编辑；空标题视为「取消」，同样结束编辑。
    pub fn commit_inline_edit(&mut self, text: &str) -> bool {
        let Some((uid, mode)) = self.inline_edit.as_ref().map(|(u, m)| (u.clone(), *m)) else {
            return false;
        };
        let text = text.trim();
        // 空标题 = 取消本次编辑（不修改数据，但结束编辑态）
        if text.is_empty() {
            self.inline_edit = None;
            return true;
        }
        let changed = match mode {
            InlineMode::Rename => {
                self.snapshot.rename(&uid, text.to_string());
                true
            }
            InlineMode::NewSub => {
                // 确保父任务展开，新建的子任务立即可见
                self.collapsed.remove(&uid);
                let prio = self.new_sub_priority;
                self.snapshot.create_sub(&uid, text.to_string(), prio).is_some()
            }
        };
        if changed {
            self.inline_edit = None;
        }
        changed
    }

    /// 取消内联编辑（点击外部 / Escape）
    pub fn cancel_inline_edit(&mut self) {
        self.inline_edit = None;
    }

    // ---------- 月历页辅助 ----------

    /// 切换月份（delta: -1 上一月 / +1 下一月）
    #[allow(clippy::needless_pass_by_value)]
    pub fn shift_calendar_month(&mut self, delta: i32) {
        let (y, m) = crate::domain::types::shift_month(self.cal_year, self.cal_month, delta);
        self.cal_year = y;
        self.cal_month = m;
    }

    /// 回到今天
    #[allow(clippy::needless_pass_by_value)]
    pub fn calendar_today(&mut self) {
        let t = chrono::Local::now().date_naive();
        self.cal_year = t.year();
        self.cal_month = t.month();
        self.cal_sel = t;
    }

    /// 选中某一天
    #[allow(clippy::needless_pass_by_value)]
    pub fn select_calendar_date(&mut self, d: chrono::NaiveDate) {
        self.cal_sel = d;
        self.cal_year = d.year();
        self.cal_month = d.month();
    }
}

/// 根视图
pub struct AppView {
    pub state: Entity<AppState>,
    _subscriptions: Vec<Subscription>,
    /// 已为哪个 toast 序号安排过自动消失计时器（避免重复排期）
    toast_timer_seq: u64,
}

impl AppView {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 订阅快速添加输入框：回车提交新任务
        let input = state.read(cx).quick_input.clone();
        let sub = cx.subscribe_in(&input, window, move |this, _input, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let text = this.state.read(cx).quick_input.read(cx).value().to_string();
                let text = text.trim().to_string();
                if text.is_empty() {
                    return;
                }
                let category = this.state.read(cx).resolve_quick_category();
                let priority = this.state.read(cx).quick_add_priority;
                this.state.update(cx, |st, cx| {
                    st.snapshot.create_top(text, category, priority);
                    st.save();
                    st.notify_msg("已添加待办");
                    cx.notify();
                });
                // 清空输入框
                let input = this.state.read(cx).quick_input.clone();
                input.update(cx, |i, cx| i.set_value("", window, cx));
            }
        });

        // 订阅内联编辑输入框：回车提交「改标题 / 新增子任务」
        let inline = state.read(cx).inline_input.clone();
        let sub2 = cx.subscribe_in(&inline, window, move |this, _input, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let text = this.state.read(cx).inline_input.read(cx).value().to_string();
                this.state.update(cx, |st, cx| {
                    if st.commit_inline_edit(&text) {
                        st.save();
                        st.notify_msg("已保存");
                    }
                    cx.notify();
                });
                let input = this.state.read(cx).inline_input.clone();
                input.update(cx, |i, cx| i.set_value("", window, cx));
            }
        });
        // 观察 AppState：每次新 toast（seq 变化）时安排一次 2.2s 自动消失，
        // 与原型 index.html 的 say() 行为一致。
        let sub3 = cx.observe_in(&state, window, move |this, state, _window, cx| {
            let seq = state.read(cx).toast_seq;
            if seq == 0 || seq == this.toast_timer_seq {
                return;
            }
            this.toast_timer_seq = seq;
            let weak = state.downgrade();
            cx.spawn(async move |_this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(2200))
                    .await;
                let _ = weak.update(cx, |st, cx| {
                    // 仅清除仍是同一条 toast（seq 未再变化）的消息
                    if st.toast_seq == seq {
                        st.toast = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        });
        Self { state, _subscriptions: vec![sub, sub2, sub3], toast_timer_seq: 0 }
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.state.read(cx).theme().palette();
        let page = self.state.read(cx).page;
        let filter = self.state.read(cx).filter.clone();

        let main: AnyElement = match page {
            Page::Today => render_today(&self.state, &filter, window, cx).into_any_element(),
            Page::Stats => render_stats(&self.state, cx).into_any_element(),
            Page::Calendar => render_calendar(&self.state, cx).into_any_element(),
            Page::Settings => render_settings(&self.state, cx).into_any_element(),
        };
        // 页面切换淡入动画：id 随 page 变化 → 每次切页动画从头播放
        // （对内容统一包一层 div 再做透明度动画）
        let main = div().w_full().size_full().child(main).with_animation(
            format!("page-fade-{page:?}"),
            Animation::new(std::time::Duration::from_millis(180)).with_easing(|t| {
                1.0 - (1.0 - t).powi(3) // ease-out cubic
            }),
            |e, t| e.opacity(0.0 + 1.0 * t),
        );

        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(p.bg)
            .text_color(p.ink)
            .child(render_topbar(&self.state, page, cx));
        // 今日页：分类导航固定在顶部（滚动容器之外，滚动时不跟随）
        if page == Page::Today {
            root = root.child(crate::ui::today::render_cat_nav(&self.state, &filter, cx));
        }
        root = root.child(
            div()
                .id("main-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(main),
        );
        if page == Page::Today {
            // 下拉展开时插入一层透明遮罩：点击菜单外区域即关闭菜单
            // （遮罩位于快速添加栏之前，添加栏本身仍可点击）
            if self.state.read(cx).open_menu.is_some() {
                let s = self.state.clone();
                root = root.child(
                    div()
                        .id("menu-overlay")
                        .absolute()
                        .inset_0()
                        .on_click(move |_, _w, cx| {
                            s.update(cx, |st, cx| {
                                st.close_menu();
                                cx.notify();
                            });
                        }),
                );
            }
            // 右键菜单遮罩已由 gpui-component 的 ContextMenu 组件自动管理，
            // 此处不再需要手写遮罩。
            root = root.child(render_quick_add(&self.state, cx));
            // 下拉菜单浮在最上层（根节点绝对定位）
            if let Some(menu) = render_menus(&self.state, cx) {
                root = root.child(menu);
            }
            // 子任务输入卡片的优先级下拉遮罩：点击外部任意处关闭
            if self.state.read(cx).new_sub_prio_open {
                let s = self.state.clone();
                root = root.child(
                    div()
                        .id("newsub-prio-overlay")
                        .absolute()
                        .inset_0()
                        .on_click(move |_, _w, cx| {
                            s.update(cx, |st, cx| {
                                st.new_sub_prio_open = false;
                                cx.notify();
                            });
                        }),
                );
            }
        }
        // toast 提示：底部居中的深色胶囊，浮在最上层（对齐原型 #toast）
        if let Some(msg) = self.state.read(cx).toast.clone() {
            root = root.child(
                div()
                    .id("app-toast")
                    .absolute()
                    .bottom(px(28.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px(px(22.0))
                            .py(px(12.0))
                            .rounded(px(24.0))
                            .bg(p.ink)
                            .text_color(p.bg)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .shadow_lg()
                            .child(msg),
                    ),
            );
        }
        root
    }
}

/// 分类弹窗输入框
fn new_cat_input(window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).placeholder("分类名称"))
}
