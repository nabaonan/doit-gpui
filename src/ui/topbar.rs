//! 顶栏（对齐原型：logo + 导航 Tab + 同步胶囊 + 日期进度胶囊）
//! 铺满窗口宽，左右贴边对齐。

use crate::app::{AppState, Page};
use crate::theme::palette::metrics;
use crate::ui::icons;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 渲染顶栏（函数式，返回 element）
pub fn render_topbar(
    state: &Entity<AppState>,
    page: Page,
    cx: &App,
) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let sync_status = state.read(cx).sync_status.clone();

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
                // 右侧同步胶囊
                .child(
                    div()
                        .ml_auto()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
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
                                .text_color(if sync_status == "dirty" { p.warn } else { p.ink_2 })
                                .child(
                                    svg()
                                        .data(&icons::cloud())
                                        .w(px(18.0))
                                        .h(px(18.0))
                                        .text_color(if sync_status == "dirty" { p.warn } else { p.mint }),
                                )
                                .child(if sync_status == "dirty" { "有未同步更改" } else { "已同步" }),
                        ),
                ),
        )
}
