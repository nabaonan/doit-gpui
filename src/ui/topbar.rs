//! 顶栏（对齐原型：logo + 导航 Tab + 同步胶囊 + 日期进度胶囊）
//! 铺满窗口宽，左右贴边对齐。

use crate::app::{AppState, Page};
use crate::theme::palette::metrics;
use crate::ui::icons;
use chrono::Datelike;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 渲染顶栏（函数式，返回 element）
pub fn render_topbar(
    state: &Entity<AppState>,
    page: Page,
    cx: &App,
) -> impl IntoElement {
    let theme = state.read(cx).theme();
    let p = theme.palette();
    let sync_status = state.read(cx).sync_status.clone();
    let (status_text, status_warn) = crate::ui::sync_badge(&sync_status);
    let status_icon = crate::ui::sync_icon(&sync_status);
    let is_syncing = sync_status == "syncing";

    // 右侧「日期 + 今日进度」胶囊：与桌面端一致，进度按今日全部顶级任务统计，
    // 不受当前分类筛选影响；点击回到今日页。
    let now = chrono::Local::now();
    let weekday = ["日", "一", "二", "三", "四", "五", "六"]
        [now.weekday().num_days_from_sunday() as usize];
    let date_label = format!("{}月{}日 周{}", now.month(), now.day(), weekday);
    let (today_done, today_total) = {
        let snapshot = &state.read(cx).snapshot;
        let nodes = snapshot.today_tree(None);
        (
            nodes.iter().filter(|n| n.task.done).count(),
            nodes.len(),
        )
    };
    let today_pct = if today_total == 0 {
        0u32
    } else {
        ((today_done as f32 / today_total as f32) * 100.0).round() as u32
    };
    let (ring_hex, track_hex) = theme.ring_colors();
    let ring_svg = icons::progress_ring(9.0, 3.0, today_pct as f32, ring_hex, track_hex);

    // 导航项
    let nav = |id: &'static str, label: &'static str, icon: fn() -> Vec<u8>, nav_page: Page| {
        let active = page == nav_page;
        let s = state.clone();
        div()
            .id(id)
            .h(px(38.0))
            .px(px(15.0))
            .rounded(px(metrics::RADIUS_SM))
            .flex()
            .items_center()
            .gap(px(7.0))
            .cursor_pointer()
            .text_size(px(14.0))
            .font_weight(if active { FontWeight::BOLD } else { FontWeight::MEDIUM })
            .text_color(if active { p.mint_deep } else { p.ink_2 })
            .when(active, |e| e.bg(p.mint_soft))
            .hover(move |h| h.bg(p.mint_soft).text_color(p.mint_deep))
            .child(
                svg()
                    .data(&icon())
                    .w(px(18.0))
                    .h(px(18.0))
                    .text_color(if active { p.mint_deep } else { p.ink_2 }),
            )
            .child(label)
            .on_click(move |_, _window, cx| {
                s.update(cx, |st, cx| {
                    st.page = nav_page;
                    cx.notify();
                });
            })
    };

    div()
        .id("topbar")
        .w_full()
        .h(px(metrics::TOPBAR_H))
        .flex()
        .items_center()
        .bg(p.glass)
        .border_b_1()
        .border_color(p.line)
        .child(
            div()
                .w_full()
                .px(px(34.0))
                .flex()
                .items_center()
                .gap(px(18.0))
                // 品牌
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            div()
                                .w(px(34.0))
                                .h(px(34.0))
                                .rounded(px(11.0))
                                .bg(p.mint)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    svg()
                                        .data(&icons::check())
                                        .w(px(20.0))
                                        .h(px(20.0))
                                        .text_color(rgb(0xFFFFFF)),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::BOLD)
                                .child("doit"),
                        ),
                )
                // 导航
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(nav("nav-today", "今日", icons::today, Page::Today))
                        .child(nav("nav-stats", "统计", icons::stats, Page::Stats))
                        .child(nav("nav-cal", "月历", icons::calendar, Page::Calendar))
                        .child(nav("nav-set", "设置", icons::settings, Page::Settings)),
                )
                // 右侧胶囊：同步状态（点击即同步） + 日期/今日进度（点击回今日）
                .child(
                    div()
                        .ml_auto()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child({
                            let s_sync = state.clone();
                            div()
                                .id("sync-pill")
                                .h(px(38.0))
                                .px(px(13.0))
                                .rounded_full()
                                .border_1()
                                .border_color(p.line)
                                .bg(p.card)
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if status_warn { p.warn } else { p.ink_2 })
                                .cursor_pointer()
                                .hover(|h| h.bg(p.mint_soft))
                                .on_click(move |_, _, cx| AppState::sync_now_entity(&s_sync, cx))
                                .child(
                                    svg()
                                        .data(&status_icon())
                                        .w(px(18.0))
                                        .h(px(18.0))
                                        .text_color(if status_warn { p.warn } else { p.mint }),
                                )
                                .child(status_text)
                                .when(is_syncing, |e| {
                                    e.child(
                                        svg()
                                            .data(&icons::sync())
                                            .w(px(14.0))
                                            .h(px(14.0))
                                            .text_color(p.mint),
                                    )
                                })
                        })
                        .child({
                            let s_today = state.clone();
                            div()
                                .id("date-progress-pill")
                                .h(px(38.0))
                                .px(px(13.0))
                                .rounded_full()
                                .border_1()
                                .border_color(p.line)
                                .bg(p.card)
                                .flex()
                                .items_center()
                                .gap(px(9.0))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.ink_2)
                                .cursor_pointer()
                                .hover(|h| h.bg(p.mint_soft))
                                .on_click(move |_, _, cx| {
                                    s_today.update(cx, |st, cx| {
                                        st.page = Page::Today;
                                        cx.notify();
                                    });
                                })
                                .child(date_label)
                                .child(div().w(px(1.0)).h(px(18.0)).bg(p.line))
                                .child(
                                    div()
                                        .relative()
                                        .w(px(24.0))
                                        .h(px(24.0))
                                        .flex_none()
                                        .child(
                                            svg()
                                                .data(&ring_svg)
                                                .w(px(24.0))
                                                .h(px(24.0)),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left(px(0.0))
                                                .top(px(0.0))
                                                .w(px(24.0))
                                                .h(px(24.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(8.0))
                                                .text_color(p.ink_2)
                                                .child(format!("{today_pct}%")),
                                        ),
                                )
                        }),
                ),
        )
}
