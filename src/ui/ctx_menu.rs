//! 右键菜单（参考原型 #ctx 上下文菜单设计）
//!
//! 采用 gpui-component 官方 `ContextMenu` / `PopupMenu` 组件实现：
//! - 右键捕获、菜单位置锚定、窗口边距钳制、点击外部关闭均由组件自动处理，
//!   避免手写 hit-test / 遮罩引发的「右键无效」问题。
//! - 每个任务卡片一个稳定 ElementId（`task-<uid>`），组件据此维护独立菜单状态。
//!
//! 菜单项（未完成任务）：
//! - 编辑（原地改标题，进入内联编辑）
//! - 子任务（内联新增子任务）
//! - 优先级 ▸（子菜单：紧急/高/普通/低/不设，当前项 checked 高亮）
//! - 改分类 ▸（子菜单：各分类，当前项 checked 高亮；仅顶级任务显示）
//! - 删除（危险项）
//!
//! 已完成任务仅提供「删除」（与原型一致）。

use crate::app::AppState;
use crate::theme::palette::priority;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 为指定任务组装右键菜单（`.context_menu(...)` 的 builder 回调函数体）。
///
/// 快照参数来自渲染时读取 state，避免回调中再借 read；数据变更通过闭包
/// 捕获的 `Entity<AppState>` 完成。
///
/// 参数较多是出于「渲染时快照 + 菜单闭包 move 捕获」的需要，
/// 将菜单项所需信息一次性传入，避免在回调内再次读取状态。
#[allow(clippy::too_many_arguments)]
pub fn build_menu_for(
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
    uid: &str,
    is_done: bool,
    is_sub: bool,
    priority_now: u8,
    category_now: &str,
    categories: &[CategoryRef],
    state: &Entity<AppState>,
) -> PopupMenu {
    // 已完成：仅删除
    if is_done {
        return menu.item(delete_item(uid, state));
    }
    let mut m = menu;
    m = m.item(edit_item(uid, state));
    m = m.item(subtask_item(uid, state));
    m = m.submenu_with_icon(
        Some(IconName::Flag.into()),
        "优先级",
        window,
        cx,
        priority_submenu(uid, priority_now, state),
    );
    if !is_sub {
        m = m.submenu_with_icon(
            Some(IconName::Folder.into()),
            "改分类",
            window,
            cx,
            category_submenu(uid, category_now, categories, state),
        );
    }
    m = m.separator();
    m.item(delete_item(uid, state))
}

/// 「编辑」：进入内联改标题
fn edit_item(uid: &str, state: &Entity<AppState>) -> PopupMenuItem {
    let s = state.clone();
    let uid = uid.to_string();
    PopupMenuItem::new("编辑")
        .icon(IconName::Pencil)
        .on_click(move |_, window, cx| {
            s.update(cx, |st, cx| {
                st.ctx_edit(uid.clone(), window, cx);
                cx.notify();
            });
        })
}

/// 「子任务」：进入内联新增子任务
fn subtask_item(uid: &str, state: &Entity<AppState>) -> PopupMenuItem {
    let s = state.clone();
    let uid = uid.to_string();
    PopupMenuItem::new("子任务")
        .icon(IconName::ListPlus)
        .on_click(move |_, window, cx| {
            s.update(cx, |st, cx| {
                st.ctx_add_sub(uid.clone(), window, cx);
                cx.notify();
            });
        })
}

/// 「优先级」子菜单构建器：紧急/高/普通/低/不设，当前项 checked；
/// 每项前置对应优先级颜色的圆点。
fn priority_submenu(
    uid: &str,
    now: u8,
    state: &Entity<AppState>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let s = state.clone();
    let uid = uid.to_string();
    move |menu, _w, cx| {
        let p = state_palette(&s, cx);
        let mut m = menu;
        for (val, name) in &priority::OPTIONS {
            let s2 = s.clone();
            let uid2 = uid.clone();
            let v = *val;
            let is_cur = v == now;
            let color = p.priority(v);
            let label = (*name).to_string();
            m = m.item(
                PopupMenuItem::element(move |_w, _cx| dot_row(color, label.clone(), is_cur, p))
                    .checked(is_cur)
                    .on_click(move |_, _w, cx| {
                        s2.update(cx, |st, cx| {
                            st.ctx_set_priority(uid2.clone(), v);
                            cx.notify();
                        });
                    }),
            );
        }
        m
    }
}

/// "彩色圆点 + 名称" 一行（右键菜单子菜单项；当前项用薄荷深绿加粗）
///
/// 注意：不用 `svg().data()` 画圆点——GPUI 的 SVG 是单色渲染（只取 alpha
/// 遮罩、用 text_color 填充，会忽略 fill 色），彩色圆点必须用 div 底画。
fn dot_row(
    color: gpui_kit::Rgba,
    label: String,
    current: bool,
    p: &'static crate::theme::palette::DoitPalette,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(9.0))
        .w_full()
        .min_h(px(22.0))
        .child(div().w(px(10.0)).h(px(10.0)).rounded_full().bg(color))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(if current { FontWeight::BOLD } else { FontWeight::NORMAL })
                .text_color(if current { p.mint_deep } else { p.ink_2 })
                .child(label),
        )
        .into_any_element()
}

/// 从 AppState 读取当前主题色板（菜单构建时调用）
fn state_palette(state: &Entity<AppState>, cx: &App) -> &'static crate::theme::palette::DoitPalette {
    state.read(cx).theme().palette()
}

/// 「改分类」子菜单构建器：各分类（名+色hex），当前项 checked（仅顶级任务显示）
fn category_submenu(
    uid: &str,
    now: &str,
    categories: &[CategoryRef],
    state: &Entity<AppState>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let s = state.clone();
    let uid = uid.to_string();
    let now_owned = now.to_string();
    let cats: Vec<CategoryRef> = categories.to_vec();
    move |menu, _w, cx| {
        let p = state_palette(&s, cx);
        let mut m = menu;
        for c in &cats {
            let s2 = s.clone();
            let uid2 = uid.clone();
            let n = c.name.clone();
            let is_cur = n == now_owned;
            let color = u32::from_str_radix(c.color.trim_start_matches('#'), 16)
                .map_or(gpui_kit::rgb(0x3FBB9C), gpui_kit::rgb);
            let label = n.clone();
            m = m.item(
                PopupMenuItem::element(move |_w, _cx| dot_row(color, label.clone(), is_cur, p))
                    .checked(is_cur)
                    .on_click(move |_, _w, cx| {
                        s2.update(cx, |st, cx| {
                            st.ctx_set_category(uid2.clone(), n.clone());
                            cx.notify();
                        });
                    }),
            );
        }
        m
    }
}

/// 分类（名 + 颜色）引用，用于改分类子菜单
#[derive(Clone)]
pub struct CategoryRef {
    pub name: String,
    pub color: String,
}

/// 「删除」
fn delete_item(uid: &str, state: &Entity<AppState>) -> PopupMenuItem {
    let s = state.clone();
    let uid = uid.to_string();
    PopupMenuItem::new("删除")
        .icon(IconName::Trash)
        .on_click(move |_, _w, cx| {
            s.update(cx, |st, cx| {
                st.ctx_delete(uid.clone());
                cx.notify();
            });
        })
}
