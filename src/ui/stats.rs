//! 统计页：指标卡 + 今日进度行 + 热力图 + 折线图 + 环形图。

use std::collections::BTreeMap;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CLine, Points};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::model::completed_on;
use crate::theme::{hex_color, Theme};

use chrono::Datelike;

pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    use crate::app::HeatScope;
    use ratatui::layout::{Constraint, Layout};
    // 布局：切换胶囊 / 日期范围 / 图表行 / 底部行
    let rows = Layout::vertical([
        Constraint::Length(1),  // 周期切换（近一周/近一月/近一年）
        Constraint::Length(1),  // 日期范围（图外独立一行）
        Constraint::Length(12), // 热力图（+周/月时并列分类占比）
        Constraint::Length(16), //  趋势图（或 + 分类占比）
        Constraint::Min(0),
    ])
    .split(area);

    draw_period_tabs(f, app, rows[0], theme);
    // 日期范围行需知道热力图卡片宽度（53 周裁剪口径）
    let heat_w = if app.heat == HeatScope::Year {
        rows[2].width
    } else {
        (rows[2].width as u32 * 56 / 100) as u16
    };
    draw_period_range(f, app, rows[1], heat_w, theme);

    match app.heat {
        // 近一周 / 近一月：热力图(56%) + 分类占比(44%) 并排，趋势图全宽在底
        HeatScope::Week | HeatScope::Month => {
            let cols = Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
                .split(rows[2]);
            draw_heatmap(f, app, cols[0], theme);
            draw_donut(f, app, cols[1], theme);
            draw_trend(f, app, rows[3], theme);
        }
        // 近一年：热力图全屏宽度横幅，分类占比(44%) + 趋势图(56%) 并排在底
        HeatScope::Year => {
            draw_heatmap(f, app, rows[2], theme);
            let cols = Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)])
                .split(rows[3]);
            draw_donut(f, app, cols[0], theme);
            draw_trend(f, app, cols[1], theme);
        }
    }
}

/// 当前周期的日期范围（热力图卡片外的独立一行）。
fn period_range_text(app: &App, card_width: u16) -> String {
    use crate::app::HeatScope;
    let today = chrono::Local::now().date_naive();
    match app.heat {
        HeatScope::Week => {
            let mon =
                today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            let sun = mon + chrono::Duration::days(6);
            format!(
                "{} ~ {}（周{}）",
                mon.format("%m/%d"),
                sun.format("%m/%d"),
                wd_names()[today.weekday().num_days_from_monday() as usize]
            )
        }
        HeatScope::Month => format!("{} 年 {} 月", today.year(), today.month()),
        HeatScope::Year => {
            let weeks = year_weeks(today).2;
            // 与热力图同款裁剪口径：卡片内容区放不下 53 周时显示实际周数
            let usable = card_width.saturating_sub(6) as usize;
            if usable >= weeks {
                format!("{} 年（{} 周）", today.year(), weeks)
            } else {
                format!("{} 年（近 {} 周，共 {} 周）", today.year(), usable, weeks)
            }
        }
    }
}

fn wd_names() -> [&'static str; 7] {
    ["一", "二", "三", "四", "五", "六", "日"]
}

/// 本年周数信息：(周一对齐的起始日, 年末日, 周数)。
fn year_weeks(today: chrono::NaiveDate) -> (chrono::NaiveDate, chrono::NaiveDate, usize) {
    let y = today.year();
    let jan1 = chrono::NaiveDate::from_ymd_opt(y, 1, 1).unwrap();
    let off = jan1.weekday().num_days_from_monday() as i64;
    let start = jan1 - chrono::Duration::days(off);
    let end = chrono::NaiveDate::from_ymd_opt(y, 12, 31).unwrap();
    let total_days = off + (end - jan1).num_days() + 1;
    (start, end, ((total_days + 6) / 7) as usize)
}

fn draw_period_range(f: &mut Frame, app: &App, area: Rect, card_width: u16, theme: &Theme) {
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", period_range_text(app, card_width)),
            Style::default().fg(theme.faint),
        )))
        .style(Style::default().bg(theme.bg)),
        area,
    );
}

/// 周期起点（含当天）：本周一 / 本月 1 号 / 今年 1 月 1 号。
fn period_start(app: &App, today: chrono::NaiveDate) -> chrono::NaiveDate {
    use crate::app::HeatScope;
    match app.heat {
        HeatScope::Week => {
            today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64)
        }
        HeatScope::Month => chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap(),
        HeatScope::Year => chrono::NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap(),
    }
}

/// 周期切换胶囊：选中的高亮底色，←/→ 或 h/l 切换。
fn draw_period_tabs(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    use crate::app::HeatScope;
    let tabs = [
        (HeatScope::Week, "近一周"),
        (HeatScope::Month, "近一月"),
        (HeatScope::Year, "近一年"),
    ];
    let mut spans = vec![Span::raw(" ")];
    for (s, label) in tabs {
        if app.heat == s {
            spans.push(Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(theme.panel)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {} ", label),
                Style::default().fg(theme.muted),
            ));
        }
        spans.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        area,
    );
}

/// 周期切换胶囊（近一周/近一月/近一年）的可点击列区间，与 draw_period_tabs 同源计算。
/// 统计页第 0 行（全局第 2 行）。
pub fn period_tab_hits(app: &App) -> Vec<(crate::app::HeatScope, u16, u16)> {
    use crate::app::HeatScope;
    let scopes: [(HeatScope, &str); 3] = [
        (HeatScope::Week, "近一周"),
        (HeatScope::Month, "近一月"),
        (HeatScope::Year, "近一年"),
    ];
    let mut out = Vec::new();
    let mut x: u16 = 1; // 首行前置空格
    for (s, label) in scopes {
        let w = crate::ui::disp_w(&format!(" {} ", label)) as u16;
        out.push((s, x, w));
        x += w + 1; // 胶囊后随空格
    }
    out
}

fn completion_by_day(app: &App) -> BTreeMap<chrono::NaiveDate, u32> {
    use crate::model::Subtask;
    let mut map = BTreeMap::new();
    let bump = |ts: i64, map: &mut BTreeMap<chrono::NaiveDate, u32>| {
        if ts > 0 {
            *map.entry(completed_on(ts)).or_insert(0) += 1;
        }
    };
    fn walk(subs: &[Subtask], bump: &impl Fn(i64, &mut BTreeMap<chrono::NaiveDate, u32>), map: &mut BTreeMap<chrono::NaiveDate, u32>) {
        for s in subs {
            if let Some(c) = s.completed_at {
                bump(c, map);
            }
            walk(&s.subs, bump, map);
        }
    }
    for t in app.snap.active_tasks() {
        if let Some(c) = t.completed_at {
            bump(c, &mut map);
        }
        walk(&t.subs, &bump, &mut map);
    }
    map
}

/// 某日完成数（与热力图 completion_by_day 同口径，含任意层级子任务）。
pub fn day_completed(app: &App, d: chrono::NaiveDate) -> u32 {
    completion_by_day(app).get(&d).copied().unwrap_or(0)
}

/// 鼠标点击热力图格子 → 对应日期（三种周期各自与渲染同布局计算，列/行均为终端全局坐标）。
/// 命中「亮色点」（有完成的格子）时返回该日期，其余返回 None。
pub fn heat_day_at(app: &App, row: u16, col: u16) -> Option<chrono::NaiveDate> {
    use crate::app::HeatScope;
    use chrono::Datelike;

    let area_w = app.area.width;
    // 近一年为全屏宽度卡片，本周/本月为 56% 卡片
    let heat_w = if app.heat == HeatScope::Year {
        area_w
    } else {
        (area_w as u32 * 56 / 100) as u16
    };
    if col >= heat_w {
        return None;
    }
    // 内容区顶 2（顶栏+分类条）+ tabs(1) + range(1) + 卡片块内顶层 => 卡片 inner 顶
    let inner_top = 2 + 1 + 1 + 1; // 5
    let inner_h = 10u16;
    if row < inner_top {
        return None;
    }
    if row >= inner_top + inner_h {
        return None;
    }
    // 块左 border(1) + padding(2) → 文本起点
    let text_x = 3u16;

    let today = chrono::Local::now().date_naive();
    let (y, _m) = (today.year(), today.month());
    match app.heat {
        HeatScope::Week => {
            let pad = (inner_h - 2) / 2;
            if row != inner_top + pad {
                return None;
            }
            if col < text_x {
                return None;
            }
            let c = (col - text_x) / 2;
            if c > 6 {
                return None;
            }
            let mon = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            Some(mon + chrono::Duration::days(c as i64))
        }
        HeatScope::Month => {
            let first = chrono::NaiveDate::from_ymd_opt(y, _m, 1)?;
            let (ny, nm) = if _m == 12 { (y + 1, 1) } else { (y, _m + 1) };
            let dim = (chrono::NaiveDate::from_ymd_opt(ny, nm, 1)?.pred_opt()?).day() as usize;
            let off = first.weekday().num_days_from_monday() as usize;
            let n_rows = (off + dim + 6) / 7;
            let pad = (inner_h.saturating_sub(n_rows as u16)) / 2;
            let base_row = inner_top + pad;
            if row < base_row {
                return None;
            }
            let r = (row - base_row) as usize;
            if r >= n_rows || col < text_x + 2 {
                return None;
            }
            let c = ((col - text_x - 2) / 2) as usize;
            if c > 6 {
                return None;
            }
            let idx = r * 7 + c;
            if idx < off || idx >= off + dim {
                return None;
            }
            Some(first + chrono::Duration::days((idx - off) as i64))
        }
        HeatScope::Year => {
            let pad = (inner_h - 7) / 2;
            if row < inner_top + pad {
                return None;
            }
            let dow = (row - (inner_top + pad)) as usize;
            if dow > 6 || col < text_x {
                return None;
            }
            let (start, end, weeks) = year_weeks(today);
            let jan1 = chrono::NaiveDate::from_ymd_opt(y, 1, 1)?;
            // 与渲染同口径：全宽大格(2列/周)，放不下才退化窄格(1列/周)
            let inner_w = heat_w.saturating_sub(2);
            if inner_w.saturating_sub(4) as usize / 2 >= weeks {
                // 大格模式：每格 2 列
                let wk = ((col - text_x) / 2) as usize;
                if wk >= weeks {
                    return None;
                }
                let d = start + chrono::Duration::days((wk * 7 + dow) as i64);
                if d < jan1 || d > end {
                    return None;
                }
                Some(d)
            } else {
                // 窄格模式：每格 1 列，裁掉最早的周
                let usable = inner_w.saturating_sub(4) as usize;
                let start_wk = weeks.saturating_sub(usable).min(weeks);
                let drawn = (col - text_x) as usize;
                if drawn >= usable {
                    return None;
                }
                let wk = start_wk + drawn;
                if wk >= weeks {
                    return None;
                }
                let d = start + chrono::Duration::days((wk * 7 + dow) as i64);
                if d < jan1 || d > end {
                    return None;
                }
                Some(d)
            }
        }
    }
}


/// 方格统一用半高块 `▄ `：实心 1 字符见方 + 1 空格空隙，横纵间距物理相等；
/// 本年 53 周占 2 列/周，宽度不足时自动裁掉最早的周。
fn draw_heatmap(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    use crate::app::HeatScope;
    let scale: [Color; 5] = [
        theme.border, // 0 次 / 未来：空格底色，随主题变化
        Color::Rgb(0xB6, 0xDC, 0xC2),
        Color::Rgb(0x8B, 0xC7, 0xA3),
        Color::Rgb(0x5F, 0xB0, 0x82),
        Color::Rgb(0x3E, 0x9B, 0x6E),
    ];
    // 标题 = 卡片名 + 少/多图例（周期在顶部切换胶囊上显示）
    let mut title_spans = vec![Span::styled(
        " 完成热力图 ",
        Style::default().fg(theme.muted),
    )];
    title_spans.push(Span::styled(" 少 ", Style::default().fg(theme.faint)));
    for c in scale {
        title_spans.push(Span::styled("▄ ", Style::default().fg(c)));
    }
    title_spans.push(Span::styled("多 ", Style::default().fg(theme.faint)));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Line::from(title_spans));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let map = completion_by_day(app);
    let today = chrono::Local::now().date_naive();
    let wd_names = wd_names();

    let cell = |d: chrono::NaiveDate, sym: &'static str| -> Span<'static> {
        let n = map.get(&d).copied().unwrap_or(0);
        let level = if d > today || n == 0 {
            0
        } else {
            match n {
                1 => 1,
                2..=3 => 2,
                4..=5 => 3,
                _ => 4,
            }
        };
        Span::styled(sym.to_string(), Style::default().fg(scale[level]))
    };

    let (y, _m, _d) = (today.year(), today.month(), today.day());
    let mut rows: Vec<Line> = Vec::new();
    match app.heat {
        HeatScope::Week => {
            let mon = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            let mut spans = Vec::new();
            for i in 0..7 {
                spans.push(cell(mon + chrono::Duration::days(i), "▄ "));
            }
            rows.push(Line::from(spans));
            // 星期标签与方格同列：每格 2 列（字 + 空隙），对齐方格位置
            let mut labels = Vec::new();
            for name in wd_names {
                labels.push(Span::styled(
                    format!("{} ", name),
                    Style::default().fg(theme.faint),
                ));
            }
            rows.push(Line::from(labels));
        }
        HeatScope::Month => {
            let first = chrono::NaiveDate::from_ymd_opt(y, _m, 1).unwrap();
            let (ny, nm) = if _m == 12 { (y + 1, 1) } else { (y, _m + 1) };
            let dim =
                (chrono::NaiveDate::from_ymd_opt(ny, nm, 1).unwrap() - chrono::Duration::days(1))
                    .day() as usize;
            let off = first.weekday().num_days_from_monday() as usize;
            let n_rows = (off + dim + 6) / 7;
            for r in 0..n_rows {
                let mut spans = vec![Span::styled(
                    format!("{} ", wd_names[r]),
                    Style::default().fg(theme.faint),
                )];
                for c in 0..7 {
                    let idx = r * 7 + c;
                    if idx < off || idx >= off + dim {
                        spans.push(Span::raw("  "));
                    } else {
                        spans.push(cell(first + chrono::Duration::days(idx as i64), "▄ "));
                    }
                }
                rows.push(Line::from(spans));
            }
        }
        HeatScope::Year => {
            let (start, end, weeks) = year_weeks(today);
            let jan1 = chrono::NaiveDate::from_ymd_opt(y, 1, 1).unwrap();

            // 近一年热力图：全屏宽度，53 周 × 2 列用大格 `▄ `（横纵等距）。
            // 只有终端极窄放不下时（占 2 列/周），才退化为 1 列小格 `▪`。
            let usable2 = inner.width.saturating_sub(4) as usize / 2;
            let wide = usable2 >= weeks;
            let (start_wk, sym) = if wide {
                (0usize, "▄ ")
            } else {
                let u1 = inner.width.saturating_sub(4) as usize;
                (weeks.saturating_sub(u1).min(weeks), "▪")
            };
            for dow in 0..7usize {
                let mut spans = Vec::new();
                for wk in start_wk..weeks {
                    let d = start + chrono::Duration::days((wk * 7 + dow) as i64);
                    if d < jan1 || d > end {
                        if wide {
                            spans.push(Span::raw("  "));
                        } else {
                            spans.push(Span::raw(" "));
                        }
                    } else {
                        spans.push(cell(d, sym));
                    }
                }
                rows.push(Line::from(spans));
            }
        }
    }

    // 垂直居中
    let mut lines: Vec<Line> = Vec::new();
    let pad = inner.height.saturating_sub(rows.len() as u16) / 2;
    for _ in 0..pad {
        lines.push(Line::from(""));
    }
    lines.extend(rows);
    f.render_widget(Paragraph::new(lines).block(
        Block::new().padding(ratatui::widgets::Padding::horizontal(2)),
    ), inner);
}

/// 分类占比（区间内完成的任务）：随顶部周期切换联动。
fn draw_donut(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(
            format!(" 分类占比（{}完成）", app.heat.label()),
            Style::default().fg(theme.muted),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width <= 16 || inner.height <= 4 {
        return;
    }

    let today = chrono::Local::now().date_naive();
    let start = period_start(app, today);
    let is_in_period = |ts: i64| ts > 0 && completed_on(ts) >= start;

    // 图例：只展示「当前仍存在」且「有内容（区间有完成或有未完成）」的分类；
    // 已删除分类不在 categories 列表、空壳分类（无完成也无待办）一并隐藏；
    // 「默认」仅在确有区间完成但无法归属到现存分类时出现。
    let colors: Vec<(String, Color, usize)> = {
        let mut v: Vec<(String, Color, usize, usize)> = app
            .snap
            .categories
            .iter()
            .map(|c| {
                let n = app
                    .snap
                    .tasks
                    .iter()
                    .filter(|t| {
                        t.category == c.name
                            && t.completed_at.map(is_in_period).unwrap_or(false)
                    })
                    .count();
                let pending = app.snap.tasks.iter().filter(|t| t.category == c.name && !t.done).count();
                (c.name.clone(), hex_color(&c.color), n, pending)
            })
            .collect();
        v.retain(|(_, _, n, p)| *n > 0 || *p > 0);
        let mut out: Vec<(String, Color, usize)> =
            v.into_iter().map(|(n, c, cnt, _)| (n, c, cnt)).collect();
        let known: usize = out.iter().map(|(_, _, n)| *n).sum();
        let rest = app
            .snap
            .tasks
            .iter()
            .filter(|t| t.completed_at.map(is_in_period).unwrap_or(false))
            .count()
            .saturating_sub(known);
        if rest > 0 {
            out.push(("默认".into(), theme.faint, rest));
        }
        out
    };
    let total: usize = colors.iter().map(|(_, _, n)| *n).sum();

    // donut canvas：按终端格子纵横比（1 格宽 ≈ 2 格高）修正 y 量程画正圆
    let (cw, ch) = (14.0_f64, inner.height as f64);
    let r = 4.0_f64;
    let x_span = 2.0 * r + 1.2;
    let y_span = x_span * 2.0 * ch / cw;
    f.render_widget(
        Canvas::default()
            .background_color(theme.panel)
            .x_bounds([-x_span / 2.0, x_span / 2.0])
            .y_bounds([-y_span / 2.0, y_span / 2.0])
            .paint(|ctx| {
                if total == 0 {
                    // 区间无完成：灰色空环
                    let steps = 125usize;
                    let pts: Vec<(f64, f64)> = (0..=steps)
                        .map(|s| {
                            let a = s as f64 * (std::f64::consts::TAU / steps as f64);
                            (a.cos() * r, a.sin() * r)
                        })
                        .collect();
                    ctx.draw(&Points { coords: &pts, color: theme.border });
                    return;
                }
                let mut start_a = 0.0f64;
                let seg = std::f64::consts::TAU / total as f64;
                for (_, color, n) in colors.iter() {
                    if *n == 0 {
                        continue; // 0 完成的分类不画段，只在图例出现
                    }
                    let end_a = start_a + seg * *n as f64;
                    let steps = ((end_a - start_a) / 0.02).max(2.0) as usize;
                    for s in 0..=steps {
                        let a = start_a + (end_a - start_a) * (s as f64 / steps as f64);
                        ctx.draw(&Points {
                            coords: &[
                                (a.cos() * r, a.sin() * r),
                                (a.cos() * (r - 0.6), a.sin() * (r - 0.6)),
                            ],
                            color: *color,
                        });
                    }
                    start_a = end_a;
                }
            }),
        Rect { width: 14, height: inner.height, ..inner },
    );

    // 图例（画布右侧垂直居中）
    let legend_area = Rect {
        x: inner.x + 14,
        y: inner.y,
        width: inner.width.saturating_sub(14),
        height: inner.height,
        ..inner
    };
    let mut lines: Vec<Line> = Vec::new();
    let pad = legend_area.height.saturating_sub(colors.len().max(1) as u16) / 2;
    for _ in 0..pad {
        lines.push(Line::from(""));
    }
    if colors.is_empty() {
        lines.push(Line::from(Span::styled(
            "该区间暂无完成",
            Style::default().fg(theme.faint),
        )));
    }
    for (name, color, n) in &colors {
        let pct = if total > 0 { 100 * n / total } else { 0 };
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(*color)),
            Span::styled(
                format!("{}  {} ({}%)", name, n, pct),
                Style::default().fg(theme.muted),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines), legend_area);
}

/// 完成趋势折线图（全宽）：随周期切换——本周/本月按天，近一年按月。
fn draw_trend(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    use crate::app::HeatScope;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(
            match app.heat {
                HeatScope::Week => " 每日完成趋势（本周）",
                HeatScope::Month => " 每日完成趋势（本月）",
                HeatScope::Year => " 每月完成趋势（近 12 个月）",
            },
            Style::default().fg(theme.muted),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width <= 8 || inner.height <= 3 {
        return;
    }

    let map = completion_by_day(app);
    let today = chrono::Local::now().date_naive();

    // 数据点：首/末标签 + 值序列
    let (first_lbl, last_lbl, vals): (String, String, Vec<u32>) = match app.heat {
        HeatScope::Week => {
            let mon =
                today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            (
                mon.format("%m/%d").to_string(),
                (mon + chrono::Duration::days(6)).format("%m/%d").to_string(),
                (0..7)
                    .map(|i| {
                        map.get(&(mon + chrono::Duration::days(i)))
                            .copied()
                            .unwrap_or(0)
                    })
                    .collect(),
            )
        }
        HeatScope::Month => {
            let first = chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
            let (ny, nm) = if today.month() == 12 {
                (today.year() + 1, 1)
            } else {
                (today.year(), today.month() + 1)
            };
            let dim = (chrono::NaiveDate::from_ymd_opt(ny, nm, 1).unwrap()
                - chrono::Duration::days(1))
            .day() as i64;
            (
                first.format("%m/%d").to_string(),
                format!("{:02}/{:02}", today.month(), dim),
                (0..dim)
                    .map(|i| {
                        map.get(&(first + chrono::Duration::days(i)))
                            .copied()
                            .unwrap_or(0)
                    })
                    .collect(),
            )
        }
        HeatScope::Year => {
            // 近 12 个月（含当月），按月聚合
            let mut by_month: BTreeMap<(i32, u32), u32> = BTreeMap::new();
            for (d, n) in &map {
                *by_month.entry((d.year(), d.month())).or_insert(0) += *n;
            }
            let mut vals = Vec::new();
            for k in (0..12u32).rev() {
                let (yy, mm) = if k >= today.month() as u32 {
                    (today.year() - 1, today.month() as u32 + 12 - k)
                } else {
                    (today.year(), today.month() as u32 - k)
                };
                vals.push(by_month.get(&(yy, mm)).copied().unwrap_or(0));
            }
            (
                format!("{}-{:02}", today.year() - 1, today.month()),
                format!("{}-{:02}", today.year(), today.month()),
                vals,
            )
        }
    };

    let n = vals.len();
    let max = vals.iter().copied().max().unwrap_or(0).max(1) as f64;
    let (w, h) = (inner.width as f64, inner.height as f64);
    // 纵轴收敛：每 1 次完成最多占 2.5 行，不随区域高度拉伸
    let unit = ((h - 2.0) / max).min(2.5).max(0.5);
    let pt = |i: usize| -> (f64, f64) {
        let x = 1.5 + i as f64 * ((w - 4.0) / (n.max(2) - 1) as f64);
        let y = 0.8 + vals[i] as f64 * unit;
        (x, y)
    };
    f.render_widget(
        Canvas::default()
            // Canvas 默认把画布背景重置为终端默认色，浅色主题下会露黑底
            .background_color(theme.panel)
            .x_bounds([0.0, w])
            .y_bounds([0.0, h])
            .paint(|ctx| {
                for i in 0..n - 1 {
                    let (x1, y1) = pt(i);
                    let (x2, y2) = pt(i + 1);
                    ctx.draw(&CLine { x1, y1, x2, y2, color: theme.accent });
                }
                let pts: Vec<(f64, f64)> = (0..n).map(pt).collect();
                ctx.draw(&Points { coords: &pts, color: theme.accent });
            }),
        inner,
    );
    // 轴标签
    let lbl = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
        ..inner
    };
    let half = inner.width as usize / 2;
    f.render_widget(
        Paragraph::new(format!(
            "{:<fw$}{:>lw$}",
            first_lbl,
            last_lbl,
            fw = half,
            lw = inner.width as usize - half
        ))
        .style(Style::default().fg(theme.faint)),
        lbl,
    );
}
