//! 今日页：卡片式任务列表（父子独立卡片）+ 输入条。

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::{App, InputMode};
use crate::theme::{hex_color, Theme};
use crate::ui::{disp_w, ellipsize, pri_badge, ring_char};

/// 卡片高度（上边框 + 内容 + 下边框）与卡片间空行。
pub const CARD_H: u16 = 3;
pub const CARD_GAP: u16 = 1;
pub const SUB_INDENT: u16 = 4;

pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let rows = app.visible_ids();
    let mut scroll = app.list_scroll as usize;
    let n_rows = rows.len();
    let slot = CARD_H + CARD_GAP;
    let max_visible = (area.height / slot) as usize;

    // 让选中卡片保持可见
    if let Some(sel_idx) = rows.iter().position(|(id, _)| Some(id) == app.sel.as_ref()) {
        if sel_idx < scroll {
            scroll = sel_idx;
        }
        if sel_idx >= scroll + max_visible && max_visible > 0 {
            scroll = sel_idx + 1 - max_visible;
        }
    }
    scroll = scroll.min(n_rows.saturating_sub(max_visible));

    let mut y = area.y;
    let bottom = area.y + area.height;
    for (i, (id, depth)) in rows.iter().enumerate() {
        if i < scroll {
            continue;
        }
        if y + CARD_H > bottom {
            break;
        }
        let r = Rect { x: area.x, y, width: area.width, height: CARD_H };
        let indent = *depth as u16 * SUB_INDENT;
        let r = Rect {
            x: area.x + indent,
            width: area.width.saturating_sub(indent + 1),
            ..r
        };
        draw_card(f, app, id, *depth, r, theme);
        y += slot;
    }

    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " （此分类暂无任务 · 按 a 新增）",
                Style::default().fg(theme.faint),
            )))
            .style(Style::default().bg(theme.bg)),
            area,
        );
    }

    // 空余区域补滚动提示
    if n_rows > max_visible {
        let hint = format!(" j/k 滚动 · {} / {} ", scroll + 1, n_rows);
        let hr = Rect { x: area.x, y: bottom.saturating_sub(1).min(area.y + area.height), width: area.width, height: 0 };
        let _ = hr;
        let _ = hint;
    }
}

/// 任意识别：返回 (标题, 优先级, 完成, 深度, 分类, 该节点子任务)。
fn find<'a>(
    app: &'a App,
    id: &str,
) -> Option<(
    String,
    u8,
    bool,
    u8,
    &'a str,
    &'a [crate::model::Subtask],
)> {
    use crate::model::find_sub;
    for t in &app.snap.tasks {
        if t.id == id {
            return Some((t.title.clone(), t.priority, t.done, 0, &t.category, &t.subs));
        }
        if let (Some(s), cat) = (find_sub(&t.subs, id), &t.category) {
            let depth = depth_of(&t.subs, id);
            return Some((s.title.clone(), s.priority, s.done, depth, cat, &s.subs));
        }
    }
    None
}

/// 计算子任务在树中的深度（顶级子任务=1）。
fn depth_of(subs: &[crate::model::Subtask], id: &str) -> u8 {
    fn go(subs: &[crate::model::Subtask], id: &str, depth: u8) -> Option<u8> {
        for s in subs {
            if s.id == id {
                return Some(depth);
            }
            if let Some(d) = go(&s.subs, id, depth + 1) {
                return Some(d);
            }
        }
        None
    }
    go(subs, id, 1).unwrap_or(1)
}

fn draw_card(f: &mut Frame, app: &App, id: &str, depth: u8, area: Rect, theme: &Theme) {
    let Some((title, pri, done, _depth, cat, subs)) = find(app, id) else {
        return;
    };
    let selected = app.sel.as_deref() == Some(id);
    let border_color = if selected { theme.selected } else { theme.border };

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme.panel))
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // 右侧 meta（先构建以便右对齐）：优先级徽标 → 分类 → 进度环 → 折叠箭头
    let mut meta: Vec<Span> = Vec::new();
    if depth > 0 {
        for s in pri_badge(pri, None, theme) {
            meta.push(s);
        }
    } else {
        if pri > 0 {
            for s in pri_badge(pri, None, theme) {
                meta.push(s);
            }
        }
        let cat_color = app
            .snap
            .categories
            .iter()
            .find(|c| c.name == cat)
            .map(|c| hex_color(&c.color))
            .unwrap_or(theme.faint);
        meta.push(Span::raw("  "));
        meta.push(Span::styled("●", Style::default().fg(cat_color)));
        meta.push(Span::styled(
            format!(" {}", ellipsize(cat, 6)),
            Style::default().fg(theme.muted),
        ));
    }
    if !subs.is_empty() {
        let done_n = crate::model::done_leaf_count(subs);
        let total_n = crate::model::leaf_count(subs);
        let pct = if total_n > 0 { 100 * done_n / total_n } else { 0 };
        meta.push(Span::styled(
            format!("  {}{}%", ring_char(pct), pct),
            Style::default().fg(if pct == 100 { theme.accent } else { theme.muted }),
        ));
        let expanded = app.expanded.iter().any(|e| e == id);
        meta.push(Span::styled(
            if expanded { " ▲" } else { " ▼" },
            Style::default().fg(theme.faint),
        ));
    }
    let meta_w: usize = meta.iter().map(|s| disp_w(&s.content)).sum();

    // 左侧：选中标记 + 勾选框
    let inner_w = inner.width as usize;
    let left_w = 3 + 2; // 标记(3) + 勾选框(2)
    // 标题宽度 = 剩余空间（至少留 1 空隙，最少 6 列）
    let title_w = inner_w.saturating_sub(left_w + meta_w + 1).max(6).min(40);
    let title_style = if done {
        Style::default()
            .fg(theme.faint)
            .add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(theme.text)
    };
    let mut spans: Vec<Span> = Vec::new();
    if selected {
        spans.push(Span::styled(
            " > ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::raw("   "));
    }
    if done {
        spans.push(Span::styled(
            "✔ ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        // ◯（U+25EF 大圆圈）比 ○ 占格更满，加粗后视觉更大
        spans.push(Span::styled(
            "◯ ",
            Style::default()
                .fg(theme.faint)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(ellipsize(&title, title_w), title_style));
    // 中间填充到右对齐
    let used = left_w + disp_w(&ellipsize(&title, title_w)) + meta_w;
    if used < inner_w {
        spans.push(Span::raw(" ".repeat(inner_w - used)));
    }
    spans.extend(meta);

    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// 输入条（新增/编辑），位于内容区与键位栏之间。
pub fn draw_input_bar(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let Some(inp) = &app.input else { return };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.bg))
        .padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans: Vec<Span> = Vec::new();
    let mut hint = match inp.mode {
        InputMode::AddTask => "Enter 创建 · Tab 换分类 · Ctrl+P 优先级 · Esc/Ctrl+C 取消 ",
        InputMode::AddSub => "Enter 创建 · Ctrl+P 优先级 · Esc/Ctrl+C 取消 ",
        InputMode::Edit => "Enter 保存 · Esc/Ctrl+C 取消 ",
        _ => "Enter 保存配置 · Esc/Ctrl+C 取消 ",
    };
    match inp.mode {
        InputMode::WebdavUrl | InputMode::WebdavUser | InputMode::WebdavPass => {
            let label = match inp.mode {
                InputMode::WebdavUrl => " 服务器地址 ",
                InputMode::WebdavUser => " 账号 ",
                _ => " 密码/令牌 ",
            };
            spans.push(Span::styled(
                label,
                Style::default()
                    .bg(theme.accent)
                    .fg(theme.panel)
                    .add_modifier(Modifier::BOLD),
            ));
            // 密码输入打码显示
            let shown = if inp.mode == InputMode::WebdavPass && !inp.buffer.is_empty() {
                "•".repeat(inp.buffer.chars().count())
            } else {
                inp.buffer.clone()
            };
            let placeholder = match inp.mode {
                InputMode::WebdavUrl => "https://dav.example.com/dav/…",
                InputMode::WebdavUser => "WebDAV 用户名或邮箱",
                _ => "应用密码或访问令牌",
            };
            let text = if inp.buffer.is_empty() {
                Span::styled(placeholder.to_string(), Style::default().fg(theme.faint))
            } else {
                Span::styled(shown, Style::default().fg(theme.text))
            };
            spans.push(Span::raw(" "));
            spans.push(text);
            spans.push(Span::styled(" ▏", Style::default().fg(theme.accent)));
        }
        InputMode::AddTask | InputMode::AddSub => {
            // 左侧优先级徽标（可切换）
            for s in pri_badge(inp.pri, None, theme) {
                spans.push(s);
            }
            spans.push(Span::raw(" "));
            let placeholder = if inp.mode == InputMode::AddSub {
                "添加子任务，回车创建…"
            } else {
                "添加待办，回车快速创建…"
            };
            let text = if inp.buffer.is_empty() {
                Span::styled(placeholder.to_string(), Style::default().fg(theme.faint))
            } else {
                Span::styled(inp.buffer.clone(), Style::default().fg(theme.text))
            };
            spans.push(text);
            spans.push(Span::styled(" ▏", Style::default().fg(theme.accent)));
            if inp.mode == InputMode::AddTask {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[{}▾]", ellipsize(&inp.cat, 6)),
                    Style::default().fg(theme.muted),
                ));
            }
        }
        InputMode::Edit => {
            spans.push(Span::styled(
                " 编辑 ",
                Style::default()
                    .bg(theme.accent)
                    .fg(theme.panel)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {} ▏", inp.buffer),
                Style::default().fg(theme.text),
            ));
        }
    }

    // 右对齐操作提示（宽度不够时逐级精简）
    let width = inner.width as usize;
    let used: usize = spans.iter().map(|s| disp_w(&s.content)).sum();
    if used + disp_w(hint) > width {
        hint = "Ctrl+C 取消 ";
    }
    let used2: usize = spans.iter().map(|s| disp_w(&s.content)).sum();
    if used2 + disp_w(hint) <= width {
        let pad = width - used2 - disp_w(hint);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(hint, Style::default().fg(theme.faint)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

pub fn input_bar_height(app: &App) -> u16 {
    if app.input.is_some() { 3 } else { 0 }
}
