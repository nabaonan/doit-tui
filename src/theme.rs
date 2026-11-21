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

/// 分类内置色卡（8 色，与原型一致）。
pub const CAT_PALETTE: [&str; 8] = [
    "#4A86D6", "#4FBF9E", "#EF9F27", "#DE5B55", "#7F77DD", "#D4537E", "#1D9E75", "#3AA6C9",
];

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

/// "#RRGGBB" → ratatui Color；解析失败返回灰色。
pub fn hex_color(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::Rgb(0x88, 0x87, 0x80)
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
