use gpui_kit::base::Selectable as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{
    ActiveTheme, Colorize, Sizable, WindowExt,
    button::Button,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputState, InputEvent},
};
use gpui_kit::prelude::*;
use gpui_kit::gpui::{
    div, Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled, Subscription,
    Window, px, ElementId, Hsla,
};
use crate::app::{DoitAppHandle, add_shortcut_matches};
use crate::types::*;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// Shared preset palette (mirrors the original app).

fn fresh_id(prefix: &str, seq: u64) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    format!("{prefix}-{millis}-{seq}")
}

/// Parse a hex colour into an `Hsla`, falling back to a neutral blue.
fn color_from_hex(hex: &str) -> Hsla {
    Colorize::parse_hex(hex).unwrap_or_else(|_| Hsla::blue())
}

/// Quick-pick palette the official `ColorPicker` shows above its built-in
/// swatches.
///
/// The official component derives every swatch's `ElementId` from the colour's
/// hex, and its *default* featured colours (the theme red/blue/green/accent
/// tones) all duplicate hexes in its built-in palettes. With accessibility
/// active, opening the popover therefore builds two identical a11y node ids and
/// panics ("Duplicate a11y node id"). Fix: pass our own featured set that is
/// disjoint from the component's 9 built-in palettes (stone/red/orange/yellow/
/// green/cyan/blue/purple/pink) — the app's brand colours those palettes do not
/// already list. Everything else is reachable from the built-in palettes below.
fn picker_featured_colors() -> Vec<Hsla> {
    ["#F59E0B", "#84CC16", "#14B8A6", "#6366F1", "#D946EF"]
        .iter()
        .map(|hex| color_from_hex(hex))
        .collect()
}

// ── Category management panel ───────────────────────────────────────────────

pub struct CategoriesPanel {
    local: Vec<Category>,
    default_id: Option<String>,
    editing_id: Option<String>,
    show_done: bool,
    edit_name: Entity<InputState>,
    new_name: Entity<InputState>,
    edit_color: Entity<ColorPickerState>,
    new_color_picker: Entity<ColorPickerState>,
    /// One committed colour picker per row, so every row's swatch shows and
    /// edits its own colour directly.
    row_pickers: HashMap<String, Entity<ColorPickerState>>,
    /// The row currently under the cursor; its 编辑/默认/删除 actions are only
    /// shown while hovered.
    hover_row: Option<String>,
    seq: u64,
    _subs: Vec<Subscription>,
}

impl CategoriesPanel {
    pub fn new(
        categories: Vec<Category>,
        default_id: Option<String>,
        show_done: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let edit_name = cx.new(|cx| InputState::new(window, cx).placeholder("分类名称"));
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("新分类名称"));
        let edit_color = cx.new(|cx| ColorPickerState::new(window, cx));
        let new_color_picker = cx.new(|cx| {
            ColorPickerState::new(window, cx)
                .default_value(color_from_hex(COLOR_PALETTE[8].1))
        });

        let mut subs = Vec::new();
        subs.push(cx.subscribe::<InputState, InputEvent>(&edit_name, |this, st, ev, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                let value = st.read(cx).value().trim().to_string();
                if !value.is_empty() {
                    if let Some(id) = this.editing_id.clone() {
                        if let Some(item) = this.local.iter_mut().find(|c| c.id == id) {
                            item.name = value.into();
                        }
                        this.editing_id = None;
                        this.push(cx);
                        cx.notify();
                    }
                }
            }
        }));
        // Picking a colour in the edit row writes it back to the edited item.
        subs.push(cx.subscribe::<ColorPickerState, ColorPickerEvent>(
            &edit_color,
            |this, _st, ev, cx| {
                if let ColorPickerEvent::Change(Some(color)) = ev {
                    if let Some(id) = this.editing_id.clone() {
                        if let Some(item) = this.local.iter_mut().find(|c| c.id == id) {
                            item.color = color.to_hex();
                        }
                        this.push(cx);
                        cx.notify();
                    }
                }
            },
        ));
        subs.push(cx.subscribe::<ColorPickerState, ColorPickerEvent>(
            &new_color_picker,
            |this, _st, _ev, cx| {
                this.push(cx);
                cx.notify();
            },
        ));
        // Pressing Enter (or the configured add shortcut) while the new-label
        // input is focused adds the entry instead of closing the containing
        // dialog. The dialog binds `enter -> Confirm` (close), so we intercept
        // the keystroke before that binding and consume it after adding.
        let panel = cx.entity();
        let new_name_handle = new_name.clone();
        subs.push(cx.intercept_keystrokes(move |event, window, cx| {
            let shortcut = cx
                .try_global::<DoitAppHandle>()
                .map(|h| h.0.read(cx).settings.add_todo_shortcut.clone())
                .unwrap_or_else(|| ShortcutConfig {
                    key: "Enter".into(),
                    ctrl: false,
                    shift: false,
                    alt: false,
                    meta: false,
                });
            if !add_shortcut_matches(&event.keystroke, &shortcut) {
                return;
            }
            if !new_name_handle.read(cx).focus_handle(cx).is_focused(window) {
                return;
            }
            panel.update(cx, |p, cx| p.add_new(window, cx));
            cx.stop_propagation();
        }));

        Self {
            local: categories,
            default_id,
            editing_id: None,
            show_done,
            edit_name,
            new_name,
            edit_color,
            new_color_picker,
            row_pickers: HashMap::new(),
            hover_row: None,
            seq: 0,
            _subs: subs,
        }
    }

    fn push(&mut self, cx: &mut Context<Self>) {
        let app = cx.try_global::<DoitAppHandle>().map(|h| h.0.clone());
        if let Some(app) = app {
            let categories = self.local.clone();
            let default_id = self.default_id.clone();
            app.update(cx, |app, cx| app.set_categories(categories, default_id, cx));
        }
    }

    /// Lazily create (and subscribe to) the committed colour picker for one
    /// row, keeping its swatch in sync with the stored colour. Picks made in a
    /// row's swatch write back to that row immediately.
    fn ensure_row_picker(
        &mut self,
        id: &str,
        color: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        if let Some(st) = self.row_pickers.get(id) {
            let wanted = color_from_hex(color);
            if st.read(cx).value() != Some(wanted) {
                st.update(cx, |s, cx| s.set_value(wanted, window, cx));
            }
            return st.clone();
        }
        let st = cx.new(|cx| ColorPickerState::new(window, cx).default_value(color_from_hex(color)));
        let target = id.to_string();
        self._subs.push(cx.subscribe::<ColorPickerState, ColorPickerEvent>(
            &st,
            move |this, _st, ev, cx| {
                if let ColorPickerEvent::Change(Some(color)) = ev {
                    if let Some(item) = this.local.iter_mut().find(|c| c.id == target) {
                        item.color = color.to_hex();
                    }
                    this.push(cx);
                    cx.notify();
                }
            },
        ));
        self.row_pickers.insert(id.to_string(), st.clone());
        st
    }

    /// Add a new category from the add-row input + colour picker. Shared by the
    /// 添加 button and the Enter / add-shortcut key handler.
    fn add_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let color = self
            .new_color_picker
            .read(cx)
            .value()
            .map(|c| c.to_hex())
            .unwrap_or_default();
        self.seq += 1;
        let mut id = fresh_id("cat", self.seq);
        while self.local.iter().any(|c| c.id == id) {
            self.seq += 1;
            id = fresh_id("cat", self.seq);
        }
        self.local.push(Category {
            id,
            name: name.into(),
            color,
        });
        self.new_name.update(cx, |s, cx| s.set_value("", window, cx));
        self.push(cx);
        cx.notify();
    }
}

impl Render for CategoriesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let edit_name = self.edit_name.clone();
        let new_name = self.new_name.clone();
        let edit_color = self.edit_color.clone();
        let new_color_picker = self.new_color_picker.clone();
        let rows = self.local.clone();
        let row_pickers: Vec<(String, Entity<ColorPickerState>)> = rows
            .iter()
            .map(|r| {
                let picker = self.ensure_row_picker(&r.id, &r.color, window, cx);
                (r.id.clone(), picker)
            })
            .collect();

        div()
            .v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("点「编辑」修改名称与颜色，勾选默认分类。"),
            )
            .child(
                div()
                    .id("cat-list")
                    .flex_1()
                    .min_h_0()
                    .v_flex()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(rows.iter().map(|cat| {
                        let id = cat.id.clone();
                        let name = cat.name.clone();
                        let is_default = self.default_id.as_deref() == Some(&*id);
                        let editing = self.editing_id.as_deref() == Some(&*id);
                        let row_picker = row_pickers
                            .iter()
                            .find(|(pid, _)| pid == &id)
                            .map(|(_, p)| p.clone())
                            .unwrap();

                        let is_hovered = self.hover_row.as_deref() == Some(&*id);
                        let hover_entity = cx.entity();
                        let hover_id = id.clone();
                        let mut row = div()
                            .id(ElementId::Name(format!("cat-row-{id}").into()))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px(px(8.))
                            .py(px(6.))
                            .rounded_lg()
                            .hover(|s| s.bg(cx.theme().muted))
                            .on_hover(move |entered, _, cx| {
                                let hover_id = hover_id.clone();
                                hover_entity.update(cx, |p, cx| {
                                    let prev = p.hover_row.clone();
                                    p.hover_row = if *entered { Some(hover_id) } else { None };
                                    if p.hover_row != prev {
                                        cx.notify();
                                    }
                                });
                            });

                        if editing {
                            row = row
                                .child(
                                    div().w(px(160.)).child(
                                        Input::new(&edit_name).small().flex_1(),
                                    ),
                                )
                                .child(
                                    ColorPicker::new(&edit_color)
                                        .featured_colors(picker_featured_colors())
                                        .small(),
                                )
                                .child(
                                    Button::new(ElementId::Name(format!("cat-save-{id}").into()))
                                        .small()
                                        .ghost()
                                        .label("保存")
                                        .on_click({
                                            let entity = entity.clone();
                                            let id = id.clone();
                                            move |_e, _win, cx| {
                                                let value = {
                                                    let p = entity.read(cx);
                                                    p.edit_name.read(cx).value().trim().to_string()
                                                };
                                                entity.update(cx, |p, cx| {
                                                    if !value.is_empty() {
                                                        if let Some(item) = p
                                                            .local
                                                            .iter_mut()
                                                            .find(|c| c.id == id)
                                                        {
                                                            item.name = value.into();
                                                        }
                                                    }
                                                    p.editing_id = None;
                                                    p.push(cx);
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                                .child(
                                    Button::new(ElementId::Name(format!("cat-cancel-{id}").into()))
                                        .small()
                                        .ghost()
                                        .label("取消")
                                        .on_click({
                                            let entity = entity.clone();
                                            move |_e, _win, cx| {
                                                entity.update(cx, |p, cx| {
                                                    p.editing_id = None;
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                                .child(
                                    Button::new(ElementId::Name(format!("cat-del-{id}").into()))
                                        .danger()
                                        .xsmall()
                                        .label("删除")
                                        .on_click({
                                            let entity = entity.clone();
                                            let id = id.clone();
                                            move |_e, _win, cx| {
                                                entity.update(cx, |p, cx| {
                                                    p.local.retain(|c| c.id != id);
                                                    if p.default_id.as_deref() == Some(&*id) {
                                                        p.default_id = None;
                                                    }
                                                    p.push(cx);
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                );
                        } else {
                            row = row
                                .child(
                                    ColorPicker::new(&row_picker)
                                        .featured_colors(picker_featured_colors())
                                        .small(),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_color(cx.theme().foreground)
                                        .child(name.clone()),
                                )
                                .when(is_hovered, |el| {
                                    el.child(
                                        Button::new(ElementId::Name(format!("cat-edit-{id}").into()))
                                            .ghost()
                                            .xsmall()
                                            .tooltip("重命名")
                                            .label("编辑")
                                            .on_click({
                                                let entity = entity.clone();
                                                let edit_name = edit_name.clone();
                                                let edit_color = edit_color.clone();
                                                let id = id.clone();
                                                move |_e, window, cx| {
                                                    let name = {
                                                        let p = entity.read(cx);
                                                        p.local
                                                            .iter()
                                                            .find(|c| c.id == id)
                                                            .map(|c| c.name.clone())
                                                            .unwrap_or_default()
                                                    };
                                                    let color_hex = {
                                                        let p = entity.read(cx);
                                                        p.local
                                                            .iter()
                                                            .find(|c| c.id == id)
                                                            .map(|c| c.color.clone())
                                                            .unwrap_or_default()
                                                    };
                                                    entity.update(cx, |p, cx| {
                                                        p.editing_id = Some(id.clone());
                                                        cx.notify();
                                                    });
                                                    edit_name.update(cx, |s, cx| {
                                                        s.set_value(name, window, cx)
                                                    });
                                                    edit_color.update(cx, |s, cx| {
                                                        s.set_value(color_from_hex(&color_hex), window, cx)
                                                    });
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new(ElementId::Name(format!("cat-default-{id}").into()))
                                            .small()
                                            .ghost()
                                            .when(is_default, |b| b.selected(true))
                                            .label(if is_default {
                                                "默认".to_string()
                                            } else {
                                                "设为默认".to_string()
                                            })
                                            .on_click({
                                                let entity = entity.clone();
                                                let id = id.clone();
                                                move |_e, _win, cx| {
                                                    entity.update(cx, |p, cx| {
                                                        p.default_id = if p.default_id.as_deref()
                                                            == Some(&*id)
                                                        {
                                                            None
                                                        } else {
                                                            Some(id.clone())
                                                        };
                                                        p.push(cx);
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new(ElementId::Name(format!("cat-del-{id}").into()))
                                            .danger()
                                            .xsmall()
                                            .label("删除")
                                            .on_click({
                                                let entity = entity.clone();
                                                let id = id.clone();
                                                move |_e, _win, cx| {
                                                    entity.update(cx, |p, cx| {
                                                        p.local.retain(|c| c.id != id);
                                                        if p.default_id.as_deref() == Some(&*id) {
                                                            p.default_id = None;
                                                        }
                                                        p.push(cx);
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    )
                                });
                        }

                        row.into_any_element()
                    }))
                    .into_any_element(),
            )
            .child(div().border_t_1().border_color(cx.theme().border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        ColorPicker::new(&new_color_picker)
                            .featured_colors(picker_featured_colors())
                            .small(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&new_name).small().flex_1()),
                    )
                    .child(
                        Button::new("cat-add")
                            .small()
                            .primary()
                            .label("添加")
                            .on_click({
                                let entity = cx.entity();
                                move |_e, window, cx| {
                                    entity.update(cx, |p, cx| p.add_new(window, cx));
                                }
                            }),
                    ),
            )
            .when(self.show_done && !self.local.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .justify_end()
                        .child(
                            Button::new("cat-done")
                                .small()
                                .primary()
                                .label("完成")
                                .on_click({
                                    let entity = entity.clone();
                                    move |_e, window, cx| {
                                        entity.update(cx, |p, _cx| {
                                            p.editing_id = None;
                                        });
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
            })
    }
}

// ── Tag management panel ────────────────────────────────────────────────────

pub struct TagsPanel {
    local: Vec<Tag>,
    editing_id: Option<String>,
    show_done: bool,
    edit_name: Entity<InputState>,
    new_name: Entity<InputState>,
    edit_color: Entity<ColorPickerState>,
    new_color_picker: Entity<ColorPickerState>,
    /// One committed colour picker per row, so every row's swatch shows and
    /// edits its own colour directly.
    row_pickers: HashMap<String, Entity<ColorPickerState>>,
    /// The row currently under the cursor; its 编辑/删除 actions are only
    /// shown while hovered.
    hover_row: Option<String>,
    seq: u64,
    _subs: Vec<Subscription>,
}

impl TagsPanel {
    pub fn new(
        tags: Vec<Tag>,
        show_done: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let edit_name = cx.new(|cx| InputState::new(window, cx).placeholder("标签名称"));
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("新标签名称"));
        let edit_color = cx.new(|cx| ColorPickerState::new(window, cx));
        let new_color_picker = cx.new(|cx| {
            ColorPickerState::new(window, cx)
                .default_value(color_from_hex(COLOR_PALETTE[2].1))
        });

        let mut subs = Vec::new();
        subs.push(cx.subscribe::<InputState, InputEvent>(&edit_name, |this, st, ev, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                let value = st.read(cx).value().trim().to_string();
                if !value.is_empty() {
                    if let Some(id) = this.editing_id.clone() {
                        if let Some(item) = this.local.iter_mut().find(|t| t.id == id) {
                            item.name = value.into();
                        }
                        this.editing_id = None;
                        this.push(cx);
                        cx.notify();
                    }
                }
            }
        }));
        // Picking a colour in the edit row writes it back to the edited tag.
        subs.push(cx.subscribe::<ColorPickerState, ColorPickerEvent>(
            &edit_color,
            |this, _st, ev, cx| {
                if let ColorPickerEvent::Change(Some(color)) = ev {
                    if let Some(id) = this.editing_id.clone() {
                        if let Some(item) = this.local.iter_mut().find(|t| t.id == id) {
                            item.color = color.to_hex();
                        }
                        this.push(cx);
                        cx.notify();
                    }
                }
            },
        ));
        subs.push(cx.subscribe::<ColorPickerState, ColorPickerEvent>(
            &new_color_picker,
            |this, _st, _ev, cx| {
                this.push(cx);
                cx.notify();
            },
        ));
        // Pressing Enter (or the configured add shortcut) while the new-label
        // input is focused adds the entry instead of closing the containing
        // dialog. The dialog binds `enter -> Confirm` (close), so we intercept
        // the keystroke before that binding and consume it after adding.
        let panel = cx.entity();
        let new_name_handle = new_name.clone();
        subs.push(cx.intercept_keystrokes(move |event, window, cx| {
            let shortcut = cx
                .try_global::<DoitAppHandle>()
                .map(|h| h.0.read(cx).settings.add_todo_shortcut.clone())
                .unwrap_or_else(|| ShortcutConfig {
                    key: "Enter".into(),
                    ctrl: false,
                    shift: false,
                    alt: false,
                    meta: false,
                });
            if !add_shortcut_matches(&event.keystroke, &shortcut) {
                return;
            }
            if !new_name_handle.read(cx).focus_handle(cx).is_focused(window) {
                return;
            }
            panel.update(cx, |p, cx| p.add_new(window, cx));
            cx.stop_propagation();
        }));

        Self {
            local: tags,
            editing_id: None,
            show_done,
            edit_name,
            new_name,
            edit_color,
            new_color_picker,
            row_pickers: HashMap::new(),
            hover_row: None,
            seq: 0,
            _subs: subs,
        }
    }

    fn push(&mut self, cx: &mut Context<Self>) {
        let app = cx.try_global::<DoitAppHandle>().map(|h| h.0.clone());
        if let Some(app) = app {
            let tags = self.local.clone();
            app.update(cx, |app, cx| app.set_tags(tags, cx));
        }
    }

    /// Lazily create (and subscribe to) the committed colour picker for one
    /// row, keeping its swatch in sync with the stored colour. Picks made in a
    /// row's swatch write back to that row immediately.
    fn ensure_row_picker(
        &mut self,
        id: &str,
        color: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        if let Some(st) = self.row_pickers.get(id) {
            let wanted = color_from_hex(color);
            if st.read(cx).value() != Some(wanted) {
                st.update(cx, |s, cx| s.set_value(wanted, window, cx));
            }
            return st.clone();
        }
        let st = cx.new(|cx| ColorPickerState::new(window, cx).default_value(color_from_hex(color)));
        let target = id.to_string();
        self._subs.push(cx.subscribe::<ColorPickerState, ColorPickerEvent>(
            &st,
            move |this, _st, ev, cx| {
                if let ColorPickerEvent::Change(Some(color)) = ev {
                    if let Some(item) = this.local.iter_mut().find(|t| t.id == target) {
                        item.color = color.to_hex();
                    }
                    this.push(cx);
                    cx.notify();
                }
            },
        ));
        self.row_pickers.insert(id.to_string(), st.clone());
        st
    }

    /// Add a new tag from the add-row input + colour picker. Shared by the
    /// 添加 button and the Enter / add-shortcut key handler.
    fn add_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let color = self
            .new_color_picker
            .read(cx)
            .value()
            .map(|c| c.to_hex())
            .unwrap_or_default();
        self.seq += 1;
        let mut id = fresh_id("tag", self.seq);
        while self.local.iter().any(|t| t.id == id) {
            self.seq += 1;
            id = fresh_id("tag", self.seq);
        }
        self.local.push(Tag {
            id,
            name: name.into(),
            color,
        });
        self.new_name.update(cx, |s, cx| s.set_value("", window, cx));
        self.push(cx);
        cx.notify();
    }
}

impl Render for TagsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let edit_name = self.edit_name.clone();
        let new_name = self.new_name.clone();
        let edit_color = self.edit_color.clone();
        let new_color_picker = self.new_color_picker.clone();
        let rows = self.local.clone();
        let row_pickers: Vec<(String, Entity<ColorPickerState>)> = rows
            .iter()
            .map(|r| {
                let picker = self.ensure_row_picker(&r.id, &r.color, window, cx);
                (r.id.clone(), picker)
            })
            .collect();

        div()
            .v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("点「编辑」修改名称与颜色。"),
            )
            .child(
                div()
                    .id("tag-list")
                    .flex_1()
                    .min_h_0()
                    .v_flex()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(rows.iter().map(|tag| {
                        let id = tag.id.clone();
                        let name = tag.name.clone();
                        let editing = self.editing_id.as_deref() == Some(&*id);
                        let row_picker = row_pickers
                            .iter()
                            .find(|(pid, _)| pid == &id)
                            .map(|(_, p)| p.clone())
                            .unwrap();
                        let is_hovered = self.hover_row.as_deref() == Some(&*id);
                        let hover_entity = cx.entity();
                        let hover_id = id.clone();
                        let mut row = div()
                            .id(ElementId::Name(format!("tag-row-{id}").into()))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px(px(8.))
                            .py(px(6.))
                            .rounded_lg()
                            .hover(|s| s.bg(cx.theme().muted))
                            .on_hover(move |entered, _, cx| {
                                let hover_id = hover_id.clone();
                                hover_entity.update(cx, |p, cx| {
                                    let prev = p.hover_row.clone();
                                    p.hover_row = if *entered { Some(hover_id) } else { None };
                                    if p.hover_row != prev {
                                        cx.notify();
                                    }
                                });
                            });

                        if editing {
                            row = row
                                .child(
                                    div().w(px(180.)).child(
                                        Input::new(&edit_name).small().flex_1(),
                                    ),
                                )
                                .child(
                                    ColorPicker::new(&edit_color)
                                        .featured_colors(picker_featured_colors())
                                        .small(),
                                )
                                .child(
                                    Button::new(ElementId::Name(format!("tag-save-{id}").into()))
                                        .small()
                                        .ghost()
                                        .label("保存")
                                        .on_click({
                                            let entity = entity.clone();
                                            let id = id.clone();
                                            move |_e, _win, cx| {
                                                let value = {
                                                    let p = entity.read(cx);
                                                    p.edit_name.read(cx).value().trim().to_string()
                                                };
                                                entity.update(cx, |p, cx| {
                                                    if !value.is_empty() {
                                                        if let Some(item) = p
                                                            .local
                                                            .iter_mut()
                                                            .find(|t| t.id == id)
                                                        {
                                                            item.name = value.into();
                                                        }
                                                    }
                                                    p.editing_id = None;
                                                    p.push(cx);
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                                .child(
                                    Button::new(ElementId::Name(format!("tag-cancel-{id}").into()))
                                        .small()
                                        .ghost()
                                        .label("取消")
                                        .on_click({
                                            let entity = entity.clone();
                                            move |_e, _win, cx| {
                                                entity.update(cx, |p, cx| {
                                                    p.editing_id = None;
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                                .child(
                                    Button::new(ElementId::Name(format!("tag-del-{id}").into()))
                                        .danger()
                                        .xsmall()
                                        .label("删除")
                                        .on_click({
                                            let entity = entity.clone();
                                            let id = id.clone();
                                            move |_e, _win, cx| {
                                                entity.update(cx, |p, cx| {
                                                    p.local.retain(|t| t.id != id);
                                                    p.push(cx);
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                );
                        } else {
                            row = row
                                .child(
                                    ColorPicker::new(&row_picker)
                                        .featured_colors(picker_featured_colors())
                                        .small(),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_color(cx.theme().foreground)
                                        .child(name.clone()),
                                )
                                .when(is_hovered, |el| {
                                    el.child(
                                        Button::new(ElementId::Name(format!("tag-edit-{id}").into()))
                                            .ghost()
                                            .xsmall()
                                            .tooltip("重命名")
                                            .label("编辑")
                                            .on_click({
                                                let entity = entity.clone();
                                                let edit_name = edit_name.clone();
                                                let edit_color = edit_color.clone();
                                                let id = id.clone();
                                                move |_e, window, cx| {
                                                    let name = {
                                                        let p = entity.read(cx);
                                                        p.local
                                                            .iter()
                                                            .find(|t| t.id == id)
                                                            .map(|t| t.name.clone())
                                                            .unwrap_or_default()
                                                    };
                                                    let color_hex = {
                                                        let p = entity.read(cx);
                                                        p.local
                                                            .iter()
                                                            .find(|t| t.id == id)
                                                            .map(|t| t.color.clone())
                                                            .unwrap_or_default()
                                                    };
                                                    entity.update(cx, |p, cx| {
                                                        p.editing_id = Some(id.clone());
                                                        cx.notify();
                                                    });
                                                    edit_name.update(cx, |s, cx| {
                                                        s.set_value(name, window, cx)
                                                    });
                                                    edit_color.update(cx, |s, cx| {
                                                        s.set_value(color_from_hex(&color_hex), window, cx)
                                                    });
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new(ElementId::Name(format!("tag-del-{id}").into()))
                                            .danger()
                                            .xsmall()
                                            .label("删除")
                                            .on_click({
                                                let entity = entity.clone();
                                                let id = id.clone();
                                                move |_e, _win, cx| {
                                                    entity.update(cx, |p, cx| {
                                                        p.local.retain(|t| t.id != id);
                                                        p.push(cx);
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    )
                                });
                        }

                        row.into_any_element()
                    }))
                    .into_any_element(),
            )
            .child(div().border_t_1().border_color(cx.theme().border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        ColorPicker::new(&new_color_picker)
                            .featured_colors(picker_featured_colors())
                            .small(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&new_name).small().flex_1()),
                    )
                    .child(
                        Button::new("tag-add")
                            .small()
                            .primary()
                            .label("添加")
                            .on_click({
                                let entity = cx.entity();
                                move |_e, window, cx| {
                                    entity.update(cx, |p, cx| p.add_new(window, cx));
                                }
                            }),
                    ),
            )
            .when(self.show_done && !self.local.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .justify_end()
                        .child(
                            Button::new("tag-done")
                                .small()
                                .primary()
                                .label("完成")
                                .on_click({
                                    let entity = entity.clone();
                                    move |_e, window, cx| {
                                        entity.update(cx, |p, _cx| {
                                            p.editing_id = None;
                                        });
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
            })
    }
}
