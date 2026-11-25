//! 月历页：月视图网格（选中日高亮 + 当日完成数）+ 底部选中日任务面板。
//! 键位：h/l 前后一天 · j/k 上一行/下一行（±7 天）· ←/→ 翻月 · g 回到今天。

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::{hex_color, Theme};
use crate::ui::{ellipsize, zh_weekday};

use chrono::Datelike;

pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    use ratatui::layout::{Constraint, Layout};
    let rows = Layout::vertical([
        Constraint::Length(15), // 月历网格（表头 + 至多 6 周 × 2 行：日期行+圆点行）
        Constraint::Min(3),     // 选中日完成记录面板
    ])
    .split(area);
    draw_grid(f, app, rows[0], theme);
    draw_day_panel(f, app, rows[1], theme);
}

/// 鼠标点击月历网格 → 日期（与 draw_grid 同布局：内容区第 2 行开始，块上边框 1 行，表头 1 行；
/// 每格 5 列宽，padding 3；每格占 2 行：日期行 + 圆点行）。
pub fn day_at(app: &App, row: u16, col: u16) -> Option<chrono::NaiveDate> {
    use chrono::Datelike;
    let (year, month) = app.cal_month;
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1)?;
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let dim = (chrono::NaiveDate::from_ymd_opt(ny, nm, 1)?.pred_opt()?).day() as i32;
    let off = first.weekday().num_days_from_monday() as usize;

    // 布局：内容区顶(2) + 网格块上边框(1) = inner 顶(3)；表头 1 行(3)；日期从 4 起
    if row < 4 {
        return None;
    }
    let rel = row - 4;
    if rel > 11 {
        return None; // 最多 6 周 × 2 行
    }
    let week = rel / 2;
    if col < 4 {
        return None; // 块左 border(1) + padding(3)
    }
    let wd = (col - 4) / 5;
    if wd > 6 {
        return None;
    }
    let idx = week as usize * 7 + wd as usize;
    if idx < off {
        return None;
    }
    let day = idx - off + 1;
    if day as i32 > dim {
        return None;
    }
    Some(first + chrono::Duration::days((day - 1) as i64))
}

fn draw_grid(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let (year, month) = app.cal_month;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(
            format!(
                " {}年{}月 · 选中 {}日 ",
                year,
                month,
                app.cal_sel.day()
            ),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " h/l 选日 · j/k 行 · ←/→ 翻月 · g 今天 ",
            Style::default().fg(theme.faint),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    let last_day: i32 = {
        let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        (chrono::NaiveDate::from_ymd_opt(ny, nm, 1).unwrap() - chrono::Duration::days(1))
            .day() as i32
    };
    let offset = first.weekday().num_days_from_monday() as usize;

    let done_map = {
        use crate::model::Subtask;
        use std::collections::BTreeMap;
        fn walk(subs: &[Subtask], m: &mut BTreeMap<chrono::NaiveDate, u32>) {
            for s in subs {
                if let Some(c) = s.completed_at {
                    let d = crate::model::completed_on(c);
                    *m.entry(d).or_insert(0) += 1;
                }
                walk(&s.subs, m);
            }
        }
        let mut m: BTreeMap<chrono::NaiveDate, u32> = BTreeMap::new();
        for t in app.snap.active_tasks() {
            if let Some(c) = t.completed_at {
                let d = crate::model::completed_on(c);
                *m.entry(d).or_insert(0) += 1;
            }
            walk(&t.subs, &mut m);
        }
        m
    };

    let today = chrono::Local::now().date_naive();
    let mut lines = Vec::new();

    // 表头（周一开头）
    let head: Vec<Span> = ["一", "二", "三", "四", "五", "六", "日"]
        .iter()
        .map(|w| Span::styled(format!(" {:>3} ", w), Style::default().fg(theme.muted)))
        .collect();
    lines.push(Line::from(head));

    let mut day = 1i32 - offset as i32;
    let n_rows = (offset + last_day as usize + 6) / 7;
    for _ in 0..n_rows {
        let mut num_spans = Vec::new();
        let mut dot_spans = Vec::new();
        for _ in 0..7 {
            if day < 1 || day > last_day as i32 {
                num_spans.push(Span::raw("     "));
                dot_spans.push(Span::raw("     "));
                day += 1;
                continue;
            }
            let d = first + chrono::Duration::days(day as i64 - 1);
            let n = done_map.get(&d).copied().unwrap_or(0);
            let selected = d == app.cal_sel;
            let num_style = if selected {
                Style::default()
                    .bg(theme.accent)
                    .fg(theme.panel)
                    .add_modifier(Modifier::BOLD)
            } else if d == today {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            num_spans.push(Span::styled(format!(" {:>3} ", d.day()), num_style));
            // 圆点行：有完成任务的日期，在日期正下方显示一个点（不显示数量）
            if n > 0 {
                let dot_style = if selected {
                    Style::default().fg(theme.panel)
                } else {
                    Style::default().fg(theme.accent)
                };
                dot_spans.push(Span::styled("  •  ", dot_style));
            } else {
                dot_spans.push(Span::raw("     "));
            }
            day += 1;
        }
        lines.push(Line::from(num_spans));
        lines.push(Line::from(dot_spans));
    }

    f.render_widget(
        Paragraph::new(lines).block(Block::new().padding(ratatui::widgets::Padding::horizontal(3))),
        inner,
    );
}

/// 选中日任务面板：只展示当天完成的记录（未完成 / 当天新增不显示）。
fn draw_day_panel(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let (m, d) = (app.cal_sel.month(), app.cal_sel.day());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(
            format!(" {} 月 {} 日 {} · 完成记录 ", m, d, zh_weekday(app.cal_sel.weekday())),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ))
        .padding(ratatui::widgets::Padding::horizontal(2));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (done_all, mut lines) = day_items(app, app.cal_sel, theme);

    if done_all == 0 {
        lines.push(Line::from(Span::styled(
            " 当天无完成记录",
            Style::default().fg(theme.faint),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// 该日期完成的记录：(条数, 展示行)。只统计当天真正完成的（父任务整体完成或任意层级子任务完成）。
fn day_items<'a>(app: &'a App, day: chrono::NaiveDate, theme: &'a Theme) -> (usize, Vec<Line<'a>>) {
    let mut lines: Vec<Line> = Vec::new();
    let mut done_all = 0usize;

    for t in app.snap.active_tasks() {
        let cat_color = app
            .snap
            .categories
            .iter()
            .find(|c| c.name == t.category)
            .map(|c| hex_color(&c.color))
            .unwrap_or(theme.faint);
        let top_done = t.completed_at.map(|c| crate::model::completed_on(c) == day).unwrap_or(false);
        let sub_done = collect_done_on(&t.subs, day);

        if !top_done && sub_done.is_empty() {
            continue;
        }
        if top_done {
            done_all += 1;
            lines.push(Line::from(vec![
                Span::styled(" ✔ ", Style::default().fg(theme.accent)),
                Span::styled(format!("{:<22}", ellipsize(&t.title, 22)), Style::default().fg(theme.text)),
                Span::styled("●", Style::default().fg(cat_color)),
                Span::styled(format!(" {}", ellipsize(&t.category, 8)), Style::default().fg(theme.muted)),
            ]));
        }
        for s_title in sub_done {
            done_all += 1;
            lines.push(Line::from(vec![
                Span::styled("     ✔ ", Style::default().fg(theme.faint)),
                Span::styled(format!("{:<18}", ellipsize(&s_title, 18)), Style::default().fg(theme.text)),
                Span::styled("· 子任务", Style::default().fg(theme.faint)),
            ]));
        }
    }
    (done_all, lines)
}

/// 递归收集该日期完成的子任务标题。
fn collect_done_on(subs: &[crate::model::Subtask], day: chrono::NaiveDate) -> Vec<String> {
    let mut out = Vec::new();
    for s in subs {
        if let Some(c) = s.completed_at
            && crate::model::completed_on(c) == day
        {
            out.push(s.title.clone());
        }
        out.extend(collect_done_on(&s.subs, day));
    }
    out
}
