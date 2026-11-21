//! 领域模型：与两端（doit-apk / doit-desktop）共用的 SnapshotV2 数据契约。
//!
//! 优先级契约（与手机端一致）：0 不设 / 1 低 / 2 普通 / 3 高 / 4 紧急。
//! 时间戳统一为 i64 毫秒（UTC）。

use serde::{Deserialize, Serialize};

pub const SNAPSHOT_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Priority(pub u8);

impl Priority {
    #[allow(dead_code)]
    pub fn value(self) -> u8 {
        self.0.min(4)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    /// 递归子任务：支持无限层级嵌套（旧数据无此字段 → 默认为空）。
    #[serde(default)]
    pub subs: Vec<Subtask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub category: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub subs: Vec<Subtask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub color: String,
}

/// SnapshotV2：version + revision + deviceId + categories + tasks（平铺顶级任务列表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub revision: u64,
    #[serde(rename = "deviceId")]
    pub device_id: String,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub tasks: Vec<Task>,
}

impl Snapshot {
    pub fn new(device_id: &str) -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            revision: 0,
            device_id: device_id.to_string(),
            categories: Vec::new(),
            tasks: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.revision += 1;
    }

    /// 递归完成联动（recalcUp 语义）：有子节点的节点完成 = 所有子节点均完成，
    /// 自底向上重算顶级任务整棵子树并同步 completed_at。
    pub fn recalc_parent(&mut self, task_id: &str) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            let now = now_ms();
            recalc_subs(&mut t.subs, now);
            let all_done = !t.subs.is_empty() && t.subs.iter().all(subtree_done);
            if all_done && !t.done {
                t.done = true;
                t.updated_at = now;
                t.completed_at = Some(now);
            } else if !all_done && t.done && !t.subs.is_empty() {
                t.done = false;
                t.updated_at = now;
                t.completed_at = None;
            }
        }
    }
}

/// 子树整体完成：叶子节点看自身 done；有子节点的节点看所有后代叶子。
pub fn subtree_done(u: &Subtask) -> bool {
    if u.subs.is_empty() {
        u.done
    } else {
        u.subs.iter().all(subtree_done)
    }
}

fn recalc_subs(subs: &mut [Subtask], now: i64) {
    for s in subs.iter_mut() {
        recalc_subs(&mut s.subs, now);
        if !s.subs.is_empty() {
            let all = s.subs.iter().all(subtree_done);
            if all && !s.done {
                s.done = true;
                s.updated_at = now;
                s.completed_at = Some(now);
            } else if !all && s.done {
                s.done = false;
                s.updated_at = now;
                s.completed_at = None;
            }
        }
    }
}

/// 递归设置某个节点及其所有后代的完成状态（用于整棵子树切换完成）。
pub fn set_node_done(s: &mut Subtask, done: bool, now: i64) {
    s.done = done;
    s.updated_at = now;
    s.completed_at = if done { Some(now) } else { None };
    for c in &mut s.subs {
        set_node_done(c, done, now);
    }
}

/// 递归查找某深度子任务（按 id）。
pub fn find_sub<'a>(subs: &'a [Subtask], id: &str) -> Option<&'a Subtask> {
    for s in subs {
        if s.id == id {
            return Some(s);
        }
        if let Some(f) = find_sub(&s.subs, id) {
            return Some(f);
        }
    }
    None
}

pub fn find_sub_mut<'a>(subs: &'a mut [Subtask], id: &str) -> Option<&'a mut Subtask> {
    for s in subs.iter_mut() {
        if s.id == id {
            return Some(s);
        }
        if let Some(f) = find_sub_mut(&mut s.subs, id) {
            return Some(f);
        }
    }
    None
}

/// 子树内是否包含某节点（任意深度）。
pub fn subs_contain(subs: &[Subtask], id: &str) -> bool {
    subs.iter().any(|s| s.id == id || subs_contain(&s.subs, id))
}

/// 递归移除某节点，返回被移除的节点。
pub fn remove_sub(subs: &mut Vec<Subtask>, id: &str) -> Option<Subtask> {
    if let Some(pos) = subs.iter().position(|s| s.id == id) {
        return Some(subs.remove(pos));
    }
    for s in subs.iter_mut() {
        if let Some(f) = remove_sub(&mut s.subs, id) {
            return Some(f);
        }
    }
    None
}

/// 子树的叶子（无子节点）总数。
pub fn leaf_count(subs: &[Subtask]) -> usize {
    subs.iter().map(|s| leaf_count(&s.subs).max(1)).sum()
}

/// 子树中已完成的叶子数。
pub fn done_leaf_count(subs: &[Subtask]) -> usize {
    subs.iter()
        .map(|s| if s.subs.is_empty() { if s.done { 1 } else { 0 } } else { done_leaf_count(&s.subs) })
        .sum()
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn completed_on(ts: i64) -> chrono::NaiveDate {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ts)
        .single()
        .map(|d| d.date_naive())
        .unwrap_or_else(|| chrono::Local::now().date_naive())
}
