//! 内联 SVG 图标（1:1 取自 doit-desktop 的 constants/icons.ts，24x24 viewBox）
//! 通过 svg().data(bytes) 渲染，颜色用 currentColor 由父级 text_color 控制。

/// 生成带 currentColor 描边的 SVG 字节
fn icon(inner: &str) -> Vec<u8> {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{inner}</svg>"
    )
    .into_bytes()
}

pub fn today() -> Vec<u8> {
    icon("<rect x=\"3\" y=\"5\" width=\"18\" height=\"16\" rx=\"2\"/><path d=\"M3 10h18M8 3v4M16 3v4\"/>")
}
pub fn stats() -> Vec<u8> {
    icon("<path d=\"M4 20V10M10 20V4M16 20v-7M21 20H3\"/>")
}
pub fn calendar() -> Vec<u8> {
    icon("<rect x=\"3\" y=\"4\" width=\"18\" height=\"17\" rx=\"2\"/><path d=\"M3 9h18M8 2v4M16 2v4\"/>")
}
pub fn settings() -> Vec<u8> {
    icon("<circle cx=\"12\" cy=\"12\" r=\"3\"/><path d=\"M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2\"/>")
}
pub fn check() -> Vec<u8> {
    icon("<path d=\"M5.5 12.8l4.2 4.2L18.5 7.5\" stroke-width=\"2.8\"/>")
}
pub fn chevron_down() -> Vec<u8> {
    icon("<path d=\"M6 9l6 6 6-6\" stroke-width=\"2\"/>")
}
pub fn chevron_left() -> Vec<u8> {
    icon("<path d=\"M15 6l-6 6 6 6\" stroke-width=\"2\"/>")
}
pub fn chevron_right() -> Vec<u8> {
    icon("<path d=\"M9 6l6 6-6 6\" stroke-width=\"2\"/>")
}
pub fn plus() -> Vec<u8> {
    icon("<path d=\"M12 5v14M5 12h14\" stroke-width=\"2.2\"/>")
}
pub fn cloud() -> Vec<u8> {
    icon("<path d=\"M7 18a4 4 0 010-8 5 5 0 019-2 4 4 0 011 10H7z\"/>")
}
pub fn cloud_ok() -> Vec<u8> {
    icon("<path d=\"M7 18a4 4 0 010-8 5 5 0 019-2 4 4 0 011 10H7z\"/><path d=\"M9.4 13.2l2.2 2.2 4.3-4.4\" stroke-width=\"2\"/>")
}
pub fn trash() -> Vec<u8> {
    icon("<path d=\"M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13\"/>")
}
pub fn pencil() -> Vec<u8> {
    icon("<path d=\"M4 20l4-1 11-11-3-3L5 16z\"/>")
}
pub fn close() -> Vec<u8> {
    icon("<path d=\"M6 6l12 12M18 6L6 18\" stroke-width=\"2\"/>")
}
pub fn folder() -> Vec<u8> {
    icon("<path d=\"M3 7h6l2 2h10v10H3z\"/>")
}
pub fn flag() -> Vec<u8> {
    icon("<path d=\"M6 21V4h11l-2 4 2 4H6\"/>")
}

/// 父任务只读虚线圈（r=10.8, dasharray 4 3）
pub fn dash_ring() -> Vec<u8> {
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10.8\" stroke-dasharray=\"4 3\"/></svg>"
        .as_bytes()
        .to_vec()
}

/// 环形进度 SVG（给定半径/线宽/百分比/颜色 hex），供进度环使用
pub fn progress_ring(r: f32, stroke: f32, pct: f32, color_hex: &str, track_hex: &str) -> Vec<u8> {
    let size = (r + stroke) * 2.0;
    let c = 2.0 * std::f32::consts::PI * r;
    let filled = c * (pct / 100.0).clamp(0.0, 1.0);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\" width=\"{size}\" height=\"{size}\">\
<circle cx=\"{mid}\" cy=\"{mid}\" r=\"{r}\" fill=\"none\" stroke=\"{track_hex}\" stroke-width=\"{stroke}\"/>\
<circle cx=\"{mid}\" cy=\"{mid}\" r=\"{r}\" fill=\"none\" stroke=\"{color_hex}\" stroke-width=\"{stroke}\" stroke-linecap=\"round\" stroke-dasharray=\"{filled} {c}\" transform=\"rotate(-90 {mid} {mid})\"/>\
</svg>",
        mid = size / 2.0,
    )
    .into_bytes()
}
