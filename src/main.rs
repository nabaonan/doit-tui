//! doit-tui 入口：终端初始化 / 事件循环 / 全局键位。

mod app;
mod model;
mod store;
mod sync;
mod theme;
mod ui;

use app::{App, InputMode, Page, SyncStatus};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use store::Store;

use crate::theme::{Theme, ThemeKind};

fn main() -> anyhow::Result<()> {
    let store = Store::new();
    let mut app = App::new(&store);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &store);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    store: &Store,
) -> anyhow::Result<()> {
    while !app.quit {
        app.expire_toast();
        terminal.draw(|f| draw(f, app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    on_key(key, app, store);
                }
            }
        }
    }
    Ok(())
}

fn theme_of(app: &App) -> Theme {
    if app.settings.theme == "dark" {
        ThemeKind::Dark.theme()
    } else {
        ThemeKind::Light.theme()
    }
}

fn draw(f: &mut Frame, app: &App) {
    let theme = theme_of(app);
    let bg = Style::default().bg(theme.bg);
    f.render_widget(Paragraph::new("").style(bg), f.area());

    let input_h = ui::today::input_bar_height(app);
    let rows = Layout::vertical([
        Constraint::Length(1), // 顶栏（导航 + 胶囊）
        Constraint::Length(1), // 分类条
        Constraint::Min(0),    // 内容区
        Constraint::Length(input_h),
        Constraint::Length(1), // 键位栏
    ])
    .split(f.area());

    ui::draw_topbar(f, app, rows[0], &theme);
    ui::draw_catbar(f, app, rows[1], &theme);

    match app.page {
        Page::Today => ui::today::draw(f, app, rows[2], &theme),
        Page::Stats => ui::stats::draw(f, app, rows[2], &theme),
        Page::Calendar => ui::calendar::draw(f, app, rows[2], &theme),
        Page::Settings => ui::settings::draw(f, app, rows[2], &theme),
    }

    if app.input.is_some() {
        ui::today::draw_input_bar(f, app, rows[3], &theme);
    }
    ui::draw_footer(f, app, rows[4], &theme);
    ui::draw_toast(f, app, f.area(), &theme);

    if app.cat_modal.is_some() {
        ui::draw_cat_modal(f, app, &theme);
    }
    if app.del_cat_modal.is_some() {
        ui::draw_del_cat_modal(f, app, &theme);
    }
    if app.help_open {
        ui::draw_help(f, &theme);
    }
}

fn on_key(key: KeyEvent, app: &mut App, store: &Store) {
    // Ctrl+C：优先关闭当前打开的弹层（输入条/弹框/帮助）；没有弹层时退出程序。
    // 必须最先处理，否则会被下方 Char('c') 分支误认成"新建分类"。
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.input.is_some() {
            app.close_input();
        } else if app.cat_modal.is_some() {
            app.close_cat_modal();
        } else if app.del_cat_modal.is_some() {
            app.close_del_cat_modal();
        } else if app.help_open {
            app.help_open = false;
        } else {
            app.quit = true;
        }
        return;
    }
    // 其余 Ctrl 组合键一律忽略（Ctrl+P 例外，交给输入条处理），
    // 防止 Ctrl+A/D/E/P 等误触发单字母快捷键（如误删任务）。
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('p') if app.input.is_some() => {}
            KeyCode::Char(_) => return,
            _ => {}
        }
    }
    // 删除分类确认弹框：Enter/y 确认，Esc/n 取消（防误删不认其他键）
    if app.del_cat_modal.is_some() {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.confirm_del_cat_modal(store);
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => app.close_del_cat_modal(),
            _ => {}
        }
        return;
    }
    // 新建分类弹框
    if app.cat_modal.is_some() {
        match key.code {
            KeyCode::Enter => app.submit_cat_modal(store),
            KeyCode::Esc => app.close_cat_modal(),
            KeyCode::Backspace => app.cat_modal_backspace(),
            KeyCode::Left | KeyCode::Char('h') => app.cat_modal_cycle_color(-1),
            KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => app.cat_modal_cycle_color(1),
            // 数字 1-8 直接选色（方向键可能被终端/输入法拦截时的可靠备选）
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                app.cat_modal_set_color((c as u8 - b'1') as usize);
            }
            KeyCode::Char(c) => app.cat_modal_char(c),
            _ => {}
        }
        return;
    }
    // 输入条：编辑文本期间只处理编辑键，其余字符一律进入文本
    if app.input.is_some() {
        match key.code {
            KeyCode::Enter => {
                let keep = app.submit_input(store);
                if !keep {
                    app.close_input();
                }
            }
            KeyCode::Esc => app.close_input(),
            KeyCode::Backspace => app.input_backspace(),
            KeyCode::Tab => app.input_cycle_cat(),
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.input_cycle_pri()
            }
            KeyCode::Char(c) => app.input_char(c),
            _ => {}
        }
        return;
    }
    // 帮助弹层
    if app.help_open {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter => app.help_open = false,
            _ => {}
        }
        return;
    }

    // 全局键位
    match key.code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('?') => app.help_open = true,
        KeyCode::Char('1') => app.page = Page::Today,
        KeyCode::Char('2') => app.page = Page::Stats,
        KeyCode::Char('3') => app.page = Page::Calendar,
        KeyCode::Char('4') => app.page = Page::Settings,
        KeyCode::Tab => app.page = app.page.next(),
        KeyCode::BackTab => app.page = app.page.prev(),
        KeyCode::Char('t') => {
            let kind = if app.settings.theme == "dark" {
                "light"
            } else {
                "dark"
            };
            app.settings.theme = kind.into();
            let _ = store.save_settings(&app.settings);
        }
        KeyCode::Char('s') => do_sync(app, store),
        KeyCode::Char('[') => app.cycle_cat(-1),
        KeyCode::Char(']') => app.cycle_cat(1),
        KeyCode::Char('c') => app.open_cat_modal(),
        KeyCode::Char('x') => app.open_del_cat_modal(),
        _ => {}
    }

    // 页面专属键位
    match app.page {
        Page::Today => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.move_sel(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_sel(-1),
            KeyCode::Char(' ') => app.toggle_done(store),
            KeyCode::Char('a') => app.open_input(InputMode::AddTask),
            KeyCode::Char('A') => app.open_input(InputMode::AddSub),
            KeyCode::Char('e') => app.open_input(InputMode::Edit),
            KeyCode::Char('d') => app.del_selected(store),
            KeyCode::Char('p') => app.cycle_pri(store),
            KeyCode::Char('h') => app.toggle_fold(true),
            KeyCode::Char('l') => app.toggle_fold(false),
            _ => {}
        },
        Page::Stats => match key.code {
            KeyCode::Left | KeyCode::Char('h') => app.cycle_heat(-1),
            KeyCode::Right | KeyCode::Char('l') => app.cycle_heat(1),
            _ => {}
        },
        Page::Calendar => match key.code {
            KeyCode::Left => shift_month(app, -1),
            KeyCode::Right => shift_month(app, 1),
            KeyCode::Char('h') => app.move_cal_day(-1),
            KeyCode::Char('l') => app.move_cal_day(1),
            KeyCode::Char('j') => app.move_cal_day(7),
            KeyCode::Char('k') => app.move_cal_day(-7),
            KeyCode::Char('g') => app.goto_today(),
            _ => {}
        },
        Page::Settings => match key.code {
            KeyCode::Char('u') => app.open_input(InputMode::WebdavUrl),
            KeyCode::Char('n') => app.open_input(InputMode::WebdavUser),
            KeyCode::Char('p') => app.open_input(InputMode::WebdavPass),
            KeyCode::Char('e') => {
                app.settings.webdav.enabled = !app.settings.webdav.enabled;
                let _ = store.save_settings(&app.settings);
                // 同步状态跟随开关：关=未启用，开=已同步（待下次同步确认）
                app.sync_status = if app.settings.webdav.enabled {
                    SyncStatus::Synced
                } else {
                    SyncStatus::Disabled
                };
            }
            _ => {}
        },
    }
}

fn shift_month(app: &mut App, delta: i32) {
    let (mut y, mut m) = app.cal_month;
    m = (m as i32 + delta - 1).rem_euclid(12) as u32 + 1;
    if delta > 0 && m == 1 {
        y += 1;
    }
    if delta < 0 && m == 12 {
        y -= 1;
    }
    app.cal_month = (y, m);
}

fn do_sync(app: &mut App, store: &Store) {
    if !app.settings.webdav.enabled {
        app.toast("同步未启用 · 设置页按 e 开启");
        return;
    }
    app.sync_status = SyncStatus::Syncing;
    match sync::sync(store, &app.snap, &app.settings.webdav) {
        Ok((merged, note)) => {
            app.snap = merged;
            app.revision = app.snap.revision;
            app.sync_status = SyncStatus::Synced;
            app.toast(note);
            let _ = store.save_snapshot(&app.snap);
        }
        Err(e) => {
            app.sync_status = SyncStatus::Error;
            app.toast(format!("同步失败：{}", e));
        }
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use crate::store::Store;

    fn test_app() -> (Store, App) {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-key-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-key-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.sel = Some(app.snap.tasks[0].id.clone());
        (store, app)
    }

    fn press(app: &mut App, store: &Store, code: KeyCode) {
        on_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            app,
            store,
        );
    }

    fn ctrl(app: &mut App, store: &Store, code: KeyCode) {
        on_key(
            KeyEvent::new(code, KeyModifiers::CONTROL),
            app,
            store,
        );
    }

    #[test]
    fn ctrl_c_closes_layer_then_quits() {
        let (store, mut app) = test_app();
        // 输入条打开：Ctrl+C 关输入条（不退出）
        app.open_input(InputMode::AddTask);
        ctrl(&mut app, &store, KeyCode::Char('c'));
        assert!(app.input.is_none() && !app.quit, "Ctrl+C 应关闭输入条");
        // 弹框打开：Ctrl+C 关弹框（不退出）
        app.open_cat_modal();
        ctrl(&mut app, &store, KeyCode::Char('c'));
        assert!(app.cat_modal.is_none() && !app.quit, "Ctrl+C 应关闭弹框");
        // 主界面：Ctrl+C 退出
        ctrl(&mut app, &store, KeyCode::Char('c'));
        assert!(app.quit, "无弹层时 Ctrl+C 应退出程序");
    }

    #[test]
    fn ctrl_combos_do_not_trigger_shortcuts() {
        let (store, mut app) = test_app();
        let before = app.snap.tasks.len();
        // 主界面 Ctrl+D 不应误删任务
        ctrl(&mut app, &store, KeyCode::Char('d'));
        assert_eq!(app.snap.tasks.len(), before, "Ctrl+D 不应删除任务");
        assert!(!app.quit);
        // 输入条里 Ctrl+A 不应把 'a' 打进缓冲
        app.open_input(InputMode::AddTask);
        ctrl(&mut app, &store, KeyCode::Char('a'));
        let inp = app.input.as_ref().unwrap();
        assert!(inp.buffer.is_empty(), "Ctrl+A 不应输入字符");
        assert!(app.input.is_some());
        // 输入条里 Ctrl+P 仍应切换优先级
        let pri0 = app.input.as_ref().unwrap().pri;
        ctrl(&mut app, &store, KeyCode::Char('p'));
        assert_ne!(app.input.as_ref().unwrap().pri, pri0, "Ctrl+P 应切换优先级");
    }

    #[test]
    fn cat_modal_arrows_cycle_color() {
        let (store, mut app) = test_app();
        app.open_cat_modal();
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, 0);
        press(&mut app, &store, KeyCode::Right);
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, 1, "Right 应切到第 2 个色");
        press(&mut app, &store, KeyCode::Tab);
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, 2);
        press(&mut app, &store, KeyCode::Left);
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, 1, "Left 应回退");
        // 循环回绕
        press(&mut app, &store, KeyCode::Left);
        press(&mut app, &store, KeyCode::Left);
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, crate::theme::CAT_PALETTE.len() - 1);
    }

    #[test]
    fn cat_modal_digit_selects_color() {
        let (store, mut app) = test_app();
        app.open_cat_modal();
        press(&mut app, &store, KeyCode::Char('5'));
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, 4, "按 5 应选中第 5 个色");
        press(&mut app, &store, KeyCode::Char('9'));
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, 4, "9 超出色卡数，不应变化");
        press(&mut app, &store, KeyCode::Char('测'));
        assert_eq!(app.cat_modal.as_ref().unwrap().color_idx, 4);
        assert_eq!(app.cat_modal.as_ref().unwrap().buffer, "测");
    }

    #[test]
    fn esc_closes_input() {
        let (store, mut app) = test_app();
        app.open_input(InputMode::AddTask);
        assert!(app.input.is_some());
        press(&mut app, &store, KeyCode::Esc);
        assert!(app.input.is_none(), "Esc 必须关闭输入条");
    }

    #[test]
    fn digits_go_into_buffer_not_pages() {
        let (store, mut app) = test_app();
        app.open_input(InputMode::AddTask);
        press(&mut app, &store, KeyCode::Char('1'));
        press(&mut app, &store, KeyCode::Char('2'));
        assert!(app.input.is_some(), "输入中数字不应关闭输入条");
        assert_eq!(app.page, Page::Today, "输入中数字不应切换页面");
        let inp = app.input.as_ref().unwrap();
        assert_eq!(inp.buffer, "12", "数字应进入文本缓冲");
    }

    #[test]
    fn brackets_go_into_buffer() {
        let (store, mut app) = test_app();
        app.open_input(InputMode::AddTask);
        press(&mut app, &store, KeyCode::Char('['));
        press(&mut app, &store, KeyCode::Char(']'));
        let inp = app.input.as_ref().unwrap();
        assert_eq!(inp.buffer, "[]");
    }

    #[test]
    fn sync_disabled_shows_not_synced() {
        let (store, mut app) = test_app();
        app.settings.webdav.enabled = false;
        app.sync_status = SyncStatus::Disabled;
        // 未启用时做任何改动都不应显示"有改动/已同步"
        app.toggle_done(&store);
        assert_eq!(app.sync_status, SyncStatus::Disabled);
    }

    #[test]
    fn stats_heat_scope_cycles() {
        let (store, mut app) = test_app();
        app.page = Page::Stats;
        assert_eq!(app.heat, crate::app::HeatScope::Year);
        // l / → 前进，到头回绕
        press(&mut app, &store, KeyCode::Char('l'));
        assert_eq!(app.heat, crate::app::HeatScope::Week);
        press(&mut app, &store, KeyCode::Right);
        assert_eq!(app.heat, crate::app::HeatScope::Month);
        // h / ← 后退，到头回绕
        press(&mut app, &store, KeyCode::Left);
        assert_eq!(app.heat, crate::app::HeatScope::Week);
        press(&mut app, &store, KeyCode::Char('h'));
        assert_eq!(app.heat, crate::app::HeatScope::Year);
        // 不影响月历翻月与选中
        assert!(!app.quit);
    }

    #[test]
    fn calendar_day_navigation() {
        use chrono::Datelike;
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-cal-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-cal-test");
        let store = Store::new();
        let mut app = App::new(&store);
        let today = chrono::Local::now().date_naive();
        assert_eq!(app.cal_sel, today, "初始选中今天");

        app.move_cal_day(1);
        assert_eq!(app.cal_sel, today + chrono::Duration::days(1));
        app.move_cal_day(-7);
        assert_eq!(app.cal_sel, today + chrono::Duration::days(-6));

        // 跨月自动翻月：12 月 1 日向前一天 → 11 月 30，cal_month 同步
        app.cal_sel = chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        app.cal_month = (2026, 12);
        app.move_cal_day(-1);
        assert_eq!(app.cal_sel, chrono::NaiveDate::from_ymd_opt(2026, 11, 30).unwrap());
        assert_eq!(app.cal_month, (2026, 11), "选中日跨月应自动翻到对应月份");

        // 回到今天
        app.goto_today();
        assert_eq!(app.cal_sel, today);
        assert_eq!(app.cal_month, (today.year(), today.month()));
    }

    #[test]
    fn nested_subtasks_unlimited() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-nest-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-nest-test");
        let store = Store::new();
        let mut app = App::new(&store);
        use crate::model::{find_sub, subtree_done};

        // 逐层添加 一/二/三 级子任务
        app.sel = Some("t1".into());
        app.open_input(InputMode::AddSub);
        for c in "一级".chars() {
            app.input_char(c);
        }
        app.submit_input(&store);
        let l1 = app.sel.clone().unwrap();
        let dep = |id: &str, a: &App| a.visible_ids().iter().find(|(i, _)| i == id).map(|(_, d)| *d);
        assert_eq!(dep(&l1, &app), Some(1), "一级子任务应深度 1");

        app.sel = Some(l1.clone());
        app.open_input(InputMode::AddSub);
        for c in "二级".chars() {
            app.input_char(c);
        }
        app.submit_input(&store);
        let l2 = app.sel.clone().unwrap();
        assert_eq!(dep(&l2, &app), Some(2), "二级子任务应深度 2");

        app.sel = Some(l2.clone());
        app.open_input(InputMode::AddSub);
        for c in "三级".chars() {
            app.input_char(c);
        }
        app.submit_input(&store);
        let l3 = app.sel.clone().unwrap();
        assert_eq!(dep(&l3, &app), Some(3), "三级子任务应深度 3");

        // 折叠二级 → 三级不可见
        app.sel = Some(l2.clone());
        app.toggle_fold(true);
        assert!(!app.visible_ids().iter().any(|(i, _)| i == &l3), "折叠后三级应隐藏");
        app.sel = Some(l2.clone());
        app.toggle_fold(false);

        // 完成三级叶子 → 二级、一级递归自动完成（顶级 t1 因原有未完成子任务仍未完成）
        app.sel = Some(l3.clone());
        app.toggle_done(&store);
        let t1 = app.snap.tasks.iter().find(|t| t.id == "t1").unwrap();
        let s1 = find_sub(&t1.subs, &l1).unwrap();
        let s2 = find_sub(&t1.subs, &l2).unwrap();
        let s3 = find_sub(&t1.subs, &l3).unwrap();
        assert!(subtree_done(s1) && subtree_done(s2) && s3.done);
        assert!(!t1.done);
    }

    #[test]
    fn webdav_config_edit_roundtrip() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-webdav-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-webdav-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.page = Page::Settings;

        // u 打开服务器地址输入，Enter 保存并落盘
        app.open_input(InputMode::WebdavUrl);
        assert!(app.input.is_some());
        for c in "https://dav.example.com/dav/".chars() {
            app.input_char(c);
        }
        let keep = app.submit_input(&store);
        assert!(!keep && app.input.is_none(), "WebDAV 配置提交后输入条应关闭");
        assert_eq!(app.settings.webdav.url, "https://dav.example.com/dav/");
        assert_eq!(
            store.load_settings().webdav.url,
            "https://dav.example.com/dav/",
            "配置应写入 settings.json"
        );

        // 账号 / 密码同样可编辑；密码打开时预填当前值
        app.open_input(InputMode::WebdavUser);
        for c in "user@mail.com".chars() {
            app.input_char(c);
        }
        assert!(!app.submit_input(&store));
        app.open_input(InputMode::WebdavPass);
        for c in "app-password".chars() {
            app.input_char(c);
        }
        assert!(!app.submit_input(&store));
        let reloaded = store.load_settings();
        assert_eq!(reloaded.webdav.username, "user@mail.com");
        assert_eq!(reloaded.webdav.password, "app-password");
    }

    #[test]
    fn x_on_all_filter_rejected() {
        let (store, mut app) = test_app();
        assert_eq!(app.cat, "全部");
        press(&mut app, &store, KeyCode::Char('x'));
        assert!(app.del_cat_modal.is_none(), "「全部」不是分类，不应弹出删除框");
    }

    #[test]
    fn del_cat_modal_confirm_deletes_and_moves_tasks() {
        let (store, mut app) = test_app();
        // 全部 → 工作 → 生活 → 学习
        press(&mut app, &store, KeyCode::Char(']'));
        press(&mut app, &store, KeyCode::Char(']'));
        press(&mut app, &store, KeyCode::Char(']'));
        assert_eq!(app.cat, "学习");
        press(&mut app, &store, KeyCode::Char('x'));
        assert_eq!(app.del_cat_modal.as_ref().unwrap().name, "学习");
        // Esc 取消：分类还在
        press(&mut app, &store, KeyCode::Esc);
        assert!(app.del_cat_modal.is_none());
        assert!(app.snap.categories.iter().any(|c| c.name == "学习"));
        // 再次弹出并确认：分类删除，其任务移入「默认」，过滤回到「全部」
        press(&mut app, &store, KeyCode::Char('x'));
        press(&mut app, &store, KeyCode::Enter);
        assert!(app.del_cat_modal.is_none());
        assert!(!app.snap.categories.iter().any(|c| c.name == "学习"));
        assert_eq!(app.cat, "全部");
        let t4 = app.snap.tasks.iter().find(|t| t.id == "t4").unwrap();
        assert_eq!(t4.category, "默认", "被删分类的任务应移入「默认」");
        assert!(app.snap.tasks.iter().all(|t| t.category != "学习"));
    }

    #[test]
    fn ctrl_c_closes_del_cat_modal_first() {
        let (store, mut app) = test_app();
        press(&mut app, &store, KeyCode::Char(']'));
        press(&mut app, &store, KeyCode::Char('x'));
        assert!(app.del_cat_modal.is_some());
        ctrl(&mut app, &store, KeyCode::Char('c'));
        assert!(app.del_cat_modal.is_none(), "Ctrl+C 应关闭删除确认框");
        assert!(!app.quit, "Ctrl+C 关闭弹框时不应退出程序");
        assert!(app.snap.categories.iter().any(|c| c.name == "工作"), "取消后分类应保留");
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn dump_first_frame() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-render-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-render-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.sel = Some(app.snap.tasks[0].id.clone());
        app.expanded.clear();
        app.expanded.push(app.snap.tasks[0].id.clone());
        let backend = TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let b = terminal.backend();
        println!("== 帧内容（每行行号:内容） ==");
        for y in 0..30 {
            let mut line = String::new();
            for x in 0..100 {
                line.push(b.buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            println!("{:02}|{}", y, line.trim_end());
        }
    }

    #[test]
    fn dump_modal_frame() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-modal-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-modal-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.open_cat_modal();
        app.cat_modal_char('测');
        app.cat_modal_set_color(3);
        let backend = TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let b = terminal.backend();
        println!("== 弹框帧（10-18 行） ==");
        for y in 10..19 {
            let mut line = String::new();
            for x in 0..100 {
                line.push(b.buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            println!("{:02}|{}", y, line.trim_end());
        }
    }

    #[test]
    fn dump_del_cat_frame() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-delcat-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-delcat-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.cat = "工作".into();
        app.open_del_cat_modal();
        let backend = TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let b = terminal.backend();
        println!("== 删除分类弹框帧（10-19 行） ==");
        for y in 10..19 {
            let mut line = String::new();
            for x in 0..100 {
                line.push(b.buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            println!("{:02}|{}", y, line.trim_end());
        }
    }

    #[test]
    fn dump_stats_frame() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-stats-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-stats-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.page = Page::Stats;
        let backend = TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let b = terminal.backend();
        println!("== 统计页帧 ==");
        for y in 0..30 {
            let mut line = String::new();
            for x in 0..100 {
                line.push(b.buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            println!("{:02}|{}", y, line.trim_end());
        }
    }

    #[test]
    fn stats_canvas_bg_follows_theme() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-statsbg-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-statsbg-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.page = Page::Stats;
        let backend = TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let b = terminal.backend();
        // 折线图与环形图画布区（浅色主题 panel=#FFFFFF）不应残留终端默认背景
        let expect = ratatui::style::Color::Rgb(0xFF, 0xFF, 0xFF);
        for (x, y) in [(30, 22), (30, 26), (70, 22)] {
            let cell = &b.buffer()[(x, y)];
            assert_eq!(
                cell.bg,
                expect,
                "画布区 ({x},{y}) 背景应为 panel 色，实际 {:?}",
                cell.bg
            );
        }
    }

    #[test]
    fn dump_calendar_frame() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-cal-render") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-cal-render");
        let store = Store::new();
        let mut app = App::new(&store);
        app.page = Page::Calendar;
        let backend = TestBackend::new(100, 34);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let b = terminal.backend();
        println!("== Calendar 帧 ==");
        for y in 0..34 {
            let mut line = String::new();
            for x in 0..100 {
                line.push(b.buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            println!("{:02}|{}", y, line.trim_end());
        }
    }

    #[test]
    fn dump_heat_scopes() {
        use crate::app::HeatScope;
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-heat-test") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-heat-test");
        let store = Store::new();
        let mut app = App::new(&store);
        app.page = Page::Stats;
        for scope in [HeatScope::Week, HeatScope::Month, HeatScope::Year] {
            app.heat = scope;
            let backend = TestBackend::new(120, 34);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| draw(f, &app)).unwrap();
            let b = terminal.backend();
            println!("== {:?} ==", scope);
            for y in 1..26 {
                let mut line = String::new();
                for x in 0..120 {
                    line.push(b.buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
                }
                println!("{:02}|{}", y, line.trim_end());
            }
        }
    }

    #[test]
    fn dump_composer_frame() {
        unsafe { std::env::set_var("DOIT_TUI_DIR", "/tmp/doit-tui-render-test2") };
        let _ = std::fs::remove_dir_all("/tmp/doit-tui-render-test2");
        let store = Store::new();
        let mut app = App::new(&store);
        app.sel = Some(app.snap.tasks[0].id.clone());
        app.open_input(InputMode::AddTask);
        app.input_char('a');
        app.input_char('1');
        app.input_char('2');
        let backend = TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let b = terminal.backend();
        println!("== composer 帧（25-29 行） ==");
        for y in 25..30 {
            let mut line = String::new();
            for x in 0..100 {
                line.push(b.buffer()[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            println!("{:02}|{}", y, line.trim_end());
        }
    }
}
