//! 主题与配色：与两端设计令牌对齐。
//!
//! 优先级四色（与手机端 Color.kt 一致）：
//! 低 #4FBF9E / 普通 #4A86D6 / 高 #E08A34 / 紧急 #DE5B55。
//! 强调色对齐桌面端薄荷绿 #3E9B6E。

use ratatui::style::Color;

pub const PRIORITY_LOW: Color = Color::Rgb(0x4F, 0xBF, 0x9E);
pub const PRIORITY_MID: Color = Color::Rgb(0x4A, 0x86, 0xD6);
pub const PRIORITY_HIGH: Color = Color::Rgb(0xE0, 0x8A, 0x34);
pub const PRIORITY_URGENT: Color = Color::Rgb(0xDE, 0x5B, 0x55);

/// 分类内置色板 —— 与手机端 doit-apk `Color.kt` PaletteColors、桌面端 `constants/tokens.ts`
/// CATEGORY_SWATCHES 严格一致，三端必须同步修改，否则同一分类在两端会显示成不同颜色。
/// 统一带 # 大写存储；薄荷绿打头，整体清新。
pub const CAT_PALETTE: [&str; 8] = [
    "#34C7A8", "#5AA9E6", "#F2A65A", "#A78BFA", "#22B8CF", "#F08A8A", "#AACB5E", "#7C96B0",
];

/// 分类颜色非法/缺失时的回退色（与桌面端 DEFAULT_CATEGORY_COLOR 一致）。
pub const DEFAULT_CATEGORY_COLOR: &str = "#34C7A8";

/// 把任意分类色规范为合法的 #RRGGBB（对应桌面端 domain/color.ts 的 normalizeCategoryColor）：
/// trim → 无 # 则补 # → 仅 6 位 hex 合法并转大写 → 否则回退默认色。
/// 目的：手机端历史数据不带 # 的值（如 `5B9BD5`）也能补 # 匹配色板回显。
pub fn normalize_category_color(color: &str) -> String {
    let trimmed = color.trim();
    let with_hash = if trimmed.starts_with('#') {
        trimmed.to_string()
    } else {
        format!("#{trimmed}")
    };
    let hex = with_hash.strip_prefix('#').unwrap_or("");
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        format!("#{}", with_hash[1..].to_ascii_uppercase())
    } else {
        DEFAULT_CATEGORY_COLOR.to_string()
    }
}

pub fn priority_color(p: u8) -> Option<Color> {
    match p {
        1 => Some(PRIORITY_LOW),
        2 => Some(PRIORITY_MID),
        3 => Some(PRIORITY_HIGH),
        4 => Some(PRIORITY_URGENT),
        _ => None,
    }
}

pub fn priority_label(p: u8) -> Option<&'static str> {
    match p {
        1 => Some("低"),
        2 => Some("普通"),
        3 => Some("高"),
        4 => Some("紧急"),
        _ => None,
    }
}

/// "#RRGGBB"（或历史不带 # 的值）→ ratatui Color；非法值回退统一默认色。
/// 先归一化再解析，保证手机端不带 # 的历史色也能正确显示。
pub fn hex_color(s: &str) -> Color {
    let normalized = normalize_category_color(s);
    let hex = normalized.strip_prefix('#').unwrap_or(&normalized);
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::Rgb(0x34, 0xC7, 0xA8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeKind {
    #[default]
    Light,
    Dark,
}

#[allow(dead_code)]
impl ThemeKind {
    pub fn toggle(self) -> Self {
        match self {
            ThemeKind::Light => ThemeKind::Dark,
            ThemeKind::Dark => ThemeKind::Light,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::Light => "薄荷绿",
            ThemeKind::Dark => "深夜",
        }
    }
}

/// 主题令牌：背景 / 面板 / 文字 / 弱文字 / 强调 / 描边。
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub panel: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub border: Color,
    pub selected: Color,      // 选中卡片描边
}

pub const LIGHT: Theme = Theme {
    bg: Color::Rgb(0xF6, 0xF8, 0xF4),
    panel: Color::Rgb(0xFF, 0xFF, 0xFF),
    text: Color::Rgb(0x2B, 0x33, 0x2D),
    muted: Color::Rgb(0x6D, 0x77, 0x6E),
    faint: Color::Rgb(0xA8, 0xB2, 0xA8),
    accent: Color::Rgb(0x3E, 0x9B, 0x6E),
    border: Color::Rgb(0xD6, 0xDF, 0xD3),
    selected: Color::Rgb(0x3E, 0x9B, 0x6E),
};

pub const DARK: Theme = Theme {
    bg: Color::Rgb(0x1E, 0x24, 0x20),
    panel: Color::Rgb(0x27, 0x30, 0x2A),
    text: Color::Rgb(0xE4, 0xEA, 0xE2),
    muted: Color::Rgb(0x9A, 0xA5, 0x9B),
    faint: Color::Rgb(0x5F, 0x6B, 0x61),
    accent: Color::Rgb(0x5F, 0xC1, 0x8E),
    border: Color::Rgb(0x44, 0x52, 0x48),
    selected: Color::Rgb(0x5F, 0xC1, 0x8E),
};

impl ThemeKind {
    pub fn theme(self) -> Theme {
        match self {
            ThemeKind::Light => LIGHT,
            ThemeKind::Dark => DARK,
        }
    }
}
