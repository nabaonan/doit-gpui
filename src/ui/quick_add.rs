//! 底部快速添加栏（对齐原型：优先级胶囊 + 输入框 + 分类下拉）
//!
//! 作为根 flex 布局的最后一项，固定在底部（不随主区滚动）。
//! - 左侧优先级胶囊：点击展开下拉（紧急 / 高 / 普通 / 低 / 不设）
//! - 右侧分类触发器：点击展开自定义分类下拉（含「全部」）
//!
//! 下拉菜单本身由 `render_menus` 以「根节点绝对定位」的方式渲染，
//! 保证浮在主内容之上；点击菜单外区域由根视图的透明遮罩关闭。

use crate::app::{AppState, QuickMenu};
use crate::theme::palette::{DoitPalette, metrics, priority};
use crate::ui::icons;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 菜单横向对齐：页面左右留白 26 + 栏内留白 14
const BAR_INSET: f32 = 40.0;
/// 菜单底边距窗口底部的高度（栏顶距底部 76 + 间距 6）
const MENU_BOTTOM: f32 = 82.0;

/// 渲染底部快速添加栏
pub fn render_quick_add(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let prio = state.read(cx).quick_add_priority;
    let prio_color = p.priority(prio);
    let input = state.read(cx).quick_input.clone();
    let sel_cat = state.read(cx).quick_add_category.clone();
    let cat_color = state
        .read(cx)
        .snapshot
        .categories
        .iter()
        .find(|c| c.name == sel_cat)
        .map(|c| c.color.clone())
        .unwrap_or_default();

    div()
        .id("qa-bar-wrap")
        .w_full()
        .px(px(34.0))
        .pb(px(18.0))
        .pt(px(10.0))
        .child(
            div()
                .id("qa-bar")
                .w_full()
                .h(px(58.0))
                .px(px(14.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .rounded(px(metrics::RADIUS))
                .border_1()
                .border_color(p.mint)
                .bg(p.card)
                // 优先级胶囊（点击展开下拉）
                .child(priority_pill(state, prio, prio_color))
                // 输入框
                .child(Input::new(&input).flex_1())
                // 分类下拉触发器
                .child(category_trigger(state, &sel_cat, &cat_color, p)),
        )
}

/// 左侧优先级胶囊（彩色圆点 + 名称 + 箭头）
fn priority_pill(
    state: &Entity<AppState>,
    prio: u8,
    color: gpui_kit::Rgba,
) -> impl IntoElement {
    let s = state.clone();
    div()
        .id("qa-prio")
        .h(px(34.0))
        .px(px(11.0))
        .rounded(px(9.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .text_size(px(12.0))
        .font_weight(FontWeight::BOLD)
        .bg(color.alpha(0.14))
        .text_color(color)
        .hover(move |h| h.bg(color.alpha(0.24)))
        .child(div().w(px(9.0)).h(px(9.0)).rounded_full().bg(color))
        .child(priority::name(prio))
        .child(
            svg()
                .data(&icons::chevron_down())
                .w(px(12.0))
                .h(px(12.0))
                .text_color(color),
        )
        .on_click(move |_, _w, cx| {
            s.update(cx, |st, cx| {
                st.toggle_menu(QuickMenu::Priority);
                cx.notify();
            });
        })
}

/// 右侧分类触发器（分类圆点 + 名称 + 箭头）
fn category_trigger(
    state: &Entity<AppState>,
    sel_cat: &str,
    cat_color: &str,
    p: &'static DoitPalette,
) -> impl IntoElement {
    let s = state.clone();
    let dot = if sel_cat == "全部" || cat_color.is_empty() {
        None
    } else {
        // 无 # 的历史色归一化后仍可显示；非法色回退默认薄荷绿
        let hex = crate::domain::color::parse_color_or_default(cat_color);
        Some(
            div()
                .w(px(9.0))
                .h(px(9.0))
                .rounded_full()
                .bg(rgb(hex)),
        )
    };
    div()
        .id("qa-cat")
        .h(px(34.0))
        .px(px(11.0))
        .rounded(px(9.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .border_1()
        .border_color(p.line)
        .bg(p.bg)
        .text_size(px(13.0))
        .text_color(p.ink_2)
        .cursor_pointer()
        .hover(move |h| h.border_color(p.mint).text_color(p.mint_deep))
        .when(dot.is_some(), |e| e.child(dot.unwrap()))
        .child(sel_cat.to_string())
        .child(
            svg()
                .data(&icons::chevron_down())
                .w(px(14.0))
                .h(px(14.0))
                .text_color(p.ink_3),
        )
        .on_click(move |_, _w, cx| {
            s.update(cx, |st, cx| {
                st.toggle_menu(QuickMenu::Category);
                cx.notify();
            });
        })
}

/// 渲染当前展开的下拉菜单（根节点绝对定位，浮在主区之上）
pub fn render_menus(state: &Entity<AppState>, cx: &App) -> Option<AnyElement> {
    let open = state.read(cx).open_menu?;
    let p = state.read(cx).theme().palette();
    Some(match open {
        QuickMenu::Priority => priority_menu(state, p, cx).into_any_element(),
        QuickMenu::Category => category_menu(state, p, cx).into_any_element(),
    })
}

/// 优先级下拉菜单
fn priority_menu(
    state: &Entity<AppState>,
    p: &'static DoitPalette,
    cx: &App,
) -> impl IntoElement {
    let cur = state.read(cx).quick_add_priority;
    let mut menu = div()
        .id("qa-prio-menu")
        .absolute()
        .left(px(BAR_INSET))
        .bottom(px(MENU_BOTTOM))
        .w(px(150.0))
        .rounded(px(11.0))
        .border_1()
        .border_color(p.line)
        .bg(p.card)
        .shadow_lg()
        .p(px(6.0))
        .flex()
        .flex_col()
        .gap(px(2.0));

    for (val, name) in &priority::OPTIONS {
        let s = state.clone();
        let v = *val;
        let col = p.priority(v);
        let is_cur = v == cur;
        menu = menu.child(
            div()
                .id(format!("qa-prio-opt-{v}"))
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(11.0))
                .py(px(9.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .text_size(px(13.0))
                .font_weight(if is_cur {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if is_cur { p.mint_deep } else { p.ink_2 })
                .hover(move |h| h.bg(p.mint_soft).text_color(p.mint_deep))
                .child(div().w(px(9.0)).h(px(9.0)).rounded_full().bg(col))
                .child((*name).to_string())
                .on_click(move |_, _w, cx| {
                    s.update(cx, |st, cx| {
                        st.quick_add_priority = v;
                        st.close_menu();
                        cx.notify();
                    });
                }),
        );
    }
    menu
}

/// 分类下拉菜单（含「全部」）
fn category_menu(
    state: &Entity<AppState>,
    p: &'static DoitPalette,
    cx: &App,
) -> impl IntoElement {
    let cats: Vec<crate::domain::types::Category> = state
        .read(cx)
        .snapshot
        .visible_categories()
        .into_iter()
        .cloned()
        .collect();
    let cur = state.read(cx).quick_add_category.clone();

    let mut menu = div()
        .id("qa-cat-menu")
        .absolute()
        .right(px(BAR_INSET))
        .bottom(px(MENU_BOTTOM))
        .w(px(168.0))
        .rounded(px(11.0))
        .border_1()
        .border_color(p.line)
        .bg(p.card)
        .shadow_lg()
        .p(px(6.0))
        .flex()
        .flex_col()
        .gap(px(2.0));

    menu = menu.child(cat_option(state, "全部", None, cur == "全部", p));
    for c in &cats {
        menu = menu.child(cat_option(state, &c.name, Some(c.color.clone()), cur == c.name, p));
    }
    menu
}

/// 分类下拉中的一项
fn cat_option(
    state: &Entity<AppState>,
    name: &str,
    color: Option<String>,
    is_cur: bool,
    p: &'static DoitPalette,
) -> impl IntoElement {
    let s = state.clone();
    let target = name.to_string();
    let dot = color.map(|hex| {
        let h = crate::domain::color::parse_color_or_default(&hex);
        div()
            .w(px(9.0))
            .h(px(9.0))
            .rounded_full()
            .bg(rgb(h))
    });
    div()
        .id(format!("qa-cat-opt-{name}"))
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(11.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .text_size(px(13.0))
        .font_weight(if is_cur {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(if is_cur { p.mint_deep } else { p.ink_2 })
        .hover(move |h| h.bg(p.mint_soft).text_color(p.mint_deep))
        .when(dot.is_some(), |e| e.child(dot.unwrap()))
        .child(name.to_string())
        .on_click(move |_, _w, cx| {
            s.update(cx, |st, cx| {
                st.quick_add_category.clone_from(&target);
                st.close_menu();
                cx.notify();
            });
        })
}
