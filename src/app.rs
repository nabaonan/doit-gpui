use crate::backup::BackupPanel;
use crate::settings::SettingsPanel;
use crate::types::*;
use chrono::{Datelike, Days, NaiveDate};
use chrono::{Duration as ChronoDuration, Local};
use gpui_kit::base::Disableable as _;
use gpui_kit::base::Selectable;
use gpui_kit::base::StyledExt;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::calendar::{Calendar, CalendarEvent, CalendarState, Date};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::select::{
    SearchableVec, Select, SelectEvent, SelectItem, SelectState,
};
use gpui_kit::component::chart::{BarChart, LineChart, PieChart};
use gpui_kit::component::marker::Marker;
use gpui_kit::component::Colorize;
use gpui_kit::component::IndexPath;
use gpui_kit::component::{
    ActiveTheme, IconName, Root, Sizable, Theme, ThemeMode, WindowExt,
    button::Button,
    input::{Input, InputState, InputEvent},
    menu::{ContextMenuExt, PopupMenuItem},
};
use gpui_kit::prelude::*;
use gpui_kit::gpui::{
    self, div, App, Hsla, Context, Entity, Global, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, SharedString, Styled, Subscription, Window, WindowAppearance, px,
    ElementId,
};
use std::time::Instant;
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

    /// Inline editing of a todo's content (used by the row + context menu).
    editing_id: Option<String>,
    edit_input: Entity<InputState>,
    _edit_sub: Subscription,

    /// Top-bar category filter dropdown.
    cat_select: Entity<CatSelectState>,
    _cat_select_sub: Subscription,

    /// Tracks a pending long-press (row id, when it started).
    long_press: Option<(String, Instant)>,

    _input_sub: Subscription,
    _calendar_sub: Subscription,
}

impl DoitApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = AppSettings::default();

        // Honor the self-signed certificate preference for WebDAV requests.
        crate::http::set_trust_self_signed(settings.cloud_sync.trust_self_signed);

        let input = cx.new(|cx| InputState::new(window, cx).placeholder("添加待办事项..."));

        let input_clone = input.clone();
        // Enter submission is handled in `render_input_row`'s capture handler,
        // which has window access to clear the field after adding. Keep this
        // subscription inert to avoid double-adding on Enter.
        let input_sub = cx.subscribe::<InputState, InputEvent>(&input_clone, |_this, _st, _ev, _cx| {});

        // Apply the stored theme on startup.
        Theme::change(initial_theme(&settings, window), Some(window), cx);

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
                this.long_press = None;
                cx.notify();
            },
        );

        let edit_input = cx.new(|cx| InputState::new(window, cx));
        let edit_sub = cx.subscribe::<InputState, InputEvent>(&edit_input, |this, _st, ev, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.commit_edit(cx);
            }
        });

        Self {
            todos: sample_todos(),
            settings,
            next_id: 10,
            view_mode: ViewMode::Today,
            cat_filter: CatFilter::None,
            new_todo_input: input,
            settings_panel,
            backup_panel,
            calendar_state,
            selected_cal_date: today.format("%Y-%m-%d").to_string(),
            long_press: None,
            _input_sub: input_sub,
            _calendar_sub: calendar_sub,
            stats_period: StatsPeriod::Month,
            stats_month: current_ym(),
            stats_week_offset: 0,
            editing_id: None,
            edit_input,
            _edit_sub: edit_sub,
            cat_select,
            _cat_select_sub: cat_select_sub,
        }
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
        cx.notify();
    }

    fn toggle_todo(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(item) = self.todos.iter_mut().find(|t| t.id == id) {
            item.completed = !item.completed;
            item.completed_at = if item.completed { Some(now_iso()) } else { None };
        }
        self.resort();
        cx.notify();
    }

    fn delete_todo(&mut self, id: &str, cx: &mut Context<Self>) {
        let id_str = id.to_string();
        self.todos.retain(|t| t.id != id_str);
        cx.notify();
    }

    fn resort(&mut self) {
        self.todos
            .sort_by(|a, b| a.completed.cmp(&b.completed).then(a.order.cmp(&b.order)));
    }

    pub(crate) fn clear_all(&mut self, cx: &mut Context<Self>) {
        self.todos.clear();
        self.settings.tags.clear();
        self.settings.categories.clear();
        self.settings.default_category_id = None;
        self.cat_filter = CatFilter::None;
        cx.notify();
    }

    /// Snapshot of all user data (used by the WebDAV backup panel).
    pub(crate) fn snapshot_data(&self) -> (Vec<TodoItem>, AppSettings) {
        (self.todos.clone(), self.settings.clone())
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
        let mode = initial_theme(&self.settings, window);
        Theme::change(mode, Some(window), cx);
        if let CatFilter::Id(ref id) = self.cat_filter {
            if !self.settings.categories.iter().any(|c| c.id == *id) {
                self.cat_filter = CatFilter::None;
            }
        }
        cx.notify();
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
        cx.notify();
    }

    /// Replace tags (from the tag management dialog).
    pub(crate) fn set_tags(&mut self, tags: Vec<Tag>, cx: &mut Context<Self>) {
        self.settings.tags = tags;
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
        cx.notify();
    }

    /// Assign or clear the category on a single todo.
    pub(crate) fn set_cat(
        &mut self,
        id: &str,
        cat_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(item) = self.todos.iter_mut().find(|t| t.id == id) {
            item.cat_id = cat_id;
        }
        cx.notify();
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
        cx.notify();
    }

    /// Apply the edited content (Enter or the save button).
    fn commit_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.editing_id.take() {
            let value = self.edit_input.read(cx).value().trim().to_string();
            if !value.is_empty() {
                if let Some(todo) = self.todos.iter_mut().find(|t| t.id == id) {
                    todo.content = value.into();
                }
            }
        }
        cx.notify();
    }

    /// Leave inline-edit mode without applying.
    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.editing_id = None;
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

impl Render for DoitApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_bg = cx.theme().background;

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
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
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
                    .child(div().text_base().font_bold().child("Doit"))
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
                                        this.long_press = None;
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
                            .icon(IconName::HardDrive)
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
                                let panel = this.settings_panel.clone();
                                let base = this.settings.clone();
                                panel.update(cx, |p, cx| p.sync_from(base, window, cx));
                                window.open_dialog(cx, move |dialog, _window, _cx| {
                                    dialog
                                        .title("设置")
                                        .w(px(720.))
                                        .h(px(500.))
                                        .child(panel.clone())
                                });
                            })),
                    ),
            )
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

impl DoitApp {
    fn render_today(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_todos();
        let tags = self.settings.tags.clone();
        let is_longpress = self.settings.completion_mode.as_ref() == "longpress";

        div()
            .v_flex()
            .gap_2()
            .p_4()
            .child(self.render_input_row(window, cx))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .children(active.iter().map(|todo| {
                        self.render_todo_row(todo, &tags, is_longpress, window, cx)
                            .into_any_element()
                    })),
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
                    // A bare Enter always submits; otherwise the configured
                    // shortcut (e.g. Cmd+N) submits too.
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
        is_longpress: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = todo.id.clone();
        let completed = todo.completed;
        let content = todo.content.clone();
        let tag_id = todo.tag_id.clone();
        let duration = self.settings.long_press_duration;
        let tag_for_display: Option<(SharedString, SharedString)> = tag_id
            .as_ref()
            .and_then(|tid| tags.iter().find(|t| t.id == *tid))
            .map(|t| (t.name.clone(), t.color.clone().into()));
        let is_editing = self.editing_id.as_deref() == Some(&*id);

        div()
            .id(ElementId::Name(id.clone().into()))
            .flex()
            .items_center()
            .gap_2()
            .px(px(12.))
            .py(px(8.))
            .rounded_lg()
            .hover(|s| s.bg(cx.theme().muted))
            .when(is_longpress, |el| {
                let id_down = id.clone();
                el.on_mouse_down(
                    MouseButton::Left,
                    move |_, _window, cx| {
                        if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                            let app = handle.0.clone();
                            app.update(cx, |app, _| {
                                app.long_press = Some((id_down.clone(), Instant::now()));
                            });
                        }
                    },
                )
                .on_mouse_up(
                    MouseButton::Left,
                    {
                        let id_up = id.clone();
                        move |_, _window, cx| {
                            if let Some(handle) = cx.try_global::<DoitAppHandle>() {
                                let app = handle.0.clone();
                                let complete = app
                                    .read(cx)
                                    .long_press
                                    .as_ref()
                                    .map(|(i, t)| *i == id_up && t.elapsed().as_secs() >= duration as u64)
                                    .unwrap_or(false);
                                if complete {
                                    app.update(cx, |app, cx| {
                                        app.long_press = None;
                                        app.toggle_todo(&id_up, cx);
                                    });
                                } else {
                                    app.update(cx, |app, _| {
                                        app.long_press = None;
                                    });
                                }
                            }
                        }
                    },
                )
            })
            .context_menu({
                let id_for_menu = id.clone();
                let menu_tags = tags.to_vec();
                let menu_cats = self.settings.categories.clone();
                let current_tag = tag_id.clone();
                let current_cat = todo.cat_id.clone();
                let content_for_edit = content.clone();
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
                    .submenu("移动到分类", window, cx, {
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
                if is_longpress {
                    div()
                        .size_4()
                        .rounded_full()
                        .border_1()
                        .flex_shrink_0()
                        .border_color(if completed {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .bg(if completed {
                            cx.theme().primary
                        } else {
                            gpui::transparent_black()
                        })
                        .into_any_element()
                } else {
                    Checkbox::new(ElementId::Name(format!("chk-{}", &id).into()))
                        .checked(completed)
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
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(if completed {
                            cx.theme().muted_foreground
                        } else {
                            cx.theme().foreground
                        })
                        .when(completed, |el| el.line_through())
                        .child(content.clone())
                        .into_any_element()
                },
            )
            .when(!is_editing, |el| {
                el.when_some(tag_for_display, |el, (name, color)| {
                    // Paint a self-drawn capsule: the <Tag> component's variants
                    // pick theme-dependent shades that read as backgroundless in
                    // dark mode, so fill the chip ourselves with the item's own
                    // colour so the background always shows.
                    let bg = Colorize::parse_hex(&color).unwrap_or_else(|_| Hsla::blue());
                    let rgb = bg.to_rgb();
                    let luminance = 0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b;
                    let fg = if luminance > 0.55 {
                        gpui::hsla(0., 0., 0., 0.9)
                    } else {
                        gpui::hsla(0., 0., 1., 0.95)
                    };
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .text_xs()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded_full()
                            .bg(bg)
                            .text_color(fg)
                            .child(name),
                    )
                })
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
                    // Interactive month grid.
                    .child(
                        div()
                            .w(px(380.))
                            .child(Calendar::new(&self.calendar_state).small()),
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
}
impl DoitApp {
    fn render_timeline(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.completed_by_day();
        let muted = cx.theme().muted_foreground;

        div()
            .v_flex()
            .gap_4()
            .p_6()
            .child(div().text_lg().font_bold().child("时间轴"))
            .when(groups.is_empty(), |el| {
                el.child(
                    div()
                        .py_10()
                        .text_color(cx.theme().muted_foreground)
                        .child("还没有完成记录，完成一条待办点亮你的时间轴"),
                )
            })
            .children(groups.into_iter().map(|(day, items)| {
                // Each day is a Marker row (date + completion count); the
                // completed items follow indented. Category is a filter at the
                // top, not shown on the timeline.
                div()
                    .v_flex()
                    .gap_2()
                    .child(
                        Marker::new()
                            .id(ElementId::Name(format!("tl-{}", &day).into()))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_bold()
                                            .child(format!(
                                                "{} · {}",
                                                month_day(&day),
                                                weekday_cn(&day)
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(format!("完成 {} 个待办", items.len())),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .pl_4()
                            .v_flex()
                            .gap_1()
                            .children(items.into_iter().map(|item| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .py(px(4.))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .w(px(44.))
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
                    )
                    .into_any_element()
            }))
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
        let pies = vec![
            PieDatum { value: done as f32, color: cx.theme().success },
            PieDatum {
                value: open as f32,
                color: cx.theme().muted_foreground,
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
                                        .fill(move |_, _, _, _| chart_1)
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
                                            .outer_radius(80.),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap_4()
                                    .child(stats_legend(cx.theme().success, "已完成", done, cx))
                                    .child(stats_legend(
                                        cx.theme().muted_foreground,
                                        "未完成",
                                        open,
                                        cx,
                                    )),
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

/// True when a keystroke should confirm an "add": a bare Enter, or the
/// user-configured add shortcut (which defaults to Enter) from settings.
/// Shared by the todo input and the category/tag add inputs.
pub(crate) fn add_shortcut_matches(
    ks: &gpui_kit::gpui::Keystroke,
    sc: &ShortcutConfig,
) -> bool {
    let bare_enter = ks.key.eq_ignore_ascii_case("enter")
        && !ks.modifiers.control
        && !ks.modifiers.shift
        && !ks.modifiers.alt
        && !ks.modifiers.platform;
    bare_enter
        || (ks.key.eq_ignore_ascii_case(sc.key.as_ref())
            && ks.modifiers.control == sc.ctrl
            && ks.modifiers.shift == sc.shift
            && ks.modifiers.alt == sc.alt
            && ks.modifiers.platform == sc.meta)
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

// ── Time / demo helpers ─────────────────────────────────────────────────────

fn now_iso() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
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
