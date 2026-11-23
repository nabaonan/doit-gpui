//! 月历页（对齐原型：月历面板 + 当日完成列表）
//!
//! - 上/下月按钮翻页、「回到今天」快捷键
//! - 点击任意日期即可切换选中日（修复此前无法点击的问题）
//! - 有已完成任务的日期显示薄荷绿小圆点
//! - 右侧列表跟随选中日期展示当天完成的顶级任务
//!
//! 说明：一度尝试 gpui-kit 自带 `Calendar` 组件，但该组件不向 item 回调暴露
//! 日期值（且 clear_children 后无法加回日期数字），导致整片格子空白、无法
//! 按日标记完成圆点——故恢复此前的手绘实现（稳定且功能完整）。

use crate::app::AppState;
use crate::domain::types::local_date;
use crate::theme::palette::metrics;
use crate::ui::icons;
use chrono::{Datelike, Local, NaiveDate};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashMap;

/// 渲染月历页
pub fn render_calendar(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let snap = &state.read(cx).snapshot;
    let (year, month) = {
        let st = state.read(cx);
        (st.cal_year, st.cal_month)
    };
    let cal_sel = state.read(cx).cal_sel;
    let today = Local::now().date_naive();

    // ---- 每天的完成数量（顶级任务，按完成日期聚合）----
    let mut done_by_day: HashMap<NaiveDate, usize> = HashMap::new();
    for t in &snap.tasks {
        if t.parent_uid.is_none() && t.done && t.deleted_at.is_none() {
            if let Some(c) = t.completed_at {
                *done_by_day.entry(local_date(c)).or_insert(0) += 1;
            }
        }
    }

    // ---- 月历网格计算（与 domain::types::month_grid 共用同一套逻辑）----
    let cells = crate::domain::types::month_grid(year, month);

    let mut grid = div().grid().grid_cols(7).gap(px(6.0)).id("cal-grid");
    // 星期行
    for d in ["日", "一", "二", "三", "四", "五", "六"] {
        grid = grid.child(
            div()
                .text_align(TextAlign::Center)
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.ink_3)
                .child(d),
        );
    }

    // 日期格
    for cell in &cells {
        let Some(date) = *cell else {
            grid = grid.child(div().h(px(42.0)));
            continue;
        };
        let day_num = date.day() as i32;
        let is_today = date == today;
        let is_sel = date == cal_sel;
        let cnt = *done_by_day.get(&date).unwrap_or(&0);
        let has_done = cnt > 0;

        let s = state.clone();
        let picked = date;
        let border = if is_sel || is_today { p.mint } else { p.line };
        grid = grid.child(
            div()
                .id(format!("cal-{}", date.format("%Y-%m-%d")))
                .h(px(42.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(border)
                .bg(if is_sel { p.mint_soft } else { p.card })
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(3.0))
                .cursor_pointer()
                .hover(move |h| h.border_color(p.mint).bg(p.mint_soft))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if is_sel || is_today { p.mint_deep } else { p.ink })
                        .child(day_num.to_string()),
                )
                // 完成标记：固定高度容器，保证所有格子对齐
                .child(
                    div().h(px(7.0)).flex().items_center().when(has_done, |e| {
                        e.child(
                            div()
                                .w(px(6.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(p.mint),
                        )
                    }),
                )
                .on_click(move |_, _w, cx| {
                    s.update(cx, |st, cx| {
                        st.select_calendar_date(picked);
                        cx.notify();
                    });
                }),
        );
    }

    // ---- 面板头部：翻月 + 回到今天 ----
    let prev_s = state.clone();
    let next_s = state.clone();
    let today_s = state.clone();
    let head = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .mb(px(14.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .id("cal-prev")
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
                                .data(&icons::chevron_left())
                                .w(px(16.0))
                                .h(px(16.0))
                                .text_color(p.ink_2),
                        )
                        .on_click(move |_, _w, cx| {
                            prev_s.update(cx, |st, cx| {
                                st.shift_calendar_month(-1);
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .text_size(px(17.0))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("{year}年 {month}月")),
                )
                .child(
                    div()
                        .id("cal-next")
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
                                .data(&icons::chevron_right())
                                .w(px(16.0))
                                .h(px(16.0))
                                .text_color(p.ink_2),
                        )
                        .on_click(move |_, _w, cx| {
                            next_s.update(cx, |st, cx| {
                                st.shift_calendar_month(1);
                                cx.notify();
                            });
                        }),
                ),
        )
        .child(
            div()
                .id("cal-today")
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.line)
                .bg(p.card)
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink_2)
                .cursor_pointer()
                .hover(move |h| h.border_color(p.mint).text_color(p.mint_deep))
                .child("回到今天")
                .on_click(move |_, _w, cx| {
                    today_s.update(cx, |st, cx| {
                        st.calendar_today();
                        cx.notify();
                    });
                }),
        );

    // ---- 选中日的完成列表 ----
    let done_on: Vec<&crate::domain::types::Task> = snap
        .tasks
        .iter()
        .filter(|t| t.parent_uid.is_none() && t.done && t.deleted_at.is_none())
        .filter(|t| t.completed_at.is_some_and(|c| local_date(c) == cal_sel))
        .collect();

    let sel_label = format!("{}月{}日 完成（{}）", cal_sel.month(), cal_sel.day(), done_on.len());

    let mut day_list = div().flex().flex_col().gap(px(8.0));
    if done_on.is_empty() {
        day_list = day_list.child(
            div()
                .w_full()
                .py(px(30.0))
                .text_center()
                .text_size(px(13.0))
                .text_color(p.ink_3)
                .child(format!("{}月{}日 没有完成的任务", cal_sel.month(), cal_sel.day())),
        );
    } else {
        for t in done_on {
            day_list = day_list.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(metrics::RADIUS_SM))
                    .border_1()
                    .border_color(p.line)
                    .bg(p.card)
                    .px(px(12.0))
                    .py(px(9.0))
                    .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(p.mint))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(14.0))
                            .text_color(p.ink_2)
                            .child(t.title.clone()),
                    ),
            );
        }
    }

    div()
        .w_full()
        .p(px(34.0))
        .flex()
        .flex_col()
        .gap(px(18.0))
        .child(div().text_size(px(22.0)).font_weight(FontWeight::BOLD).child("月历"))
        .child(
            div()
                .w_full()
                .flex()
                .gap(px(20.0))
                .child(
                    div()
                        .w(px(360.0))
                        .flex_shrink_0()
                        .rounded(px(metrics::RADIUS))
                        .border_1()
                        .border_color(p.line)
                        .bg(p.card)
                        .p(px(16.0))
                        .child(head)
                        .child(grid),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .rounded(px(metrics::RADIUS))
                        .border_1()
                        .border_color(p.line)
                        .bg(p.card)
                        .p(px(18.0))
                        .child(
                            div()
                                .text_size(px(15.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.ink_2)
                                .mb(px(12.0))
                                .child(sel_label),
                        )
                        .child(day_list),
                ),
        )
}
