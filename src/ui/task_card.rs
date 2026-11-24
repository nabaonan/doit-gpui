//! 任务卡片（独立卡片展示，对齐原型 TaskNode）
//!
//! 关键设计：**父任务与子任务各自是一张独立卡片**，同级堆叠、逐级缩进，
//! 子任务不再嵌套在父卡片内部（与原型 `.task` / `.subtasks` 的兄弟结构一致）。
//!
//! 勾选控件三态：
//! - 已完成 → 实心薄荷绿圆 + 白色对钩
//! - 含子任务（未完成） → 只读虚线圈（完成态由子任务推导，不可直接点）
//! - 普通未完成 → 空心圆，可点击完成

use crate::app::AppState;
use crate::domain::types::TaskNode;
use crate::theme::palette::metrics;
use crate::ui::icons;
use gpui_kit::component::menu::ContextMenuExt;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 渲染单张任务卡片（不含子任务，子任务由调用方以独立卡片渲染）
///
/// # Panics
/// 在状态不可读等极端情况下可能 panic（当前视图生命周期内不会发生）。
pub fn render_card(
    node: &TaskNode,
    depth: u32,
    state: &Entity<AppState>,
    show_category: bool,
    cx: &App,
) -> AnyElement {
    let p = state.read(cx).theme().palette();
    let t = &node.task;
    let has_subs = node.has_live_subs();
    let is_sub = depth > 0;

    // ---- 勾选控件 ----
    let check: AnyElement = if t.done {
        // 已完成：实心圆 + 白色对钩（颜色直接加在 svg 上，GPUI 不继承父级 currentColor）
        div()
            .id(format!("chk-{}", t.uid))
            .w(px(metrics::CHECK_SIZE))
            .h(px(metrics::CHECK_SIZE))
            .rounded_full()
            .bg(p.mint)
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .data(&icons::check())
                    .w(px(15.0))
                    .h(px(15.0))
                    .text_color(rgb(0xFFFFFF)),
            )
            .into_any_element()
    } else if has_subs {
        // 含子任务：只读虚线圈
        div()
            .id(format!("chk-{}", t.uid))
            .w(px(metrics::CHECK_SIZE))
            .h(px(metrics::CHECK_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .data(&icons::dash_ring())
                    .w(px(23.0))
                    .h(px(23.0))
                    .text_color(p.ink_3),
            )
            .into_any_element()
    } else {
        // 普通未完成：空心圆，可点击
        let s = state.clone();
        let uid = t.uid.clone();
        div()
            .id(format!("chk-{uid}"))
            .w(px(metrics::CHECK_SIZE))
            .h(px(metrics::CHECK_SIZE))
            .rounded_full()
            .border_2()
            .border_color(p.ink_3)
            .bg(p.card)
            .cursor_pointer()
            .hover(move |h| h.border_color(p.mint))
            .on_click(move |_, _w, cx| {
                // 独立按钮：阻止冒泡，避免触发卡片本体的折叠/展开（点击父任务切换）
                cx.stop_propagation();
                s.update(cx, |st, cx| {
                    if st.snapshot.toggle_done(&uid) {
                        st.save();
                        st.notify_msg("已完成");
                    }
                    cx.notify();
                });
            })
            .into_any_element()
    };

    // ---- 标题 ----
    // 内联编辑（改标题）时用输入框替代标题文字
    let is_editing = state
        .read(cx)
        .inline_edit
        .as_ref()
        .is_some_and(|(uid, mode)| uid == &t.uid && *mode == crate::app::InlineMode::Rename);

    let title: AnyElement = if is_editing {
        let input = state.read(cx).inline_input.clone();
        // 内联编辑输入框：点击输入区域不触发卡片折叠切换
        div()
            .id(format!("edit-{}", t.uid))
            .flex_1()
            .min_w_0()
            .on_click(|_, _w, cx| cx.stop_propagation())
            .child(gpui_kit::component::input::Input::new(&input).w_full().h(px(32.0)))
            .into_any_element()
    } else {
        div()
            .flex_1()
            .min_w_0()
            .text_size(if is_sub { px(15.5) } else { px(16.0) })
            .font_weight(if is_sub { FontWeight::MEDIUM } else { FontWeight::SEMIBOLD })
            .text_color(if t.done { p.ink_3 } else { p.ink })
            .child(t.title.clone())
            .into_any_element()
    };

    // ---- 右侧操作区 ----
    let mut side = div().flex().items_center().gap(px(8.0)).id(format!("side-{}", t.uid));

    // 分类标签（仅顶级任务且在「全部」视图下显示，与原型一致）
    if !is_sub && show_category {
        let cat_hex = state
            .read(cx)
            .snapshot
            .categories
            .iter()
            .find(|c| c.name == t.category)
            .map_or(crate::domain::color::DEFAULT_CATEGORY_COLOR_HEX, |c| {
                crate::domain::color::parse_color_or_default(&c.color)
            });
        side = side.child(
            div()
                .px(px(9.0))
                .py(px(2.0))
                .rounded(px(7.0))
                .bg(p.mint_soft)
                .flex()
                .items_center()
                .gap(px(5.0))
                .text_color(p.ink_2)
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(rgb(cat_hex)))
                .child(t.category.clone()),
        );
    }

    // 优先级标签
    if t.priority > 0 {
        let col = p.priority(t.priority);
        side = side.child(
            div()
                .px(px(9.0))
                .py(px(2.0))
                .rounded(px(7.0))
                .bg(col.alpha(0.14))
                .text_color(col)
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .child(crate::theme::palette::priority::name(t.priority)),
        );
    }

    // 子任务完成进度环
    if let Some(pct) = node.sub_progress() {
        let (ring_hex, track_hex) = state.read(cx).theme().ring_colors();
        let ring_svg = icons::progress_ring(8.0, 3.0, pct as f32, ring_hex, track_hex);
        side = side.child(
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(svg().data(&ring_svg).w(px(20.0)).h(px(20.0)))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.ink_3)
                        .child(format!("{pct}%")),
                ),
        );
    }

    // 展开 / 收起按钮
    if has_subs {
        let s = state.clone();
        let uid = t.uid.clone();
        // 收起时箭头向右（表示可展开），展开时箭头向下
        let is_collapsed = state.read(cx).collapsed.contains(&t.uid);
        let chevron: Vec<u8> = if is_collapsed {
            icons::chevron_right()
        } else {
            icons::chevron_down()
        };
        side = side.child(
            div()
                .id(format!("exp-{}", t.uid))
                .w(px(28.0))
                .h(px(28.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.line)
                .bg(p.card)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(move |h| h.border_color(p.mint))
                .child(
                    svg()
                        .data(&chevron)
                        .w(px(16.0))
                        .h(px(16.0))
                        .text_color(p.ink_2),
                )
                .on_click(move |_, _w, cx| {
                    // 独立展开/收起按钮：阻止冒泡，避免触发卡片本体的折叠切换
                    cx.stop_propagation();
                    s.update(cx, |st, cx| {
                        if st.collapsed.contains(&uid) {
                            st.collapsed.remove(&uid);
                        } else {
                            st.collapsed.insert(uid.clone());
                        }
                        cx.notify();
                    });
                }),
        );
    }

    let row = div()
        .flex()
        .items_center()
        .gap(px(13.0))
        .child(check)
        .child(title)
        .child(side);

    // ---- 独立卡片本体（缩进用外层 padding 实现，避免 width:100% 溢出）----
    // 右键菜单用 gpui-component 官方 ContextMenu 组件（自带右键捕获/定位/关闭），
    // 卡片 id 需稳定唯一（task-<uid>），组件据此维护每个任务的菜单状态。
    let card = div()
        .id(format!("task-{}", t.uid))
        .w_full()
        .rounded(px(metrics::RADIUS))
        .border_1()
        .border_color(p.line)
        .bg(if is_sub { p.sub_card } else { p.card })
        .shadow_sm()
        .px(px(16.0))
        .py(px(10.0))
        // 有子任务时：点击父任务卡片本体（非勾选/按钮/输入区域）切换展开/收起
        .when(has_subs, |e| {
            let s = state.clone();
            let uid = t.uid.clone();
            e.on_click(move |_, _w, cx| {
                s.update(cx, |st, cx| {
                    if st.collapsed.contains(&uid) {
                        st.collapsed.remove(&uid);
                    } else {
                        st.collapsed.insert(uid.clone());
                    }
                    cx.notify();
                });
            })
        })
        .context_menu({
            let state = state.clone();
            let uid = t.uid.clone();
            let is_done = t.done;
            let p_now = t.priority;
            let cat_now = t.category.clone();
            let cats: Vec<crate::ui::ctx_menu::CategoryRef> = state
                .read(cx)
                .snapshot
                .categories
                .iter()
                .map(|c| crate::ui::ctx_menu::CategoryRef {
                    name: c.name.clone(),
                    color: c.color.clone(),
                })
                .collect();
            move |menu, w, cx| {
                crate::ui::ctx_menu::build_menu_for(
                    menu,
                    w,
                    cx,
                    &uid,
                    is_done,
                    is_sub,
                    p_now,
                    &cat_now,
                    &cats,
                    &state,
                )
            }
        })
        .child(row);

    div()
        .w_full()
        .pl(px(depth as f32 * metrics::SUB_INDENT))
        .child(card)
        .into_any_element()
}

/// 渲染「新增子任务」输入卡片（独立一张、子任务样式、缩进一级）。
///
/// 用于右键菜单「子任务」：不在父卡片内部嵌套输入框，而是在父任务
/// 卡片之后新建一张独立子任务卡片承载输入，回车后成为真正的子任务。
///
/// 卡片上提供优先级选择（点击循环切换，带颜色圆点与名称）；不提供分类
/// 选择——子任务分类永远跟随父任务。
pub fn render_sub_input_card(
    parent_uid: &str,
    parent_depth: u32,
    state: &Entity<AppState>,
    cx: &App,
) -> AnyElement {
    let p = state.read(cx).theme().palette();
    let input = state.read(cx).inline_input.clone();
    let prio = state.read(cx).new_sub_priority;
    let prio_color = p.priority(prio);
    let open = state.read(cx).new_sub_prio_open;
    let s = state.clone();

    // 优先级胶囊（点击展开绝对浮动下拉，可选档位）
    // 说明：不用 Button+DropdownMenu（本版 gpui 中该组合 render 时会重复设置
    // hover style 触发断言崩溃）；改用胶囊 + 绝对定位浮动 PopupMenu（不占布局）。
    let s_pill = s.clone();
    let prio_pill = div()
        .id(format!("newsub-prio-{parent_uid}"))
        .h(px(32.0))
        .px(px(10.0))
        .rounded(px(9.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .bg(prio_color.alpha(0.14))
        .hover(move |h| h.bg(prio_color.alpha(0.24)))
        .on_click(move |_, _w, cx| {
            s_pill.update(cx, |st, cx| {
                st.new_sub_prio_open = !st.new_sub_prio_open;
                cx.notify();
            });
        })
        .child(div().w(px(9.0)).h(px(9.0)).rounded_full().bg(prio_color))
        .child(crate::theme::palette::priority::name(prio))
        .child(
            svg()
                .data(&icons::chevron_down())
                .w(px(12.0))
                .h(px(12.0))
                .text_color(prio_color),
        );

    // 绝对浮动的优先级下拉（展开时覆盖在下方内容之上，不把子任务顶下去）
    let mut dropdown = div()
        .w(px(152.0))
        .rounded(px(11.0))
        .border_1()
        .border_color(p.line)
        .bg(p.card)
        .shadow_lg()
        .p(px(6.0))
        .flex()
        .flex_col()
        .gap(px(2.0));
    for (val, name) in &crate::theme::palette::priority::OPTIONS {
        let s2 = s.clone();
        let v = *val;
        let col = p.priority(v);
        let is_cur = v == prio;
        dropdown = dropdown.child(
            div()
                .id(format!("newsub-prio-opt-{v}"))
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(11.0))
                .py(px(8.0))
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
                    s2.update(cx, |st, cx| {
                        st.new_sub_priority = v;
                        st.new_sub_prio_open = false;
                        cx.notify();
                    });
                }),
        );
    }

    let card = div()
        .id(format!("task-newsub-{parent_uid}"))
        .w_full()
        .rounded(px(metrics::RADIUS))
        .border_1()
        .border_dashed()
        .border_color(p.mint)
        .bg(p.sub_card)
        .px(px(16.0))
        .py(px(9.0))
        .flex()
        .items_center()
        .gap(px(13.0))
        .child(prio_pill)
        .child(gpui_kit::component::input::Input::new(&input).w_full().h(px(32.0)));

    // 外层相对容器：下拉为绝对定位浮层（覆盖在下方内容之上，不把子任务顶下去）
    let mut wrap = div()
        .w_full()
        .relative()
        .pl(px((parent_depth + 1) as f32 * metrics::SUB_INDENT))
        .child(card);
    if open {
        wrap = wrap.child(
            div()
                .id(format!("newsub-prio-dd-{parent_uid}"))
                .absolute()
                .left(px(16.0))
                .top(px(50.0))
                .child(dropdown),
        );
    }
    wrap.into_any_element()
}

/// 把任务树摊平成「卡片 + 层级」列表：父任务与子任务各自一张独立卡片，
/// 被折叠的节点跳过其整棵子树。
pub fn flatten<'a, S: std::hash::BuildHasher>(
    nodes: &'a [TaskNode],
    depth: u32,
    collapsed: &std::collections::HashSet<String, S>,
    out: &mut Vec<(&'a TaskNode, u32)>,
) {
    for node in nodes {
        if node.task.deleted_at.is_some() {
            continue;
        }
        out.push((node, depth));
        if !collapsed.contains(&node.task.uid) {
            flatten(&node.subs, depth + 1, collapsed, out);
        }
    }
}

#[cfg(test)]
mod tests {
    // 注意：不要 `use super::*`，否则会带入 gpui 的 `test` 宏，与内置 #[test] 冲突
    use super::flatten;
    use crate::domain::types::{Task, TaskNode};

    fn mk(uid: &str, parent: Option<&str>) -> Task {
        let now = crate::domain::types::now_millis();
        Task {
            uid: uid.into(),
            parent_uid: parent.map(Into::into),
            title: uid.into(),
            category: "工作".into(),
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

    /// p → (c1 → (g1), c2)
    fn tree() -> Vec<TaskNode> {
        let g1 = TaskNode { task: mk("g1", Some("c1")), subs: vec![] };
        let c1 = TaskNode { task: mk("c1", Some("p")), subs: vec![g1] };
        let c2 = TaskNode { task: mk("c2", Some("p")), subs: vec![] };
        vec![TaskNode { task: mk("p", None), subs: vec![c1, c2] }]
    }

    #[test]
    fn flatten_emits_parent_and_children_as_separate_cards() {
        let nodes = tree();
        let mut out = Vec::new();
        flatten(&nodes, 0, &std::collections::HashSet::new(), &mut out);
        let ids: Vec<String> = out.iter().map(|(n, _)| n.task.uid.clone()).collect();
        let depths: Vec<u32> = out.iter().map(|(_, d)| *d).collect();
        assert_eq!(ids, vec!["p", "c1", "g1", "c2"]);
        assert_eq!(depths, vec![0, 1, 2, 1], "每级子任务缩进层级应递增 1");
    }

    #[test]
    fn flatten_skips_collapsed_subtree() {
        let nodes = tree();
        let mut collapsed = std::collections::HashSet::new();
        collapsed.insert("c1".to_string());
        let mut out = Vec::new();
        flatten(&nodes, 0, &collapsed, &mut out);
        let ids: Vec<String> = out.iter().map(|(n, _)| n.task.uid.clone()).collect();
        assert_eq!(
            ids,
            vec!["p", "c1", "c2"],
            "被折叠的节点保留自身卡片（便于再次展开），但跳过其子树 g1"
        );
    }
}
