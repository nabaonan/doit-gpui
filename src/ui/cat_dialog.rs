//! 分类管理弹框（基于 gpui-kit 自带 `Dialog` 组件 + 显式 `DialogFooter`）
//!
//! - 添加分类：多输入行，每行左侧色块点击弹出「气泡」展示一整行内置色
//!   （选中效果与设置页「完成进度风格」一致：同色描边环 + 白色内环 + 白色对钩）；
//!   「再添一个」用 gpui-kit Button 追加行；底部「保存 / 取消」由自定义
//!   DialogFooter 提供（此前用纯 Dialog 时没有保存按钮，正是因为纯 Dialog 不
//!   渲染 button_props，必须显式加 DialogFooter）。
//! - 编辑分类：单输入框改名。
//! - 删除分类：确认（任务移入默认）。
//!
//! open_dialog 的 builder 是 `Fn`（层重建时可多次调用），故所有内容元素都在
//! 闭包内部构建（不 move 外部构建结果）。

use crate::app::AppState;
use crate::theme::palette::DoitPalette;
use crate::ui::icons;
use gpui_kit::component::button::{Button, ButtonRounded};
use gpui_kit::component::dialog::{DialogFooter, DialogTitle};
use gpui_kit::component::input::Input;
use gpui_kit::component::WindowExt;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 打开「添加分类」Dialog（可一次加多个，每个带颜色气泡）
pub fn open_add(state: &Entity<AppState>, window: &mut Window, cx: &mut App) {
    state.update(cx, |st, cx| st.open_add_categories(window, cx));
    let st = state.clone();
    window.open_dialog(cx, move |dialog, window, cx| {
        let st_content = st.clone();
        dialog
            .title(DialogTitle::new().child("添加分类"))
            .content(move |content, window, cx| {
                let rows: Vec<(usize, usize)> = st_content
                    .read(cx)
                    .cat_add_rows
                    .clone()
                    .map(|r| r.iter().enumerate().map(|(i, (_, c))| (i, *c)).collect())
                    .unwrap_or_default();
                let bubble_open = st_content.read(cx).cat_color_open;
                content
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .child(rows_block(&st_content, &rows, bubble_open, window, cx))
                            .child(add_more_button(&st_content, window, cx)),
                    )
            })
            .width(px(400.0))
            .footer(
                DialogFooter::new()
                    .justify_end()
                    .child(cancel_button(&st, window, cx))
                    .child(save_button(&st, window, cx, |st, window, cx| {
                        let ok = st.confirm_add_categories(cx);
                        if ok {
                            window.close_dialog(cx);
                        }
                    })),
            )
    });
}

/// 打开「编辑分类」Dialog
pub fn open_edit(state: &Entity<AppState>, name: &str, window: &mut Window, cx: &mut App) {
    state.update(cx, |st, cx| st.open_edit_category(name, window, cx));
    let st = state.clone();
    window.open_dialog(cx, move |dialog, window, cx| {
        let st_content = st.clone();
        dialog
            .title(DialogTitle::new().child("编辑分类"))
            .content(move |content, _window, cx| {
                let p = st_content.read(cx).theme().palette();
                let color_ix = st_content
                    .read(cx)
                    .cat_edit
                    .as_ref()
                    .map_or(0, |(_, ix, _)| *ix);
                let input: Option<Entity<gpui_kit::component::input::InputState>> =
                    st_content.read(cx).cat_edit.as_ref().map(|(_, _, i)| i.clone());
                let bubble_open = st_content.read(cx).cat_color_open == Some(0);
                let input_el: Option<AnyElement> =
                    input.map(|i| Input::new(&i).flex_1().h(px(36.0)).into_any_element());
                let hex = u32::from_str_radix(
                    crate::domain::types::SnapshotV2::CAT_PALETTE[color_ix % 8]
                        .trim_start_matches('#'),
                    16,
                )
                .unwrap_or(0x999999);
                let s2 = st_content.clone();
                let row = div()
                    .id("cat-edit-row")
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    // 色块（点开气泡换颜色）
                    .child(
                        div()
                            .id("cat-edit-color")
                            .w(px(36.0))
                            .h(px(36.0))
                            .rounded_full()
                            .border_1()
                            .border_color(p.line)
                            .bg(rgb(hex))
                            .cursor_pointer()
                            .hover(|h| h.border_color(p.mint))
                            .on_click(move |_, _w, cx| {
                                s2.update(cx, |st, cx| {
                                    st.cat_toggle_color_bubble(0);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(input_el.unwrap_or_else(|| div().flex_1().into_any_element()));
                let body = if bubble_open {
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(row)
                        .child(edit_color_bubble(&st_content, color_ix, p))
                } else {
                    div().w_full().child(row)
                };
                content.child(body)
            })
            .width(px(360.0))
            .footer(
                DialogFooter::new()
                    .justify_end()
                    .child(cancel_button(&st, window, cx))
                    .child(save_button(&st, window, cx, |st, window, cx| {
                        let ok = st.confirm_edit_category(cx);
                        if ok {
                            window.close_dialog(cx);
                        }
                    })),
            )
    });
}

/// 打开「删除分类」确认 Dialog
pub fn open_confirm_delete(state: &Entity<AppState>, name: &str, window: &mut Window, cx: &mut App) {
    state.update(cx, |st, _| st.open_delete_category(name));
    let st = state.clone();
    let n = name.to_string();
    window.open_dialog(cx, move |dialog, window, cx| {
        let p = st.read(cx).theme().palette();
        let n_content = n.clone();
        dialog
            .title(DialogTitle::new().child("删除分类"))
            .content(move |content, _window, _cx| {
                content.child(
                    div()
                        .text_size(px(13.0))
                        .text_color(p.ink_2)
                        .child(format!(
                            "确定删除分类「{n_content}」？该分类下的任务将移入「默认」，不会丢失。"
                        )),
                )
            })
            .width(px(360.0))
            .footer({
                let nn = n.clone();
                DialogFooter::new()
                    .justify_end()
                    .child(cancel_button(&st, window, cx))
                    .child(
                        save_button(&st, window, cx, move |st, window, cx| {
                            let ok = st.confirm_delete_category(&nn);
                            if ok {
                                window.close_dialog(cx);
                            }
                        }),
                    )
            })
    });
}

/// 底部「取消」按钮
fn cancel_button(st: &Entity<AppState>, _window: &mut Window, cx: &mut App) -> AnyElement {
    let p = st.read(cx).theme().palette();
    let s = st.clone();
    Button::new("cat-dlg-cancel")
        .label("取消")
        .h(px(32.0))
        .rounded(ButtonRounded::Medium)
        .text_color(p.ink_2)
        .on_click(move |_, window, cx| {
            s.update(cx, |st, cx| {
                st.cat_add_rows = None;
                st.cat_edit = None;
                st.cat_delete = None;
                st.cat_color_open = None;
                cx.notify();
            });
            window.close_dialog(cx);
        })
        .into_any_element()
}

/// 底部「保存」按钮（handler 收到关闭前的确认逻辑）
fn save_button(
    st: &Entity<AppState>,
    _window: &mut Window,
    cx: &mut App,
    on_confirm: impl Fn(&mut AppState, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let p = st.read(cx).theme().palette();
    let s = st.clone();
    Button::new("cat-dlg-save")
        .label("保存")
        .h(px(32.0))
        .rounded(ButtonRounded::Medium)
        .bg(p.mint)
        .text_color(rgb(0xFFFFFF))
        .on_click(move |_, window, cx| {
            s.update(cx, |st, cx| on_confirm(st, window, cx));
        })
        .into_any_element()
}

/// 添加弹窗的输入行列表（每行：色块 + 输入框；展开气泡时附颜色气泡）
fn rows_block(
    st: &Entity<AppState>,
    rows: &[(usize, usize)],
    bubble_open: Option<usize>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let p = st.read(cx).theme().palette();
    let mut rows_div = div().flex().flex_col().gap(px(10.0)).w_full();
    for (i, color_ix) in rows {
        let i = *i;
        let color_ix = *color_ix;
        let hex = u32::from_str_radix(
            crate::domain::types::SnapshotV2::CAT_PALETTE[color_ix % 8]
                .trim_start_matches('#'),
            16,
        )
        .unwrap_or(0x999999);
        let s2 = st.clone();
        let row_input: Option<AnyElement> = st
            .read(cx)
            .cat_add_rows
            .as_ref()
            .and_then(|r| r.get(i).map(|(e, _)| e.clone()))
            .map(|e| Input::new(&e).flex_1().h(px(36.0)).into_any_element());
        let line = div()
            .id(format!("cat-add-row-{i}"))
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .id(format!("cat-color-{i}"))
                    .w(px(36.0))
                    .h(px(36.0))
                    .rounded_full()
                    .border_1()
                    .border_color(p.line)
                    .bg(rgb(hex))
                    .cursor_pointer()
                    .hover(|h| h.border_color(p.mint))
                    .on_click(move |_, _w, cx| {
                        s2.update(cx, |st, cx| {
                            st.cat_toggle_color_bubble(i);
                            cx.notify();
                        });
                    }),
            )
            .child(row_input.unwrap_or_else(|| div().flex_1().into_any_element()));
        rows_div = if bubble_open == Some(i) {
            rows_div.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(line)
                    .child(color_bubble(st, i, color_ix, p)),
            )
        } else {
            rows_div.child(line)
        };
    }
    rows_div.into_any_element()
}

/// 「再添一个」按钮（gpui-kit Button）
fn add_more_button(st: &Entity<AppState>, _window: &mut Window, cx: &mut App) -> AnyElement {
    let p = st.read(cx).theme().palette();
    let s = st.clone();
    Button::new("cat-add-more")
        .label("再添一个")
        .icon(gpui_kit::component::IconName::Plus)
        .h(px(32.0))
        .text_color(p.mint_deep)
        .rounded(ButtonRounded::Medium)
        .on_click(move |_, window, cx| {
            s.update(cx, |st, cx| st.cat_add_input(window, cx));
            s.update(cx, |st, _| st.cat_color_open = None);
        })
        .into_any_element()
}

/// 颜色气泡：一整行内置色，选中效果与设置页「完成进度风格」一致
fn color_bubble(
    state: &Entity<AppState>,
    row: usize,
    current: usize,
    p: &'static DoitPalette,
) -> AnyElement {
    let s = state.clone();
    let mut line = div()
        .id(format!("cat-color-bubble-{row}"))
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(p.line)
        .bg(p.card)
        .shadow_md();
    for (ix, hex) in crate::domain::types::SnapshotV2::CAT_PALETTE.iter().enumerate() {
        let color = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x999999);
        let on = ix == current;
        let s2 = s.clone();
        // 未选中：纯色圆角方块 + 淡描边；选中：同色描边环 + 白色内环 + 白色对钩
        line = line.child(
            div()
                .id(format!("cat-color-swatch-{row}-{ix}"))
                .w(px(30.0))
                .h(px(30.0))
                .rounded(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(rgb(color))
                .when(on, |e| e.p(px(2.0)))
                .when(!on, |e| e.border_1().border_color(p.ink_3.alpha(0.30)))
                .hover(move |h| h.bg(rgb(color)))
                .when(on, |e| {
                    e.child(
                        div()
                            .w_full()
                            .h_full()
                            .rounded(px(8.0))
                            .bg(p.card)
                            .p(px(2.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .w_full()
                                    .h_full()
                                    .rounded(px(6.0))
                                    .bg(rgb(color))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .data(&icons::check())
                                            .w(px(15.0))
                                            .h(px(15.0))
                                            .text_color(rgb(0xFFFFFF)),
                                    ),
                            ),
                    )
                })
                .on_click(move |_, _w, cx| {
                    s2.update(cx, |st, cx| {
                        st.cat_set_row_color(row, ix);
                        cx.notify();
                    });
                }),
        );
    }
    line.into_any_element()
}

/// 编辑分类弹框的颜色气泡：一整行内置色（样式同 color_bubble，但选中事件写
/// `cat_set_edit_color`，写回到编辑弹窗里的颜色索引）。
fn edit_color_bubble(
    state: &Entity<AppState>,
    current: usize,
    p: &'static DoitPalette,
) -> AnyElement {
    let s = state.clone();
    let mut line = div()
        .id("cat-edit-color-bubble")
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(p.line)
        .bg(p.card)
        .shadow_md();
    for (ix, hex) in crate::domain::types::SnapshotV2::CAT_PALETTE.iter().enumerate() {
        let color = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x999999);
        let on = ix == current;
        let s2 = s.clone();
        // 未选中：纯色圆角方块 + 淡描边；选中：同色描边环 + 白色内环 + 白色对钩
        line = line.child(
            div()
                .id(format!("cat-edit-swatch-{ix}"))
                .w(px(30.0))
                .h(px(30.0))
                .rounded(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(rgb(color))
                .when(on, |e| e.p(px(2.0)))
                .when(!on, |e| e.border_1().border_color(p.ink_3.alpha(0.30)))
                .hover(move |h| h.bg(rgb(color)))
                .when(on, |e| {
                    e.child(
                        div()
                            .w_full()
                            .h_full()
                            .rounded(px(8.0))
                            .bg(p.card)
                            .p(px(2.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .w_full()
                                    .h_full()
                                    .rounded(px(6.0))
                                    .bg(rgb(color))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .data(&icons::check())
                                            .w(px(15.0))
                                            .h(px(15.0))
                                            .text_color(rgb(0xFFFFFF)),
                                    ),
                            ),
                    )
                })
                .on_click(move |_, _w, cx| {
                    s2.update(cx, |st, cx| {
                        st.cat_set_edit_color(ix);
                        cx.notify();
                    });
                }),
        );
    }
    line.into_any_element()
}
