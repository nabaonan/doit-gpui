//! doit 设计令牌（1:1 翻译自 doit-desktop 的 tokens.css）
//!
//! 这里集中存放全部色值，渲染时直接引用，不使用 gpui-component 默认主题色，
//! 以保证薄荷绿审美与原型完全一致。亮/暗双主题各一套。

use gpui_kit::{Hsla, Rgba, rgb};
use std::sync::LazyLock;

/// 应用主题模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTheme {
    #[default]
    Light,
    Dark,
}

impl AppTheme {
    /// 在亮 / 暗之间切换
    #[must_use]
    pub fn toggle(self) -> Self {
        match self {
            AppTheme::Light => AppTheme::Dark,
            AppTheme::Dark => AppTheme::Light,
        }
    }
    /// 取当前主题色板（Rgba 可直接用于 .bg() / .text_color()）
    pub fn palette(self) -> &'static DoitPalette {
        match self {
            AppTheme::Light => &LIGHT,
            AppTheme::Dark => &DARK,
        }
    }

    /// 子任务进度环的填充色与轨道色（SVG 需要 hex 字符串）
    pub fn ring_colors(self) -> (&'static str, &'static str) {
        match self {
            AppTheme::Light => ("#3FBB9C", "#E6EDEA"),
            AppTheme::Dark => ("#46C7A6", "#26322F"),
        }
    }
}

/// doit 完整色板（对应 tokens.css 的全部 CSS 变量）
/// 字段用 Rgba：rgb() 返回该类型，且实现了 Into<Fill> / Into<Hsla>
#[derive(Debug, Clone, Copy)]
pub struct DoitPalette {
    pub mint: Rgba,
    pub mint_deep: Rgba,
    pub mint_soft: Rgba,
    pub ink: Rgba,
    pub ink_2: Rgba,
    pub ink_3: Rgba,
    pub line: Rgba,
    pub card: Rgba,
    pub bg: Rgba,
    /// 玻璃拟态顶栏底色（GPUI 无 backdrop-filter，用低透明度近似）
    pub glass: Hsla,
    pub prio_none: Rgba,
    pub prio_low: Rgba,
    pub prio_normal: Rgba,
    pub prio_high: Rgba,
    pub prio_urgent: Rgba,
    pub heat: [Rgba; 5],
    pub warn: Rgba,
    pub warn_bg: Rgba,
    pub done_anim: Rgba,
    /// 子任务卡片底色（mint-soft 52% 与 card 混合，等价原型的 color-mix）
    pub sub_card: Rgba,
}

impl DoitPalette {
    /// 根据优先级值（0-4）取对应颜色
    pub fn priority(self, value: u8) -> Rgba {
        match value {
            1 => self.prio_low,
            2 => self.prio_normal,
            3 => self.prio_high,
            4 => self.prio_urgent,
            _ => self.prio_none,
        }
    }
    /// 热力图等级（0-4）取色
    pub fn heat(self, level: usize) -> Rgba {
        self.heat[level.min(4)]
    }
}

/// Rgba → `#RRGGBB`（自绘 SVG 时需要字面色值）
pub fn rgba_hex(c: Rgba) -> String {
    let ch = |v: f32| -> u8 { (v.clamp(0.0, 1.0) * 255.0).round() as u8 };
    format!("#{:02X}{:02X}{:02X}", ch(c.r), ch(c.g), ch(c.b))
}

/// 亮色主题（:root）
pub static LIGHT: LazyLock<DoitPalette> = LazyLock::new(|| DoitPalette {
    mint: rgb(0x3FBB9C),
    mint_deep: rgb(0x1E7D66),
    mint_soft: rgb(0xE8F7F1),
    ink: rgb(0x16211F),
    ink_2: rgb(0x5A6B66),
    ink_3: rgb(0x93A39C),
    line: rgb(0xE6EDEA),
    card: rgb(0xFFFFFF),
    bg: rgb(0xEEF4F1),
    glass: rgb(0xFFFFFF).alpha(0.72).into(),
    prio_none: rgb(0x8A9892),
    prio_low: rgb(0x4FBF9E),
    prio_normal: rgb(0x4A86D6),
    prio_high: rgb(0xE08A34),
    prio_urgent: rgb(0xDE5B55),
    heat: [
        rgb(0xEAF0ED),
        rgb(0xCDE9DF),
        rgb(0x9BD8C4),
        rgb(0x5FC2A4),
        rgb(0x2E9C7C),
    ],
    warn: rgb(0xF0A23B),
    warn_bg: rgb(0xFFF6E8),
    done_anim: rgb(0x3FBB9C),
    sub_card: rgb(0xF3FBF8),
});

/// 暗色主题（[data-theme="dark"]）
pub static DARK: LazyLock<DoitPalette> = LazyLock::new(|| DoitPalette {
    mint: rgb(0x46C7A6),
    mint_deep: rgb(0x7FE3C8),
    mint_soft: rgb(0x16302A),
    ink: rgb(0xE8F1EE),
    ink_2: rgb(0x9FB0AA),
    ink_3: rgb(0x6B7B75),
    line: rgb(0x26322F),
    card: rgb(0x19221F),
    bg: rgb(0x0C1316),
    glass: rgb(0x19221F).alpha(0.62).into(),
    prio_none: rgb(0x6E7D77),
    prio_low: rgb(0x55C9A8),
    prio_normal: rgb(0x6FA8DC),
    prio_high: rgb(0xFFA468),
    prio_urgent: rgb(0xFF6F6A),
    heat: [
        rgb(0x1B2422),
        rgb(0x1F3A33),
        rgb(0x27584B),
        rgb(0x34866F),
        rgb(0x46C7A6),
    ],
    warn: rgb(0xF0A23B),
    warn_bg: rgb(0x2A2415),
    done_anim: rgb(0x46C7A6),
    sub_card: rgb(0x172925),
});

/// 尺寸令牌（对应 tokens.css 的圆角 / 顶栏高度等）
pub mod metrics {
    use gpui_kit::{Pixels, px};
    pub const RADIUS: f32 = 16.0;       // --radius
    pub const RADIUS_SM: f32 = 11.0;    // --radius-sm
    pub const TOPBAR_H: f32 = 60.0;     // --tb-h
    pub const CHECK_SIZE: f32 = 23.0;   // 勾选圈直径
    pub const SUB_INDENT: f32 = 52.0;   // 每级子任务缩进（用户要求相等）
    pub const HEAT_CELL: f32 = 13.0;    // --heat-cell
    pub const HEAT_GAP: f32 = 5.0;      // --heat-gap
    pub fn radius() -> Pixels { px(RADIUS) }
    pub fn radius_sm() -> Pixels { px(RADIUS_SM) }
}

/// 优先级元数据（取值契约 0-4，展示顺序从高到低 + 不设）
pub mod priority {
    pub const NONE: u8 = 0;
    pub const LOW: u8 = 1;
    pub const NORMAL: u8 = 2;
    pub const HIGH: u8 = 3;
    pub const URGENT: u8 = 4;
    pub const DEFAULT: u8 = NORMAL;

    /// 展示顺序：紧急→高→普通→低→不设
    pub const OPTIONS: [(u8, &str); 5] = [
        (URGENT, "紧急"),
        (HIGH, "高"),
        (NORMAL, "普通"),
        (LOW, "低"),
        (NONE, "不设"),
    ];

    pub fn name(value: u8) -> &'static str {
        match value {
            1 => "低",
            2 => "普通",
            3 => "高",
            4 => "紧急",
            _ => "不设",
        }
    }
}
