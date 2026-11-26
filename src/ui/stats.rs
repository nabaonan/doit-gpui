//! 统计页（对齐原型：区间切换 / 完成热力图 / 分类占比环图 / 完成趋势折线图）
//!
//! 顶部为「近一周 / 近一月 / 近一年」区间切换，区间决定三张图的数据口径：
//! - 近一周：当前所在自然周（周一 → 周日）
//! - 近一月：当前所在自然月（按周对齐，格子恒为正方形）
//! - 近一年：当前自然年（GitHub 式 53 周 × 7 天）
//!
//! 热力图与环形图按原型设计稿自绘：热力图为方格矩阵（附周几表头与「少…多」图例），
//! 环形图为 SVG 扇区环 + 右侧分类图例。

use crate::app::{AppState, StatsRange};
use crate::domain::types::{
    days_between, days_in_month, local_date, start_of_week, weekday_mon0,
};
use crate::theme::palette::{DoitPalette, metrics};
use gpui_kit::component::chart::{LineChart, PieChart};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashMap;

/// 分类占比切片
#[derive(Clone)]
struct CatSlice {
    name: String,
    count: f32,
    hex: String,
}

/// 趋势点
#[derive(Clone)]
struct TrendPoint {
    label: String,
    count: f64,
}

/// 统计区间（起止日均含）
#[derive(Clone, Copy)]
struct Range {
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
}

/// 热力图的一行（一周 = 7 格）
struct HeatRow {
    cells: Vec<HeatCell>,
}

/// 热力图格子
#[derive(Clone)]
struct HeatCell {
    level: usize,
    /// 是否落在统计区间内（区间外渲染为透明）
    in_range: bool,
    key: String,
    /// 当日完成的顶级任务数（hover 气泡用）
    count: usize,
    /// 对应日期（区间外为 None），气泡展示用
    date: Option<chrono::NaiveDate>,
}

/// 渲染统计页
pub fn render_stats(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let selected = state.read(cx).stats_range;
    let today = chrono::Local::now().date_naive();
    let range = range_of(selected, today);
    let counts = done_counts(state, cx);

    // 分类占比（区间内完成的顶级任务）
    let entries = category_entries(state, range, cx);

    // 完成趋势
    let trend = trend_points(selected, range, &counts);

    let (trend_title, _heat_title) = match selected {
        StatsRange::Year => ("每月完成趋势", "完成热力图（近一年）"),
        _ => ("每日完成趋势", "完成热力图"),
    };

    div()
        .w_full()
        .max_w(px(1080.0))
        .mx_auto()
        .p(px(34.0))
        .flex()
        .flex_col()
        .gap(px(18.0))
        // 区间切换
        .child(range_tabs(state, selected, p))
        // 上排：热力图 + 分类占比
        // 年视图 53 列横向较宽——热力图占满整行（格子才能放大为正方形），
        // 分类占比环移到其下方；周/月视图保持两列并排。
        .child({
            let is_year = selected == StatsRange::Year;
            div()
                .w_full()
                .grid()
                .grid_cols(if is_year { 1u16 } else { 2u16 })
                .gap(px(18.0))
                .child(heatmap_card(selected, today, &counts, p))
                .child(ring_card(&entries, p))
        })
        // 下排：完成趋势（通栏）
        .child(stat_card(
            p,
            trend_title,
            "悬停或点击数据点查看完成了几项任务",
            div().h(px(220.0)).w_full().child(
                LineChart::new(trend)
                    .id("trend-line")
                    .x(|d| d.label.clone())
                    .y(|d| d.count)
                    .stroke(p.mint)
                    .dot(),
            ),
        ))
}

/// 区间切换标签（近一周 / 近一月 / 近一年）
fn range_tabs(
    state: &Entity<AppState>,
    selected: StatsRange,
    p: &'static DoitPalette,
) -> impl IntoElement {
    let mut tabs = div().id("stats-range-tabs").flex().items_center().gap(px(7.0));
    for (r, label) in [
        (StatsRange::Week, "近一周"),
        (StatsRange::Month, "近一月"),
        (StatsRange::Year, "近一年"),
    ] {
        let s = state.clone();
        let active = r == selected;
        tabs = tabs.child(
            div()
                .id(format!("range-tab-{label}"))
                .px(px(18.0))
                .py(px(8.0))
                .rounded(px(11.0))
                .border_1()
                .border_color(if active { p.mint } else { p.line })
                .bg(if active { p.mint } else { p.card })
                .text_color(if active { rgb(0xFFFFFF) } else { p.ink_2 })
                .text_size(px(13.0))
                .font_weight(if active {
                    FontWeight::BOLD
                } else {
                    FontWeight::MEDIUM
                })
                .cursor_pointer()
                .hover(move |h| h.border_color(p.mint))
                .child(label)
                .on_click(move |_, _w, cx| {
                    s.update(cx, |st, cx| {
                        st.stats_range = r;
                        cx.notify();
                    });
                }),
        );
    }
    tabs
}

/// 分区卡片
fn stat_card(
    p: &'static DoitPalette,
    title: &str,
    hint: &str,
    content: impl IntoElement,
) -> impl IntoElement {
    div()
        .w_full()
        .rounded(px(metrics::RADIUS))
        .border_1()
        .border_color(p.line)
        .bg(p.card)
        .shadow_sm()
        .p(px(20.0))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink_2)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(p.ink_3)
                .mt(px(3.0))
                .mb(px(12.0))
                .child(hint.to_string()),
        )
        .child(content)
}

/// 完成热力图卡片
fn heatmap_card(
    selected: StatsRange,
    today: chrono::NaiveDate,
    counts: &HashMap<chrono::NaiveDate, usize>,
    p: &'static DoitPalette,
) -> impl IntoElement {
    let (rows, show_head, cell_h, radius) = build_heatmap(selected, today, counts);

    // 周几表头（年视图为 GitHub 式 53 列，不展示表头）
    let mut head = div().grid().grid_cols(7).mb(px(7.0));
    for w in ["一", "二", "三", "四", "五", "六", "日"] {
        head = head.child(
            div()
                .text_center()
                .text_size(px(10.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink_3)
                .child(w),
        );
    }

    // 方格矩阵：年视图「行=周几、列=第几周」用 grid 均列 + 格子 aspect 1:1 保证正方形
    // 并铺满卡片宽度（年视图占整行，gap 用 1.5px 让格子更大）；周/月视图每行 7 格
    // flex 撑满，格高 cell_h、宽自适应。
    let mut grid = div()
        .id("heatmap")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(if cell_h > 20.0 { 5.0 } else { 3.0 }));
    for row in &rows {
        let is_year = cell_h <= 8.0;
        let gap = px(if cell_h > 20.0 {
            5.0
        } else if is_year {
            1.5
        } else {
            3.0
        });
        let mut line: Div = if is_year {
            div().grid().grid_cols(row.cells.len() as u16).gap(gap)
        } else {
            div().flex().gap(gap)
        };
        for c in &row.cells {
            let bg = if c.in_range {
                p.heat(c.level)
            } else {
                p.heat(0).alpha(0.0)
            };
            // 气泡内容（日期 + 完成数），用 gpui-kit 的 Tooltip 组件渲染，
            // 划过时在色块旁浮出，效果与折线图的数据点气泡一致。
            let tip_text = c
                .date
                .map(|d| format!("{} · 完成 {} 项", d.format("%m月%d日"), c.count))
                .unwrap_or_default();
            let tip_dot = if c.in_range { p.heat(c.level) } else { p.ink_3 };
            // 年视图小格：aspect 1:1 正方形（铺满均分列宽），划过时描边薄荷绿作为选中效果；
            // 周/月视图高度固定（flex 行内格子高即 cell_h，宽由列均分）
            let cell = div()
                .id(format!("heat-{}", c.key))
                .rounded(px(radius))
                .bg(bg);
            let cell = if is_year {
                cell.aspect_ratio(1.0)
                    .hover(|h| h.border_1().border_color(p.mint))
            } else {
                cell.flex_1()
                    .h(px(cell_h))
                    .hover(|h| h.border_1().border_color(p.mint))
            };
            line = line.child(
                cell.tooltip(move |window, cx| {
                        let tip_text = tip_text.clone();
                        Tooltip::element(move |_, _| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(7.0))
                                .child(div().w(px(9.0)).h(px(9.0)).rounded_full().bg(tip_dot))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(tip_text.clone()),
                                )
                        })
                        .build(window, cx)
                    }),
            );
        }
        grid = grid.child(line);
    }

    // 「少 ▪▪▪▪▪ 多」图例
    let mut legend = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .mt(px(12.0))
        .text_size(px(11.0))
        .text_color(p.ink_3);
    legend = legend.child("少");
    for l in 0..5 {
        legend = legend.child(
            div()
                .w(px(11.0))
                .h(px(11.0))
                .rounded(px(3.0))
                .bg(p.heat(l)),
        );
    }
    legend = legend.child("多");

    stat_card(
        p,
        "完成热力图",
        "鼠标划过色块查看当日完成了几项任务",
        div()
            .w_full()
            .flex()
            .flex_col()
            .when(show_head, |e| e.child(head))
            .child(grid)
            .child(legend),
    )
}

/// 分类占比卡片（gpui-kit PieChart 环形图 + 图例）
///
/// 使用 gpui-kit 自带 `PieChart` 组件绘制：`inner_radius` 形成环形、
/// `interactive` 开启自带 hover 扇区外凸 + 浮动 tooltip（划过提示分类名与完成数），
/// 无需手写 canvas / 热区 / 气泡。
fn ring_card(entries: &[CatSlice], p: &'static DoitPalette) -> impl IntoElement {
    // 有完成数据才画扇区；空数据让 PieChart 显示空环
    let _total: f32 = entries.iter().map(|e| e.count).sum();
    let data: Vec<CatSlice> = entries
        .iter()
        .filter(|e| e.count > 0.0)
        .cloned()
        .collect();

    let mut legend = div().flex().flex_col().gap(px(8.0)).text_size(px(13.0));
    for e in entries {
        let hex = crate::domain::color::parse_color_or_default(&e.hex);
        legend = legend.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().w(px(10.0)).h(px(10.0)).rounded_full().bg(rgb(hex)))
                .child(format!("{} · {}", e.name, e.count as i32)),
        );
    }

    // PieChart：环形 + 扇区色 + hover tooltip（分类名 · 完成数）
    // 注意：inner/outer_radius 是「像素」而非比例——不设 outer 时按高度*0.4 取默认
    // （156 容器 → 约 62px），inner 必须用像素值（≈外径 0.56）。
    let pie = div()
        .relative()
        .w(px(156.0))
        .h(px(156.0))
        .flex_none()
        .child(
            PieChart::new(data)
                .id("cat-ring-pie")
                .inner_radius(35.0)
                .value(move |e| e.count)
                .color(move |e| {
                    let hex = crate::domain::color::parse_color_or_default(&e.hex);
                    Hsla::from(rgb(hex))
                })
                .tooltip_name(move |e| SharedString::from(e.name.clone()))
                .tooltip_value(move |_e, value, _share| {
                    SharedString::from(format!("完成 {} 项", value as i32))
                }),
        );

    stat_card(
        p,
        "分类占比（区间完成）",
        "鼠标划过扇区查看该分类完成数与占比",
        div().w_full().flex().items_center().gap(px(20.0)).child(pie).child(legend),
    )
}

/// 统计区间：起止日（含）
fn range_of(r: StatsRange, today: chrono::NaiveDate) -> Range {
    use chrono::Datelike;
    match r {
        StatsRange::Week => Range {
            start: start_of_week(today),
            end: today,
        },
        StatsRange::Month => Range {
            start: chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today),
            end: today,
        },
        StatsRange::Year => Range {
            start: chrono::NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today),
            end: today,
        },
    }
}

/// 每天完成的顶级任务数
fn done_counts(state: &Entity<AppState>, cx: &App) -> HashMap<chrono::NaiveDate, usize> {
    let mut counts: HashMap<chrono::NaiveDate, usize> = HashMap::new();
    for t in &state.read(cx).snapshot.tasks {
        if t.parent_uid.is_none() && t.done && t.deleted_at.is_none() {
            if let Some(c) = t.completed_at {
                *counts.entry(local_date(c)).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// 区间内各分类的完成数（含 0 的分类，按数量倒序）
fn category_entries(state: &Entity<AppState>, range: Range, cx: &App) -> Vec<CatSlice> {
    let mut by_cat: HashMap<String, f32> = HashMap::new();
    for t in &state.read(cx).snapshot.tasks {
        if t.parent_uid.is_none() && t.done && t.deleted_at.is_none() {
            if let Some(c) = t.completed_at {
                let d = local_date(c);
                if d >= range.start && d <= range.end {
                    *by_cat.entry(t.category.clone()).or_insert(0.0) += 1.0;
                }
            }
        }
    }

    let categories = state.read(cx).snapshot.categories.clone();
    // 主列表只用可见分类（墓碑分类不属于当前分类，对齐桌面端 visibleCategories）
    let mut names: Vec<String> = state
        .read(cx)
        .snapshot
        .visible_categories()
        .into_iter()
        .map(|c| c.name.clone())
        .collect();
    // 区间内仍有完成记录、但已从分类列表移除的旧分类也要出现在图例中
    for name in by_cat.keys() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }

    let mut entries: Vec<CatSlice> = names
        .into_iter()
        .map(|name| {
            let hex = categories
                .iter()
                .find(|c| c.name == name)
                .map_or_else(
                    || crate::domain::color::DEFAULT_CATEGORY_COLOR.to_string(),
                    |c| crate::domain::color::normalize_category_color(&c.color),
                );
            CatSlice {
                count: *by_cat.get(&name).unwrap_or(&0.0),
                name,
                hex,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.count
            .partial_cmp(&a.count)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    entries
}

/// 趋势数据点：周 / 月按天，年按自然月分桶
fn trend_points(
    selected: StatsRange,
    range: Range,
    counts: &HashMap<chrono::NaiveDate, usize>,
) -> Vec<TrendPoint> {
    use chrono::Datelike;
    let mut points: Vec<TrendPoint> = Vec::new();
    if selected == StatsRange::Year {
        let year = range.start.year();
        for m in 1..=range.start.month() {
            let label = format!("{year}年{m}月");
            let sum: usize = counts
                .iter()
                .filter(|(d, _)| d.year() == year && d.month() == m)
                .map(|(_, c)| *c)
                .sum();
            points.push(TrendPoint {
                label,
                count: sum as f64,
            });
        }
    } else {
        let total = days_between(range.start, range.end) + 1;
        for i in 0..total {
            let d = range.start + chrono::Duration::days(i);
            points.push(TrendPoint {
                label: d.format("%m/%d").to_string(),
                count: *counts.get(&d).unwrap_or(&0) as f64,
            });
        }
    }
    points
}

/// 构建热力图：每行一周，格宽自适应（与原型 1fr 列一致）
///
/// 返回：行集合 / 是否展示周几表头 / 格子高度 / 圆角
fn build_heatmap(
    selected: StatsRange,
    today: chrono::NaiveDate,
    counts: &HashMap<chrono::NaiveDate, usize>,
) -> (Vec<HeatRow>, bool, f32, f32) {
    use chrono::Datelike;
    let mut rows: Vec<HeatRow> = Vec::new();
    match selected {
        StatsRange::Week => {
            // 当前所在周：周一 → 周日，一行 7 格
            let start = start_of_week(today);
            let cells = (0..7)
                .map(|i| {
                    let d = start + chrono::Duration::days(i);
                    HeatCell {
                        level: heat_level(*counts.get(&d).unwrap_or(&0)),
                        in_range: true,
                        key: d.format("%Y-%m-%d").to_string(),
                        count: *counts.get(&d).unwrap_or(&0),
                        date: Some(d),
                    }
                })
                .collect();
            rows.push(HeatRow { cells });
            (rows, true, 62.0, 10.0)
        }
        StatsRange::Month => {
            // 当前所在月：按周对齐（周一为首列），首行补齐空白格
            let year = today.year();
            let month = today.month();
            let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(today);
            let total = days_in_month(year, month) as usize;
            let offset = weekday_mon0(first) as usize;
            let mut cells: Vec<HeatCell> = Vec::new();
            for _ in 0..offset {
                // 月初之前的空白占位
                cells.push(HeatCell {
                    level: 0,
                    in_range: false,
                    key: format!("pad-{year}-{month}-{}", cells.len()),
                    count: 0,
                    date: None,
                });
            }
            for day in 0..total {
                let d = first + chrono::Duration::days(day as i64);
                cells.push(HeatCell {
                    level: heat_level(*counts.get(&d).unwrap_or(&0)),
                    in_range: true,
                    key: d.format("%Y-%m-%d").to_string(),
                    count: *counts.get(&d).unwrap_or(&0),
                    date: Some(d),
                });
            }
            for chunk in cells.chunks(7) {
                let mut line = chunk.to_vec();
                while line.len() < 7 {
                    line.push(HeatCell {
                        level: 0,
                        in_range: false,
                        key: format!("pad-{year}-{month}-{}", line.len()),
                        count: 0,
                        date: None,
                    });
                }
                rows.push(HeatRow { cells: line });
            }
            (rows, true, 62.0, 10.0)
        }
        StatsRange::Year => {
            // 当前自然年：GitHub 式「行=周几，列=第几周」
            let year = today.year();
            let first = chrono::NaiveDate::from_ymd_opt(year, 1, 1).unwrap_or(today);
            let last = chrono::NaiveDate::from_ymd_opt(year, 12, 31).unwrap_or(today);
            let aligned = start_of_week(first);
            let days = days_between(aligned, last) + 1;
            let weeks = ((days + 6) / 7) as usize;
            for wd in 0..7usize {
                let mut cells: Vec<HeatCell> = Vec::new();
                for w in 0..weeks {
                    let idx = (w * 7 + wd) as i64;
                    let d = aligned + chrono::Duration::days(idx);
                    let in_range = d >= first && d <= today;
                    cells.push(HeatCell {
                        level: heat_level(*counts.get(&d).unwrap_or(&0)),
                        in_range,
                        key: d.format("%Y-%m-%d").to_string(),
                        count: *counts.get(&d).unwrap_or(&0),
                        date: Some(d),
                    });
                }
                rows.push(HeatRow { cells });
            }
            (rows, false, 8.0, 2.0)
        }
    }
}

/// 热力等级：0 → 1；1-2 → 1；3-4 → 2；5-7 → 3；≥8 → 4（与原型一致）
fn heat_level(c: usize) -> usize {
    match c {
        0 => 0,
        1..=2 => 1,
        3..=4 => 2,
        5..=7 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    // 注意：不要 `use super::*`，否则会带入 gpui 的 `test` 宏，与内置 #[test] 冲突
    use super::{build_heatmap, heat_level, range_of, trend_points};
    use crate::app::StatsRange;
    use crate::domain::types::{add_days, last_day_of_month};
    use std::collections::HashMap;

    fn empty_counts() -> HashMap<chrono::NaiveDate, usize> {
        HashMap::new()
    }

    #[test]
    fn week_heatmap_has_seven_cells_in_one_row() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap(); // 周日
        let (rows, head, _h, _r) = build_heatmap(StatsRange::Week, today, &empty_counts());
        assert_eq!(rows.len(), 1, "一周只有一行");
        assert_eq!(rows[0].cells.len(), 7);
        assert!(head);
        // 本周周一为 11-16
        assert_eq!(
            rows[0].cells[0].key,
            chrono::NaiveDate::from_ymd_opt(2026, 11, 16)
                .unwrap()
                .format("%Y-%m-%d")
                .to_string()
        );
        assert_eq!(rows[0].cells[6].key, "2026-11-22");
    }

    #[test]
    fn month_heatmap_aligns_to_monday() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let (rows, head, _h, _r) = build_heatmap(StatsRange::Month, today, &empty_counts());
        assert!(head);
        assert_eq!(rows.len(), 6, "11/1 是周日（占 1 格）+ 30 天 → 6 行");
        for row in &rows {
            assert_eq!(row.cells.len(), 7, "每行恒为 7 格");
        }
        // 11-01 是周日 → 首行最后一格；11-02 是周一 → 第二行第一格
        assert_eq!(rows[0].cells[6].key, "2026-11-01");
        assert_eq!(rows[1].cells[0].key, "2026-11-02");
        // 首行第一格为占位（区间外）
        assert!(!rows[0].cells[0].in_range);
    }

    #[test]
    fn year_heatmap_marks_out_of_range_days() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        let (rows, head, _h, _r) = build_heatmap(StatsRange::Year, today, &empty_counts());
        assert_eq!(rows.len(), 7, "行=周几");
        assert!(!head, "年视图不展示周几表头");
        for row in &rows {
            assert!(row.cells.len() >= 52 && row.cells.len() <= 53);
        }
        // 1 月 1 日之前的补齐格子应为区间外
        assert!(rows.iter().any(|r| r.cells.iter().any(|c| !c.in_range)));
    }

    #[test]
    fn heat_levels_follow_prototype_buckets() {
        assert_eq!(heat_level(0), 0);
        assert_eq!(heat_level(1), 1);
        assert_eq!(heat_level(2), 1);
        assert_eq!(heat_level(3), 2);
        assert_eq!(heat_level(5), 3);
        assert_eq!(heat_level(9), 4);
    }

    #[test]
    fn range_of_uses_calendar_periods() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let week = range_of(StatsRange::Week, today);
        assert_eq!(week.start, chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap());
        let month = range_of(StatsRange::Month, today);
        assert_eq!(month.start, chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
        let year = range_of(StatsRange::Year, today);
        assert_eq!(year.start, chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(year.end, today);
    }

    #[test]
    fn trend_covers_elapsed_days() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let range = range_of(StatsRange::Week, today);
        let points = trend_points(StatsRange::Week, range, &empty_counts());
        assert_eq!(points.len(), 7, "周一 → 周日共 7 天");
        assert_eq!(points[0].label, "11/16");
        assert_eq!(points[6].label, "11/22");
        // 月视图：1 号 → 今天
        let range = range_of(StatsRange::Month, today);
        let points = trend_points(StatsRange::Month, range, &empty_counts());
        assert_eq!(points.len(), 22);
        assert_eq!(points[0].label, "11/01");
    }

    #[test]
    fn month_end_helper_matches_days_in_month() {
        assert_eq!(
            last_day_of_month(2026, 11),
            add_days(chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(), 29)
        );
    }
}
