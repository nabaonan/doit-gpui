use crate::backup::BackupPanel;
use crate::settings::SettingsPanel;
use crate::types::*;
use chrono::{Datelike, Days, NaiveDate};
use chrono::{Duration as ChronoDuration, Local};
use gpui_kit::base::Disableable as _;
use gpui_kit::base::Selectable;
use gpui_kit::base::StyledExt;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::calendar::{CalendarEvent, CalendarState, Date};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::{
    DialogAction, DialogButtonProps, DialogClose,
};
use gpui_kit::component::time_field::{TimeField, TimeFieldState};
use gpui_kit::component::select::{
    SearchableVec, Select, SelectEvent, SelectItem, SelectState,
};
use gpui_kit::component::chart::{BarChart, LineChart, PieChart};
use gpui_kit::base::animation::{EffectTransition, ease_out_cubic};
use gpui_kit::base::{Progress, ProgressIndicator, ProgressTrack};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::Colorize;
use gpui_kit::component::IndexPath;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, Theme, ThemeMode, WindowExt,
    button::Button,
    input::{Input, InputState, InputEvent, AnyInputState},
    menu::{ContextMenuExt, PopupMenuItem},
    tag::Tag as ComponentTag,
};
use gpui_kit::prelude::*;
use gpui_kit::gpui::{
    self, div, AnyElement, AnyView, App, Bounds, Context, CursorStyle, Element,
    ElementId, Entity, Global, GlobalElementId, Hsla, InspectorElementId, InteractiveElement,
    IntoElement, LayoutId, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Position,
    Render, SharedString, Size, Style, Styled, Subscription, SystemNotification, Window,
    WindowAppearance, linear_color_stop, px, relative, rgb,
};
use std::collections::{HashMap, HashSet};
use std::cell::Cell;
use std::rc::Rc;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
// ── Global handle for context-menu callbacks ────────────────────────────────

pub struct DoitAppHandle(pub Entity<DoitApp>);
impl Global for DoitAppHandle {}

/// How the statistics view groups its range.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StatsPeriod { Week, Month }

#[derive(Clone)]
struct BarDatum { label: String, value: f64 }
#[derive(Clone)]
struct LineDatum { label: String, value: f64 }
#[derive(Clone)]
struct PieDatum { value: f32, color: Hsla }

type CatSelectState = SelectState<SearchableVec<CategoryItem>>;

/// A category option shown in the top-bar Select filter (`None` = 全部).
#[derive(Clone)]
struct CategoryItem {
    id: Option<String>,
    name: SharedString,
    color: String,
}

impl SelectItem for CategoryItem {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }

    fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(color_dot(&self.color, cx))
            .child(self.name.clone())
    }
}

fn category_select_items(cats: &[Category]) -> Vec<CategoryItem> {
    let mut items = vec![CategoryItem {
        id: None,
        name: "全部".into(),
        color: String::new(),
    }];
    items.extend(cats.iter().map(|c| CategoryItem {
        id: Some(c.id.clone()),
        name: c.name.clone(),
        color: c.color.clone(),
    }));
    items
}

// ── App Entity ──────────────────────────────────────────────────────────────

pub struct DoitApp {
    pub todos: Vec<TodoItem>,
    pub settings: AppSettings,
    /// When the local data last changed (`%Y-%m-%dT%H:%M:%S`); used to decide
    /// whether cloud sync may overwrite local data.
    pub last_modified: String,
    next_id: u64,

    pub view_mode: ViewMode,
    pub cat_filter: CatFilter,

    pub new_todo_input: Entity<InputState>,
    pub settings_panel: Entity<SettingsPanel>,
    pub backup_panel: Entity<BackupPanel>,

    /// Calendar view state: the interactive month grid + the selected date.
    pub calendar_state: Entity<CalendarState>,
    selected_cal_date: String,

    /// Statistics range selection (period + month anchor + week offset).
    stats_period: StatsPeriod,
    stats_month: (i32, u32),
    stats_week_offset: i32,

    /// 热力图格的可用宽度：由渲染期的高度探测元素回填（见 `HeatmapWidthProbe`）。
    /// taffy 对 flex 子项按 ContentSize 测量，`aspect_ratio` 会被忽略，所以格子
    /// 的高度无法由列宽推导，必须实测布局宽度后再算。
    heatmap_width: Rc<Cell<f32>>,

    /// Inline editing of a todo's content (used by the row + context menu).
    editing_id: Option<String>,
    edit_input: Entity<InputState>,
    _edit_sub: Subscription,

    /// Long-press completion: the todo currently being held-down and its
    /// progress fraction (0..=1). Only used in "longpress" completion mode.
    long_press: Option<(String, f32)>,

    /// Shared `TimeField` state backing the reminder time-picker dialog.
    reminder_time: Entity<TimeFieldState>,

    /// The per-second reminder ticker state: whether the loop is live.
    reminder_ticker_on: bool,

    /// Top-bar category filter dropdown.
    cat_select: Entity<CatSelectState>,
    _cat_select_sub: Subscription,

    /// Subtasks: parent ids whose children are currently collapsed in the today
    /// view (in-memory only).
    collapsed: HashSet<String>,

    /// 设置弹框的当前尺寸。右下角有拖拽把手，可手动调整；本次会话内记住，
    /// 不写入设置文件（避免动持久化 schema）。
    settings_dialog_size: (Pixels, Pixels),
    /// 正在手动调整设置弹框尺寸时的状态：(按下点, 起始宽, 起始高)。
    settings_resize: Option<(Point<Pixels>, Pixels, Pixels)>,

    _input_sub: Subscription,
    _calendar_sub: Subscription,
}

/// 零高度的宽度探测元素：渲染时拿到父级给出的真实可用宽度，回填到
/// `DoitApp::heatmap_width`，并在宽度变化时请求下一帧重绘，让热力图格子按
/// 实测宽度算成正方形（宽度变化只滞后一帧，肉眼不可见）。
struct HeatmapWidthProbe {
    width: Rc<Cell<f32>>,
    app: Entity<DoitApp>,
}

impl Element for HeatmapWidthProbe {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(0.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let width: f32 = bounds.size.width.into();
        if (width - self.width.get()).abs() > 0.5 {
            self.width.set(width);
            let app = self.app.clone();
            window.defer(cx, move |_window, cx| {
                app.update(cx, |_, cx| cx.notify());
            });
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

impl IntoElement for HeatmapWidthProbe {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl DoitApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Restore the saved data if present; first run still seeds sample data.
        // `last_modified` carries the saved data's "as of" time (or now on a
        // fresh install) so sync keeps judging by the latest version.
        let (seed_todos, settings, data_stamp) =
            load_local_data().unwrap_or_else(|| (sample_todos(), AppSettings::default(), now_iso()));

        // 兜底：启动时清掉已到点的提醒（到点即发系统通知并清除；此逻辑处理
        // 上次运行到点前退出、遗留过期 remind_at 的情况）。
        let mut pruned = false;
        let seed_todos: Vec<TodoItem> = seed_todos
            .into_iter()
            .map(|mut t| {
                if !t.completed
                    && t.remind_at.is_some()
                    && reminder_remaining_secs(&t.remind_at).is_none()
                {
                    t.remind_at = None;
                    pruned = true;
                }
                t
            })
            .collect();

        // Honor the self-signed certificate preference for WebDAV requests.
        crate::http::set_trust_self_signed(settings.cloud_sync.trust_self_signed);

        let input = cx.new(|cx| InputState::new(window, cx).placeholder("添加待办事项..."));

        let input_clone = input.clone();
        // Enter submission is handled in `render_input_row`'s capture handler,
        // which has window access to clear the field after adding. Keep this
        // subscription inert to avoid double-adding on Enter.
        let input_sub = cx.subscribe::<InputState, InputEvent>(&input_clone, |_this, _st, _ev, _cx| {});

        // Apply the stored theme on startup.
        change_theme(initial_theme(&settings, window), window, cx);

        let backup_panel = cx.new(|cx| BackupPanel::new(&settings, window, cx));
        let settings_panel = cx.new(|cx| SettingsPanel::new(settings.clone(), window, cx));

        // Calendar view: open on today; the subscription tracks the selected date.
        let today = Local::now().date_naive();
        let calendar_state = cx.new(|cx| {
            let mut state = CalendarState::new(window, cx);
            state.set_date(today, window, cx);
            state
        });
        let calendar_sub = cx.subscribe::<CalendarState, CalendarEvent>(&calendar_state, {
            move |this, _state, event, cx| {
                let date = match event {
                    CalendarEvent::Selected(Date::Single(Some(d))) => *d,
                    _ => return,
                };
                this.selected_cal_date = date.format("%Y-%m-%d").to_string();
                cx.notify();
            }
        });

        let cat_select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(category_select_items(&settings.categories)),
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        let cat_select_sub = cx.subscribe::<CatSelectState, SelectEvent<SearchableVec<CategoryItem>>>(
            &cat_select,
            |this, _st, ev, cx| {
                let SelectEvent::Confirm(value) = ev;
                this.cat_filter = match value {
                    Some(Some(id)) => CatFilter::Id(id.clone()),
                    _ => CatFilter::None,
                };
                cx.notify();
            },
        );

        let edit_input = cx.new(|cx| InputState::new(window, cx));
        let edit_sub = cx.subscribe::<InputState, InputEvent>(&edit_input, |this, _st, ev, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.commit_edit(cx);
            }
        });

        let reminder_time = cx.new(|cx| TimeFieldState::new(window, cx));

        let this = Self {
            next_id: next_todo_id(&seed_todos),
            todos: seed_todos,
            settings,
            last_modified: data_stamp,
            view_mode: ViewMode::Today,
            cat_filter: CatFilter::None,
            new_todo_input: input,
            settings_panel,
            backup_panel,
            calendar_state,
            selected_cal_date: today.format("%Y-%m-%d").to_string(),
            collapsed: HashSet::new(),
            settings_dialog_size: (px(720.), px(500.)),
            settings_resize: None,
            _input_sub: input_sub,
            _calendar_sub: calendar_sub,
            stats_period: StatsPeriod::Month,
            stats_month: current_ym(),
            stats_week_offset: 0,
            heatmap_width: Rc::new(Cell::new(0.0)),
            editing_id: None,
            edit_input,
            _edit_sub: edit_sub,
            long_press: None,
            reminder_time,
            reminder_ticker_on: false,
            cat_select,
            _cat_select_sub: cat_select_sub,
        };

        // Persist the start-of-run pruning of arrived reminders.
        if pruned {
            this.save_local();
        }

        this
    }

    // ── Mutations ──

    fn add_todo(&mut self, content: String, cx: &mut Context<Self>) {
        let id = format!("t{}", self.next_id);
        self.next_id += 1;
        self.todos.push(TodoItem {
            id: id.clone(),
            content: content.into(),
            completed: false,
            created_at: now_iso(),
            completed_at: None,
            order: self.todos.len() as u32,
            tag_id: None,
            cat_id: match &self.cat_filter {
                CatFilter::Id(cid) => Some(cid.clone()),
                _ => None,
            },
            parent_id: None,
            remind_at: None,
        });
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    fn toggle_todo(&mut self, id: &str, cx: &mut Context<Self>) {
        // A parent's completion is derived from its subtasks — leave it alone.
        let has_children = self
            .todos
            .iter()
            .any(|t| t.parent_id.as_deref() == Some(id));
        if has_children {
            return;
        }
        if let Some(item) = self.todos.iter_mut().find(|t| t.id == id) {
            item.completed = !item.completed;
            item.completed_at = if item.completed { Some(now_iso()) } else { None };
        }
        sync_subtree_completion(&mut self.todos, &now_iso());
        self.resort();
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    fn delete_todo(&mut self, id: &str, cx: &mut Context<Self>) {
        // Deleting a parent also deletes its whole subtree (children have no
        // hierarchy limit), and the parent of a deleted item may now be done.
        let mut doomed = vec![id.to_string()];
        let mut i = 0;
        while i < doomed.len() {
            let parent = doomed[i].clone();
            self.todos
                .iter()
                .filter(|t| t.parent_id.as_deref() == Some(parent.as_str()))
                .for_each(|c| doomed.push(c.id.clone()));
            i += 1;
        }
        self.todos.retain(|t| !doomed.contains(&t.id));
        for sid in &doomed {
            self.collapsed.remove(sid);
        }
        sync_subtree_completion(&mut self.todos, &now_iso());
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    fn drop_todo(&mut self, dragged: &str, slot: &DropSlot, cx: &mut Context<Self>) {
        let Some(new_parent) = move_todo_in_list(&mut self.todos, dragged, slot) else {
            return;
        };
        // 落到折叠的父级里：展开它，否则刚拖进去就"消失"了。
        if let Some(pid) = new_parent {
            self.collapsed.remove(&pid);
        }
        // 父子完成状态联动：把子任务挪进/挪出已完成父级都要重新推导。
        sync_subtree_completion(&mut self.todos, &now_iso());
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    /// Add a subtask under `parent_id` and drop the user straight into inline
    /// edit so they can name it. The child inherits the parent's tag/category
    /// so it stays visible under the same category filter.
    fn add_subtask(&mut self, parent_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(parent) = self.todos.iter().find(|t| t.id == parent_id) else {
            return;
        };
        // 拖拽重排会把 order 全局重编号，兄弟"个数"不再等于下一个空位，取最大值 +1。
        let next_order = self
            .todos
            .iter()
            .filter(|t| t.parent_id.as_deref() == Some(parent_id))
            .map(|t| t.order)
            .max()
            .map_or(0, |order| order + 1);
        let id = format!("t{}", self.next_id);
        self.next_id += 1;
        self.todos.push(TodoItem {
            id: id.clone(),
            content: "".into(),
            completed: false,
            created_at: now_iso(),
            completed_at: None,
            order: next_order,
            tag_id: parent.tag_id.clone(),
            cat_id: parent.cat_id.clone(),
            parent_id: Some(parent_id.to_string()),
            remind_at: None,
        });
        // A newly added incomplete child un-completes a finished parent.
        sync_subtree_completion(&mut self.todos, &now_iso());
        self.last_modified = now_iso();
        self.save_local();
        self.start_edit(&id, "", window, cx);
    }

    fn resort(&mut self) {
        self.todos
            .sort_by(|a, b| a.completed.cmp(&b.completed).then(a.order.cmp(&b.order)));
    }

    /// Expand/collapse a parent's subtasks in the today view.
    fn toggle_collapse(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.collapsed.remove(id) {
            self.collapsed.insert(id.to_string());
        }
        cx.notify();
    }

    pub(crate) fn clear_all(&mut self, cx: &mut Context<Self>) {
        self.todos.clear();
        self.collapsed.clear();
        self.settings.tags.clear();
        self.settings.categories.clear();
        self.settings.default_category_id = None;
        self.cat_filter = CatFilter::None;
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    /// Restore a downloaded snapshot, re-applying the theme it carries.
    pub(crate) fn apply_snapshot(
        &mut self,
        snap: SyncSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.todos = snap.todos;
        self.resort();
        self.settings = snap.settings;
        sync_subtree_completion(&mut self.todos, &now_iso());
        // Drop collapse state for parents that no longer exist.
        self.collapsed.retain(|id| self.todos.iter().any(|t| t.id == *id));
        // Adopt the remote's stamp: until the user edits again, local data
        // matches the cloud, so a follow-up sync won't keep warning.
        self.last_modified = snap.exported_at;
        let mode = initial_theme(&self.settings, window);
        change_theme(mode, window, cx);
        if let CatFilter::Id(ref id) = self.cat_filter {
            if !self.settings.categories.iter().any(|c| c.id == *id) {
                self.cat_filter = CatFilter::None;
            }
        }
        self.save_local();
        cx.notify();
    }

    /// Persist the current data to the local snapshot file so a restart
    /// restores it instead of re-seeding sample data. The stamp mirrors
    /// `last_modified` (when this machine's data last changed), so a restart
    /// keeps the authoritative "as of" time for the next sync decision.
    pub(crate) fn save_local(&self) {
        let snap = SyncSnapshot {
            version: 1,
            exported_at: self.last_modified.clone(),
            todos: self.todos.clone(),
            settings: self.settings.clone(),
        };
        write_snapshot_at(&snap, &local_data_path());
    }

    /// Replace categories + default id (from the category management dialog).
    pub(crate) fn set_categories(
        &mut self,
        categories: Vec<Category>,
        default_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.settings.categories = categories;
        let valid = default_id
            .as_deref()
            .map(|id| self.settings.categories.iter().any(|c| c.id == id))
            .unwrap_or(false);
        self.settings.default_category_id = if valid { default_id } else { None };
        if let CatFilter::Id(ref id) = self.cat_filter {
            if !self.settings.categories.iter().any(|c| c.id == *id) {
                self.cat_filter = CatFilter::None;
            }
        }
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    /// Replace tags (from the tag management dialog).
    pub(crate) fn set_tags(&mut self, tags: Vec<Tag>, cx: &mut Context<Self>) {
        self.settings.tags = tags;
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    /// Assign or clear the tag on a single todo.
    pub(crate) fn set_tag(
        &mut self,
        id: &str,
        tag_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(item) = self.todos.iter_mut().find(|t| t.id == id) {
            item.tag_id = tag_id;
        }
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    /// Assign or clear the category on a single todo.
    pub(crate) fn set_cat(
        &mut self,
        id: &str,
        cat_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if cascade_cat(&mut self.todos, id, cat_id) {
            self.last_modified = now_iso();
            self.save_local();
            cx.notify();
        }
    }

    /// Open the reminder time-picker dialog for a todo.
    fn open_reminder_dialog(
        &mut self,
        id: &str,
        has_reminder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Seed the time field with the existing reminder, or now if fresh.
        let initial = self
            .todos
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.remind_at.as_deref())
            .and_then(parse_remind_at)
            .map(|d| d.time())
            .unwrap_or_else(|| Local::now().time());
        self.reminder_time.update(cx, |s, cx| {
            s.set_time(initial, window, cx);
        });
        cx.notify();

        let has = has_reminder;
        let target = id.to_string();
        let reminder_time = self.reminder_time.clone();
        window.open_dialog(cx, move |dialog, _window, cx| {
            let mut d = dialog
                .title("设置提醒")
                .w(px(340.))
                .child(
                    div()
                        .v_flex()
                        .gap_4()
                        .p_4()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("选择今天的时间点，到点后自动提醒你。"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(TimeField::new(&reminder_time)),
                        ),
                );
            d = d.button_props(
                DialogButtonProps::default()
                    .ok_text("确定")
                    .show_cancel(true)
                    .cancel_text("取消")
                    .on_ok({
                        let target = target.clone();
                        move |_click, window, cx| {
                            if let Some(h) = cx.try_global::<DoitAppHandle>() {
                                let app = h.0.clone();
                                app.update(cx, |app, cx| {
                                    app.set_reminder(&target, window, cx);
                                });
                            }
                            true
                        }
                    }),
            );
            // Dialog 不自动渲染 OK/Cancel，必须由 footer 显式绘制按钮。
            d = d.footer(
                div()
                    .flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(if has {
                        let clear_target = target.clone();
                        Button::new("clear-reminder")
                            .ghost()
                            .danger()
                            .label("清除提醒")
                            .on_click(move |_, window, cx| {
                                if let Some(h) = cx.try_global::<DoitAppHandle>() {
                                    let app = h.0.clone();
                                    app.update(cx, |app, cx| {
                                        app.clear_reminder(&clear_target, cx);
                                    });
                                }
                                window.close_dialog(cx);
                            })
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                DialogClose::new().trigger(|button| {
                                    button.label("取消")
                                }),
                            )
                            .child(
                                DialogAction::new().child(
                                    Button::new("confirm-reminder").primary().label("确定"),
                                ),
                            ),
                    ),
            );
            d
        });
    }

    /// Persist the reminder time (`HH:MM` on the current day) to a todo.
    fn set_reminder(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let time = self.reminder_time.read(cx).time();
        let when = Local::now()
            .date_naive()
            .and_time(time)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        if let Some(item) = self.todos.iter_mut().find(|t| t.id == id) {
            item.remind_at = Some(when);
        }
        self.last_modified = now_iso();
        self.save_local();
        self.ensure_reminder_ticker(window, cx);
        cx.notify();
    }

    /// Remove a todo's reminder.
    fn clear_reminder(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(item) = self.todos.iter_mut().find(|t| t.id == id) {
            item.remind_at = None;
        }
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    /// Lazily start / keep alive the per-second reminder ticker. Called from
    /// `render`; it does nothing when there are no unexpired reminders, and
    /// never spawns a second loop while one is running.
    ///
    /// Ticking does two things each second: re-render the live countdowns, and
    /// when a reminder arrives post an OS-level notification and clear the
    /// todo's reminder (the row shows no leftover text).
    fn ensure_reminder_ticker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let any_live = self
            .todos
            .iter()
            .any(|t| reminder_remaining_secs(&t.remind_at).is_some());
        if !any_live || self.reminder_ticker_on {
            return;
        }
        self.reminder_ticker_on = true;
        window.spawn(cx, async move |async_cx| loop {
            async_cx
                .background_executor()
                .timer(Duration::from_secs(1))
                .await;
            let alive = async_cx
                .update(|_window, app_cx| {
                    let mut due = Vec::new();
                    if let Some(handle) = app_cx.try_global::<DoitAppHandle>() {
                        let app = handle.0.clone();
                        app.update(app_cx, |app, cx| {
                            // Collect todos whose reminder has arrived; their
                            // countdown expires into an OS notification + the
                            // row's reminder is cleared (no text remains).
                            for t in app.todos.iter() {
                                if t.completed || t.remind_at.is_none() {
                                    continue;
                                }
                                if reminder_remaining_secs(&t.remind_at).is_none() {
                                    due.push((t.id.clone(), t.content.clone()));
                                }
                            }
                            if !due.is_empty() {
                                for (id, _) in &due {
                                    if let Some(item) =
                                        app.todos.iter_mut().find(|t| &t.id == id)
                                    {
                                        item.remind_at = None;
                                    }
                                }
                                app.last_modified = now_iso();
                                app.save_local();
                            }
                            // Keep ticking while some reminder is still counting down.
                            let any_live = app
                                .todos
                                .iter()
                                .any(|t| reminder_remaining_secs(&t.remind_at).is_some());
                            app.reminder_ticker_on = any_live;
                            if !due.is_empty() || any_live {
                                cx.notify();
                            }
                        });
                    }
                    // Post an OS-level notification for each reminder that fired.
                    for (_, content) in &due {
                        app_cx.show_system_notification(SystemNotification {
                            tag: "doit-reminder".into(),
                            title: "提醒时间到".into(),
                            body: format!("「{content}」").into(),
                            actions: Vec::new(),
                        });
                    }
                    !due.is_empty()
                        || app_cx
                            .try_global::<DoitAppHandle>()
                            .map(|h| h.0.read(app_cx).reminder_ticker_on)
                            .unwrap_or(false)
                })
                .unwrap_or(false);
            if !alive {
                break;
            }
        })
        .detach();
    }

    /// Enter inline-edit mode for a todo.
    fn start_edit(
        &mut self,
        id: &str,
        content: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_id = Some(id.to_string());
        self.edit_input.update(cx, |s, cx| {
            s.set_value(SharedString::from(content), window, cx);
        });
        // 进入内联编辑后立即把光标放进输入框（右键「编辑」/「添加子任务」都走这里）。
        let handle = AnyInputState::from(self.edit_input.clone()).focus_handle(cx);
        window.focus(&handle, cx);
        cx.notify();
    }

    /// Apply the edited content (Enter or the save button).
    fn commit_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.editing_id.take() {
            let value = self.edit_input.read(cx).value().trim().to_string();
            if value.is_empty() {
                // 内容为空：不添加。若是刚插入的空骨架（子任务），直接移除，
                // 避免列表残留空白行；编辑已有待办时清空则保持原内容。
                if self.todos.iter().find(|t| t.id == id && t.content.is_empty()).is_some() {
                    self.todos.retain(|t| t.id != id);
                }
            } else if let Some(todo) = self.todos.iter_mut().find(|t| t.id == id) {
                todo.content = value.into();
            }
        }
        self.last_modified = now_iso();
        self.save_local();
        cx.notify();
    }

    /// Leave inline-edit mode without applying.
    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.editing_id.take() {
            // 取消一个仍为空的新建骨架时同样移除，不留空白行。
            if self.todos.iter().find(|t| t.id == id && t.content.is_empty()).is_some() {
                self.todos.retain(|t| t.id != id);
            }
        }
        cx.notify();
    }


    fn filtered_todos(&self) -> Vec<TodoItem> {
        self.todos
            .iter()
            .filter(|t| match &self.cat_filter {
                CatFilter::None => true,
                CatFilter::Id(fid) => t.cat_id.as_deref() == Some(fid),
            })
            .cloned()
            .collect()
    }

    /// Tasks shown in the "今日待办" view: everything not yet done, plus tasks
    /// completed today (they stay visible at the bottom with a strikethrough).
    fn active_todos(&self) -> Vec<TodoItem> {
        let today = date_part(&now_iso());
        self.filtered_todos()
            .into_iter()
            .filter(|t| {
                if !t.completed {
                    return true;
                }
                t.completed_at.as_deref().map(date_part) == Some(today.clone())
            })
            .collect()
    }


    /// All completed items, grouped by completion date (desc) with items ordered by completion time (desc).
    fn completed_by_day(&self) -> Vec<(String, Vec<TodoItem>)> {
        let mut completed: Vec<&TodoItem> = self
            .todos
            .iter()
            .filter(|t| t.completed && t.completed_at.is_some())
            .collect();
        completed.sort_by(|a, b| {
            b.completed_at
                .as_deref()
                .cmp(&a.completed_at.as_deref())
        });

        let mut groups: Vec<(String, Vec<TodoItem>)> = Vec::new();
        for t in completed {
            let day = t.completed_at.as_deref().map(date_part).unwrap_or_default();
            match groups.last_mut() {
                Some((last_day, items)) if *last_day == day => items.push(t.clone()),
                _ => groups.push((day, vec![t.clone()])),
            }
        }
        groups
    }

}

// ── Render ──────────────────────────────────────────────────────────────────

/// 行尾徽章：统一圆润胶囊风格；标签（`squared=false`）实心填充，
/// 分类（`squared=true`）半透明底 + 虚线边框，靠虚线而非形状区分。
/// 用 gpui-kit 自带 `Tag` 组件承载自定义颜色，避免 ad-hoc div 样式。
fn render_chip(name: SharedString, color: &str, squared: bool) -> AnyElement {
    let bg = Colorize::parse_hex(color).unwrap_or_else(|_| Hsla::blue());
    let rgb = bg.to_rgb();
    let luminance = 0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b;
    let fg = if luminance > 0.55 {
        gpui::hsla(0., 0., 0., 0.9)
    } else {
        gpui::hsla(0., 0., 1., 0.95)
    };
    let border = bg;
    if squared {
        // 分类：同样的圆润胶囊，半透明底色 + 虚线边框区分于实心标签。
        ComponentTag::custom(bg.opacity(0.35), fg, bg)
            .small()
            .rounded_full()
            .border_dashed()
            .child(name)
            .into_any_element()
    } else {
        // 标签：实心胶囊。
        ComponentTag::custom(bg, fg, border)
            .small()
            .rounded_full()
            .child(name)
            .into_any_element()
    }
}

/// 热力图 hover 提示 view：内容直接使用 gpui 自带 Tooltip 组件渲染
/// （`Tooltip::new(...).build(...)`，保证样式与内置 tooltip 完全一致）。
/// 原生 `.tooltip()` 每次进入 hover 都会重建这个 view，因此外层 div 首帧
/// 播放淡入 + 下滑动画，每次划过都重新渐变出现。
struct HeatmapTooltip {
    text: SharedString,
}

impl HeatmapTooltip {
    fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
        }
    }

    /// Build the tooltip view (matches gpui-kit's `Tooltip::build` contract).
    fn build(self, _: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for HeatmapTooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 用自带组件渲染 tooltip 本体，动画包在外层 div 上。
        let tooltip = Tooltip::new(self.text.clone()).build(window, cx);
        EffectTransition::new(Duration::from_millis(150))
            .ease(ease_out_cubic)
            .slide_y(px(4.), px(0.))
            .fade(0.0, 1.0)
            .apply(div().child(tooltip), "heatmap-tooltip-enter")
    }
}

impl Render for DoitApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_bg = cx.theme().background;

        // Keep the per-second countdown ticker alive while any reminder is live.
        self.ensure_reminder_ticker(window, cx);

        div()
            .v_flex()
            .size_full()
            .bg(theme_bg)
            .child(self.render_title_bar(window, cx))
            .child(
                div()
                    .id("main-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.render_content(window, cx).into_any_element()),
            )
            // gpui-kit >= 0.7.1: the Root's WindowState plugin mounts the
            // dialog/sheet/notification layers automatically.
    }
}

// ── Title Bar ───────────────────────────────────────────────────────────────

impl DoitApp {
    fn render_title_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let views = [ViewMode::Today, ViewMode::Calendar, ViewMode::Timeline, ViewMode::Stats];

        div()
            .flex()
            .h(px(48.))
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .children(views.iter().map(|view| {
                                let v = *view;
                                Button::new(("view-btn", view.index()))
                                    .when(self.view_mode == v, |b| b.selected(true))
                                    .small()
                                    .ghost()
                                    .label(view.label())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.view_mode = v;
                                        cx.notify();
                                    }))
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .child(self.render_category_filter(window, cx))
                    .child(
                        Button::new("backup-btn")
                            .ghost()
                            .icon(Icon::empty().path("icons/cloud.svg"))
                            .small()
                            .tooltip("云备份")
                            .on_click(cx.listener(|this, _, window, cx| {
                                let panel = this.backup_panel.clone();
                                let settings = this.settings.clone();
                                panel.update(cx, |p, cx| {
                                    p.sync_from(&settings, window, cx);
                                });
                                window.open_dialog(cx, move |dialog, _w, _c| {
                                    dialog
                                        .title("云备份")
                                        .w(px(460.))
                                        .h(px(360.))
                                        .child(panel.clone())
                                });
                            })),
                    )
                    .child(
                        Button::new("settings-btn")
                            .ghost()
                            .icon(IconName::Settings)
                            .small()
                            .tooltip("设置")
                            .on_click(cx.listener(|this, _, window, cx| {
                                open_settings_dialog(this, window, cx);
                            })),
                    ),
            )
    }
}

// ── Resizable settings dialog ───────────────────────────────────────────────

/// 打开设置弹框。尺寸每帧从 app 读，右下角把手拖动时实时改变弹框大小，
/// 拖完本次会话内保持。
fn open_settings_dialog(app: &mut DoitApp, window: &mut Window, cx: &mut App) {
    let panel = app.settings_panel.clone();
    let base = app.settings.clone();
    panel.update(cx, |p, cx| p.sync_from(base, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let (dialog_w, dialog_h) = cx
            .try_global::<DoitAppHandle>()
            .map(|h| h.0.read(cx).settings_dialog_size)
            .unwrap_or((px(720.), px(500.)));
        let mut body = div().relative().size_full().child(panel.clone());
        if let Some(app) = cx.try_global::<DoitAppHandle>().map(|h| h.0.clone()) {
            body = body
                .child(settings_resize_grip(app.clone(), cx))
                .child(SettingsResizeTracker::new(app));
        }
        dialog.title("设置").w(dialog_w).h(dialog_h).child(body)
    });
}

/// 设置弹框右下角的手动调整尺寸把手（三道递增的短横线，命中区 14px）。
fn settings_resize_grip(app: Entity<DoitApp>, cx: &mut App) -> AnyElement {
    let bar_color = cx.theme().muted_foreground;
    let hover_bg = cx.theme().muted;
    let bar = |width: Pixels| {
        div()
            .w(width)
            .h(px(1.5))
            .rounded_full()
            .bg(bar_color)
    };
    div()
        .id("settings-resize-grip")
        .absolute()
        .right(px(1.))
        .bottom(px(1.))
        .size(px(14.))
        .flex()
        .items_end()
        .justify_end()
        .p(px(3.))
        .rounded_sm()
        .cursor(CursorStyle::ResizeUpRightDownLeft)
        .hover(move |s| s.bg(hover_bg))
        .on_mouse_down(gpui::MouseButton::Left, {
            let app = app.clone();
            move |event, _window, cx| {
                app.update(cx, |app, cx| app.begin_settings_resize(event.position, cx));
            }
        })
        .child(
            div()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(2.))
                .child(bar(px(8.)))
                .child(bar(px(5.)))
                .child(bar(px(2.))),
        )
        .into_any_element()
}

/// 手动调整设置弹框尺寸时跟随指针的全局鼠标监听。
///
/// `window.on_mouse_event` 只能在 paint 阶段注册、且只对下一帧生效，所以每帧
/// 都要随弹框重新挂一次；监听挂在 window 上（而不是元素 hitbox 上），指针拖出
/// 弹框范围也能继续收到 MouseMove，不丢事件。
struct SettingsResizeTracker {
    app: Entity<DoitApp>,
}

impl SettingsResizeTracker {
    fn new(app: Entity<DoitApp>) -> Self {
        Self { app }
    }
}

impl IntoElement for SettingsResizeTracker {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SettingsResizeTracker {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // 1×1 的绝对定位占位节点：不参与面板布局，但保证每帧都被绘制，
        // 从而每帧都能注册一次全局鼠标监听。
        let mut style = Style::default();
        style.position = Position::Absolute;
        style.size = Size::new(px(1.).into(), px(1.).into());
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        ()
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // 只在拖拽进行中挂监听，平时不拦截任何鼠标事件。
        if self.app.read(cx).settings_resize.is_none() {
            return;
        }
        let app = self.app.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if !phase.bubble() {
                return;
            }
            let app = app.clone();
            app.update(cx, |app, cx| {
                app.drag_settings_resize(event.position, event.pressed_button, window, cx)
            });
        });
        let app = self.app.clone();
        window.on_mouse_event(move |_event: &MouseUpEvent, phase, _window, cx| {
            if !phase.bubble() {
                return;
            }
            let app = app.clone();
            app.update(cx, |app, cx| app.end_settings_resize(cx));
        });
    }
}

impl DoitApp {
    /// 记下设置弹框右下角把手的按下点，开始手动调整尺寸。
    fn begin_settings_resize(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.settings_resize = Some((
            position,
            self.settings_dialog_size.0,
            self.settings_dialog_size.1,
        ));
        cx.notify();
    }

    /// 拖动中：新尺寸 = 起始尺寸 + 指针位移，并夹在窗口可用范围内。指针已松开
    /// （在弹框外松开导致漏掉 MouseUp）时直接结束本次调整。
    fn drag_settings_resize(
        &mut self,
        position: Point<Pixels>,
        pressed_button: Option<gpui::MouseButton>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some((start, start_w, start_h)) = self.settings_resize else {
            return;
        };
        if pressed_button.is_none() {
            self.end_settings_resize(cx);
            return;
        }
        let view = window.viewport_size();
        let max_w = (view.width - px(64.)).max(px(560.));
        let max_h = (view.height - px(120.)).max(px(420.));
        // 弹框水平方向是居中的（组件按 (视口 - 宽) / 2 定位），宽度每加 1px，
        // 右边只外扩 0.5px；所以水平位移乘 2，让右下角始终贴着指针走。
        let width = (start_w + (position.x - start.x) * 2.0).clamp(px(560.), max_w);
        let height = (start_h + (position.y - start.y)).clamp(px(420.), max_h);
        if (width, height) != self.settings_dialog_size {
            self.settings_dialog_size = (width, height);
            cx.notify();
        }
    }

    /// 松开鼠标，结束手动调整。
    fn end_settings_resize(&mut self, cx: &mut Context<Self>) {
        if self.settings_resize.take().is_some() {
            cx.notify();
        }
    }
}

impl DoitApp {
    /// A dropdown that selects the category filter shown at the top-right.
    fn render_category_filter(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.sync_cat_select(window, cx);
        // Slightly wider than the trigger so the trailing check mark of the
        // selected option is not clipped.
        Select::new(&self.cat_select)
            .placeholder("分类")
            .menu_width(px(160.))
    }

    /// Keep the category Select state in sync with current categories & filter.
    fn sync_cat_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cats = self.settings.categories.clone();
        let filter = self.cat_filter.clone();
        self.cat_select.update(cx, |st, cx| {
            st.set_items(SearchableVec::new(category_select_items(&cats)), window, cx);
            let target: Option<String> = match &filter {
                CatFilter::None => None,
                CatFilter::Id(id) => Some(id.clone()),
            };
            if st.selected_value() != Some(&target) {
                st.set_selected_value(&target, window, cx);
            }
        });
    }
}

// ── Content View Router ─────────────────────────────────────────────────────

impl DoitApp {
    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.view_mode {
            ViewMode::Today => self.render_today(window, cx).into_any_element(),
            ViewMode::Calendar => self.render_calendar(window, cx).into_any_element(),
            ViewMode::Timeline => self.render_timeline(window, cx).into_any_element(),
            ViewMode::Stats => self.render_stats(window, cx).into_any_element(),
        }
    }
}

// ── Today View ──────────────────────────────────────────────────────────────

/// Extra left padding per nesting level a subtask is rendered below its parent.
///
/// Indent is computed so a subtask's chevron lands exactly under its parent's
/// text start. A parent row's text starts after: 12px row padding + 14px chevron
/// + 4px gap + [12px checkbox slot + 4px gap] (checkbox mode) and then the text.
/// Long-press mode has no checkbox slot, so its text (and thus indent) sits 16px
/// further left. depth-1 chevron at 12+indent == parent text start:
///   checkbox mode: 12 + 14 + 4 + 12 + 4 = 46 → indent 34
///   long-press   : 12 + 14 + 4          = 30 → indent 18
const SUBTASK_INDENT: f32 = 34.;
const SUBTASK_INDENT_LONGPRESS: f32 = 18.;

impl DoitApp {
    fn render_today(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_todos();
        let tags = self.settings.tags.clone();

        div()
            .v_flex()
            .gap_2()
            .p_4()
            .child(self.render_input_row(window, cx))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .children(self.render_todo_forest(&active, &tags, window, cx)),
            )
            .when(active.is_empty(), |el| {
                el.child(
                    div()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .text_color(cx.theme().muted_foreground)
                        .py_8()
                        .child("还没有待办事项，在上方输入添加"),
                )
            })
    }

    /// Render the active todos as a nested tree (subtasks under parents, any
    /// depth). Rows whose parent is not in the visible set render as roots.
    fn render_todo_forest(
        &mut self,
        items: &[TodoItem],
        tags: &[Tag],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let roots: Vec<&TodoItem> = items
            .iter()
            .filter(|t| match &t.parent_id {
                None => true,
                Some(pid) => !items.iter().any(|o| o.id == *pid),
            })
            .collect();
        let mut out: Vec<AnyElement> = Vec::with_capacity(roots.len() + 1);
        for root in roots.iter() {
            out.push(self.render_todo_node(root, items, tags, 0, window, cx));
        }
        // 末尾热区：最后一行若是子任务，它的下边缘只能插到父级内部，所以补一条
        // "追加到顶层末尾"的落点，否则想排到最末无处可落。
        if !roots.is_empty() {
            out.push(render_drop_tail());
        }
        out
    }

    fn render_todo_node(
        &mut self,
        todo: &TodoItem,
        all: &[TodoItem],
        tags: &[Tag],
        depth: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let children = child_ids_of(all, &todo.id);
        // Affordances are driven by ALL todos, not just the visible set: a
        // parent whose subtasks are today-hidden must still stay non-toggleable.
        let has_children = !child_ids_of(&self.todos, &todo.id).is_empty();
        let collapsed = self.collapsed.contains(&todo.id);
        let row = self
            .render_todo_row(
                todo,
                tags,
                depth,
                has_children,
                collapsed,
                window,
                cx,
            )
            .into_any_element();
        div()
            .v_flex()
            .child(row)
            .when(!collapsed, |el| {
                el.children(children.into_iter().map(|child| {
                    self.render_todo_node(child, all, tags, depth + 1, window, cx)
                }))
            })
            .into_any_element()
    }

    fn render_input_row(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shortcut = self.settings.add_todo_shortcut.clone();
        let input = self.new_todo_input.clone();

        div()
            .flex()
            .gap_2()
            .items_center()
            .capture_key_down({
                move |event, window, cx| {
                    let ks = &event.keystroke;
                    // Only the configured add shortcut submits (the default
                    // config is a bare Enter); any other keystroke is left to
                    // the input.
                    if !add_shortcut_matches(ks, &shortcut) {
                        return;
                    }
                    if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                        let app = handle.0.clone();
                        let input = input.clone();
                        app.update(cx, |app, cx| {
                            let val = input.read(cx).value().trim().to_string();
                            if !val.is_empty() {
                                app.add_todo(val, cx);
                                input.update(cx, |s, cx| s.set_value("", window, cx));
                            }
                        });
                    }
                }
            })
            .child(
                Input::new(&self.new_todo_input)
                    .flex_1()
                    .appearance(false)
                    .px(px(12.))
                    .py(px(8.))
                    .rounded_lg()
                    .bg(cx.theme().muted),
            )
    }

    fn render_todo_row(
        &mut self,
        todo: &TodoItem,
        tags: &[Tag],
        depth: usize,
        has_children: bool,
        collapsed: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = todo.id.clone();
        let completed = todo.completed;
        let content = todo.content.clone();
        let tag_id = todo.tag_id.clone();
        let tag_for_display: Option<(SharedString, SharedString)> = tag_id
            .as_ref()
            .and_then(|tid| tags.iter().find(|t| t.id == *tid))
            .map(|t| (t.name.clone(), t.color.clone().into()));
        // 分类只在"全部"筛选下、且仅在最顶层任务上展示：让用户在混合列表里看清每
        // 项归属，而又不重复刷屏。已按分类筛选时分类恒定可见、子任务继承父分类，
        // 两者都不必再贴胶囊。
        let cat_for_display: Option<(SharedString, SharedString)> =
            if self.cat_filter == CatFilter::None && todo.parent_id.is_none() {
                todo.cat_id
                    .as_ref()
                    .and_then(|cid| self.settings.categories.iter().find(|c| c.id == *cid))
                    .map(|c| (c.name.clone(), c.color.clone().into()))
            } else {
                None
            };
        let is_editing = self.editing_id.as_deref() == Some(&*id);
        // "长按完成" 模式下：整行是可长按区域，行背景用绿色进度条从左到右填充，
        // 走到头即 toggle 完成。is_editing / has_children 的行不参与长按。
        let longpress = self.settings.completion_mode == "longpress";
        let pressing_frac = if longpress {
            self.long_press
                .as_ref()
                .and_then(|(pid, f)| (pid == &id).then_some(*f))
        } else {
            None
        };
        let press_secs = (self.settings.long_press_duration.max(1)) as f32;
        // 子任务每深一层多缩一格，让子任务的箭头正对父级文字起点。拖拽热区也用
        // 同一个缩进，插入线才和该层的内容起点对齐。
        let indent_step = if longpress {
            SUBTASK_INDENT_LONGPRESS
        } else {
            SUBTASK_INDENT
        };
        let indent = 12. + indent_step * depth as f32;
        // 拖拽落点的合法性（can_drop 只做视觉反馈，真正的校验在 drop_todo）：
        // · 行边缘热区把行插到"本行的父级"这一层 → 本行及其祖先都不能拖到这里，
        //   否则父级会被塞进自己的子树里；
        // · 标题正文区是"成为本行的最后一个子任务" → 本行整棵子树都不能拖上来。
        let band_blockers = ancestor_ids(&self.todos, &id);
        let into_blockers = collect_subtree(&self.todos, &id);

        div()
            .id(ElementId::Name(id.clone().into()))
            .flex()
            .items_center()
            .gap_1()
            .px(px(12.))
            .py(px(8.))
            .rounded_lg()
            // 长按模式下的绿色完成进度条作为背景填充，clip 到圆角行。
            .relative()
            .overflow_hidden()
            .when_some(pressing_frac, |el, frac| {
                el.child(
                    Progress::new(ElementId::Name(format!("press-{}", &id).into()))
                        .value(frac * 100.0)
                        .absolute()
                        .inset_0()
                        .h_full()
                        // gpui component 层的 Progress 自带 transition 平滑动画，
                        // 逐帧推进 target 会导致指示条永远追不上、视觉走完时间
                        // 远超设置的秒数。这里用底层三件套（无动画），宽度直接
                        // 由代码按真实墙钟推进，3 秒恰好在第 3 秒走到头。
                        // 行的 overflow_hidden 只按矩形裁剪、不按圆角裁剪，故
                        // track/indicator 需自带与行一致的 rounded_lg（8px）
                        // 圆角，避免绿色填充左上/左下直角溢出到行外。
                        .child(
                            ProgressTrack::new()
                                .absolute()
                                .size_full()
                                .rounded_lg()
                                .bg(cx.theme().success.opacity(0.2)),
                        )
                        .child(
                            ProgressIndicator::new()
                                .absolute()
                                .top_0()
                                .left_0()
                                .h_full()
                                .rounded_lg()
                                .bg(cx.theme().success)
                                .w(relative(frac.min(1.0))),
                        ),
                )
            })
            .pl(px(indent))
            .hover(|s| s.bg(cx.theme().muted))
            // 拖拽源：整行可拖（编辑态除外，否则会抢走输入框里的选字手势）。拖动
            // 超过 gpui 的 2px 阈值才成立，单击/双击不受影响。
            .when(!is_editing, |el| {
                el.cursor_grab().on_drag(
                    TodoDrag {
                        id: id.clone().into(),
                        content: content.clone().into(),
                    },
                    move |drag, _offset, _window, cx| {
                        let mut descendants = 0;
                        if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                            let app = handle.0.clone();
                            // 一开始拖就撤销长按计时，否则拖着拖着会被"完成"截胡。
                            app.update(cx, |app, cx| {
                                if app.long_press.is_some() {
                                    app.long_press = None;
                                    cx.notify();
                                }
                            });
                            let todos = app.read(cx);
                            descendants = collect_subtree(&todos.todos, &drag.id).len() - 1;
                        }
                        cx.new(|_| DragGhost {
                            content: drag.content.clone(),
                            descendants,
                        })
                    },
                )
            })
            // 长按完成模式：整行都是可长按完成区域（父任务/编辑中除外）。
            .when(longpress && !has_children && !is_editing, |el| {
                let down_id = id.clone();
                let secs = press_secs;
                el.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                        let app = handle.0.clone();
                        app.update(cx, |app, cx| {
                            app.long_press = Some((down_id.clone(), 0.0));
                            cx.notify();
                        });
                        let tick_id = down_id.clone();
                        window
                            .spawn(&*cx, async move |async_cx| {
                                let start = Instant::now();
                                loop {
                                    async_cx
                                        .background_executor()
                                        .timer(Duration::from_millis(16))
                                        .await;
                                    let frac = (start.elapsed().as_secs_f32() / secs).min(1.0);
                                    let done = frac >= 1.0;
                                    let keep_going = async_cx
                                        .update(|_w, app_cx| {
                                            let mut keep = false;
                                            if let Some(handle) =
                                                app_cx.try_global::<DoitAppHandle>()
                                            {
                                                let a = handle.0.clone();
                                                a.update(app_cx, |app, cx| {
                                                    // 已被 mouse_up / hover out 取消。
                                                    if app
                                                        .long_press
                                                        .as_ref()
                                                        .map(|(i, _)| i)
                                                        != Some(&tick_id)
                                                    {
                                                        return;
                                                    }
                                                    if done {
                                                        app.long_press = None;
                                                        app.toggle_todo(&tick_id, cx);
                                                    } else {
                                                        app.long_press =
                                                            Some((tick_id.clone(), frac));
                                                        cx.notify();
                                                    }
                                                    keep = true;
                                                });
                                            }
                                            keep
                                        })
                                        .unwrap_or(false);
                                    if done || !keep_going {
                                        break;
                                    }
                                }
                            })
                            .detach();
                    }
                })
                .on_mouse_up(gpui::MouseButton::Left, {
                    let up_id = id.clone();
                    move |_, _, cx| {
                        if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                            let app = handle.0.clone();
                            app.update(cx, |app, cx| {
                                if app.long_press.as_ref().map(|(i, _)| i) == Some(&up_id) {
                                    app.long_press = None;
                                    cx.notify();
                                }
                            });
                        }
                    }
                })
                .on_hover({
                    let out_id = id.clone();
                    move |hovered, _, cx| {
                        if !hovered {
                            if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                let app = handle.0.clone();
                                app.update(cx, |app, cx| {
                                    if app.long_press.as_ref().map(|(i, _)| i) == Some(&out_id) {
                                        app.long_press = None;
                                        cx.notify();
                                    }
                                });
                            }
                        }
                    }
                })
            })
            // 右键打开上下文菜单那一帧，gpui 的菜单会在 prepaint 阶段抢占焦点，而此前
            // 聚焦的输入框在本帧更早的 prepaint 里已上报过 a11y 焦点，两处 set_focus
            // 触发 gpui 的 "set_focus called more than once" 断言导致 debug 崩溃。
            // 延迟到下一帧 render 前清焦：菜单成为该帧唯一焦点；菜单关闭时 gpui 仍会
            // 恢复这里保存的原焦点（previous_focus 在事件阶段就已捕获，不受影响）。
            .on_mouse_down(gpui::MouseButton::Right, |_event, window, cx| {
                // 延迟清焦，让菜单成为下一帧唯一 a11y 焦点（见上方说明）。
                window.defer(cx, |window, cx| window.blur(cx));
            })
            .context_menu({
                let id_for_menu = id.clone();
                let menu_tags = tags.to_vec();
                let menu_cats = self.settings.categories.clone();
                let current_tag = tag_id.clone();
                let current_cat = todo.cat_id.clone();
                let content_for_edit = content.clone();
                let todo_has_reminder = todo.remind_at.is_some();
                // 分类只归属顶层任务，子任务（有 parent）不提供"移动到分类"菜单。
                let is_subtask = todo.parent_id.is_some();
                move |menu, window, cx| {
                    let id_for_menu = id_for_menu.clone();
                    menu.item(
                        PopupMenuItem::new("编辑")
                            .on_click({
                                let id_for_menu = id_for_menu.clone();
                                let content_for_edit = content_for_edit.clone();
                                move |_, window, cx| {
                                    if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                        let app = handle.0.clone();
                                        app.update(cx, |app, cx| {
                                            app.start_edit(
                                                &id_for_menu,
                                                &content_for_edit,
                                                window,
                                                cx,
                                            );
                                        });
                                    }
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("添加子任务")
                            .icon(IconName::Plus)
                            .on_click({
                                let id_for_menu = id_for_menu.clone();
                                move |_, window, cx| {
                                    if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                        let app = handle.0.clone();
                                        app.update(cx, |app, cx| {
                                            app.add_subtask(&id_for_menu, window, cx);
                                        });
                                    }
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new(if todo_has_reminder {
                            "修改提醒"
                        } else {
                            "设置提醒"
                        })
                        .icon(gpui_kit::assets::IconName::Clock)
                        .on_click({
                            let id_for_menu = id_for_menu.clone();
                            let has = todo_has_reminder;
                            move |_, window, cx| {
                                if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                    let app = handle.0.clone();
                                    app.update(cx, |app, cx| {
                                        app.open_reminder_dialog(
                                            &id_for_menu,
                                            has,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }
                        }),
                    )
                    .submenu("设置标签", window, cx, {
                        let menu_tags = menu_tags.clone();
                        let id_for_menu = id_for_menu.clone();
                        let current_tag = current_tag.clone();
                        move |sub, _window, _cx| {
                            let mut m = sub;
                            for tag in &menu_tags {
                                let tag_id = tag.id.clone();
                                let tag_name = tag.name.clone();
                                let tag_color = tag.color.clone();
                                let active = current_tag.as_deref() == Some(tag_id.as_str());
                                let id_for_menu = id_for_menu.clone();
                                m = m.item(
                                    PopupMenuItem::element(move |_window, cx| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(color_dot(&tag_color, cx))
                                            .child(tag_name.clone())
                                    })
                                    .checked(active)
                                    .on_click(move |_, _, cx| {
                                        if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                            let app = handle.0.clone();
                                            let id_for_menu = id_for_menu.clone();
                                            app.update(cx, |app, cx| {
                                                app.set_tag(
                                                    &id_for_menu,
                                                    Some(tag_id.clone()),
                                                    cx,
                                                );
                                            });
                                        }
                                    }),
                                );
                            }
                            m = m.item(
                                PopupMenuItem::new("清除标签")
                                    .icon(IconName::Close)
                                    .on_click({
                                        let id_for_menu = id_for_menu.clone();
                                        move |_, _, cx| {
                                            if let Some(handle) =
                                                cx.try_global::<DoitAppHandle>()
                                            {
                                                let app = handle.0.clone();
                                                app.update(cx, |app, cx| {
                                                    app.set_tag(&id_for_menu, None, cx);
                                                });
                                            }
                                        }
                                    }),
                            );
                            m
                        }
                    })
                    .when(!is_subtask, |m| {
                        m.submenu("移动到分类", window, cx, {
                        let menu_cats = menu_cats.clone();
                        let id_for_menu = id_for_menu.clone();
                        let current_cat = current_cat.clone();
                        move |sub, _window, _cx| {
                            let mut m = sub;
                            for cat in &menu_cats {
                                let cat_id = cat.id.clone();
                                let cat_name = cat.name.clone();
                                let cat_color = cat.color.clone();
                                let active =
                                    current_cat.as_deref() == Some(cat_id.as_str());
                                let id_for_menu = id_for_menu.clone();
                                m = m.item(
                                    PopupMenuItem::element(move |_window, cx| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(color_dot(&cat_color, cx))
                                            .child(cat_name.clone())
                                    })
                                    .checked(active)
                                    .on_click(move |_, _, cx| {
                                        if let Some(handle) =
                                            cx.try_global::<DoitAppHandle>()
                                        {
                                            let app = handle.0.clone();
                                            let id_for_menu = id_for_menu.clone();
                                            app.update(cx, |app, cx| {
                                                app.set_cat(
                                                    &id_for_menu,
                                                    Some(cat_id.clone()),
                                                    cx,
                                                );
                                            });
                                        }
                                    }),
                                );
                            }
                            m = m.item(
                                PopupMenuItem::new("未分类")
                                    .checked(current_cat.is_none())
                                    .on_click({
                                        let id_for_menu = id_for_menu.clone();
                                        move |_, _, cx| {
                                            if let Some(handle) =
                                                cx.try_global::<DoitAppHandle>()
                                            {
                                                let app = handle.0.clone();
                                                app.update(cx, |app, cx| {
                                                    app.set_cat(&id_for_menu, None, cx);
                                                });
                                            }
                                        }
                                    }),
                            );
                            m
                        }
                    })
                    })
                    .separator()
                    .item(
                        PopupMenuItem::element(move |_window, cx| {
                            div()
                                .text_color(cx.theme().danger)
                                .child("删除")
                        })
                        .on_click({
                            let id_for_menu = id_for_menu.clone();
                            move |_, _, cx| {
                                if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                    let app = handle.0.clone();
                                    app.update(cx, |app, cx| {
                                        app.delete_todo(&id_for_menu, cx);
                                    });
                                }
                            }
                        }),
                    )
                }
            })
            .child(
                // Collapse toggle for parents with subtasks; spacer otherwise.
                if has_children {
                    Button::new(ElementId::Name(format!("chev-{}", &id).into()))
                        .ghost()
                        .icon(Icon::empty().path(if collapsed {
                            "icons/chevron-right.svg"
                        } else {
                            "icons/chevron-down.svg"
                        }))
                        .size(px(14.))
                        .tooltip(if collapsed { "展开子任务" } else { "收起子任务" })
                        .on_click({
                            let cid = id.clone();
                            move |_, _, cx| {
                                if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                    let app = handle.0.clone();
                                    app.update(cx, |app, cx| app.toggle_collapse(&cid, cx));
                                }
                            }
                        })
                        .into_any_element()
                } else {
                    div().size(px(16.)).flex_none().into_any_element()
                },
            )
            .child(
                if longpress {
                    // 长按完成模式：没有勾选框，也就不需要保留占位槽，箭头右侧
                    // 直接接文字（半个字间距），避免行中间留一段透明空洞。
                    div().flex_none().into_any_element()
                } else {
                    Checkbox::new(ElementId::Name(format!("chk-{}", &id).into()))
                        .checked(completed)
                        // A parent's completion is derived from its subtasks.
                        .disabled(has_children)
                        .on_click({
                            let id = id.clone();
                            move |_, _, cx| {
                                if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                    let app = handle.0.clone();
                                    app.update(cx, |app, cx| app.toggle_todo(&id, cx));
                                }
                            }
                        })
                        .into_any_element()
                },
            )
            .child(
                if is_editing {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Input::new(&self.edit_input).flex_1())
                        .child(
                            Button::new(ElementId::Name(format!("edit-save-{}", &id).into()))
                                .small()
                                .ghost()
                                .label("保存")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.commit_edit(cx);
                                })),
                        )
                        .child(
                            Button::new(ElementId::Name(format!("edit-cancel-{}", &id).into()))
                                .small()
                                .ghost()
                                .label("取消")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_edit(cx);
                                })),
                        )
                        .into_any_element()
                } else {
                    let edit_id = id.clone();
                    let edit_content = content.clone();
                    div()
                        .flex_1()
                        .min_w_0()
                        .id(ElementId::Name(format!("title-{}", &id).into()))
                        .text_color(if completed {
                            cx.theme().muted_foreground
                        } else {
                            cx.theme().foreground
                        })
                        .when(completed, |el| el.line_through())
                        .rounded_sm()
                        // 拖到标题正文上 = 成为本行的最后一个子任务。标注/勾选框
                        // 不参与这个落点，避免"拖到复选框上想插到前面"时误嵌套。
                        .can_drop({
                            let blocked = into_blockers.clone();
                            move |value, _, _| {
                                value.downcast_ref::<TodoDrag>().is_some_and(|drag| {
                                    !blocked.iter().any(|id| id == &*drag.id)
                                })
                            }
                        })
                        .on_drop::<TodoDrag>({
                            let slot = DropSlot::LastChild(id.clone());
                            move |drag, _window, cx| {
                                if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                    let app = handle.0.clone();
                                    let id = drag.id.to_string();
                                    app.update(cx, |app, cx| app.drop_todo(&id, &slot, cx));
                                    cx.stop_propagation();
                                }
                            }
                        })
                        .drag_over::<TodoDrag>(move |style, _, _, cx| {
                            style
                                .bg(cx.theme().drop_target.opacity(0.25))
                                .rounded_sm()
                        })
                        .on_click(move |event, window, cx| {
                            // 双击标题进入编辑（Finder/资源管理器的重命名习惯）。
                            // 单击不承担动作：勾选靠勾选框，长按靠按住整行，所以
                            // 这里只认双击，避免误触。
                            if event.click_count() != 2 {
                                return;
                            }
                            if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                let app = handle.0.clone();
                                app.update(cx, |app, cx| {
                                    app.start_edit(&edit_id, &edit_content, window, cx)
                                });
                            }
                        })
                        .child(content.clone())
                        .into_any_element()
                },
            )
            .when(!is_editing, |el| {
                // 行尾徽章：标签是胶囊形，分类是偏方形描边徽章，两者形状区分。
                el.when_some(tag_for_display, |el, (name, color)| {
                    el.child(render_chip(name, &color, false))
                })
                .when_some(cat_for_display, |el, (name, color)| {
                    el.child(render_chip(name, &color, true))
                })
                // 倒计时：仅在提醒尚未到点(还有剩余时间)时显示；到点即由 ticker
                // 清除提醒，行尾不残留任何文字。
                .when_some(
                    todo.remind_at.as_ref().and_then(|at| {
                        reminder_remaining_secs(&Some(at.clone()))
                    }),
                    |el, secs| {
                        el.child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(Icon::new(gpui_kit::assets::IconName::Clock).size_3())
                                .child(format_countdown(secs)),
                        )
                    },
                )
            })
            // 拖拽落点热区：上下各 8px 的贴边条，落在上面 = 插到本行的前/后。
            // 最后画（就在最上层），且只占行的内边距区域，不会挡勾选框/文字的
            // 点击、双击。
            .children(if is_editing {
                Vec::new()
            } else {
                // 有展开的子任务时，下边缘自然接的是"第一个子任务"，直接插到
                // 父级内部比插到父级之后更符合手感。
                let bottom = if has_children && !collapsed {
                    DropSlot::FirstChild(id.clone())
                } else {
                    DropSlot::After(id.clone())
                };
                vec![
                    render_drop_band(
                        BandEdge::Top,
                        DropSlot::Before(id.clone()),
                        indent,
                        band_blockers.clone(),
                    ),
                    render_drop_band(BandEdge::Bottom, bottom, indent, band_blockers),
                ]
            })
    }
}

// ── Calendar View ─────────────────────────────────────────────────────────

impl DoitApp {
    fn render_calendar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_cal_date.clone();
        let muted = cx.theme().muted_foreground;
        let items: Vec<TodoItem> = self
            .todos
            .iter()
            .filter(|t| t.completed)
            .filter(|t| t.completed_at.as_deref().map(date_part) == Some(selected.clone()))
            .cloned()
            .collect();

        div()
            .v_flex()
            .gap_4()
            .p_4()
            .child(div().text_lg().font_bold().child("日历浏览"))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_5()
                    // Interactive month grid; days with completed items carry a dot.
                    .child(
                        div()
                            .w(px(380.))
                            .child(self.render_calendar_grid(cx)),
                    )
                    // Completed items for the selected date.
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_bold()
                                    .child(format!(
                                        "{} · 完成 {} 个待办",
                                        weekday_cn(&selected),
                                        items.len()
                                    )),
                            )
                            .when(items.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_sm()
                                        .text_color(muted)
                                        .child("这一天还没有完成记录"),
                                )
                            })
                            .children(items.iter().map(|item| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .py(px(4.))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(format_time(&item.completed_at)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .line_through()
                                            .text_color(muted)
                                            .child(item.content.clone()),
                                    )
                                    .into_any_element()
                            })),
                    ),
            )
    }

    /// Month grid, driven by [`CalendarState`] (selection, month nav, today).
    ///
    /// The published `Calendar` component cannot surface per-day data to its
    /// item slot (no date is exposed there), so the day view is rendered here
    /// and a small dot is added under every day that has completed items.
    fn render_calendar_grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let completed: HashSet<String> = self
            .todos
            .iter()
            .filter(|t| t.completed && t.completed_at.is_some())
            .filter_map(|t| t.completed_at.as_deref().map(date_part))
            .collect();

        // Hoist theme values into Copy locals so the render closures below
        // only capture owned `cal` clones and plain values, never `cx`.
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let secondary_hover = cx.theme().tokens.secondary_hover;
        let accent = cx.theme().accent;
        let accent_foreground = cx.theme().accent_foreground;
        let primary = cx.theme().primary;
        let primary_foreground = cx.theme().primary_foreground;
        let success = cx.theme().success;
        let radius = cx.theme().radius;
        let border = cx.theme().border;
        let radius_lg = cx.theme().radius_lg;

        let state = self.calendar_state.read(cx);
        let month = state.current_month() as u32;
        let year = state.current_year();
        let today = state.today();
        let selected = match state.date() {
            Date::Single(Some(d)) => d,
            _ => today,
        };
        let weeks = state.month_days().into_iter().next().unwrap_or_default();
        let cal = self.calendar_state.clone();

        const WEEKDAYS: [&str; 7] = ["日", "一", "二", "三", "四", "五", "六"];

        let nav_button = |is_prev: bool| -> AnyElement {
            let cal = cal.clone();
            let icon = if is_prev {
                Icon::new(IconName::ChevronLeft)
            } else {
                Icon::new(IconName::ChevronRight)
            };
            div()
                .id(ElementId::Name(if is_prev {
                    "cal-nav-prev".into()
                } else {
                    "cal-nav-next".into()
                }))
                .size_6()
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius)
                .text_color(foreground)
                .hover(move |this| this.bg(secondary_hover))
                .child(icon.size_4())
                .on_click(move |_, _, cx| {
                    cal.update(cx, |s, cx| {
                        if is_prev {
                            s.prev_month();
                        } else {
                            s.next_month();
                        }
                        cx.notify();
                    });
                })
                .into_any_element()
        };

        div()
            .w(px(220.))
            .border_1()
            .border_color(border)
            .rounded(radius_lg)
            .p_3()
            .v_flex()
            .gap_0p5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(nav_button(true))
                    .child(div().text_sm().font_medium().child(format!("{year} 年 {month} 月")))
                    .child(nav_button(false)),
            )
            .child(
                div()
                    .flex()
                    .gap_0p5()
                    .children(WEEKDAYS.iter().map(|name| {
                        div()
                            .flex_1()
                            .size_6()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .font_normal()
                            .text_color(muted)
                            .child(*name)
                    })),
            )
            .children(weeks.into_iter().map(move |week| {
                let mut row = div().flex().gap_0p5();
                for date in week {
                    let has_dot = completed.contains(&date.to_string());
                    let is_today = date == today;
                    let is_selected = date == selected;
                    let is_muted_day = date.month() != month;
                    let day_label = date.day().to_string();
                    let cal = cal.clone();
                    row = row.child(
                        div()
                            .flex_1()
                            .v_flex()
                            .items_center()
                            .gap_0p5()
                            .child(
                                div()
                                    .id(ElementId::Name(format!("cal-day-{date}").into()))
                                    // `size_6` (24px) < the flex column (~26px):
                                    // the selected/today/hover fill stays inside
                                    // the grid instead of spilling past its edge.
                                    .size_6()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(radius / 2.)
                                    .text_xs()
                                    .aria_label(if has_dot {
                                        format!("{date}，有已完成事项")
                                    } else {
                                        date.to_string()
                                    })
                                    .when(is_muted_day, |this| this.text_color(muted))
                                    .when(
                                        !is_muted_day && !is_today && !is_selected,
                                        |this| {
                                            this.hover(move |this| {
                                                this.bg(secondary_hover).text_color(foreground)
                                            })
                                        },
                                    )
                                    .when(is_today && !is_selected, move |this| {
                                        this.bg(accent).text_color(accent_foreground)
                                    })
                                    .when(is_selected, move |this| {
                                        this.bg(primary).text_color(primary_foreground)
                                    })
                                    .child(day_label)
                                    .on_click(move |_, _, cx| {
                                        cal.update(cx, |s, cx| s.activate_date(date, cx));
                                    }),
                            )
                            .child(
                                div()
                                    .h_1p5()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(has_dot, move |this| {
                                        // `success`-tinted dot signals a day with
                                        // completed items; the aria label above
                                        // keeps that status beyond colour alone.
                                        this.child(div().size_1p5().rounded_full().bg(success))
                                    }),
                            ),
                    );
                }
                row.into_any_element()
            }))
    }
}
impl DoitApp {
    fn render_timeline(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.completed_by_day();
        let is_empty = groups.is_empty();
        let muted = cx.theme().muted_foreground;
        let accent = cx.theme().chart_1;
        let rail_color = cx.theme().border;
        let background = cx.theme().background;

        // Left rail layout: a narrow date column, then a 16px rail column that
        // carries one ring per day, then the events. Rows stay contiguous (all
        // spacing lives inside a row) and every text line is a fixed 20px box, so
        // the ring lands on the day's first event line and the per-row segments
        // join into one continuous line.
        const DATE_W: f32 = 58.;
        const RAIL_W: f32 = 16.;
        const TEXT_H: f32 = 20.;
        const RING: f32 = 10.;
        // Rail column centre, minus half a pixel so a 1px line reads crisp.
        const RAIL_X: f32 = DATE_W + RAIL_W / 2. - 0.5;
        // Ring centre in the first group, and the last event's text bottom.
        const RAIL_TOP_TRIM: f32 = 13.;
        const RAIL_BOTTOM_TRIM: f32 = 3.;

        let mut day_groups: Vec<AnyElement> = Vec::new();
        for (index, (day, items)) in groups.into_iter().enumerate() {
            let gap_top = if index == 0 { px(0.) } else { px(10.) };
            day_groups.push(
                div()
                    .h_flex()
                    .items_start()
                    .pt(gap_top)
                    .child(
                        div()
                            .w(px(DATE_W))
                            .flex_none()
                            .pt(px(3.))
                            .v_flex()
                            .items_end()
                            .child(
                                div()
                                    .h(px(TEXT_H))
                                    .text_sm()
                                    .font_bold()
                                    .child(month_day(&day)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(weekday_cn(&day)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(RAIL_W))
                            .flex_none()
                            .flex()
                            .justify_center()
                            .pt(px(8.))
                            .child(
                                div()
                                    .size(px(RING))
                                    .flex_none()
                                    .rounded_full()
                                    .border_2()
                                    .border_color(accent)
                                    // Opaque centre so the rail stops at the ring
                                    // instead of running through its hole.
                                    .bg(background),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .v_flex()
                            .children(items.into_iter().map(|item| {
                                div()
                                    .h_flex()
                                    .py(px(3.))
                                    .child(
                                        div()
                                            .h(px(TEXT_H))
                                            .w(px(44.))
                                            .flex_none()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(format_time(&item.completed_at)),
                                    )
                                    .child(
                                        div()
                                            .h(px(TEXT_H))
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(item.content.clone()),
                                    )
                            })),
                    )
                    .into_any_element(),
            );
        }

        div()
            .v_flex()
            .gap_4()
            .p_6()
            .child(div().text_lg().font_bold().child("时间轴"))
            .when(is_empty, |el| {
                el.child(
                    div()
                        .py_10()
                        .text_color(cx.theme().muted_foreground)
                        .child("还没有完成记录，完成一条待办点亮你的时间轴"),
                )
            })
            .when(!is_empty, |el| {
                el.child(
                    div()
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .left(px(RAIL_X))
                                .top(px(RAIL_TOP_TRIM))
                                .bottom(px(RAIL_BOTTOM_TRIM))
                                .w(px(1.))
                                .bg(rail_color),
                        )
                        .child(div().v_flex().children(day_groups)),
                )
            })
    }
}

// ── Stats View ─────────────────────────────────────────────────────────────

impl DoitApp {
    fn render_stats(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let today = Local::now().date_naive();
        let (start, end) =
            range_bounds(self.stats_period, self.stats_month, self.stats_week_offset, today);

        // Bar: completed count per month over the last 12 months.
        let (cy, cm) = current_ym();
        let mut bars = Vec::new();
        for k in (0..12).rev() {
            let (y, m) = add_months(cy, cm, -(k as i32));
            bars.push(BarDatum {
                label: format!("{:04}-{:02}", y, m),
                value: self.completed_in_month(y, m) as f64,
            });
        }

        // Line: day-by-day completed counts within the selected range.
        let daily = self.daily_completed(start, end);

        // Pie: completed vs incomplete within the range.
        let done = self.completed_in_range(start, end);
        let open = self.incomplete_in_range(start, end);
        // Completed shares the mint fill with the bar chart; the remainder is a
        // soft neutral so the ring reads airy instead of heavy charcoal.
        let pie_open = cx.theme().muted_foreground.opacity(0.3);
        let pies = vec![
            PieDatum {
                value: done as f32,
                color: cx.theme().chart_1,
            },
            PieDatum {
                value: open as f32,
                color: pie_open,
            },
        ];

        let chart_1 = cx.theme().chart_1;
        let chart_2 = cx.theme().chart_2;
        let border = cx.theme().border;
        let radius = cx.theme().radius_lg;

        div()
            .v_flex()
            .gap_4()
            .p_4()
            .child(div().text_lg().font_bold().child("统计"))
            .child(self.render_stats_controls(cx))
            .child(self.render_heatmap(cx))
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .border_1()
                            .border_color(border)
                            .rounded(radius)
                            .p_4()
                            .v_flex()
                            .gap_2()
                            .child(div().text_sm().font_bold().child("近 12 个月完成数量"))
                            .child(
                                div().h(px(220.)).child(
                                    BarChart::new(bars)
                                        .band(|d| d.label.clone())
                                        .value(|d| d.value)
                                        .fill_gradient(move |_, _, _| {
                                            [
                                                linear_color_stop(chart_1, 0.0),
                                                linear_color_stop(chart_1.opacity(0.35), 1.0),
                                            ]
                                        })
                                        .value_axis(true)
                                        .tick_margin(2),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .w(px(300.))
                            .flex_shrink_0()
                            .border_1()
                            .border_color(border)
                            .rounded(radius)
                            .p_4()
                            .v_flex()
                            .gap_2()
                            .child(div().text_sm().font_bold().child("已完成 / 未完成"))
                            .child(
                                div()
                                    .h(px(200.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        PieChart::new(pies)
                                            .value(|d| d.value)
                                            .color(|d| d.color)
                                            .outer_radius(80.)
                                            .inner_radius(48.),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap_4()
                                    .child(stats_legend(chart_1, "已完成", done, cx))
                                    .child(stats_legend(pie_open, "未完成", open, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .border_1()
                    .border_color(border)
                    .rounded(radius)
                    .p_4()
                    .v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_bold()
                            .child(format!(
                                "每日完成趋势 · {}",
                                range_label(start, end)
                            )),
                    )
                    .child(
                        div().h(px(200.)).child(
                            LineChart::new(daily)
                                .x(|d| d.label.clone())
                                .y(|d| d.value)
                                .stroke(chart_2)
                                .dot()
                                .tick_margin(3),
                        ),
                    ),
            )
    }

    /// GitHub-style heatmap: one cell per day for the last 52 weeks; the more
    /// todos completed on a day, the deeper green that cell. Matches the
    /// "每日完成热点图" reference card (month axis on top, weekday labels on
    /// the left, 少/多 legend on the right).
    fn render_heatmap(&self, cx: &Context<Self>) -> impl IntoElement {
        let today = Local::now().date_naive();
        let this_monday = today - Days::new(today.weekday().num_days_from_monday() as u64);
        let start = this_monday - Days::new(51 * 7);
        let end = this_monday + Days::new(6);

        // Per-day completed counts across [start, end], built in one pass.
        let mut counts: HashMap<NaiveDate, usize> = HashMap::new();
        let mut total = 0usize;
        for t in &self.todos {
            if !(t.completed && t.completed_at.is_some()) {
                continue;
            }
            let day = t
                .completed_at
                .as_deref()
                .and_then(|iso| NaiveDate::parse_from_str(&date_part(iso), "%Y-%m-%d").ok());
            if let Some(d) = day {
                if d >= start && d <= end {
                    *counts.entry(d).or_default() += 1;
                    total += 1;
                }
            }
        }
        let active = counts.values().filter(|&&n| n > 0).count();

        let bg = cx.theme().background;
        let mute = cx.theme().muted_foreground;
        // Same mint family as the bar/line charts so the whole stats page
        // speaks one accent system.
        let accent = cx.theme().chart_1;
        // 5-step ramp: empty -> faint -> deeper greens -> full accent.
        let empty_c = bg.blend(mute.alpha(0.18));
        let c1 = bg.blend(accent.alpha(0.28));
        let c2 = bg.blend(accent.alpha(0.5));
        let c3 = bg.blend(accent.alpha(0.75));
        let heat_color = |n: usize| -> Hsla {
            match n {
                0 => empty_c,
                1 => c1,
                2..=3 => c2,
                4..=5 => c3,
                _ => accent,
            }
        };

        // Label each column owned by the week whose Monday falls in the first
        // few days of its month (column 0 always labelled).
        let mut month_cols: HashMap<usize, String> = HashMap::new();
        for w in 0..52 {
            let monday = start + Days::new(w as u64 * 7);
            if w == 0 || monday.day() <= 7 {
                month_cols.insert(w, format!("{}月", monday.month()));
            }
        }

        // 格子尺寸：宽度由 flex 均分，高度必须显式给出才是正方形。
        // 可用宽度 = 左侧星期栏 16 + 栏间距 4 + 52 格 + 51 个 4px 间距。
        let avail = self.heatmap_width.get();
        let cell_h = if avail > 0.0 {
            ((avail - (16.0 + 4.0 + 51.0 * 4.0)) / 52.0).clamp(4.0, 64.0)
        } else {
            0.0
        };

        let cell = |row: usize| -> AnyElement {
            div()
                .flex()
                .flex_1()
                .items_start()
                .gap_1()
                .children((0..52).map(|w| {
                    let day = start + Days::new(w as u64 * 7 + row as u64);
                    let n = *counts.get(&day).unwrap_or(&0);
                    let c = if day > today { bg } else { heat_color(n) };
                    let cell_el = div()
                        .id(ElementId::NamedInteger(
                            "heatmap-cell".into(),
                            (row * 52 + w) as u64,
                        ))
                        .flex_1()
                        .min_w_0()
                        // 宽度由列均分，高度 = 实测可用宽度算出的单格边长：
                        // 格子始终是正方形，宽窗口下也不会被拉成扁条。
                        .h(px(cell_h))
                        .rounded(px(2.))
                        .bg(c);
                    // Hover a completed day to see the exact date and count.
                    // 使用 gpui 原生 tooltip 挂载 + 自带组件外观 + 入场动画。
                    let cell_el = if n > 0 {
                        cell_el.tooltip(move |window, cx| {
                            HeatmapTooltip::new(format!(
                                "{}：完成 {} 个待办",
                                day.format("%Y-%m-%d"),
                                n
                            ))
                            .build(window, cx)
                        })
                    } else {
                        cell_el
                    };
                    cell_el.into_any_element()
                }))
                .into_any_element()
        };

        let rows: Vec<AnyElement> = (0..7)
            .map(|row| {
                let weekday = ["一", "", "三", "", "五", "", ""][row];
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w(px(16.))
                            .h(px(cell_h.max(10.)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .whitespace_nowrap()
                            .text_xs()
                            .text_color(mute)
                            .child(weekday),
                    )
                    .child(cell(row))
                    .into_any_element()
            })
            .collect();

        // Month labels sit above their grid column inside the same 52-column
        // flex line. `whitespace_nowrap` keeps them on one horizontal line —
        // without it the narrow flex slot wraps each label into stacked glyphs.
        let month_row = div()
            .flex()
            .items_center()
            .gap_1()
            .child(div().w(px(16.)))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .gap_1()
                    .children((0..52).map(|w| {
                        div().flex_1().min_w_0()
                            .when_some(month_cols.get(&w), |el, label| {
                                el.whitespace_nowrap()
                                    .text_xs()
                                    .text_color(mute)
                                    .child(label.clone())
                            })
                            .into_any_element()
                    })),
            );

        let legend = div()
            .flex()
            .items_center()
            .justify_end()
            .gap_1()
            .child(div().text_xs().whitespace_nowrap().text_color(mute).child("少"))
            .children([empty_c, c1, c2, c3, accent].into_iter().map(|c| {
                div().size_2().rounded(px(2.)).bg(c).into_any_element()
            }))
            .child(div().text_xs().whitespace_nowrap().text_color(mute).child("多"));

        let border = cx.theme().border;
        let radius = cx.theme().radius_lg;
        div()
            .w_full()
            .border_1()
            .border_color(border)
            .rounded(radius)
            .p_4()
            .v_flex()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().font_bold().child("每日完成热点图"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(mute)
                            .child(format!("最近 52 周 · 共完成 {total} 个待办 · 活跃 {active} 天")),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .v_flex()
                    .gap_2()
                    // 先渲染宽度探针，回填本帧可用宽度供下一帧算格子尺寸。
                    .child(HeatmapWidthProbe {
                        width: self.heatmap_width.clone(),
                        app: cx.entity(),
                    })
                    .child(month_row)
                    .child(div().v_flex().gap_1().children(rows))
                    .child(legend),
            )
            .when(active == 0, |el| {
                el.child(
                    div()
                        .text_sm()
                        .text_color(mute)
                        .child("还没有完成记录，完成一条待办即可点亮你的第一格。"),
                )
            })
    }

    /// Range quick buttons + month navigation at the top of statistics.
    fn render_stats_controls(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cur = current_ym();
        let last = add_months(cur.0, cur.1, -1);
        let this_month_sel =
            self.stats_period == StatsPeriod::Month && self.stats_month == cur;
        let last_month_sel =
            self.stats_period == StatsPeriod::Month && self.stats_month == last;
        let this_week_sel =
            self.stats_period == StatsPeriod::Week && self.stats_week_offset == 0;
        let last_week_sel =
            self.stats_period == StatsPeriod::Week && self.stats_week_offset == -1;

        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("stats-this-week")
                    .small()
                    .ghost()
                    .when(this_week_sel, |b| b.selected(true))
                    .label("本周")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stats_period = StatsPeriod::Week;
                        this.stats_week_offset = 0;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("stats-last-week")
                    .small()
                    .ghost()
                    .when(last_week_sel, |b| b.selected(true))
                    .label("上周")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stats_period = StatsPeriod::Week;
                        this.stats_week_offset = -1;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("stats-this-month")
                    .small()
                    .ghost()
                    .when(this_month_sel, |b| b.selected(true))
                    .label("本月")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stats_period = StatsPeriod::Month;
                        this.stats_month = current_ym();
                        this.stats_week_offset = 0;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("stats-last-month")
                    .small()
                    .ghost()
                    .when(last_month_sel, |b| b.selected(true))
                    .label("上月")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let (y, m) = add_months(current_ym().0, current_ym().1, -1);
                        this.stats_period = StatsPeriod::Month;
                        this.stats_month = (y, m);
                        this.stats_week_offset = 0;
                        cx.notify();
                    })),
            )
            .child(div().w_1().h(px(16.)).bg(cx.theme().border))
            .child(
                Button::new("stats-month-prev")
                    .ghost()
                    .xsmall()
                    .icon(IconName::ChevronLeft)
                    .disabled(self.stats_period == StatsPeriod::Week)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.stats_period == StatsPeriod::Month {
                            this.stats_month =
                                add_months(this.stats_month.0, this.stats_month.1, -1);
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new("stats-month-next")
                    .ghost()
                    .xsmall()
                    .icon(IconName::ChevronRight)
                    .disabled(self.stats_period == StatsPeriod::Week)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.stats_period == StatsPeriod::Month {
                            this.stats_month =
                                add_months(this.stats_month.0, this.stats_month.1, 1);
                            cx.notify();
                        }
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .font_bold()
                    .child(format!(
                        "{:04}年{:02}月",
                        self.stats_month.0, self.stats_month.1
                    )),
            )
    }

    // Data helpers -----------------------------------------------------

    fn completed_in_month(&self, y: i32, m: u32) -> usize {
        let (s, e) = month_bounds(y, m);
        self.todos
            .iter()
            .filter(|t| {
                t.completed
                    && t.completed_at
                        .as_deref()
                        .map(|iso| iso_in_range(iso, s, e))
                        .unwrap_or(false)
            })
            .count()
    }

    fn completed_in_range(&self, s: NaiveDate, e: NaiveDate) -> usize {
        self.todos
            .iter()
            .filter(|t| {
                t.completed
                    && t.completed_at
                        .as_deref()
                        .map(|iso| iso_in_range(iso, s, e))
                        .unwrap_or(false)
            })
            .count()
    }

    fn incomplete_in_range(&self, s: NaiveDate, e: NaiveDate) -> usize {
        self.todos
            .iter()
            .filter(|t| {
                !t.completed && iso_in_range(&t.created_at, s, e)
            })
            .count()
    }

    fn daily_completed(&self, s: NaiveDate, e: NaiveDate) -> Vec<LineDatum> {
        let mut out = Vec::new();
        let mut d = s;
        while d <= e {
            let label = d.format("%m-%d").to_string();
            let count = self.todos.iter().filter(|t| {
                t.completed
                    && t.completed_at
                        .as_deref()
                        .map(|iso| iso_in_range(iso, d, d))
                        .unwrap_or(false)
            }).count();
            out.push(LineDatum { label, value: count as f64 });
            d = d + Days::new(1);
        }
        out
    }
}

// ── Small render helpers ────────────────────────────────────────────────────


// Statistics range + date helpers.

fn current_ym() -> (i32, u32) {
    let t = Local::now().date_naive();
    (t.year(), t.month())
}

fn add_months(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let total = (y as i64) * 12 + (m as i64 - 1) + delta as i64;
    ((total.div_euclid(12)) as i32, (total.rem_euclid(12) + 1) as u32)
}

fn first_of_month(y: i32, m: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

fn month_bounds(y: i32, m: u32) -> (NaiveDate, NaiveDate) {
    let (ny, nm) = add_months(y, m, 1);
    (first_of_month(y, m), first_of_month(ny, nm) - Days::new(1))
}

fn range_bounds(period: StatsPeriod, month: (i32, u32), week_offset: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    match period {
        StatsPeriod::Month => month_bounds(month.0, month.1),
        StatsPeriod::Week => {
            let wd = today.weekday().num_days_from_monday() as i64;
            let monday = today - Days::new(wd as u64);
            let start = monday + ChronoDuration::days(week_offset as i64 * 7);
            (start, start + ChronoDuration::days(6))
        }
    }
}

/// Whether an ISO datetime (or date) falls on [s, e].
fn iso_in_range(iso: &str, s: NaiveDate, e: NaiveDate) -> bool {
    NaiveDate::parse_from_str(&date_part(iso), "%Y-%m-%d")
        .map(|d| d >= s && d <= e)
        .unwrap_or(false)
}

fn range_label(s: NaiveDate, e: NaiveDate) -> String {
    if s.year() == e.year() && s.month() == e.month() {
        format!("{:04}年{:02}月", s.year(), s.month())
    } else {
        format!("{} 至 {}", s.format("%m月%d日"), e.format("%m月%d日"))
    }
}

fn stats_legend(color: Hsla, label: &str, value: usize, _cx: &gpui_kit::gpui::App) -> impl IntoElement {
    let label = label.to_string();
    div()
        .flex()
        .items_center()
        .gap_1_5()
        .child(div().size_2().rounded_full().bg(color))
        .child(div().text_sm().child(label))
        .child(
            div()
                .text_sm()
                .font_bold()
                .child(value.to_string()),
        )
}

/// True when a keystroke is exactly the user-configured add shortcut.
/// Shared by the todo input and the category/tag add inputs.
///
/// There is deliberately no implicit bare-Enter fallback: once the user sets
/// e.g. `Ctrl+Enter`, a plain Enter must not confirm an add. The default
/// configuration is a bare Enter, so out-of-the-box behavior is unchanged.
pub(crate) fn add_shortcut_matches(
    ks: &gpui_kit::gpui::Keystroke,
    sc: &ShortcutConfig,
) -> bool {
    ks.key.eq_ignore_ascii_case(sc.key.as_ref())
        && ks.modifiers.control == sc.ctrl
        && ks.modifiers.shift == sc.shift
        && ks.modifiers.alt == sc.alt
        && ks.modifiers.platform == sc.meta
}

/// True for a bare Enter with no modifiers — the keystroke dialog surfaces bind
/// to `Confirm`. An add-input whose configured shortcut is something else must
/// swallow this, otherwise pressing Enter after typing a new label would close
/// the whole settings dialog instead of doing nothing.
pub(crate) fn is_bare_enter(ks: &gpui_kit::gpui::Keystroke) -> bool {
    ks.key.eq_ignore_ascii_case("enter")
        && !ks.modifiers.control
        && !ks.modifiers.shift
        && !ks.modifiers.alt
        && !ks.modifiers.platform
}

#[cfg(test)]
mod shortcut_tests {
    use super::{add_shortcut_matches, is_bare_enter};
    use crate::types::ShortcutConfig;
    use gpui_kit::gpui::{Keystroke, Modifiers};

    fn keystroke(key: &str, ctrl: bool, shift: bool, alt: bool, meta: bool) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                control: ctrl,
                shift,
                alt,
                platform: meta,
                function: false,
            },
            key: key.to_string(),
            key_char: None,
        }
    }

    fn shortcut(key: &str, ctrl: bool, shift: bool, alt: bool, meta: bool) -> ShortcutConfig {
        ShortcutConfig {
            key: key.into(),
            ctrl,
            shift,
            alt,
            meta,
        }
    }

    #[test]
    fn default_shortcut_is_a_bare_enter() {
        let sc = shortcut("Enter", false, false, false, false);
        assert!(add_shortcut_matches(
            &keystroke("enter", false, false, false, false),
            &sc
        ));
    }

    #[test]
    fn configured_combo_does_not_also_accept_bare_enter() {
        let sc = shortcut("enter", true, false, false, false);
        assert!(add_shortcut_matches(
            &keystroke("enter", true, false, false, false),
            &sc
        ));
        assert!(
            !add_shortcut_matches(&keystroke("enter", false, false, false, false), &sc),
            "plain Enter must not add when the user configured Ctrl+Enter"
        );
    }

    #[test]
    fn extra_or_missing_modifiers_do_not_match() {
        let sc = shortcut("enter", true, false, false, false);
        assert!(!add_shortcut_matches(
            &keystroke("enter", true, true, false, false),
            &sc
        ));
        assert!(!add_shortcut_matches(
            &keystroke("enter", true, false, false, true),
            &sc
        ));
        assert!(!add_shortcut_matches(
            &keystroke("k", true, false, false, false),
            &sc
        ));
    }

    #[test]
    fn bare_enter_predicate_ignores_modified_enters() {
        assert!(is_bare_enter(&keystroke("enter", false, false, false, false)));
        assert!(!is_bare_enter(&keystroke("enter", true, false, false, false)));
        assert!(!is_bare_enter(&keystroke("enter", false, true, false, false)));
        assert!(!is_bare_enter(&keystroke("k", false, false, false, false)));
    }
}

fn color_dot(color: &str, _cx: &gpui_kit::gpui::App) -> impl IntoElement {
    let bg = Colorize::parse_hex(color).unwrap_or_else(|_| Hsla::blue());
    div()
        .size_2()
        .rounded_full()
        .flex_shrink_0()
        .bg(bg)
        .into_any_element()
}

/// The next `t{n}` id, one past the largest persisted numeric id (sample ids
/// like `demo-1` don't match, so a fresh install still starts at 10).
fn next_todo_id(todos: &[TodoItem]) -> u64 {
    todos
        .iter()
        .filter_map(|t| t.id.strip_prefix('t')?.parse::<u64>().ok())
        .max()
        .map_or(10, |max| max + 1)
}

/// Reconcile subtask completion bottom-up: a todo that has children is complete
/// iff **all** its direct children are complete, at any nesting depth. Leaves
/// keep whatever the user set; parents are purely derived from their children.
///
/// Fixed-point loop, bounded by the number of rows so a malformed parent cycle
/// in loaded data can never spin forever.
fn sync_subtree_completion(todos: &mut Vec<TodoItem>, stamp: &str) {
    for _ in 0..=todos.len() {
        let mut changed = false;
        for i in 0..todos.len() {
            let parent_id = todos[i].id.clone();
            let children_done: Vec<bool> = todos
                .iter()
                .filter(|t| t.parent_id.as_deref() == Some(parent_id.as_str()))
                .map(|t| t.completed)
                .collect();
            if children_done.is_empty() {
                continue;
            }
            let all_done = children_done.iter().all(|done| *done);
            let item = &mut todos[i];
            if item.completed != all_done {
                item.completed = all_done;
                item.completed_at = if all_done {
                    Some(stamp.to_string())
                } else {
                    None
                };
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Direct children of `parent_id` in the given item set.
fn child_ids_of<'a>(items: &'a [TodoItem], parent_id: &str) -> Vec<&'a TodoItem> {
    items
        .iter()
        .filter(|t| t.parent_id.as_deref() == Some(parent_id))
        .collect()
}

// ── Drag & drop reordering ──────────────────────────────────────────────────

/// Payload carried while a todo row is being dragged. Dragging a row always
/// moves its whole subtree, so only the root id + its text travel with the
/// pointer; the children are resolved from `self.todos` on drop.
struct TodoDrag {
    id: SharedString,
    content: SharedString,
}

/// The floating preview that follows the pointer during a drag.
struct DragGhost {
    content: SharedString,
    descendants: usize,
}

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .max_w(px(320.))
            .px(px(10.))
            .py(px(6.))
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_sm()
            .text_color(cx.theme().popover_foreground)
            .child(div().min_w_0().truncate().child(self.content.clone()))
            .when(self.descendants > 0, |el| {
                el.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("+{} 项子任务", self.descendants)),
                )
            })
    }
}

/// Where a dragged todo should land. Every visible row contributes three of
/// these: two edge bands for "insert as a sibling at this boundary" and the
/// row body for "append as my last child"; the forest adds a trailing root slot.
#[derive(Clone)]
enum DropSlot {
    /// Insert before the given row, under the same parent.
    Before(String),
    /// Insert after the given row, under the same parent.
    After(String),
    /// Insert as the first child of the given row.
    FirstChild(String),
    /// Append as the last child of the given row.
    LastChild(String),
    /// Append at the very end of the top-level list.
    RootEnd,
}

/// Which edge band of a row produced a drop; only selects the indicator border.
#[derive(Clone, Copy)]
enum BandEdge {
    Top,
    Bottom,
}

/// `id` plus every descendant, in breadth-first order. A visited set keeps
/// malformed parent cycles in loaded data from looping forever.
fn collect_subtree(todos: &[TodoItem], id: &str) -> Vec<String> {
    let mut out = vec![id.to_string()];
    let mut seen: HashSet<String> = out.iter().cloned().collect();
    let mut i = 0;
    while i < out.len() {
        let parent = out[i].clone();
        for t in todos.iter() {
            if t.parent_id.as_deref() == Some(parent.as_str()) && seen.insert(t.id.clone()) {
                out.push(t.id.clone());
            }
        }
        i += 1;
    }
    out
}

/// `id` followed by its ancestors, bottom-up. Cycle-safe.
fn ancestor_ids(todos: &[TodoItem], id: &str) -> Vec<String> {
    let mut out = vec![id.to_string()];
    let mut current = id.to_string();
    for _ in 0..=todos.len() {
        let Some(parent) = todos
            .iter()
            .find(|t| t.id == current)
            .and_then(|t| t.parent_id.clone())
        else {
            break;
        };
        if out.contains(&parent) {
            break;
        }
        out.push(parent.clone());
        current = parent;
    }
    out
}

/// Interpret `slot` against the list that results from removing the dragged
/// subtree: returns the target parent (None = top level) and the insertion
/// index among that parent's children. Rejects slots that would nest an item
/// inside its own subtree.
fn resolve_drop_slot(
    todos: &[TodoItem],
    moving: &HashSet<String>,
    slot: &DropSlot,
) -> Option<(Option<String>, usize)> {
    let kept: Vec<&TodoItem> = todos.iter().filter(|t| !moving.contains(&t.id)).collect();
    let siblings = |parent: Option<&str>| -> Vec<&str> {
        kept.iter()
            .filter(|t| t.parent_id.as_deref() == parent)
            .map(|t| t.id.as_str())
            .collect()
    };

    match slot {
        DropSlot::Before(target) | DropSlot::After(target) => {
            let item = kept.iter().find(|t| t.id == *target)?;
            let parent = item.parent_id.clone();
            if parent.as_deref().is_some_and(|p| moving.contains(p)) {
                return None;
            }
            let position = siblings(parent.as_deref())
                .iter()
                .position(|id| *id == target.as_str())?;
            let index = position + usize::from(matches!(slot, DropSlot::After(_)));
            Some((parent, index))
        }
        DropSlot::FirstChild(parent) => {
            if moving.contains(parent) {
                return None;
            }
            Some((Some(parent.clone()), 0))
        }
        DropSlot::LastChild(parent) => {
            if moving.contains(parent) {
                return None;
            }
            Some((Some(parent.clone()), siblings(Some(parent)).len()))
        }
        DropSlot::RootEnd => Some((None, siblings(None).len())),
    }
}

/// Pure core of a drop: rebuild `todos` so `dragged` (plus its whole subtree)
/// lands at `slot`. Returns the adopted parent on success, `None` when the slot
/// is invalid (unknown target / would nest an item inside its own subtree).
///
/// The array order of `todos` is the on-screen order, so the list is rebuilt
/// depth-first instead of patching the rows around the drop. `order` follows
/// the new array index (it stays a working backup sort key), and the moved
/// subtree adopts its new parent's category — 分类只归属顶层任务，父级与子任务
/// 永不落入不同分类。
fn move_todo_in_list(
    todos: &mut Vec<TodoItem>,
    dragged: &str,
    slot: &DropSlot,
) -> Option<Option<String>> {
    if !todos.iter().any(|t| t.id == dragged) {
        return None;
    }
    let moving = collect_subtree(todos, dragged);
    let moving_set: HashSet<String> = moving.iter().cloned().collect();
    let (new_parent, index) = resolve_drop_slot(todos, &moving_set, slot)?;
    let new_cat = new_parent
        .as_ref()
        .and_then(|pid| todos.iter().find(|t| t.id == *pid))
        .and_then(|t| t.cat_id.clone());

    let display: Vec<String> = todos.iter().map(|t| t.id.clone()).collect();
    let mut items: HashMap<String, TodoItem> =
        todos.drain(..).map(|t| (t.id.clone(), t)).collect();
    // 父级缺失的行按顶层处理，避免重建树时把行弄丢。
    let known: HashSet<String> = items.keys().cloned().collect();
    for item in items.values_mut() {
        if let Some(pid) = item.parent_id.clone() {
            if !known.contains(&pid) {
                item.parent_id = None;
            }
        }
    }

    // 兄弟表（已摘掉被拖的整棵子树），再把被拖的根插到落点。
    let mut siblings: HashMap<Option<String>, Vec<String>> = HashMap::new();
    for id in &display {
        if moving_set.contains(id) {
            continue;
        }
        if let Some(item) = items.get(id) {
            siblings
                .entry(item.parent_id.clone())
                .or_default()
                .push(id.clone());
        }
    }
    if let Some(item) = items.get_mut(dragged) {
        item.parent_id = new_parent.clone();
    }
    {
        let list = siblings.entry(new_parent.clone()).or_default();
        list.insert(index.min(list.len()), dragged.to_string());
    }

    // 深度优先展开成数组顺序（即展示顺序）。visited 同时兜住数据里的环：走不到
    // 的行按原顺序追加到末尾，宁可顺序不完美也不能丢数据。
    let mut final_ids: Vec<String> = Vec::with_capacity(items.len());
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = siblings.get(&None).cloned().unwrap_or_default();
    stack.reverse();
    while let Some(id) = stack.pop() {
        if !visited.insert(id.clone()) || !items.contains_key(&id) {
            continue;
        }
        final_ids.push(id.clone());
        if let Some(children) = siblings.get(&Some(id.clone())) {
            for child in children.iter().rev() {
                stack.push(child.clone());
            }
        }
    }
    for id in &display {
        if visited.insert(id.clone()) {
            final_ids.push(id.clone());
        }
    }
    let mut leftover: Vec<String> = items
        .keys()
        .filter(|id| !visited.contains(*id))
        .cloned()
        .collect();
    leftover.sort();
    final_ids.extend(leftover);

    if new_parent.is_some() {
        for id in &moving {
            if let Some(item) = items.get_mut(id) {
                item.cat_id = new_cat.clone();
            }
        }
    }
    for (i, id) in final_ids.iter().enumerate() {
        if let Some(item) = items.get_mut(id) {
            item.order = i as u32;
        }
    }

    *todos = final_ids
        .into_iter()
        .filter_map(|id| items.remove(&id))
        .collect();
    Some(new_parent)
}

/// A transparent strip at a row's top/bottom edge that turns into a 2px
/// insertion line while a matching drag hovers it. `blocked` lists the ids that
/// cannot be dropped here (the row itself and its ancestors: inserting them at
/// this boundary would nest a parent inside its own subtree).
fn render_drop_band(
    edge: BandEdge,
    slot: DropSlot,
    indent: f32,
    blocked: Vec<String>,
) -> AnyElement {
    let band = div()
        .absolute()
        .left(px(indent))
        .right(px(12.))
        .h(px(8.))
        .when(matches!(edge, BandEdge::Top), |el| el.top_0())
        .when(matches!(edge, BandEdge::Bottom), |el| el.bottom_0())
        .can_drop({
            let blocked = blocked.clone();
            move |value, _, _| {
                value
                    .downcast_ref::<TodoDrag>()
                    .is_some_and(|drag| !blocked.iter().any(|id| id == &*drag.id))
            }
        })
        .on_drop::<TodoDrag>({
            let slot = slot.clone();
            move |drag, _window, cx| {
                if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                    let app = handle.0.clone();
                    let id = drag.id.to_string();
                    app.update(cx, |app, cx| app.drop_todo(&id, &slot, cx));
                    cx.stop_propagation();
                }
            }
        })
        .drag_over::<TodoDrag>(move |style, _, _, cx| {
            let color = cx.theme().drag_border;
            match edge {
                BandEdge::Top => style.border_t_2().border_color(color),
                BandEdge::Bottom => style.border_b_2().border_color(color),
            }
        });
    band.into_any_element()
}

/// Trailing slot after the last top-level row: "move to the end of the list".
/// Needed because the bottom band of a nested row can only insert *inside* its
/// parent, so a last-but-one child has nowhere to be dropped after.
fn render_drop_tail() -> AnyElement {
    div()
        .ml(px(12.))
        .mr(px(12.))
        .h(px(14.))
        .can_drop(|value, _, _| value.downcast_ref::<TodoDrag>().is_some())
        .on_drop::<TodoDrag>(move |drag, _window, cx| {
            if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                let app = handle.0.clone();
                let id = drag.id.to_string();
                app.update(cx, |app, cx| app.drop_todo(&id, &DropSlot::RootEnd, cx));
                cx.stop_propagation();
            }
        })
        .drag_over::<TodoDrag>(|style, _, _, cx| {
            style.border_t_2().border_color(cx.theme().drag_border)
        })
        .into_any_element()
}

/// Set a category from `root_id`'s perspective. 分类只归属顶层任务：root 是子任务
/// （有 parent_id）时拒绝并返回 false；否则把 root 的整棵子树统一切到 `cat_id`，
/// 从数据层保证父级与子任务永不落入不同分类。返回操作是否生效。
fn cascade_cat(
    todos: &mut [TodoItem],
    root_id: &str,
    cat_id: Option<String>,
) -> bool {
    let Some(root) = todos.iter().find(|t| t.id == root_id) else {
        return false;
    };
    if root.parent_id.is_some() {
        return false;
    }
    let mut frontier = vec![root_id.to_string()];
    let mut subtree = HashSet::new();
    subtree.insert(root_id.to_string());
    while let Some(pid) = frontier.pop() {
        for t in todos.iter() {
            if t.parent_id.as_deref() == Some(pid.as_str()) && subtree.insert(t.id.clone()) {
                frontier.push(t.id.clone());
            }
        }
    }
    for t in todos.iter_mut() {
        if subtree.contains(&t.id) {
            t.cat_id = cat_id.clone();
        }
    }
    true
}

// ── Time / demo helpers ─────────────────────────────────────────────────────

fn now_iso() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

// ── Local persistence ────────────────────────────────────────────────────────

/// Where the app stores its local snapshot so data survives restarts.
fn local_data_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".doit-gpui").join("snapshot.json")
}

/// Load the previously-saved local snapshot, if any, returning the data plus
/// the snapshot's own "as of" stamp for `last_modified`.
fn load_local_data() -> Option<(Vec<TodoItem>, AppSettings, String)> {
    let snap = read_snapshot_at(&local_data_path())?;
    (snap.version == 1).then(|| (snap.todos, snap.settings, snap.exported_at))
}

fn write_snapshot_at(snap: &SyncSnapshot, path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(snap) {
        let _ = std::fs::write(path, json);
    }
}

fn read_snapshot_at(path: &Path) -> Option<SyncSnapshot> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn date_part(iso: &str) -> String {
    if iso.len() >= 10 {
        iso[..10].to_string()
    } else {
        iso.to_string()
    }
}

fn month_day(day: &str) -> String {
    if day.len() >= 10 { day[5..10].to_string() } else { day.to_string() }
}

fn weekday_cn(day: &str) -> &'static str {
    use chrono::NaiveDate;
    const NAMES: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
    match NaiveDate::parse_from_str(day, "%Y-%m-%d") {
        Ok(d) => NAMES[d.weekday().num_days_from_monday() as usize],
        Err(_) => "",
    }
}

fn format_time(iso: &Option<String>) -> SharedString {
    match iso {
        Some(s) if s.len() >= 19 => s[11..16].into(),
        _ => "".into(),
    }
}

/// Parse a stored reminder timestamp into a `NaiveDateTime`.
fn parse_remind_at(iso: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S").ok()
}

/// Seconds remaining until `remind_at`, or `None` if absent / zeroed-out.
/// The stored stamp carries today's date, so an unexpired value is at most
/// 24h away and always counts down on the current day.
fn reminder_remaining_secs(remind_at: &Option<String>) -> Option<i64> {
    let at = parse_remind_at(remind_at.as_ref()?)?;
    let remaining = at.signed_duration_since(Local::now().naive_local()).num_seconds();
    (remaining > 0).then_some(remaining)
}

/// The live countdown label for a todo row:
/// * ≥ 1 hour  → "X时Y分"
/// * 1–59 min  → "Y分钟"
/// * < 1 min   → "Z秒"
fn format_countdown(secs: i64) -> SharedString {
    if secs >= 3600 {
        format!("{}时{}分", secs / 3600, (secs % 3600) / 60).into()
    } else if secs >= 60 {
        format!("{}分钟", secs / 60).into()
    } else {
        format!("{}秒", secs.max(0)).into()
    }
}

/// ISO timestamp for a completed demo item: `days_ago` days back at the given time.
fn ts(days_ago: i64, h: u32, m: u32) -> String {
    let base = Local::now() - ChronoDuration::days(days_ago);
    base.date_naive()
        .and_hms_opt(h, m, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_else(now_iso)
}

fn sample_todos() -> Vec<TodoItem> {
    let mut todos = Vec::new();
    todos.push(TodoItem {
        id: "demo-1".into(),
        content: "欢迎使用 Doit！".into(),
        completed: false,
        created_at: now_iso(),
        completed_at: None,
        order: 0,
        tag_id: Some("tag-w".into()),
        cat_id: Some("cat-work".into()),
        parent_id: None,
        remind_at: None,
    });
    todos.push(TodoItem {
        id: "demo-2".into(),
        content: "点击复选框完成待办".into(),
        completed: false,
        created_at: now_iso(),
        completed_at: None,
        order: 1,
        tag_id: Some("tag-p".into()),
        cat_id: Some("cat-life".into()),
        parent_id: None,
        remind_at: None,
    });
    todos.push(TodoItem {
        id: "demo-3".into(),
        content: "在「设置」里切换主题与完成方式".into(),
        completed: false,
        created_at: now_iso(),
        completed_at: None,
        order: 2,
        tag_id: None,
        cat_id: None,
        parent_id: None,
        remind_at: None,
    });
    // Completed demo items so the calendar / timeline / stats views have data.
    todos.push(TodoItem {
        id: "demo-c1".into(),
        content: "完成今日早间计划".into(),
        completed: true,
        created_at: ts(0, 8, 30),
        completed_at: Some(ts(0, 9, 15)),
        order: 3,
        tag_id: Some("tag-w".into()),
        cat_id: Some("cat-work".into()),
        parent_id: None,
        remind_at: None,
    });
    todos.push(TodoItem {
        id: "demo-c2".into(),
        content: "整理下午会议记录".into(),
        completed: true,
        created_at: ts(0, 13, 0),
        completed_at: Some(ts(0, 14, 40)),
        order: 4,
        tag_id: None,
        cat_id: None,
        parent_id: None,
        remind_at: None,
    });
    todos.push(TodoItem {
        id: "demo-c3".into(),
        content: "阅读《原子习惯》30 分钟".into(),
        completed: true,
        created_at: ts(1, 19, 30),
        completed_at: Some(ts(1, 20, 5)),
        order: 5,
        tag_id: Some("tag-p".into()),
        cat_id: Some("cat-life".into()),
        parent_id: None,
        remind_at: None,
    });
    todos.push(TodoItem {
        id: "demo-c4".into(),
        content: "完成项目周报".into(),
        completed: true,
        created_at: ts(2, 9, 0),
        completed_at: Some(ts(2, 10, 30)),
        order: 6,
        tag_id: None,
        cat_id: None,
        parent_id: None,
        remind_at: None,
    });
    todos
}

fn initial_theme(settings: &AppSettings, window: &Window) -> ThemeMode {
    match settings.theme.as_ref() {
        "dark" => ThemeMode::Dark,
        "light" => ThemeMode::Light,
        _ => match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            _ => ThemeMode::Light,
        },
    }
}

/// Product chart palette: a soft celadon mint for completed volume and a deep
/// teal for the trend line, with cool spares for future series. The stock ramp
/// is all blue, which fights the green completion language used by the
/// checkbox and the heatmap; the previous emerald was still too vivid next to
/// white cards, so this scale drops saturation and shifts a touch cooler.
/// Values live here as theme tokens (never at chart call sites) and every
/// switch re-applies them, because loading a mode resets the colors.
fn apply_chart_palette(cx: &mut App) {
    Theme::update(cx, |theme| {
        let (c1, c2, c3, c4, c5) = if theme.mode.is_dark() {
            (0x6bd8bd, 0x33b79b, 0x7dd3fc, 0xc4b5fd, 0xcbd5e1)
        } else {
            (0x4ac6a8, 0x14907b, 0x38bdf8, 0xa78bfa, 0x94a3b8)
        };
        theme.colors.chart_1 = rgb(c1).into();
        theme.colors.chart_2 = rgb(c2).into();
        theme.colors.chart_3 = rgb(c3).into();
        theme.colors.chart_4 = rgb(c4).into();
        theme.colors.chart_5 = rgb(c5).into();
    });
}

/// Switch the app theme and keep the product chart palette on top of it.
pub(crate) fn change_theme(mode: ThemeMode, window: &mut Window, cx: &mut App) {
    Theme::change(mode, Some(window), cx);
    apply_chart_palette(cx);
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn local_snapshot_round_trip() {
        let path = std::env::temp_dir()
            .join(format!("doit-gpui-persist-test-{}.json", std::process::id()));
        let mut settings = AppSettings::default();
        settings.tags.push(Tag {
            id: "t9".into(),
            name: "测试".into(),
            color: "#123456".into(),
        });
        let snap = SyncSnapshot {
            version: 1,
            exported_at: "2026-09-16T12:00:00".into(),
            todos: vec![TodoItem {
                id: "t1".into(),
                content: "持久化测试".into(),
                completed: true,
                created_at: "2026-09-16T10:00:00".into(),
                completed_at: Some("2026-09-16T11:00:00".into()),
                order: 0,
                tag_id: None,
                cat_id: None,
                parent_id: None,
                remind_at: None,
            }],
            settings,
        };

        write_snapshot_at(&snap, &path);
        let loaded = read_snapshot_at(&path).expect("saved snapshot should load");
        assert_eq!(loaded.todos.len(), 1);
        assert_eq!(loaded.todos[0].content, "持久化测试");
        assert!(loaded.todos[0].completed);
        assert_eq!(loaded.settings.tags.len(), 3, "default tags + the added one");
        assert_eq!(loaded.exported_at, "2026-09-16T12:00:00");
        let _ = std::fs::remove_file(&path);
    }

    /// The export path serializes with `to_string_pretty`; importing parses
    /// via `from_str`. Guard that a pretty-printed snapshot round-trips (an
    /// export users can actually re-import).
    #[test]
    fn export_json_round_trips_through_import() {
        let snap = SyncSnapshot {
            version: 1,
            exported_at: "2026-09-16T12:00:00".into(),
            todos: vec![TodoItem {
                id: "t1".into(),
                content: "导出测试".into(),
                completed: false,
                created_at: "2026-09-16T10:00:00".into(),
                completed_at: None,
                order: 0,
                tag_id: Some("tag-w".into()),
                cat_id: Some("cat-work".into()),
                parent_id: None,
                remind_at: None,
            }],
            settings: AppSettings::default(),
        };
        let json = serde_json::to_string_pretty(&snap).expect("serialize export json");
        let restored: SyncSnapshot =
            serde_json::from_str(&json).expect("import parses its own export");
        assert_eq!(restored.todos.len(), 1);
        assert_eq!(restored.todos[0].content, "导出测试");
        assert_eq!(restored.todos[0].tag_id.as_deref(), Some("tag-w"));
        assert_eq!(restored.settings.theme.as_ref(), "system");
        assert_eq!(restored.exported_at, "2026-09-16T12:00:00");
    }
}

#[cfg(test)]
mod subtree_tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>, completed: bool) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: id.into(),
            completed,
            created_at: "2026-09-16T10:00:00".into(),
            completed_at: if completed { Some("2026-09-16T11:00:00".into()) } else { None },
            order: 0,
            tag_id: None,
            cat_id: None,
            parent_id: parent.map(|p| p.to_string()),
            remind_at: None,
        }
    }

    fn completed_of(todos: &[TodoItem], id: &str) -> bool {
        todos.iter().find(|t| t.id == id).unwrap().completed
    }

    fn cat_of<'a>(todos: &'a [TodoItem], id: &str) -> Option<&'a str> {
        todos
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .cat_id
            .as_deref()
    }

    #[test]
    fn parent_is_incomplete_until_all_children_done() {
        let mut todos = vec![
            node("p", None, true),
            node("a", Some("p"), false),
            node("b", Some("p"), true),
        ];
        sync_subtree_completion(&mut todos, "2026-09-16T12:00:00");
        assert!(!completed_of(&todos, "p"), "one child unfinished");
        todos[1].completed = true;
        todos[1].completed_at = Some("2026-09-16T12:00:00".into());
        sync_subtree_completion(&mut todos, "2026-09-16T12:00:00");
        assert!(completed_of(&todos, "p"), "all children done");
        assert_eq!(
            todos.iter().find(|t| t.id == "p").unwrap().completed_at.as_deref(),
            Some("2026-09-16T12:00:00"),
            "parent gains a completion stamp"
        );
    }

    #[test]
    fn uncompleting_a_child_uncompletes_the_parent() {
        let mut todos = vec![
            node("p", None, true),
            node("a", Some("p"), true),
            node("b", Some("p"), true),
        ];
        sync_subtree_completion(&mut todos, "2026-09-16T12:00:00");
        assert!(completed_of(&todos, "p"));
        todos[2].completed = false;
        todos[2].completed_at = None;
        sync_subtree_completion(&mut todos, "2026-09-16T12:00:00");
        assert!(!completed_of(&todos, "p"));
        assert!(todos.iter().find(|t| t.id == "p").unwrap().completed_at.is_none());
    }

    #[test]
    fn completion_propagates_through_any_depth() {
        let mut todos = vec![
            node("gp", None, false),
            node("p", Some("gp"), false),
            node("a", Some("p"), false),
        ];
        todos[2].completed = true;
        todos[2].completed_at = Some("2026-09-16T12:00:00".into());
        sync_subtree_completion(&mut todos, "2026-09-16T12:00:00");
        assert!(completed_of(&todos, "p"));
        assert!(completed_of(&todos, "gp"));
    }

    #[test]
    fn leaves_keep_their_own_state() {
        let mut todos = vec![
            node("leaf", None, false),
            node("done-leaf", None, true),
            node("p", Some("leaf"), false), // "leaf" becomes a parent
            node("c", Some("leaf"), true),
        ];
        // "leaf" now has one child; it should derive (child done → incomplete)
        sync_subtree_completion(&mut todos, "2026-09-16T12:00:00");
        assert!(!completed_of(&todos, "leaf"));
        assert!(completed_of(&todos, "done-leaf"), "parentless leaf untouched");
    }

    #[test]
    fn malformed_cycle_terminates() {
        // A->B->A parent cycle must not hang the fixed-point loop.
        let mut todos = vec![node("a", Some("b"), false), node("b", Some("a"), false)];
        sync_subtree_completion(&mut todos, "2026-09-16T12:00:00");
        let _ = todos;
    }

    #[test]
    fn subtask_rejects_category_and_top_cascades() {
        // A (top) -> B -> C. Setting B's category is refused; setting A's
        // category cascades down the whole subtree so parent/child never diverge.
        let mut todos = vec![
            node("a", None, false),
            node("b", Some("a"), false),
            node("c", Some("b"), false),
        ];
        assert!(!cascade_cat(&mut todos, "b", Some("cat-x".into())));
        assert_eq!(cat_of(&todos, "a"), None);
        assert_eq!(cat_of(&todos, "b"), None);
        assert_eq!(cat_of(&todos, "c"), None);

        assert!(cascade_cat(&mut todos, "a", Some("cat-x".into())));
        assert_eq!(cat_of(&todos, "a"), Some("cat-x"));
        assert_eq!(cat_of(&todos, "b"), Some("cat-x"));
        assert_eq!(cat_of(&todos, "c"), Some("cat-x"), "grandchild follows too");

        // Clearing on the top un-sets the whole subtree.
        assert!(cascade_cat(&mut todos, "a", None));
        assert_eq!(cat_of(&todos, "a"), None);
        assert_eq!(cat_of(&todos, "b"), None);
        assert_eq!(cat_of(&todos, "c"), None);
    }
}

#[cfg(test)]
mod reorder_tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>, cat: Option<&str>) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: id.into(),
            completed: false,
            created_at: "2026-10-29T10:00:00".into(),
            completed_at: None,
            order: 0,
            tag_id: None,
            cat_id: cat.map(|c| c.to_string()),
            parent_id: parent.map(|p| p.to_string()),
            remind_at: None,
        }
    }

    fn ids(todos: &[TodoItem]) -> Vec<&str> {
        todos.iter().map(|t| t.id.as_str()).collect()
    }

    fn cat_of<'a>(todos: &'a [TodoItem], id: &str) -> Option<&'a str> {
        todos
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .cat_id
            .as_deref()
    }

    /// Invariants the rebuilt list must always satisfy: no row lost or
    /// duplicated, every parent listed before its children, and `order`
    /// mirroring the array index.
    fn assert_invariants(todos: &[TodoItem]) {
        for (index, item) in todos.iter().enumerate() {
            assert_eq!(
                item.order, index as u32,
                "order must follow the array index ({})",
                item.id
            );
            if let Some(pid) = item.parent_id.as_deref() {
                let parent_index = todos.iter().position(|t| t.id == pid);
                assert!(parent_index.is_some(), "parent of {} must exist", item.id);
                assert!(
                    parent_index.unwrap() < index,
                    "parent of {} must come first",
                    item.id
                );
            }
        }
        let mut sorted: Vec<&str> = ids(todos);
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), todos.len(), "no row lost or duplicated");
    }

    #[test]
    fn dropping_before_a_sibling_reorders_within_the_parent() {
        let mut todos = vec![
            node("p", None, None),
            node("a", Some("p"), None),
            node("b", Some("p"), None),
            node("c", Some("p"), None),
        ];
        let parent = move_todo_in_list(&mut todos, "c", &DropSlot::Before("a".into()));
        assert_eq!(parent, Some(Some("p".to_string())));
        assert_eq!(ids(&todos), vec!["p", "c", "a", "b"]);
        assert_invariants(&todos);
    }

    #[test]
    fn dropping_below_a_parent_inserts_inside_it_as_the_first_child() {
        let mut todos = vec![
            node("p", None, None),
            node("a", Some("p"), None),
            node("q", None, None),
        ];
        // 父行有展开的子任务时，下边缘热区 = 插到父级内部第一位，而不是父级之后。
        move_todo_in_list(&mut todos, "q", &DropSlot::FirstChild("p".into()));
        assert_eq!(ids(&todos), vec!["p", "q", "a"]);
        assert_invariants(&todos);
    }

    #[test]
    fn dropping_on_a_title_appends_as_last_child_and_adopts_the_category() {
        let mut todos = vec![
            node("work", None, Some("cat-work")),
            node("dep", Some("work"), Some("cat-work")),
            node("home", None, Some("cat-home")),
        ];
        let parent = move_todo_in_list(&mut todos, "home", &DropSlot::LastChild("dep".into()));
        assert_eq!(parent, Some(Some("dep".to_string())));
        assert_eq!(ids(&todos), vec!["work", "dep", "home"]);
        assert_eq!(cat_of(&todos, "home"), Some("cat-work"));
        assert_invariants(&todos);
    }

    #[test]
    fn dragging_a_parent_carries_its_whole_subtree() {
        let mut todos = vec![
            node("p", None, None),
            node("c1", Some("p"), None),
            node("c2", Some("c1"), None),
            node("q", None, None),
        ];
        assert_eq!(collect_subtree(&todos, "p"), vec!["p", "c1", "c2"]);
        move_todo_in_list(&mut todos, "p", &DropSlot::LastChild("q".into()));
        assert_eq!(ids(&todos), vec!["q", "p", "c1", "c2"]);
        assert_invariants(&todos);
    }

    #[test]
    fn promoting_a_child_to_the_root_end_detaches_it() {
        let mut todos = vec![
            node("p", None, None),
            node("c", Some("p"), None),
            node("q", None, None),
        ];
        let parent = move_todo_in_list(&mut todos, "c", &DropSlot::RootEnd);
        assert_eq!(parent, Some(None));
        assert_eq!(ids(&todos), vec!["p", "q", "c"]);
        assert!(todos.iter().find(|t| t.id == "c").unwrap().parent_id.is_none());
        assert_invariants(&todos);
    }

    #[test]
    fn a_parent_can_never_be_dropped_inside_its_own_subtree() {
        let mut todos = vec![
            node("p", None, None),
            node("c", Some("p"), None),
            node("g", Some("c"), None),
        ];
        let before: Vec<String> = ids(&todos).into_iter().map(String::from).collect();
        assert!(move_todo_in_list(&mut todos, "p", &DropSlot::LastChild("g".into())).is_none());
        assert!(move_todo_in_list(&mut todos, "c", &DropSlot::Before("g".into())).is_none());
        let after: Vec<String> = ids(&todos).into_iter().map(String::from).collect();
        assert_eq!(after, before, "invalid drops must not touch the list");
    }

    #[test]
    fn ancestor_chain_and_subtree_are_cycle_safe() {
        let mut todos = vec![node("a", None, None), node("b", Some("a"), None)];
        // 人为制造一个环，确认两个遍历都还能退出。
        todos[1].parent_id = Some("a".to_string());
        todos[0].parent_id = Some("b".to_string());
        assert_eq!(collect_subtree(&todos, "a").len(), 2);
        assert_eq!(ancestor_ids(&todos, "a").len(), 2);
    }
}

#[cfg(test)]
mod reminder_tests {
    use super::*;

    /// A `remind_at` timestamp for "now + offset seconds" on today.
    fn stamp_from_now(offset: i64) -> String {
        (Local::now() + ChronoDuration::seconds(offset))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    }

    #[test]
    fn countdown_formats() {
        assert_eq!(format_countdown(3600), "1时0分");
        assert_eq!(format_countdown(7260), "2时1分");
        assert_eq!(format_countdown(59), "59秒");
        assert_eq!(format_countdown(0), "0秒");
        assert_eq!(format_countdown(60), "1分钟");
        assert_eq!(format_countdown(3599), "59分钟");
    }

    #[test]
    fn remaining_secs_is_positive_only() {
        // Future → positive; present/just-past → None (standardizes on "已到点").
        let future = stamp_from_now(90);
        assert!(reminder_remaining_secs(&Some(future)).unwrap() > 0);
        let past = stamp_from_now(-5);
        assert!(reminder_remaining_secs(&Some(past)).is_none());
        assert!(reminder_remaining_secs(&None).is_none());
        assert!(reminder_remaining_secs(&Some("bad".into())).is_none());
    }

    #[test]
    fn stamp_round_trips() {
        // The stored format parses back, matching set_reminder's output shape.
        let now = Local::now();
        let stamp = now
            .date_naive()
            .and_time(now.time())
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let parsed = parse_remind_at(&stamp).expect("parses");
        // The parsed diff is well under a second in either direction.
        let diff = parsed - now.naive_local();
        assert!(diff.num_milliseconds().abs() < 1000);
    }
}
