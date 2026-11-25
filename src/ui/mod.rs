//! UI 渲染：顶栏 / 分类条 / 键位栏 / 帮助与弹框 / 各页面。

pub mod calendar;
pub mod settings;
pub mod stats;
pub mod today;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Page};
use crate::theme::{hex_color, priority_color, priority_label, Theme};

/// 显示宽度（中文按 2 列计）。
pub fn disp_w(s: &str) -> usize {
    s.width()
}

/// 超宽文本截断（追加 …）。
pub fn ellipsize(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut)
}

/// 进度环字符：0-25% ○，26-50% ◔，51-75% ◑，76-99% ◕，100% ●。
pub fn ring_char(pct: usize) -> &'static str {
    match pct {
        0..=25 => "○",
        26..=50 => "◔",
        51..=75 => "◑",
        76..=99 => "◕",
        _ => "●",
    }
}

// ---------- 顶栏 ----------

pub fn draw_topbar(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let (done, total) = app.today_stats();
    let pct = if total > 0 { 100 * done / total } else { 0 };
    let now = chrono::Local::now();

    // 左侧：logo + 页面 tabs
    let mut left: Vec<Span> = Vec::new();
    left.push(Span::styled(
        " doit ",
        Style::default().bg(theme.accent).fg(theme.panel).add_modifier(Modifier::BOLD),
    ));
    left.push(Span::raw(" "));
    for i in 0..4 {
        let p = Page::from_index(i);
        if p == app.page {
            left.push(Span::styled(
                format!(" {} {} ", p.label(), i + 1),
                Style::default().bg(theme.accent).fg(theme.panel).add_modifier(Modifier::BOLD),
            ));
        } else {
            left.push(Span::styled(
                format!(" {}", p.label()),
                Style::default().fg(theme.muted),
            ));
            left.push(Span::styled(
                format!(" {} ", i + 1),
                Style::default().fg(theme.faint),
            ));
        }
        left.push(Span::raw(" "));
    }

    // 右侧：同步胶囊 + 日期环形进度胶囊；宽度不足时逐级精简
    let sync_c = app.sync_status.color(theme);
    let sync_full = format!(" ● {} r{} ", app.sync_status.label(), app.revision);
    let sync_mid = format!(" ● {} ", app.sync_status.label());
    let date_full = format!(" {}月{}日 {} {}{}% ", now.month(), now.day(), zh_weekday(now.weekday()), ring_char(pct), pct);
    let date_mid = format!(" {}/{} {}{}% ", now.month(), now.day(), ring_char(pct), pct);

    let width = area.width as usize;
    let left_w: usize = left.iter().map(|s| disp_w(&s.content)).sum();
    let fits = |a: &str, b: &str| left_w + disp_w(a) + disp_w(b) <= width;
    let (sync_txt, date_txt) = if fits(&sync_full, &date_full) {
        (sync_full, date_full)
    } else if fits(&sync_mid, &date_full) {
        (sync_mid, date_full)
    } else if fits(&sync_mid, &date_mid) {
        (sync_mid, date_mid)
    } else {
        (sync_mid.clone(), String::new())
    };

    let mut spans = left;
    let right_w = disp_w(&sync_txt) + disp_w(&date_txt);
    let pad = width.saturating_sub(left_w + right_w);
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(
        sync_txt,
        Style::default().bg(theme.panel).fg(sync_c),
    ));
    if !date_txt.is_empty() {
        spans.push(Span::styled(
            date_txt,
            Style::default().bg(theme.panel).fg(theme.muted),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)), area);
}

pub fn zh_weekday(w: chrono::Weekday) -> &'static str {
    match w {
        chrono::Weekday::Mon => "周一",
        chrono::Weekday::Tue => "周二",
        chrono::Weekday::Wed => "周三",
        chrono::Weekday::Thu => "周四",
        chrono::Weekday::Fri => "周五",
        chrono::Weekday::Sat => "周六",
        chrono::Weekday::Sun => "周日",
    }
}

// ---------- 分类导航条 ----------

pub fn draw_catbar(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut spans: Vec<Span> = Vec::new();
    // 胶囊：选中=分类色/深色填充白字；未选中=panel 底 + 色点 + 灰字（对齐原型）
    let pill = |spans: &mut Vec<Span>, dot_color: String, name: &str, count: usize, selected: bool, sel_color: Option<&str>, theme: &Theme| {
        let bg = if selected {
            sel_color.map(hex_color).unwrap_or(theme.text)
        } else {
            theme.panel
        };
        let fg = if selected { theme.panel } else { theme.muted };
        let count_fg = if selected { fg } else { theme.faint };
        let dot_fg = if selected { theme.panel } else { hex_color(&dot_color) };
        spans.push(Span::styled(" ●", Style::default().bg(bg).fg(dot_fg)));
        spans.push(Span::styled(format!(" {} ", name), Style::default().bg(bg).fg(fg)));
        spans.push(Span::styled(format!("{} ", count), Style::default().bg(bg).fg(count_fg)));
    };

    let total_pending = app.cat_pending("全部");
    pill(
        &mut spans,
        "#888780".into(),
        "全部",
        total_pending,
        app.cat == "全部",
        None,
        theme,
    );
    for c in app.snap.active_categories() {
        pill(
            &mut spans,
            c.color.clone(),
            &c.name,
            app.cat_pending(&c.name),
            app.cat == c.name,
            Some(&c.color),
            theme,
        );
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        "＋ 添加分类 ",
        Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
    ));

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        area,
    );
}

/// 顶栏/分类条鼠标点击命中目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatTarget {
    Cat(String), // 分类名（含"全部"）
    AddCat,      // "＋ 添加分类"
}

/// 顶栏导航 tab 的可点击列区间（与 draw_topbar 同源计算，避免命中与渲染错位）。
/// 返回 (页面, 起始列, 宽度)，列从 0 起；顶栏为终端第 0 行。
pub fn topbar_tab_hits(app: &App) -> Vec<(Page, u16, u16)> {
    let mut out = Vec::new();
    let mut x = disp_w(" doit ") as u16 + 1; // logo + 后随空格
    for i in 0..4usize {
        let p = Page::from_index(i);
        let w = if p == app.page {
            disp_w(&format!(" {} {} ", p.label(), i + 1))
        } else {
            disp_w(&format!(" {}", p.label())) + disp_w(&format!(" {} ", i + 1))
        } as u16;
        out.push((p, x, w));
        x += w + 1; // tab 之间用空格分隔（与渲染一致）
    }
    out
}

/// 分类导航条的可点击列区间（与 draw_catbar 同源计算）。分类条为终端第 1 行。
pub fn catbar_hits(app: &App) -> Vec<(CatTarget, u16, u16)> {
    let mut out = Vec::new();
    let mut x: u16 = 0;

    let all = ("全部".to_string(), app.cat_pending("全部"));
    let w = disp_w(&format!(" ● {} {} ", all.0, all.1)) as u16;
    out.push((CatTarget::Cat(all.0), x, w));
    x += w;

    for c in app.snap.active_categories() {
        let w = disp_w(&format!(" ● {} {} ", c.name, app.cat_pending(&c.name))) as u16;
        out.push((CatTarget::Cat(c.name.clone()), x, w));
        x += w;
    }

    x += 1; // 分类与"添加分类"之间的空格
    let add_w = disp_w("＋ 添加分类 ") as u16;
    out.push((CatTarget::AddCat, x, add_w));
    out
}

// ---------- 底部键位栏 ----------

pub fn draw_footer(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let full: &[(&str, &str)] = match app.page {
        Page::Today => &[
            ("1-4", "页面"), ("[ ]", "分类"), ("c", "新建分类"), ("h/l", "折叠"),
            ("j/k", "移动"), ("space", "完成"), ("a", "新增"), ("e", "编辑"), ("d", "删除"),
            ("p", "优先级"), ("s", "同步"), ("t", "主题"), ("?", "帮助"), ("q", "退出"),
        ],
        Page::Stats => &[
            ("1-4", "页面"), ("[ ]", "分类"), ("←/→", "热力周期"),
            ("s", "同步"), ("t", "主题"), ("?", "帮助"), ("q", "退出"),
        ],
        Page::Calendar => &[
            ("1-4", "页面"), ("[ ]", "分类"), ("h/l", "选日"), ("j/k", "行"),
            ("←/→", "翻月"), ("g", "今天"), ("s", "同步"), ("t", "主题"),
            ("?", "帮助"), ("q", "退出"),
        ],
        Page::Settings => &[
            ("1-4", "页面"), ("u/n/p", "地址/账号/密码"), ("e", "启停同步"), ("s", "立即同步"),
            ("t", "主题"), ("?", "帮助"), ("q", "退出"),
        ],
    };
    let short: &[(&str, &str)] = &[
        ("1-4", "页面"), ("[ ]", "分类"), ("x", "删分类"), ("j/k", "移动"), ("space", "完成"),
        ("a", "新增"), ("Esc", "取消"), ("?", "全部键位"),
    ];

    // 原型风格：按键绿字加粗 + 说明灰字，" · " 分隔
    let render = |pairs: &[(&str, &str)], themed: bool| -> (usize, Vec<Span<'static>>) {
        let mut spans: Vec<Span> = Vec::new();
        let mut w = 0usize;
        for (i, (k, v)) in pairs.iter().enumerate() {
            if i > 0 {
                let sep = " · ";
                w += disp_w(sep);
                if themed {
                    spans.push(Span::styled(sep.to_string(), Style::default().fg(theme.faint)));
                } else {
                    spans.push(Span::raw(sep.to_string()));
                }
            }
            let key = format!(" {}", k);
            let val = format!(" {}", v);
            w += disp_w(&key) + disp_w(&val);
            if themed {
                spans.push(Span::styled(key, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(val, Style::default().fg(theme.muted)));
            } else {
                spans.push(Span::raw(key));
                spans.push(Span::raw(val));
            }
        }
        (w, spans)
    };
    let width = area.width as usize;
    let (full_w, _) = render(full, false);
    let (short_w, _) = render(short, false);
    let chosen: &[(&str, &str)] = if full_w <= width {
        full
    } else if short_w <= width {
        short
    } else {
        &[]
    };
    let mut spans = if chosen.is_empty() {
        vec![
            Span::styled(" 1-4", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" 页面", Style::default().fg(theme.muted)),
            Span::styled(" · ", Style::default().fg(theme.faint)),
            Span::styled(" ?", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" 全部键位", Style::default().fg(theme.muted)),
        ]
    } else {
        render(chosen, true).1
    };
    if spans.iter().map(|s| disp_w(&s.content)).sum::<usize>() < width {
        let pad = width - spans.iter().map(|s| disp_w(&s.content)).sum::<usize>();
        spans.push(Span::raw(" ".repeat(pad)));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        area,
    );
}

// ---------- 提示 toast ----------

pub fn draw_toast(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let Some((msg, _)) = &app.toast else { return };
    let w = (msg.chars().count() + 6).min(area.width as usize) as u16;
    let r = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + 1,
        width: w,
        height: 1,
    };
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(format!(" {} ", msg))
            .style(Style::default().bg(theme.text).fg(theme.bg)),
        r,
    );
}

// ---------- 帮助弹框 ----------

pub fn draw_help(f: &mut Frame, theme: &Theme) {
    let area = popup_centered(f.area(), 62, 20);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(" 键位帮助 ")
        .padding(Padding::horizontal(2));
    let rows: Vec<(&str, &str)> = vec![
        ("1-4", "切换页面：今日 / 统计 / 月历 / 设置"),
        ("u/n/p", "设置页编辑 WebDAV 服务器地址 / 账号 / 密码令牌"),
        ("Tab", "按顺序切换页面"),
        ("[ ]", "切换分类过滤"),
        ("c", "新建分类（弹框：名称 + 色卡选色）"),
        ("x", "删除当前分类（确认后执行，任务移入默认）"),
        ("j / k", "移动选中项"),
        ("space", "完成 / 取消完成（父子联动）"),
        ("a", "新增任务（Ctrl+P 换优先级，Tab 换分类，!1~!4 快捷）"),
        ("A", "为选中任务新增子任务（任意深度，可无限嵌套）"),
        ("e", "编辑选中标题（已完成不可编辑）"),
        ("d", "删除选中任务"),
        ("p", "循环切换优先级 0不设→1低→2普通→3高→4紧急"),
        ("h / l", "今日页折叠/展开父任务；月历页前后一天选日"),
        ("j / k", "今日页移动选中；月历页上一行/下一行（±7 天）"),
        ("s", "触发 WebDAV 同步"),
        ("←/→", "统计页切换热力图周期；月历页翻月"),
        ("g", "月历页回到今天并选中今天"),
        ("t", "切换 薄荷绿 / 深夜 主题"),
        ("?", "关闭本帮助"),
    ];
    let mut lines = Vec::new();
    for (k, v) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<10}", k), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(v.to_string(), Style::default().fg(theme.muted)),
        ]));
    }
    f.render_widget(Paragraph::new(lines).block(block), area);
}

pub fn popup_centered(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

// ---------- 新建分类弹框 ----------

pub fn draw_cat_modal(f: &mut Frame, app: &App, theme: &Theme) {
    let Some(m) = &app.cat_modal else { return };
    let area = popup_centered(f.area(), 56, 9);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.panel))
        .title(" 新建分类 ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut color_row: Vec<Span> = Vec::new();
    for (i, c) in crate::theme::CAT_PALETTE.iter().enumerate() {
        let n = i + 1;
        if i == m.color_idx {
            // 选中：数字+色点整体包在浅底里，边界括号强调
            color_row.push(Span::styled(
                format!("[{}", n),
                Style::default().fg(theme.muted).bg(theme.text),
            ));
            color_row.push(Span::styled(
                "●",
                Style::default().fg(hex_color(c)).bg(theme.text).add_modifier(Modifier::BOLD),
            ));
            color_row.push(Span::styled(
                "] ",
                Style::default().fg(theme.muted).bg(theme.text),
            ));
        } else {
            color_row.push(Span::styled(
                format!(" {} ", n),
                Style::default().fg(theme.faint),
            ));
            color_row.push(Span::styled("●", Style::default().fg(hex_color(c))));
            color_row.push(Span::raw(" "));
        }
    }

    let lines = vec![
        Line::from(Span::styled(" 分类名称", Style::default().fg(theme.muted))),
        Line::from(Span::styled(
            format!(" > {}{}", m.buffer, if cursor_blink() { "▏" } else { " " }),
            Style::default().fg(theme.text),
        )),
        Line::from(Span::styled(" 选择颜色", Style::default().fg(theme.muted))),
        Line::from(color_row),
        Line::from(Span::styled(
            " Enter 创建 · Esc/Ctrl+C 取消 · ←/→ 或 1-8 选色",
            Style::default().fg(theme.faint),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn cursor_blink() -> bool {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_millis() < 500)
        .unwrap_or(true)
}

// ---------- 删除分类确认弹框 ----------

pub fn draw_del_cat_modal(f: &mut Frame, app: &App, theme: &Theme) {
    use ratatui::style::Modifier as M;
    let Some(m) = &app.del_cat_modal else { return };
    let danger = hex_color("#DE5B55");
    let area = popup_centered(f.area(), 54, 9);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(danger))
        .style(Style::default().bg(theme.panel))
        .title(" 删除分类 ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cat = app.snap.active_categories().find(|c| c.name == m.name);
    let dot_color = cat.map(|c| c.color.as_str()).unwrap_or("#888780");
    let task_count = app.snap.active_tasks().filter(|t| t.category == m.name).count();
    let pending = app.cat_pending(&m.name);

    let lines = vec![
        Line::from(vec![
            Span::styled(" 将删除分类  ", Style::default().fg(theme.muted)),
            Span::styled("●", Style::default().fg(hex_color(dot_color))),
            Span::styled(
                format!(" {} ", m.name),
                Style::default().fg(theme.text).add_modifier(M::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            format!(" 该分类下 {} 个任务（待办 {}）将移入「默认」", task_count, pending),
            Style::default().fg(theme.muted),
        )),
        Line::from(Span::styled(
            " 任务数据不会丢失 · 删除会随 WebDAV 同步",
            Style::default().fg(theme.faint),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled(" Enter", Style::default().fg(danger).add_modifier(M::BOLD)),
            Span::styled(" 确认删除", Style::default().fg(theme.muted)),
            Span::styled("  ·  ", Style::default().fg(theme.faint)),
            Span::styled("Esc", Style::default().fg(theme.accent).add_modifier(M::BOLD)),
            Span::styled("/n 取消", Style::default().fg(theme.muted)),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

// ---------- 优先级徽标 ----------

pub fn pri_badge(pri: u8, selected_bg: Option<ratatui::style::Color>, theme: &Theme) -> Vec<Span<'_>> {
    let Some(label) = priority_label(pri) else {
        return vec![];
    };
    let color = priority_color(pri).unwrap_or(theme.muted);
    let bg = selected_bg.unwrap_or(theme.panel);
    let fg = if selected_bg.is_some() { theme.panel } else { color };
    let dim = if priority_color(pri).is_some() {
        Style::default()
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };
    vec![
        Span::styled(" ●", Style::default().bg(bg).fg(color)),
        Span::styled(format!(" {} ", label), dim.fg(fg).bg(bg)),
    ]
}

use chrono::Datelike;
