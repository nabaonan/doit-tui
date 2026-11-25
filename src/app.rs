//! 应用状态与用户动作。

use crate::model::{now_ms, Category, Snapshot, Task};
use crate::store::{new_id, Settings, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Today,
    Stats,
    Calendar,
    Settings,
}

impl Page {
    pub fn label(self) -> &'static str {
        match self {
            Page::Today => "今日",
            Page::Stats => "统计",
            Page::Calendar => "月历",
            Page::Settings => "设置",
        }
    }
    pub fn index(self) -> usize {
        match self {
            Page::Today => 0,
            Page::Stats => 1,
            Page::Calendar => 2,
            Page::Settings => 3,
        }
    }
    pub fn from_index(i: usize) -> Page {
        [Page::Today, Page::Stats, Page::Calendar, Page::Settings][i.min(3)]
    }
    pub fn next(self) -> Page {
        Page::from_index(self.index() + 1)
    }
    pub fn prev(self) -> Page {
        Page::from_index(self.index() + 3)
    }
}

/// 热力图展示周期：本周 / 本月 / 本年。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatScope {
    Week,
    Month,
    Year,
}

impl HeatScope {
    pub fn label(self) -> &'static str {
        match self {
            HeatScope::Week => "本周",
            HeatScope::Month => "本月",
            HeatScope::Year => "本年",
        }
    }
    pub fn index(self) -> usize {
        match self {
            HeatScope::Week => 0,
            HeatScope::Month => 1,
            HeatScope::Year => 2,
        }
    }
    pub fn from_index(i: usize) -> HeatScope {
        [HeatScope::Week, HeatScope::Month, HeatScope::Year][i % 3]
    }
    pub fn cycled(self, delta: i32) -> HeatScope {
        let n = 3;
        Self::from_index(((self.index() as i32 + delta) % n + n).rem_euclid(n) as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {    /// WebDAV 未启用（设置里关掉了）——不显示"已同步"。
    Disabled,
    Synced,
    Dirty,
    Syncing,
    Error,
}

impl SyncStatus {
    pub fn label(self) -> &'static str {
        match self {
            SyncStatus::Disabled => "未启用",
            SyncStatus::Synced => "已同步",
            SyncStatus::Dirty => "有改动",
            SyncStatus::Syncing => "同步中",
            SyncStatus::Error => "同步失败",
        }
    }
    pub fn color(self, t: &crate::theme::Theme) -> ratatui::style::Color {
        use ratatui::style::Color::*;
        match self {
            SyncStatus::Disabled => t.faint,
            SyncStatus::Synced => Rgb(0x4F, 0xBF, 0x9E),
            SyncStatus::Dirty => Rgb(0xE0, 0x8A, 0x34),
            SyncStatus::Syncing => Rgb(0x4A, 0x86, 0xD6),
            SyncStatus::Error => Rgb(0xDE, 0x5B, 0x55),
        }
        .into()
    }
}

/// 底部输入条模式：新增任务 / 新增子任务 / 编辑标题 / WebDAV 配置。
#[derive(Debug, Clone)]
pub struct Input {
    pub mode: InputMode,
    pub buffer: String,
    pub pri: u8,
    pub cat: String,
    pub target: Option<String>, // 编辑/子任务时的目标 id
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    AddTask,
    AddSub,
    Edit,
    WebdavUrl,
    WebdavUser,
    WebdavPass,
}

/// 新建分类弹框。
#[derive(Debug, Clone)]
pub struct CatModal {
    pub buffer: String,
    pub color_idx: usize,
}

/// 删除分类确认弹框：存放待删除的分类名。
#[derive(Debug, Clone)]
pub struct DelCatModal {
    pub name: String,
}

pub struct App {
    pub snap: Snapshot,
    pub settings: Settings,
    pub page: Page,
    pub cal_month: (i32, u32), // 月历页查看的年月
    pub cal_sel: chrono::NaiveDate, // 月历页选中的日期
    pub area: ratatui::layout::Rect, // 最近一次渲染的终端屏幕区域（鼠标命中用）
    pub cat: String,           // 当前分类过滤（"全部" 或分类名）
    pub sel: Option<String>,   // 选中的任务/子任务 id
    pub expanded: Vec<String>, // 已展开的父任务 id
    pub input: Option<Input>,
    pub cat_modal: Option<CatModal>,
    pub del_cat_modal: Option<DelCatModal>,
    pub heat: HeatScope, // 统计页热力图周期
    pub help_open: bool,
    pub quit: bool,
    pub sync_status: SyncStatus,
    pub revision: u64,
    pub toast: Option<(String, std::time::Instant)>,
    pub list_scroll: usize,
    dirty: bool,
}

impl App {
    pub fn new(store: &Store) -> Self {
        let snap = store.load_snapshot().unwrap_or_else(|_| Snapshot::new("tui"));
        let settings = store.load_settings();
        let theme = if settings.theme == "dark" {
            crate::theme::ThemeKind::Dark
        } else {
            crate::theme::ThemeKind::Light
        };
        let _ = theme;
        let mut expanded = Vec::new();
        if let Some(t) = snap.tasks.iter().filter(|t| t.deleted_at.is_none()).find(|t| !t.subs.is_empty()) {
            expanded.push(t.id.clone());
        }
        let today = chrono::Local::now();
        let sync_enabled = settings.webdav.enabled;
        App {
            revision: snap.revision,
            snap,
            settings,
            page: Page::Today,
            cal_month: (today.year(), today.month()),
            cal_sel: today.date_naive(),
            area: ratatui::layout::Rect::new(0, 0, 100, 34),
            cat: "全部".into(),
            sel: None,
            expanded,
            input: None,
            cat_modal: None,
            del_cat_modal: None,
            heat: HeatScope::Year,
            help_open: false,
            quit: false,
            sync_status: if sync_enabled {
                SyncStatus::Synced
            } else {
                SyncStatus::Disabled
            },
            toast: None,
            list_scroll: 0,
            dirty: false,
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), std::time::Instant::now()));
    }

    pub fn expire_toast(&mut self) {
        if let Some((_, t)) = &self.toast {
            if t.elapsed() > std::time::Duration::from_secs(4) {
                self.toast = None;
            }
        }
    }

    fn mark_dirty(&mut self, store: &Store) {
        self.snap.touch();
        self.revision = self.snap.revision;
        self.dirty = true;
        // 未启用同步时不显示"有改动"，保持"未启用"
        if self.settings.webdav.enabled {
            self.sync_status = SyncStatus::Dirty;
        }
        let _ = store.save_snapshot(&self.snap);
    }

    // ---------- 列表与选中 ----------

    pub fn cat_tasks(&self) -> Vec<&Task> {
        if self.cat == "全部" {
            self.snap.active_tasks().collect()
        } else {
            self.snap.tasks.iter().filter(|t| t.deleted_at.is_none() && t.category == self.cat).collect()
        }
    }

    pub fn cat_pending(&self, cat: &str) -> usize {
        self.snap
            .tasks
            .iter()
            .filter(|t| t.deleted_at.is_none() && (cat == "全部" || t.category == cat) && !t.done)
            .count()
    }

    /// 今日页可见行：顶级任务 + 递归展开的子任务，返回 (id, 深度)。
    /// 深层节点只有其所有祖先都处于展开态时才可见。
    pub fn visible_ids(&self) -> Vec<(String, u8)> {
        let mut rows = Vec::new();
        for t in self.cat_tasks() {
            rows.push((t.id.clone(), 0));
            if self.expanded.iter().any(|e| e == &t.id) {
                collect_subs(&mut rows, &t.subs, 1, &self.expanded);
            }
        }
        rows
    }

    pub(crate) fn ensure_selection(&mut self) {
        let ids = self.visible_ids();
        if ids.is_empty() {
            self.sel = None;
        } else if self.sel.is_none() || !ids.iter().any(|(id, _)| Some(id) == self.sel.as_ref()) {
            self.sel = Some(ids[0].0.clone());
        }
    }

    pub fn move_sel(&mut self, delta: i32) {
        let ids = self.visible_ids();
        if ids.is_empty() {
            return;
        }
        let cur = ids
            .iter()
            .position(|(id, _)| Some(id) == self.sel.as_ref())
            .unwrap_or(0) as i32;
        let next = ((cur + delta).max(0) as usize).min(ids.len() - 1);
        self.sel = Some(ids[next].0.clone());
    }

    // ---------- 动作 ----------

    pub fn toggle_done(&mut self, store: &Store) {
        use crate::model::{find_sub_mut, set_node_done, subs_contain, subtree_done};
        let id = match &self.sel {
            Some(i) => i.clone(),
            None => return,
        };
        let now = now_ms();
        // 顶级任务：整棵子树整体切换完成
        if let Some(t) = self.snap.tasks.iter_mut().find(|t| t.id == id) {
            let done = !t.done;
            t.done = done;
            t.updated_at = now;
            t.completed_at = if done { Some(now) } else { None };
            for s in &mut t.subs {
                set_node_done(s, done, now);
            }
            self.mark_dirty(store);
            return;
        }
        // 任意深度的子任务：整体切换该节点子树，再重算祖先完成态
        let top = self
            .snap
            .tasks
            .iter()
            .find(|t| subs_contain(&t.subs, &id))
            .map(|t| t.id.clone());
        if let Some(pid) = top {
            if let Some(t) = self.snap.tasks.iter_mut().find(|t| t.id == pid) {
                if let Some(node) = find_sub_mut(&mut t.subs, &id) {
                    let done = !subtree_done(node);
                    set_node_done(node, done, now);
                    t.updated_at = now;
                }
            }
            self.snap.recalc_parent(&pid);
            self.mark_dirty(store);
            self.ensure_selection();
        }
    }

    pub fn cycle_pri(&mut self, store: &Store) {
        use crate::model::find_sub_mut;
        let id = match &self.sel {
            Some(i) => i.clone(),
            None => return,
        };
        let bump = |p: &mut u8| *p = (*p + 1) % 5;
        let mut hit = false;
        for t in self.snap.tasks.iter_mut() {
            if t.id == id {
                bump(&mut t.priority);
                t.updated_at = now_ms();
                hit = true;
                break;
            }
        }
        if !hit {
            for t in self.snap.tasks.iter_mut() {
                if let Some(s) = find_sub_mut(&mut t.subs, &id) {
                    bump(&mut s.priority);
                    s.updated_at = now_ms();
                    t.updated_at = now_ms();
                    hit = true;
                    break;
                }
            }
        }
        if hit {
            self.mark_dirty(store);
        }
    }

    pub fn del_selected(&mut self, store: &Store) {
        use crate::model::{find_sub, subs_contain, tombstone_sub};
        let id = match &self.sel {
            Some(i) => i.clone(),
            None => return,
        };
        let now = now_ms();
        // 顶级任务删除：打墓碑（不物理移除），跨端同步才能传播删除。
        if let Some(t) = self.snap.tasks.iter_mut().find(|t| t.id == id) {
            let title = t.title.clone();
            t.deleted_at = Some(now);
            t.updated_at = now;
            self.mark_dirty(store);
            self.toast(format!("已删除：{}", title));
            self.ensure_selection();
            return;
        }
        // 任意深度子任务删除：找到其所在顶级任务，递归打墓碑。
        if let Some(pid) = self
            .snap
            .tasks
            .iter()
            .find(|t| subs_contain(&t.subs, &id))
            .map(|t| t.id.clone())
        {
            if let Some(t) = self.snap.tasks.iter_mut().find(|t| t.id == pid) {
                if let Some(node) = find_sub(&t.subs, &id) {
                    let title = node.title.clone();
                    tombstone_sub(&mut t.subs, &id, now);
                    t.updated_at = now;
                    self.snap.recalc_parent(&pid);
                    self.mark_dirty(store);
                    self.toast(format!("已删除子任务：{}", title));
                }
            }
            self.ensure_selection();
        }
    }

    /// 任意节点（顶级任务或任意深度子任务）是否带有子节点。
    pub fn node_has_children(&self, id: &str) -> bool {
        use crate::model::find_sub;
        for t in &self.snap.tasks {
            if t.id == id {
                return !t.subs.is_empty();
            }
            if let Some(s) = find_sub(&t.subs, id) {
                return !s.subs.is_empty();
            }
        }
        false
    }

    pub fn toggle_fold(&mut self, fold: bool) {
        use crate::model::{find_sub, subs_contain};
        let id = match &self.sel {
            Some(i) => i.clone(),
            None => return,
        };
        // 顶级任务
        if let Some(t) = self.snap.tasks.iter().find(|t| t.id == id) {
            if !t.subs.is_empty() {
                self.fold_task(&id, fold);
            }
            return;
        }
        // 任意深度子任务：自身有子节点时折叠自身
        if let Some(pid) = self
            .snap
            .tasks
            .iter()
            .find(|t| subs_contain(&t.subs, &id))
            .map(|t| t.id.clone())
        {
            // 找到该节点本身是否有子节点
            if let Some(t) = self.snap.tasks.iter().find(|t| t.id == pid) {
                if let Some(s) = find_sub(&t.subs, &id) {
                    if !s.subs.is_empty() {
                        self.fold_task(&id, fold);
                    }
                }
            }
        }
    }

    fn fold_task(&mut self, id: &str, fold: bool) {
        if fold {
            self.expanded.retain(|e| e != id);
        } else if !self.expanded.iter().any(|e| e == id) {
            self.expanded.push(id.to_string());
        }
    }

    /// 展开从顶级任务到 id 的全链路节点（含 id），使新添加的子任务立即可见。
    fn expand_path_to(&mut self, id: &str) {
        use crate::model::subs_contain;
        let mut chain: Vec<String> = Vec::new();
        for t in &self.snap.tasks {
            if t.id == id {
                chain.push(t.id.clone());
                break;
            }
            if subs_contain(&t.subs, id) {
                chain.push(t.id.clone());
                collect_ancestors(&t.subs, id, &mut chain);
                break;
            }
        }
        for node in chain {
            if !self.expanded.iter().any(|e| e == &node) {
                self.expanded.push(node);
            }
        }
    }

    /// 切换热力图周期（统计页 ←/→ 或 h/l）。
    pub fn cycle_heat(&mut self, delta: i32) {
        self.heat = self.heat.cycled(delta);
    }

    /// 月历页：选中日前后移动（±1 天或 ±7 天），跨月时自动翻到对应月份。
    pub fn move_cal_day(&mut self, delta: i64) {
        use chrono::Datelike;
        self.cal_sel += chrono::Duration::days(delta);
        self.cal_month = (self.cal_sel.year(), self.cal_sel.month());
    }

    /// 月历页：回到今天并选中今天。
    pub fn goto_today(&mut self) {
        use chrono::Datelike;
        let today = chrono::Local::now().date_naive();
        self.cal_sel = today;
        self.cal_month = (today.year(), today.month());
    }

    pub fn cycle_cat(&mut self, delta: i32) {        let mut names: Vec<String> = vec!["全部".into()];
        names.extend(self.snap.active_categories().map(|c| c.name.clone()));
        let cur = names
            .iter()
            .position(|n| *n == self.cat)
            .unwrap_or(0) as i32;
        let next = ((cur + delta + names.len() as i32) % names.len() as i32) as usize;
        self.cat = names[next].clone();
    }

    pub fn add_category(&mut self, store: &Store, name: &str, color: &str) -> bool {
        if name.is_empty()
            || self.snap.active_categories().any(|c| c.name == name)
        {
            self.toast("分类已存在或名称为空");
            return false;
        }
        self.snap.categories.push(Category {
            name: name.into(),
            color: crate::theme::normalize_category_color(color),
            sort_order: self.snap.categories.len() as i64,
            updated_at: now_ms(),
            deleted_at: None,
        });
        self.cat = name.into();
        self.mark_dirty(store);
        self.toast(format!("已新建分类：{}", name));
        true
    }

    // ---------- 删除分类 ----------

    /// 当前过滤在具体分类上时弹出确认框；「全部」是过滤器不是分类，拒绝。
    pub fn open_del_cat_modal(&mut self) {
        if self.cat == "全部" {
            self.toast("请先用 [ ] 选中要删除的分类");
            return;
        }
        if self.snap.categories.iter().all(|c| c.name != self.cat) {
            self.toast("当前分类不存在");
            return;
        }
        self.del_cat_modal = Some(DelCatModal {
            name: self.cat.clone(),
        });
    }

    pub fn close_del_cat_modal(&mut self) {
        self.del_cat_modal = None;
    }

    pub fn confirm_del_cat_modal(&mut self, store: &Store) {
        let Some(m) = self.del_cat_modal.clone() else { return };
        self.del_cat_modal = None;
        self.delete_category(store, &m.name);
    }

    /// 删除分类：打墓碑（不物理移除），其下任务移入「默认」（数据不丢），
    /// 分类墓碑随同步传播，避免被其它端的旧快照复活。
    pub fn delete_category(&mut self, store: &Store, name: &str) {
        let now = now_ms();
        let Some(c) = self.snap.categories.iter_mut().find(|c| c.name == name && c.deleted_at.is_none()) else {
            self.toast("分类不存在");
            return;
        };
        c.deleted_at = Some(now);
        c.updated_at = now;
        let mut moved = 0usize;
        for t in self.snap.tasks.iter_mut() {
            if t.category == name && t.deleted_at.is_none() {
                t.category = "默认".into();
                t.updated_at = now;
                moved += 1;
            }
        }
        self.cat = "全部".into();
        self.sel = None;
        self.ensure_selection();
        self.mark_dirty(store);
        if moved > 0 {
            self.toast(format!("已删除「{}」· {} 个任务移入默认", name, moved));
        } else {
            self.toast(format!("已删除分类：{}", name));
        }
    }

    // ---------- 输入条 ----------

    pub fn open_input(&mut self, mode: InputMode) {
        let target = self.sel.clone();
        let (buffer, cat) = match mode {
            InputMode::AddTask => (String::new(), self.cat.clone()),
            InputMode::AddSub => {
                // 选中任意节点（顶级任务或任意深度子任务），子任务就加在它下面
                match self.node_category(&target) {
                    Some(cat) => (String::new(), cat),
                    None => {
                        self.toast("先选中一个任务再添加子任务");
                        return;
                    }
                }
            }
            InputMode::Edit => {
                let found = self.find_entity(&target.clone().unwrap_or_default());
                match found {
                    Some((title, _pri, _done)) => (title, String::new()),
                    None => return,
                }
            }
            InputMode::WebdavUrl => (self.settings.webdav.url.clone(), String::new()),
            InputMode::WebdavUser => (self.settings.webdav.username.clone(), String::new()),
            InputMode::WebdavPass => (self.settings.webdav.password.clone(), String::new()),
        };
        let pri = match mode {
            InputMode::AddTask => 2,
            InputMode::AddSub => 2,
            InputMode::Edit => self.find_entity_pri(&target.clone().unwrap_or_default()),
            _ => 0,
        };
        self.input = Some(Input {
            mode,
            buffer,
            pri,
            cat,
            target,
        });
    }

    /// 节点所在分类：顶级任务 = 自身分类；任意深度子任务 = 其顶级任务的分类。
    fn node_category(&self, id: &Option<String>) -> Option<String> {
        use crate::model::subs_contain;
        let id = id.as_ref()?;
        for t in &self.snap.tasks {
            if t.id == *id {
                return Some(t.category.clone());
            }
            if subs_contain(&t.subs, id) {
                return Some(t.category.clone());
            }
        }
        None
    }

    fn find_entity(&self, id: &str) -> Option<(String, u8, bool)> {
        use crate::model::find_sub;
        for t in &self.snap.tasks {
            if t.id == id {
                return Some((t.title.clone(), t.priority, t.done));
            }
            if let Some(s) = find_sub(&t.subs, id) {
                return Some((s.title.clone(), s.priority, s.done));
            }
        }
        None
    }

    fn find_entity_pri(&self, id: &str) -> u8 {
        self.find_entity(id).map(|(_, p, _)| p).unwrap_or(0)
    }

    pub fn input_char(&mut self, c: char) {
        if let Some(inp) = &mut self.input {
            inp.buffer.push(c);
        }
    }

    pub fn input_backspace(&mut self) {
        if let Some(inp) = &mut self.input {
            inp.buffer.pop();
        }
    }

    pub fn input_cycle_pri(&mut self) {
        if let Some(inp) = &mut self.input {
            if matches!(inp.mode, InputMode::AddTask | InputMode::AddSub) {
                inp.pri = (inp.pri + 1) % 5;
            }
        }
    }

    pub fn input_cycle_cat(&mut self) {
        if let Some(inp) = &mut self.input {
            if inp.mode == InputMode::AddTask {
                let mut names: Vec<String> = self.snap.active_categories().map(|c| c.name.clone()).collect();
                if names.is_empty() {
                    names.push("默认".into());
                }
                let cur = names.iter().position(|n| *n == inp.cat).unwrap_or(0);
                inp.cat = names[(cur + 1) % names.len()].clone();
            }
        }
    }

    pub fn close_input(&mut self) {
        self.input = None;
    }

    /// 提交输入条。返回 true 表示输入条保持打开（连续创建）。
    pub fn submit_input(&mut self, store: &Store) -> bool {
        let Some(inp) = self.input.clone() else { return false };
        let raw = inp.buffer.trim().to_string();
        match inp.mode {
            InputMode::AddTask => {
                if raw.is_empty() {
                    self.input = None;
                    return false;
                }
                // !1~!4 后缀快速设优先级
                let (title, pri) = parse_pri_suffix(&raw).unwrap_or((raw, inp.pri));
                let cat = if self.cat != "全部" && self.cat != inp.cat { inp.cat.clone() } else { inp.cat.clone() };
                let cat = if cat == "全部" { "默认".into() } else { cat };
                let now = now_ms();
                let task = Task {
                    id: new_id("t"),
                    title,
                    category: cat,
                    priority: pri,
                    done: false,
                    created_at: now,
                    updated_at: now,
                    completed_at: None,
                    subs: vec![],
                    deleted_at: None,
                    sort_order: -1, // 新建置顶
                };
                let title = task.title.clone();
                self.snap.tasks.insert(0, task);
                self.sel = Some(self.snap.tasks[0].id.clone());
                self.mark_dirty(store);
                self.toast(format!("已新增：{}", title));
                true // 保持输入条，连续创建
            }
            InputMode::AddSub => {
                use crate::model::{find_sub_mut, subs_contain, Subtask};
                self.input = None;
                if raw.is_empty() {
                    return false;
                }
                let (title, pri) = parse_pri_suffix(&raw).unwrap_or((raw, inp.pri));
                let target = inp.target.unwrap_or_default();
                let now = now_ms();
                let add = |subs: &mut Vec<Subtask>, now: i64| -> String {
                    let sid = new_id("s");
                    subs.push(Subtask {
                        id: sid.clone(),
                        title,
                        priority: pri,
                        done: false,
                        created_at: now,
                        updated_at: now,
                        completed_at: None,
                        subs: Vec::new(),
                        deleted_at: None,
                        sort_order: subs.len() as i64,
                    });
                    sid
                };
                // 顶层任务 → 加到其 subs
                if let Some(t) = self.snap.tasks.iter_mut().find(|t| t.id == target) {
                    let pid = t.id.clone();
                    let sid = add(&mut t.subs, now);
                    t.updated_at = now;
                    self.snap.recalc_parent(&pid);
                    self.expand_path_to(&target);
                    self.sel = Some(sid);
                    self.mark_dirty(store);
                    self.toast("已新增子任务（可继续对该子任务按 A 无限嵌套）");
                    return false;
                }
                // 任意深度子任务 → 加到该节点 subs
                if let Some(pid) = self
                    .snap
                    .tasks
                    .iter()
                    .find(|t| subs_contain(&t.subs, &target))
                    .map(|t| t.id.clone())
                {
                    if let Some(t) = self.snap.tasks.iter_mut().find(|t| t.id == pid) {
                        if let Some(node) = find_sub_mut(&mut t.subs, &target) {
                            let sid = add(&mut node.subs, now);
                            node.updated_at = now;
                            t.updated_at = now;
                            self.snap.recalc_parent(&pid);
                            self.expand_path_to(&target);
                            self.sel = Some(sid);
                            self.mark_dirty(store);
                            self.toast("已新增子任务（可继续对该子任务按 A 无限嵌套）");
                            return false;
                        }
                    }
                }
                self.toast("未找到父任务");
                false
            }
            InputMode::Edit => {
                use crate::model::find_sub_mut;
                self.input = None;
                if raw.is_empty() {
                    return false;
                }
                let id = inp.target.unwrap_or_default();
                let now = now_ms();
                let mut hit = false;
                for t in self.snap.tasks.iter_mut() {
                    if t.id == id && !t.done {
                        t.title = raw.clone();
                        t.updated_at = now;
                        hit = true;
                        break;
                    }
                    if let Some(s) = find_sub_mut(&mut t.subs, &id) && !s.done {
                        s.title = raw.clone();
                        s.updated_at = now;
                        t.updated_at = now;
                        hit = true;
                        break;
                    }
                }
                if hit {
                    self.mark_dirty(store);
                    self.toast("已保存");
                } else {
                    self.toast("已完成的任务不可编辑");
                }
                false
            }
            InputMode::WebdavUrl | InputMode::WebdavUser | InputMode::WebdavPass => {
                self.input = None;
                match inp.mode {
                    InputMode::WebdavUrl => self.settings.webdav.url = raw,
                    InputMode::WebdavUser => self.settings.webdav.username = raw,
                    _ => self.settings.webdav.password = raw,
                }
                let _ = store.save_settings(&self.settings);
                self.toast("已保存 WebDAV 配置 · 按 s 立即同步");
                false
            }
        }
    }

    // ---------- 分类弹框 ----------

    pub fn open_cat_modal(&mut self) {
        self.cat_modal = Some(CatModal {
            buffer: String::new(),
            color_idx: 0,
        });
    }

    pub fn close_cat_modal(&mut self) {
        self.cat_modal = None;
    }

    pub fn cat_modal_char(&mut self, c: char) {
        if let Some(m) = &mut self.cat_modal {
            m.buffer.push(c);
        }
    }

    pub fn cat_modal_backspace(&mut self) {
        if let Some(m) = &mut self.cat_modal {
            m.buffer.pop();
        }
    }

    pub fn cat_modal_cycle_color(&mut self, delta: i32) {
        if let Some(m) = &mut self.cat_modal {
            let n = crate::theme::CAT_PALETTE.len() as i32;
            m.color_idx = (((m.color_idx as i32 + delta) % n + n) % n) as usize;
        }
    }

    /// 数字键直接选色（1-8）。
    pub fn cat_modal_set_color(&mut self, idx: usize) {
        if let Some(m) = &mut self.cat_modal {
            if idx < crate::theme::CAT_PALETTE.len() {
                m.color_idx = idx;
            }
        }
    }

    pub fn submit_cat_modal(&mut self, store: &Store) {
        let Some(m) = self.cat_modal.clone() else { return };
        let name = m.buffer.trim().to_string();
        if name.is_empty() {
            self.toast("请输入分类名称");
            return;
        }
        if self.add_category(store, &name, crate::theme::CAT_PALETTE[m.color_idx]) {
            self.cat_modal = None;
        }
    }

    // ---------- 今日统计（口径与手机端一致） ----------

    /// 分母 = 未整体完成 + 今日完成的顶级任务（当前分类过滤）；分子 = 今日完成。
    pub fn today_stats(&self) -> (usize, usize) {
        let today = chrono::Local::now().date_naive();
        let tasks = self.cat_tasks();
        let total = tasks
            .iter()
            .filter(|t| !t.done || t.completed_at.map(|c| crate::model::completed_on(c) == today).unwrap_or(false))
            .count();
        let done = tasks
            .iter()
            .filter(|t| t.done && t.completed_at.map(|c| crate::model::completed_on(c) == today).unwrap_or(false))
            .count();
        (done, total)
    }

    #[allow(dead_code)]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// 解析 "!1"~"!4" 优先级后缀。
fn parse_pri_suffix(raw: &str) -> Option<(String, u8)> {
    let raw = raw.trim_end();
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[bytes.len() - 2] == b'!' {
        let p = bytes[bytes.len() - 1] - b'0';
        if (1..=4).contains(&p) {
            return Some((raw[..raw.len() - 2].trim_end().to_string(), p));
        }
    }
    None
}

/// 递归收集可见子任务行：(id, 深度)。未被展开的节点不进入子级。
fn collect_subs(rows: &mut Vec<(String, u8)>, subs: &[crate::model::Subtask], depth: u8, expanded: &[String]) {
    for s in subs {
        if s.deleted_at.is_some() {
            continue; // 墓碑子任务不显示
        }
        rows.push((s.id.clone(), depth));
        if expanded.iter().any(|e| e == &s.id) {
            collect_subs(rows, &s.subs, depth + 1, expanded);
        }
    }
}

/// 收集从顶级任务到 id 的各层中间祖先（不含顶级，含 id），用于展开链。
fn collect_ancestors(subs: &[crate::model::Subtask], id: &str, chain: &mut Vec<String>) {
    for s in subs {
        if s.id == id {
            chain.push(s.id.clone());
            return;
        }
        if crate::model::subs_contain(&s.subs, id) {
            chain.push(s.id.clone());
            collect_ancestors(&s.subs, id, chain);
            return;
        }
    }
}

use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pri_suffix_parsed() {
        assert_eq!(
            parse_pri_suffix("买咖啡 !3"),
            Some(("买咖啡".into(), 3))
        );
        assert_eq!(parse_pri_suffix("普通任务"), None);
        assert_eq!(parse_pri_suffix("非法 !7"), None);
    }
}
