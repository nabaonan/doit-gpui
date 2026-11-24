//! 分类颜色工具（对齐 doit-desktop 的 domain/color.ts）
//!
//! 历史数据（手机端旧快照）里的颜色可能不带 `#`（如 `5B9BD5`），
//! 若不补 `#` 就会因字符串不相等导致「编辑分类时色板无法回显当前颜色」。
//! 这里统一把任意输入规范为带 `#` 的大写 `#RRGGBB`，非法值回退默认色。

/// 兼容历史数据里 color 为空 / 非法 / 不在色板的分类：统一回退薄荷绿。
pub const DEFAULT_CATEGORY_COLOR: &str = "#34C7A8";
/// 默认分类色的 `u32` 形式（UI 解析失败时回退）。
pub const DEFAULT_CATEGORY_COLOR_HEX: u32 = 0x34C7A8;

/// 把任意分类色规范为合法的 `#RRGGBB`：
/// 先 trim；无前导 `#` 则补上；仅 `^#[0-9a-fA-F]{6}$` 合法并统一转大写；否则回退默认色。
#[must_use]
pub fn normalize_category_color(color: &str) -> String {
    let value = color.trim();
    let candidate = if value.starts_with('#') {
        value.to_string()
    } else {
        format!("#{value}")
    };
    if candidate.len() == 7 && candidate.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit) {
        candidate.to_ascii_uppercase()
    } else {
        DEFAULT_CATEGORY_COLOR.to_string()
    }
}

/// UI 用：把分类色解析为 `u32`，非法值回退默认薄荷绿。
#[must_use]
pub fn parse_color_or_default(color: &str) -> u32 {
    u32::from_str_radix(normalize_category_color(color).trim_start_matches('#'), 16)
        .unwrap_or(DEFAULT_CATEGORY_COLOR_HEX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_hash_and_uppercases() {
        assert_eq!(normalize_category_color("5B9BD5"), "#5B9BD5");
        assert_eq!(normalize_category_color("  #34c7a8  "), "#34C7A8");
        assert_eq!(normalize_category_color("#a78bfa"), "#A78BFA");
    }

    #[test]
    fn normalize_falls_back_on_invalid() {
        for bad in ["", "  ", "red", "#FFF", "#12345G", "#1234567", "##12345"] {
            assert_eq!(normalize_category_color(bad), DEFAULT_CATEGORY_COLOR, "{bad}");
        }
    }

    #[test]
    fn parse_color_uses_default_on_invalid() {
        assert_eq!(parse_color_or_default("5B9BD5"), 0x5B9BD5);
        assert_eq!(parse_color_or_default(""), DEFAULT_CATEGORY_COLOR_HEX);
        assert_eq!(parse_color_or_default("oops"), DEFAULT_CATEGORY_COLOR_HEX);
    }
}
