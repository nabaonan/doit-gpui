//! 今日页（对齐原型：分类筛选条 + 独立任务卡片列表 + 底部快速添加栏）
//!
//! 任务以「独立卡片」形式同级堆叠：父任务一张卡、子任务各自一张卡，
//! 通过逐级缩进表达层级关系（每级缩进量相等）。
//!
//! 分类筛选条（导航）由 `render_cat_nav` 单独渲染，放在滚动容器之外
//! （顶部固定不随任务列表滚动）。

use crate::app::AppState;
use crate::ui::icons;
use crate::ui::task_card::{flatten, render_card, render_sub_input_card};
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashSet;

/// 分类胶囊的最小宽度（保证文字水平居中）
const CHIP_MIN_W: f32 = 84.0;

/// 各分类下「未完成顶级任务」的计数（自有父；未删除；未完成）
fn top_unfinished_by_cat(state: &Entity<AppState>, cx: &App) -> std::collections::HashMap<String, usize> {
    let mut map = std::collections::HashMap::new();
    for t in &state.read(cx).snapshot.tasks {
        if t.parent_uid.is_none()
            && !t.done
            && t.deleted_at.is_none()
            && t.category != "默认"
        {
            *map.entry(t.category.clone()).or_insert(0) += 1;
        }
    }
    map
}

/// 渲染「固定」的分类导航（放在滚动容器之外，滚动时不跟随）
pub fn render_cat_nav(state: &Entity<AppState>, filter: &str, cx: &App) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let categories = state.read(cx).snapshot.categories.clone();
    let unfinished = top_unfinished_by_cat(state, cx);

    // ---------------- 分类胶囊导航 ----------------
    let mut chips = div().flex().items_center().gap(px(8.0)).id("filter-chips");

    // 「全部」—— 使用薄荷绿高亮
    {
        let s = state.clone();
        let active = filter == "all";
        chips = chips.child(
            div()
                .id("chip-all")
                .min_w(px(CHIP_MIN_W))
                .h(px(32.0))
                .px(px(12.0))
                .flex()
                .justify_center()
                .items_center()
                .rounded_full()
                .border_1()
                .border_color(if active { p.mint } else { p.line })
                .bg(if active { p.mint } else { p.card })
                .text_color(if active { rgb(0xFFFFFF) } else { p.ink_2 })
                .text_size(px(13.0))
                .font_weight(if active { FontWeight::BOLD } else { FontWeight::MEDIUM })
                .cursor_pointer()
                .hover(move |h| {
                    let h = h.border_color(p.mint);
                    if active {
                        h
                    } else {
                        h.text_color(p.mint_deep)
                    }
                })
                .child("全部")
                .on_click(move |_, _w, cx| {
                    s.update(cx, |st, cx| {
                        st.select_filter("all");
                        cx.notify();
                    });
                }),
        );
    }

    // 各分类 —— 选中时使用该分类自身的颜色高亮；名称右侧显示未完成顶级任务数；
    // 右键弹出「编辑 / 删除」菜单
    for c in &categories {
        let s = state.clone();
        let name = c.name.clone();
        let active = filter == c.name;
        let hex = crate::domain::color::parse_color_or_default(&c.color);
        let accent = rgb(hex);
        let count = unfinished.get(&name).copied().unwrap_or(0);

        // 分类右键菜单：编辑 / 删除
        let cat_menu = {
            let s_m = s.clone();
            let n_m = name.clone();
            move |menu: PopupMenu, w: &mut Window, cx: &mut Context<PopupMenu>| {
                let s2 = s_m.clone();
                let n2 = n_m.clone();
                let mut m = menu.item(
                    PopupMenuItem::new("编辑分类")
                        .on_click(move |_, w, cx| {
                            crate::ui::cat_dialog::open_edit(&s2, &n2, w, cx);
                        }),
                );
                let s3 = s_m.clone();
                let n3 = n_m.clone();
                m = m.item(
                    PopupMenuItem::new("删除分类")
                        .icon(IconName::Trash)
                        .on_click(move |_, w, cx| {
                            crate::ui::cat_dialog::open_confirm_delete(&s3, &n3, w, cx);
                        }),
                );
                let _ = (w, cx);
                m
            }
        };

        chips = chips.child(
            div()
                .id(format!("chip-{}", c.name))
                .min_w(px(CHIP_MIN_W))
                .h(px(32.0))
                .px(px(12.0))
                .flex()
                .justify_center()
                .items_center()
                .gap(px(6.0))
                .rounded_full()
                .border_1()
                .border_color(if active { accent } else { p.line })
                .bg(if active { accent } else { p.card })
                .text_color(if active { rgb(0xFFFFFF) } else { p.ink_2 })
                .text_size(px(13.0))
                .font_weight(if active { FontWeight::BOLD } else { FontWeight::MEDIUM })
                .cursor_pointer()
                .hover(move |h| {
                    let h = h.border_color(accent);
                    if active {
                        h
                    } else {
                        h.text_color(p.mint_deep)
                    }
                })
                .when(!active, |e| e.child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(accent)))
                .child(name.clone())
                // 未完成顶级任务计数徽标
                .child(
                    div()
                        .min_w(px(18.0))
                        .h(px(18.0))
                        .px(px(5.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::BOLD)
                        .bg(if active { rgb(0xFFFFFF).alpha(0.22) } else { accent.alpha(0.14) })
                        .text_color(if active { rgb(0xFFFFFF) } else { accent })
                        .child(count.to_string()),
                )
                .on_click(move |_, _w, cx| {
                    s.update(cx, |st, cx| {
                        st.select_filter(&name);
                        cx.notify();
                    });
                })
                .context_menu(cat_menu),
        );
    }

    // 添加分类按钮（虚线胶囊 + 加号）
    {
        let s = state.clone();
        chips = chips.child(
            div()
                .id("chip-add-cat")
                .h(px(32.0))
                .px(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(p.line)
                .bg(p.card)
                .text_color(p.ink_2)
                .text_size(px(13.0))
                .cursor_pointer()
                .hover(move |h| h.border_color(p.mint).text_color(p.mint_deep))
                .child(
                    svg()
                        .data(&icons::plus())
                        .w(px(14.0))
                        .h(px(14.0)),
                )
                .child("添加分类")
                .on_click(move |_, w, cx| {
                    crate::ui::cat_dialog::open_add(&s, w, cx);
                    s.update(cx, |st, cx| {
                        st.filter = "all".into();
                        cx.notify();
                    });
                }),
        );
    }

    div()
        .w_full()
        .px(px(34.0))
        .py(px(12.0))
        .child(chips)
}

/// 渲染今日页的任务列表（函数式；分类导航由 `render_cat_nav` 单独渲染）
pub fn render_today(
    state: &Entity<AppState>,
    filter: &str,
    _window: &mut Window,
    cx: &App,
) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let collapsed: HashSet<String> = state.read(cx).collapsed.clone();
    let nodes = state.read(cx).snapshot.today_tree(Some(filter));
    let show_category = filter == "all";

    // ---------------- 任务列表（父/子各自独立卡片） ----------------
    let mut flat: Vec<(&crate::domain::types::TaskNode, u32)> = Vec::new();
    flatten(&nodes, 0, &collapsed, &mut flat);

    // 当前进行中「新增子任务」的父任务（若有）：在其卡片之后插入一张
    // 独立的子任务输入卡片，而非把输入框嵌进父卡片内部。
    let new_sub_parent: Option<(String, u32)> = state.read(cx).inline_edit.as_ref().and_then(
        |(uid, mode)| {
            if *mode == crate::app::InlineMode::NewSub {
                flat.iter()
                    .find(|(n, _)| n.task.uid == *uid)
                    .map(|(_, d)| (uid.clone(), *d))
            } else {
                None
            }
        },
    );

    let mut list = div().w_full().flex().flex_col().gap(px(12.0)).id("task-list");
    if flat.is_empty() {
        list = list.child(
            div()
                .w_full()
                .py(px(48.0))
                .text_center()
                .text_color(p.ink_3)
                .text_size(px(14.0))
                .child("这个分类今天没有待办，歇口气～"),
        );
    } else {
        for (node, depth) in &flat {
            list = list.child(render_card(node, *depth, state, show_category, cx));
            // 父任务处于「新增子任务」编辑态：紧随其后插入独立子任务输入卡片
            if let Some((puid, pdepth)) = &new_sub_parent {
                if &node.task.uid == puid {
                    list = list.child(render_sub_input_card(puid, *pdepth, state, cx));
                }
            }
        }
    }

    div()
        .w_full()
        .px(px(34.0))
        .pb(px(16.0))
        .child(list)
}
