//! 设置页：分组卡片（外观 / WebDAV 同步 / 系统 / 关于）。

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};
use ratatui::Frame;

use crate::app::{App, SyncStatus};
use crate::theme::Theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    use ratatui::layout::{Constraint, Layout};
    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(10),
        Constraint::Length(3),
        Constraint::Length(4),
    ])
    .split(area);

    // ---- 外观 ----
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(" 外观", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)))
        .padding(Padding::horizontal(2));
    let inner = block.inner(rows[0]);
    f.render_widget(block, rows[0]);
    let lines = vec![
        Line::from(vec![
            Span::styled("  主题        ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("[ {} ]", if theme.bg == crate::theme::LIGHT.bg { "薄荷绿" } else { "深夜" }),
                Style::default().bg(theme.accent).fg(theme.panel).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   按 t 切换", Style::default().fg(theme.faint)),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);

    // ---- WebDAV 同步 ----
    let cfg = &app.settings.webdav;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(" WebDAV 同步", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)))
        .padding(Padding::horizontal(2));
    let inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);
    let sync_label = if app.sync_status == SyncStatus::Syncing {
        "同步中…".to_string()
    } else {
        app.sync_status.label().to_string()
    };
    let field = |k: &str, v: String, key_hint: &str| {
        Line::from(vec![
            Span::styled(format!("  {:<12}", k), Style::default().fg(theme.muted)),
            Span::styled(v, Style::default().fg(theme.text)),
            Span::styled(format!("   {}", key_hint), Style::default().fg(theme.faint)),
        ])
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("  同步状态    ", Style::default().fg(theme.muted)),
            Span::styled(format!("● {}", sync_label), Style::default().fg(app.sync_status.color(theme))),
        ]),
        Line::from(vec![
            Span::styled("  启用同步    ", Style::default().fg(theme.muted)),
            Span::styled(
                if cfg.enabled { "[ 开 ]" } else { "[ 关 ]" },
                Style::default()
                    .fg(if cfg.enabled { theme.accent } else { theme.faint })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   按 e 切换", Style::default().fg(theme.faint)),
        ]),
        field(
            "服务器地址",
            if cfg.url.is_empty() { "（未配置）".into() } else { cfg.url.clone() },
            "按 u 编辑",
        ),
        field(
            "账号",
            if cfg.username.is_empty() { "（未配置）".into() } else { cfg.username.clone() },
            "按 n 编辑",
        ),
        field(
            "密码/令牌",
            if cfg.password.is_empty() { "（未配置）".into() } else { "••••••••".into() },
            "按 p 编辑",
        ),
        field("远端文件", crate::sync::remote_file_name(cfg), ""),
    ];
    lines.push(Line::from(Span::styled(
        "  配置文件 ~/.doit-tui/settings.json · Enter 修改即时保存 · 按 s 立即同步",
        Style::default().fg(theme.faint),
    )));
    f.render_widget(Paragraph::new(lines), inner);

    // ---- 系统 ----
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(" 系统", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)))
        .padding(Padding::horizontal(2));
    let inner = block.inner(rows[2]);
    f.render_widget(block, rows[2]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  数据目录    ", Style::default().fg(theme.muted)),
            Span::styled(
                crate::store::Store::new().dir().display().to_string(),
                Style::default().fg(theme.text),
            ),
        ])),
        inner,
    );

    // ---- 关于 ----
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(" 关于", Style::default().fg(theme.text).add_modifier(Modifier::BOLD)))
        .padding(Padding::horizontal(2));
    let inner = block.inner(rows[3]);
    f.render_widget(block, rows[3]);
    let lines = vec![
        Line::from(vec![
            Span::styled("  版本        ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("doit-tui v{} · 数据契约 SnapshotV2", env!("CARGO_PKG_VERSION")),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("  更新        ", Style::default().fg(theme.muted)),
            Span::styled(app.update_hint.clone(), Style::default().fg(theme.accent)),
            Span::styled(
                format!(" · {}", crate::update::releases_url()),
                Style::default().fg(theme.faint),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}
