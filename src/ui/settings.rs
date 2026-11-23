//! 设置页（对齐原型：外观 / WebDAV 同步 / 系统 / 关于）

use crate::app::AppState;
use crate::theme::palette::metrics;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputContentType};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::Hsla;

/// 进度色候选（与 PROG_COLORS 一致）
const PROG_COLORS: [(&str, u32); 5] = [
    ("薄荷绿", 0x3FBB9C),
    ("海蓝", 0x4A90D9),
    ("暖橙", 0xF2A65A),
    ("紫罗兰", 0x9B6DD6),
    ("玫红", 0xE5688A),
];

/// 渲染设置页
pub fn render_settings(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let is_dark = state.read(cx).settings.theme == "dark";
    let prog_color = state.read(cx).settings.progress_color.clone();
    let webdav_enabled = state.read(cx).settings.webdav.enabled;
    let sync_status = state.read(cx).sync_status.clone();

    // WebDAV 三个输入框
    let url_state = state.read(cx).webdav_url.clone();
    let user_state = state.read(cx).webdav_user.clone();
    let pass_state = state.read(cx).webdav_pass.clone();

    div()
        .w_full()
        .px(px(34.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            // 外观
            section("外观", p, cx, div()
                .flex()
                .flex_col()
                .child(field("主题", p, cx,
                    seg_control(state, "theme", ["薄荷绿", "深夜青灰"], is_dark, cx)
                ))
                .child(field("完成进度风格", p, cx,
                    div().flex().items_center().gap(px(12.0))
                        .child({
                            let mut row = div().flex().items_center().gap(px(10.0)).id("prog-colors");
                            for (name, hex) in PROG_COLORS {
                                let s = state.clone();
                                let h = format!("#{hex:06X}");
                                let on = prog_color.eq_ignore_ascii_case(&h);
                                let color = rgb(hex);
                                row = row.child(
                                    // 未选中：纯色圆角方块 + 淡描边
                                    // 已选中：同色描边环 + 白色内环 + 白色对钩
                                    div()
                                        .id(format!("prog-{name}"))
                                        .w(px(30.0)).h(px(30.0))
                                        .rounded(px(10.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .bg(color)
                                        .when(on, |e| e.p(px(2.0)))
                                        .when(!on, |e| {
                                            e.border_1().border_color(p.ink_3.alpha(0.30))
                                        })
                                        .hover(move |h2| h2.bg(color))
                                        .on_click(move |_, _w, cx| {
                                            s.update(cx, |st, cx| {
                                                st.settings.progress_color.clone_from(&h);
                                                cx.notify();
                                            });
                                        })
                                        .when(on, |e| e.child(
                                            div()
                                                .w_full().h_full()
                                                .rounded(px(8.0))
                                                .bg(p.card)
                                                .p(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .w_full().h_full()
                                                        .rounded(px(6.0))
                                                        .bg(color)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(
                                                            svg()
                                                                .data(&crate::ui::icons::check())
                                                                .w(px(15.0))
                                                                .h(px(15.0))
                                                                .text_color(rgb(0xFFFFFF)),
                                                        ),
                                                ),
                                        )),
                                );
                            }
                            row
                        })
                ))
            )
        )
        // WebDAV 同步
        .child(
            section("WebDAV 同步", p, cx, div()
                .flex().flex_col()
                .child(field("同步状态", p, cx,
                    div()
                        .flex().items_center().gap(px(7.0))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if sync_status == "dirty" { p.warn } else { p.mint })
                        .child(svg().data(&crate::ui::icons::cloud()).w(px(16.0)).h(px(16.0))
                            .text_color(if sync_status == "dirty" { p.warn } else { p.mint }))
                        .child(if sync_status == "dirty" { "有未同步更改" } else { "已同步" })
                ))
                .child(field("启用同步", p, cx, {
                    let s = state.clone();
                    Switch::new("webdav-switch")
                        .checked(webdav_enabled)
                        .color(Hsla::from(p.mint))
                        .on_change(move |checked, _window, cx: &mut App| {
                            s.update(cx, |st, cx| {
                                st.settings.webdav.enabled = *checked;
                                st.notify_msg(if *checked { "已启用 WebDAV 同步" } else { "已关闭 WebDAV 同步" });
                                cx.notify();
                            });
                        })
                }))
                .child(field("服务器地址", p, cx,
                    input_box(&url_state, p)
                ))
                .child(field("账号", p, cx,
                    input_box(&user_state, p)
                ))
                .child(field("密码 / 令牌", p, cx,
                    Input::new(&pass_state)
                        .w(px(300.0))
                        .h(px(36.0))
                        .content_type(InputContentType::Password)
                ))
                // 操作按钮
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_end()
                        .gap(px(10.0))
                        .pt(px(12.0))
                        .child({
                            let s = state.clone();
                            Button::new("webdav-test")
                                .label("测试连接")
                                .custom(
                                    ButtonCustomVariant::new(cx)
                                        .color(Hsla::from(p.card))
                                        .foreground(Hsla::from(p.ink))
                                        .hover(Hsla::from(p.mint_soft))
                                        .active(Hsla::from(p.mint_soft)),
                                )
                                .border_1()
                                .border_color(p.line)
                                .on_click(move |_, _window, cx: &mut App| {
                                    s.update(cx, |st, cx| {
                                        st.notify_msg("连接测试将在接入 WebDAV 客户端后生效");
                                        cx.notify();
                                    });
                                })
                        })
                        .child({
                            let s = state.clone();
                            let url = url_state.clone();
                            let user = user_state.clone();
                            let pass = pass_state.clone();
                            Button::new("webdav-save")
                                .label("保存并同步")
                                .custom(
                                    ButtonCustomVariant::new(cx)
                                        .color(Hsla::from(p.mint))
                                        .foreground(Hsla::from(rgb(0xFFFFFF)))
                                        .hover(Hsla::from(p.mint_deep))
                                        .active(Hsla::from(p.mint_deep)),
                                )
                                .on_click(move |_, _window, cx: &mut App| {
                                    let u = url.read(cx).value().to_string();
                                    let n = user.read(cx).value().to_string();
                                    let w = pass.read(cx).value().to_string();
                                    s.update(cx, |st, cx| {
                                        st.settings.webdav.url = u.trim().to_string();
                                        st.settings.webdav.username = n.trim().to_string();
                                        st.settings.webdav.password_ref =
                                            if w.trim().is_empty() { None } else { Some(w.trim().to_string()) };
                                        st.save();
                                        st.notify_msg("配置已保存（WebDAV 客户端待接入）");
                                        cx.notify();
                                    });
                                })
                        }),
                )
            )
        )
        .child(
            section("系统", p, cx, div()
                .flex().flex_col()
                .child(field("开机自启", p, cx, {
                    let s = state.clone();
                    Switch::new("autostart-switch")
                        .checked(false)
                        .color(Hsla::from(p.mint))
                        .on_change(move |checked, _window, cx: &mut App| {
                            s.update(cx, |st, cx| {
                                st.notify_msg(if *checked {
                                    "开机自启将在接入系统 API 后生效"
                                } else {
                                    "已关闭开机自启"
                                });
                                cx.notify();
                            });
                        })
                }))
            )
        )
        .child(
            section("关于", p, cx,
                div().flex().flex_col().gap(px(6.0))
                    .child(div().text_size(px(13.0)).text_color(p.ink_2).child("doit-gpui v0.1.0 · 数据契约 v2"))
                    .child(div().text_size(px(12.0)).text_color(p.ink_3).child("基于 GPUI Kit 的纯原生实现，与手机/桌面端共用 doit-backup.json"))
            )
        )
}

/// 输入框（沿用组件库自带外观，宽度固定）
fn input_box(
    st: &Entity<gpui_kit::component::input::InputState>,
    _p: &crate::theme::palette::DoitPalette,
) -> impl IntoElement {
    Input::new(st).w(px(300.0)).h(px(36.0))
}

/// 分区卡片
fn section(
    title: &str,
    p: &crate::theme::palette::DoitPalette,
    _cx: &App,
    content: impl IntoElement,
) -> impl IntoElement {
    div()
        .w_full()
        .rounded(px(metrics::RADIUS))
        .border_1()
        .border_color(p.line)
        .bg(p.card)
        .p(px(20.0))
        .child(div().text_size(px(14.0)).font_weight(FontWeight::SEMIBOLD).mb(px(12.0)).child(title.to_string()))
        .child(content)
}

/// 一行字段
fn field(
    label: &str,
    p: &crate::theme::palette::DoitPalette,
    _cx: &App,
    control: impl IntoElement,
) -> impl IntoElement {
    let _ = p;
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.0))
        .py(px(11.0))
        .border_b_1()
        .border_color(p.line)
        .child(div().text_size(px(13.0)).text_color(p.ink_2).child(label.to_string()))
        .child(control)
}

/// 分段控件（简版：两个可点文字按钮）
fn seg_control(
    state: &Entity<AppState>,
    kind: &str,
    labels: [&str; 2],
    second_active: bool,
    cx: &App,
) -> impl IntoElement {
    let p = state.read(cx).theme().palette();
    let mut seg = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(metrics::RADIUS_SM))
        .bg(p.mint_soft)
        .p(px(3.0))
        .id(format!("seg-{kind}"));
    for (i, label) in labels.iter().enumerate() {
        let s = state.clone();
        let active = (i == 1) == second_active;
        let k = kind.to_string();
        seg = seg.child(
            div()
                .id(format!("seg-{kind}-{i}"))
                .px(px(16.0))
                .py(px(7.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .text_size(px(13.0))
                .font_weight(if active { FontWeight::BOLD } else { FontWeight::MEDIUM })
                .bg(if active { p.card.into() } else { transparent_black() })
                .text_color(if active { p.mint_deep } else { p.ink_2 })
                .child(label.to_string())
                .on_click(move |_, _window, cx| {
                    if k == "theme" {
                        let dark = i == 1; // 第 1 项=深夜青灰
                        s.update(cx, |st, cx| {
                            st.settings.theme = if dark { "dark".into() } else { "light".into() };
                            cx.notify();
                        });
                        Theme::change(
                            if dark { ThemeMode::Dark } else { ThemeMode::Light },
                            None,
                            cx,
                        );
                        crate::app::apply_component_theme(dark, cx);
                    }
                }),
        );
    }
    seg
}
