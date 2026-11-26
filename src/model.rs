//! 领域模型：与两端（doit-apk / doit-desktop）共用的 SnapshotV2 数据契约。
//!
//! 优先级契约（与手机端一致）：0 不设 / 1 低 / 2 普通 / 3 高 / 4 紧急。
//! 时间戳统一为 i64 毫秒（UTC）。
//!
//! ## 数据契约（2026-11-25 完整对齐桌面端）
//! 磁盘 data.json 与 WebDAV 远端一律使用桌面端 wire 格式：
//!   - camelCase 字段（createdAt / parentUid / deletedAt ...）
//!   - 扁平 tasks[] + parentUid（不再是嵌套 subs）
//!   - 墓碑 deletedAt（删除不物理移除，靠墓碑跨端传播）
//!   - 快照级 exportedAt / lastSyncAt
//! 内部仍用嵌套树（Task { subs }）方便 TUI 渲染与树操作；`Snapshot` 的 serde
//! 通过 WireSnapshot 双向转换，在序列化边界完成「树 ↔ 扁平」与 camelCase 映射。

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
    /// 删除墓碑（与桌面端 Task.deletedAt 对齐）：None = 活跃。
    #[serde(default)]
    pub deleted_at: Option<i64>,
    /// 同级排序（对应桌面端 sortOrder）。
    #[serde(default)]
    pub sort_order: i64,
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
    /// 删除墓碑（与桌面端 Task.deletedAt 对齐）。
    #[serde(default)]
    pub deleted_at: Option<i64>,
    /// 同级排序（对应桌面端 sortOrder）。
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub color: String,
    /// 同级排序（对应桌面端 Category.sortOrder）。
    #[serde(default)]
    pub sort_order: i64,
    /// 更新时间（对应桌面端 Category.updatedAt，用于墓碑判定）。
    #[serde(default)]
    pub updated_at: i64,
    /// 删除墓碑（对应桌面端 Category.deletedAt）。
    #[serde(default)]
    pub deleted_at: Option<i64>,
}

/// 内部快照：serde 序列化时自动转成桌面端 WireSnapshot（扁平 + camelCase + 墓碑）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "WireSnapshot", into = "WireSnapshot")]
pub struct Snapshot {
    pub version: u32,
    pub revision: u64,
    pub exported_at: i64,
    pub device_id: String,
    pub last_sync_at: Option<i64>,
    pub categories: Vec<Category>,
    pub tasks: Vec<Task>,
}

// ------------------------- Wire 契约（桌面端一致） -------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WireCategory {
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WireTask {
    pub uid: String,
    #[serde(default)]
    pub parent_uid: Option<String>,
    pub title: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub deleted_at: Option<i64>,
    #[serde(default)]
    pub depth: u32,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSnapshot {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub exported_at: i64,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub last_sync_at: Option<i64>,
    #[serde(default)]
    pub categories: Vec<WireCategory>,
    #[serde(default)]
    pub tasks: Vec<WireTask>,
}

fn default_version() -> u32 {
    SNAPSHOT_VERSION
}

impl From<Snapshot> for WireSnapshot {
    fn from(s: Snapshot) -> Self {
        let mut tasks = Vec::new();
        // 深度优先展开内部树为扁平列表；depth 由遍历层级计算。
        for (i, t) in s.tasks.iter().enumerate() {
            flatten_task(t, None, t.sort_order_if_negative(i as i64), 0, &mut tasks);
        }
        let categories = s
            .categories
            .into_iter()
            .enumerate()
            .map(|(i, c)| WireCategory {
                name: c.name,
                color: crate::theme::normalize_category_color(&c.color),
                sort_order: if c.sort_order < 0 { i as i64 } else { c.sort_order },
                updated_at: c.updated_at,
                deleted_at: c.deleted_at,
            })
            .collect();
        WireSnapshot {
            version: s.version,
            revision: s.revision,
            exported_at: s.exported_at,
            device_id: s.device_id,
            last_sync_at: s.last_sync_at,
            categories,
            tasks,
        }
    }
}

impl Task {
    fn sort_order_if_negative(&self, fallback: i64) -> i64 {
        if self.sort_order < 0 { fallback } else { self.sort_order }
    }
}

fn flatten_task(t: &Task, parent: Option<&str>, sort_order: i64, depth: u32, out: &mut Vec<WireTask>) {
    // 墓碑任务仍写入 wire（供跨端传播）；其子树随父墓碑一起保留但各自带墓碑标记。
    out.push(WireTask {
        uid: t.id.clone(),
        parent_uid: parent.map(str::to_string),
        title: t.title.clone(),
        category: t.category.clone(),
        priority: t.priority,
        done: t.done,
        completed_at: t.completed_at,
        created_at: t.created_at,
        updated_at: t.updated_at,
        deleted_at: t.deleted_at,
        depth,
        sort_order,
    });
    for (i, sub) in t.subs.iter().enumerate() {
        flatten_sub(sub, &t.id, if sub.sort_order < 0 { i as i64 } else { sub.sort_order }, depth + 1, out);
    }
}

fn flatten_sub(s: &Subtask, parent: &str, sort_order: i64, depth: u32, out: &mut Vec<WireTask>) {
    out.push(WireTask {
        uid: s.id.clone(),
        parent_uid: Some(parent.to_string()),
        title: s.title.clone(),
        category: String::new(), // 子任务分类跟随父，wire 里置空；桌面端子任务 category 也常为空
        priority: s.priority,
        done: s.done,
        completed_at: s.completed_at,
        created_at: s.created_at,
        updated_at: s.updated_at,
        deleted_at: s.deleted_at,
        depth,
        sort_order,
    });
    for (i, child) in s.subs.iter().enumerate() {
        flatten_sub(child, &s.id, if child.sort_order < 0 { i as i64 } else { child.sort_order }, depth + 1, out);
    }
}

impl From<WireSnapshot> for Snapshot {
    fn from(w: WireSnapshot) -> Self {
        let categories = w
            .categories
            .into_iter()
            .map(|c| Category {
                name: c.name,
                color: crate::theme::normalize_category_color(&c.color),
                sort_order: c.sort_order,
                updated_at: c.updated_at,
                deleted_at: c.deleted_at,
            })
            .collect();

        // uid -> WireTask 引用，按 parentUid 递归构建树（避免 HashMap 借用冲突）。
        use std::collections::HashMap;
        let by_uid: HashMap<&str, &WireTask> = w.tasks.iter().map(|t| (t.uid.as_str(), t)).collect();
        let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
        let mut roots: Vec<String> = Vec::new();
        for t in &w.tasks {
            match &t.parent_uid {
                Some(p) if !p.is_empty() && p != &t.uid && by_uid.contains_key(p.as_str()) => {
                    children_of.entry(p.clone()).or_default().push(t.uid.clone());
                }
                _ => roots.push(t.uid.clone()),
            }
        }
        for kids in children_of.values_mut() {
            kids.sort_by_key(|id| by_uid.get(id.as_str()).map(|t| t.sort_order).unwrap_or(0));
        }
        roots.sort_by_key(|id| by_uid.get(id.as_str()).map(|t| t.sort_order).unwrap_or(0));

        fn wire_to_sub(t: &WireTask, children_of: &HashMap<String, Vec<String>>, by_uid: &HashMap<&str, &WireTask>) -> Subtask {
            let mut subs = Vec::new();
            if let Some(kids) = children_of.get(&t.uid) {
                for kid in kids {
                    if let Some(kt) = by_uid.get(kid.as_str()) {
                        subs.push(wire_to_sub(kt, children_of, by_uid));
                    }
                }
            }
            Subtask {
                id: t.uid.clone(),
                title: t.title.clone(),
                priority: t.priority.min(4),
                done: t.done,
                created_at: t.created_at,
                updated_at: t.updated_at,
                completed_at: if t.done { t.completed_at } else { None },
                subs,
                deleted_at: t.deleted_at,
                sort_order: t.sort_order,
            }
        }

        // 顶级任务：用 WireTask 直接构造 Task（字段同构），递归挂子树。
        let mut tasks: Vec<Task> = Vec::new();
        for rid in &roots {
            if let Some(t) = by_uid.get(rid.as_str()) {
                let subs = children_of
                    .get(rid)
                    .map(|kids| {
                        kids.iter()
                            .filter_map(|kid| by_uid.get(kid.as_str()).map(|kt| wire_to_sub(kt, &children_of, &by_uid)))
                            .collect()
                    })
                    .unwrap_or_default();
                tasks.push(Task {
                    id: t.uid.clone(),
                    title: t.title.clone(),
                    category: t.category.clone(),
                    priority: t.priority.min(4),
                    done: t.done,
                    created_at: t.created_at,
                    updated_at: t.updated_at,
                    completed_at: if t.done { t.completed_at } else { None },
                    subs,
                    deleted_at: t.deleted_at,
                    sort_order: t.sort_order,
                });
            }
        }
        // 防御：任何未被 roots/children 覆盖的 WireTask（异常环/悬空引用）提升为顶级，避免丢数据。
        let covered: std::collections::HashSet<String> = w.tasks.iter().filter(|t| roots.contains(&t.uid)).map(|t| t.uid.clone()).collect();
        for t in &w.tasks {
            if !covered.contains(&t.uid)
                && matches!(&t.parent_uid, Some(p) if by_uid.contains_key(p.as_str()))
            {
                // 是某节点的子节点但已被递归挂接；跳过
                continue;
            }
            if !covered.contains(&t.uid) && !tasks.iter().any(|x| x.id == t.uid) {
                // 真正孤立的节点（父不存在）→ 提升为顶级
                tasks.push(Task {
                    id: t.uid.clone(),
                    title: t.title.clone(),
                    category: t.category.clone(),
                    priority: t.priority.min(4),
                    done: t.done,
                    created_at: t.created_at,
                    updated_at: t.updated_at,
                    completed_at: if t.done { t.completed_at } else { None },
                    subs: Vec::new(),
                    deleted_at: t.deleted_at,
                    sort_order: t.sort_order,
                });
            }
        }
        tasks.sort_by_key(|t| t.sort_order);

        Snapshot {
            version: if w.version == 0 { SNAPSHOT_VERSION } else { w.version },
            revision: w.revision,
            exported_at: w.exported_at,
            device_id: w.device_id,
            last_sync_at: w.last_sync_at,
            categories,
            tasks,
        }
    }
}

impl Snapshot {
    pub fn new(device_id: &str) -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            revision: 0,
            exported_at: now_ms(),
            device_id: device_id.to_string(),
            last_sync_at: None,
            categories: Vec::new(),
            tasks: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.revision += 1;
        self.exported_at = now_ms();
    }

    /// 活跃（未删除）顶级任务，供 UI 显示。
    pub fn active_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|t| t.deleted_at.is_none())
    }

    /// 活跃（未删除）分类，供 UI 显示。
    pub fn active_categories(&self) -> impl Iterator<Item = &Category> {
        self.categories.iter().filter(|c| c.deleted_at.is_none())
    }

    /// 递归完成联动（recalcUp 语义）：有子节点的节点完成 = 所有子节点均完成，
    /// 自底向上重算顶级任务整棵子树并同步 completed_at。
    pub fn recalc_parent(&mut self, task_id: &str) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            let now = now_ms();
            recalc_subs(&mut t.subs, now);
            let live: Vec<&Subtask> = t.subs.iter().filter(|s| s.deleted_at.is_none()).collect();
            let all_done = !live.is_empty() && live.iter().all(|s| subtree_done(s));
            if all_done && !t.done {
                t.done = true;
                t.updated_at = now;
                t.completed_at = Some(now);
            } else if !all_done && t.done && !live.is_empty() {
                t.done = false;
                t.updated_at = now;
                t.completed_at = None;
            }
        }
    }

    /// 载入 / 同步后自愈：历史上删子任务没重算父任务，会留下"子任务全完成但父任务未完成"。
    pub fn recalc_all(&mut self) {
        let ids: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| t.deleted_at.is_none())
            .map(|t| t.id.clone())
            .collect();
        for id in ids {
            self.recalc_parent(&id);
        }
    }
}

/// 子树整体完成：墓碑子任务视为不存在（完成判定跳过）；活跃叶子看自身 done。
pub fn subtree_done(u: &Subtask) -> bool {
    if u.deleted_at.is_some() {
        return true;
    }
    let live: Vec<&Subtask> = u.subs.iter().filter(|s| s.deleted_at.is_none()).collect();
    if live.is_empty() {
        u.done
    } else {
        live.iter().all(|s| subtree_done(s))
    }
}

fn recalc_subs(subs: &mut [Subtask], now: i64) {
    for s in subs.iter_mut() {
        if s.deleted_at.is_some() {
            continue;
        }
        recalc_subs(&mut s.subs, now);
        let live: Vec<&Subtask> = s.subs.iter().filter(|c| c.deleted_at.is_none()).collect();
        if !live.is_empty() {
            let all = live.iter().all(|c| subtree_done(c));
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
#[allow(dead_code)]
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

/// 子树内按 id 查找可变引用并打墓碑（不物理移除）。
pub fn tombstone_sub(subs: &mut [Subtask], id: &str, now: i64) -> bool {
    for s in subs.iter_mut() {
        if s.id == id {
            s.deleted_at = Some(now);
            s.updated_at = now;
            return true;
        }
        if tombstone_sub(&mut s.subs, id, now) {
            return true;
        }
    }
    false
}

/// 子树内是否包含某活跃节点（忽略墓碑）。
#[allow(dead_code)]
pub fn active_subs_contain(subs: &[Subtask], id: &str) -> bool {
    subs.iter().any(|s| {
        s.deleted_at.is_none() && (s.id == id || active_subs_contain(&s.subs, id))
    })
}

/// 子树的活跃叶子（无子节点）总数（忽略墓碑）。
pub fn leaf_count(subs: &[Subtask]) -> usize {
    subs.iter()
        .filter(|s| s.deleted_at.is_none())
        .map(|s| leaf_count(&s.subs).max(1))
        .sum()
}

/// 子树中已完成的活跃叶子数（忽略墓碑）。
pub fn done_leaf_count(subs: &[Subtask]) -> usize {
    subs.iter()
        .filter(|s| s.deleted_at.is_none())
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


// ------------------------- 旧格式兼容读取 -------------------------

/// 旧版（2026-11-25 之前）本地 data.json 的结构：snake_case + 嵌套 subs + 无墓碑。
/// 仅为兼容读取历史数据；写盘一律走新 wire 契约。
mod legacy {
    use super::*;
    #[derive(Deserialize)]
    pub struct LegacySubtask {
        pub id: String,
        #[serde(default)] pub title: String,
        #[serde(default)] pub priority: u8,
        #[serde(default)] pub done: bool,
        #[serde(default)] pub created_at: i64,
        #[serde(default)] pub updated_at: i64,
        #[serde(default)] pub completed_at: Option<i64>,
        #[serde(default)] pub subs: Vec<LegacySubtask>,
    }
    #[derive(Deserialize)]
    pub struct LegacyTask {
        pub id: String,
        #[serde(default)] pub title: String,
        #[serde(default)] pub category: String,
        #[serde(default)] pub priority: u8,
        #[serde(default)] pub done: bool,
        #[serde(default)] pub created_at: i64,
        #[serde(default)] pub updated_at: i64,
        #[serde(default)] pub completed_at: Option<i64>,
        #[serde(default)] pub subs: Vec<LegacySubtask>,
    }
    #[derive(Deserialize)]
    pub struct LegacyCategory {
        pub name: String,
        #[serde(default)] pub color: String,
    }
    #[derive(Deserialize)]
    pub struct LegacySnapshot {
        #[serde(default)] pub version: u32,
        #[serde(default)] pub revision: u64,
        #[serde(default, rename = "deviceId")] pub device_id: String,
        #[serde(default)] pub categories: Vec<LegacyCategory>,
        #[serde(default)] pub tasks: Vec<LegacyTask>,
    }

    fn sub_from(s: LegacySubtask) -> Subtask {
        Subtask {
            id: s.id, title: s.title, priority: s.priority.min(4), done: s.done,
            created_at: s.created_at, updated_at: s.updated_at, completed_at: s.completed_at,
            subs: s.subs.into_iter().map(sub_from).collect(),
            deleted_at: None, sort_order: 0,
        }
    }

    pub fn to_snapshot(l: LegacySnapshot) -> Snapshot {
        let now = now_ms();
        Snapshot {
            version: if l.version == 0 { SNAPSHOT_VERSION } else { l.version },
            revision: l.revision,
            exported_at: now,
            device_id: if l.device_id.is_empty() { "doit-tui".into() } else { l.device_id },
            last_sync_at: None,
            categories: l.categories.into_iter().enumerate().map(|(i, c)| Category {
                name: c.name,
                color: crate::theme::normalize_category_color(&c.color),
                sort_order: i as i64, updated_at: now, deleted_at: None,
            }).collect(),
            tasks: l.tasks.into_iter().enumerate().map(|(i, t)| Task {
                id: t.id, title: t.title, category: t.category, priority: t.priority.min(4),
                done: t.done, created_at: t.created_at, updated_at: t.updated_at,
                completed_at: t.completed_at, subs: t.subs.into_iter().map(sub_from).collect(),
                deleted_at: None, sort_order: i as i64,
            }).collect(),
        }
    }
}

/// 统一快照读取：先按新 wire 契约解析；若任务的身份键是旧 `id`（无 `uid`），判定为旧格式，
/// 走 legacy 解析并归一化（保留嵌套树、补齐墓碑/排序字段、归一化颜色），再以新格式写回。
pub fn parse_snapshot(raw: &str) -> serde_json::Result<Snapshot> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let is_legacy = value
        .get("tasks")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.first())
        .map(|t| t.get("id").is_some() && t.get("uid").is_none())
        .unwrap_or(false);
    if is_legacy {
        let legacy: legacy::LegacySnapshot = serde_json::from_value(value)?;
        return Ok(legacy::to_snapshot(legacy));
    }
    serde_json::from_value::<WireSnapshot>(value).map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(id: &str, title: &str) -> Subtask {
        Subtask {
            id: id.into(), title: title.into(), priority: 2, done: false,
            created_at: 100, updated_at: 100, completed_at: None,
            subs: vec![], deleted_at: None, sort_order: 0,
        }
    }

    #[test]
    fn wire_roundtrip_preserves_nested_tree() {
        let mut child = sub("c", "子任务");
        child.subs.push(sub("g", "孙任务"));
        let mut parent = sub("s", "子任务2");
        parent.subs.push(sub("g2", "孙任务2"));
        let task = Task {
            id: "t1".into(), title: "顶级".into(), category: "工作".into(),
            priority: 2, done: false, created_at: 100, updated_at: 100,
            completed_at: None, subs: vec![child, parent],
            deleted_at: None, sort_order: 0,
        };
        let mut snap = Snapshot::new("test");
        snap.categories.push(Category {
            name: "工作".into(), color: "#5AA9E6".into(), sort_order: 0,
            updated_at: 100, deleted_at: None,
        });
        snap.tasks.push(task);

        let wire = WireSnapshot::from(snap.clone());
        // 扁平化后应有 6 条：t1 + s + g + s2 + g2 共 5 条（t1,s,g,s2,g2）
        assert!(wire.tasks.iter().all(|t| t.parent_uid.is_some() || t.uid == "t1"));
        let back: Snapshot = wire.into();
        assert_eq!(back.tasks.len(), 1);
        assert_eq!(back.tasks[0].subs.len(), 2);
        assert_eq!(back.tasks[0].subs[0].subs.len(), 1, "孙任务应正确还原");
        assert_eq!(back.tasks[0].subs[1].subs.len(), 1, "第二个子树的孙任务应正确还原");
        assert_eq!(back.tasks[0].subs[0].subs[0].title, "孙任务");
    }

    #[test]
    fn legacy_data_json_is_read_without_losing_subtasks() {
        // 模拟旧版 data.json：snake_case + 嵌套 subs + 无墓碑/排序字段。
        let raw = r#"{
          "version": 2, "revision": 39, "deviceId": "tui-old",
          "categories": [{"name":"工作","color":"\u00234A86D6"}],
          "tasks": [{
            "id":"t1","title":"顶级","category":"工作","priority":3,"done":false,
            "created_at":100,"updated_at":100,"completed_at":null,
            "subs":[{"id":"s1","title":"子任务","priority":2,"done":true,
                     "created_at":100,"updated_at":100,"completed_at":100,"subs":[]}]
          }]
        }"#;
        let snap = parse_snapshot(raw).expect("旧格式应能解析");
        assert_eq!(snap.tasks.len(), 1);
        assert_eq!(snap.tasks[0].id, "t1");
        assert_eq!(snap.tasks[0].subs.len(), 1, "旧嵌套子任务不能丢失");
        assert_eq!(snap.tasks[0].subs[0].title, "子任务");
        assert_eq!(snap.categories[0].color, "#4A86D6");
        // 写回后用新格式能再次读取
        let written = serde_json::to_string(&snap).unwrap();
        let reparsed = parse_snapshot(&written).expect("新格式应能解析");
        assert_eq!(reparsed.tasks[0].subs.len(), 1, "round-trip 后子任务仍在");
    }

    #[test]
    fn normalize_category_color_formats() {
        assert_eq!(crate::theme::normalize_category_color("5B9BD5"), "#5B9BD5");
        assert_eq!(crate::theme::normalize_category_color("#34c7a8"), "#34C7A8");
        assert_eq!(crate::theme::normalize_category_color("red"), "#34C7A8");
        assert_eq!(crate::theme::normalize_category_color(""), "#34C7A8");
    }

    fn task_with_subs(subs: Vec<Subtask>) -> Task {
        Task {
            id: "t1".into(), title: "顶级".into(), category: "工作".into(), priority: 2,
            done: false, created_at: 100, updated_at: 100, completed_at: None,
            subs, deleted_at: None, sort_order: 0,
        }
    }

    #[test]
    fn tombstoned_subtask_does_not_block_parent_completion() {
        let mut done = sub("a", "已完成");
        done.done = true;
        let mut deleted = sub("b", "已删除");
        deleted.deleted_at = Some(200);
        let mut snap = Snapshot::new("test");
        snap.tasks.push(task_with_subs(vec![done, deleted]));
        snap.recalc_all();
        let t = &snap.tasks[0];
        assert!(t.done, "有效子任务全完成时父任务应自动完成");
        assert!(t.completed_at.is_some());
    }

    #[test]
    fn tombstoned_grandchild_does_not_block_nested_completion() {
        let mut inner_done = sub("g", "孙任务");
        inner_done.done = true;
        let mut inner_deleted = sub("g2", "已删除孙任务");
        inner_deleted.deleted_at = Some(200);
        let mut outer = sub("c", "子任务");
        outer.subs = vec![inner_done, inner_deleted];
        let mut snap = Snapshot::new("test");
        snap.tasks.push(task_with_subs(vec![outer]));
        snap.recalc_all();
        assert!(snap.tasks[0].subs[0].done, "墓碑孙任务不应阻塞子任务完成");
        assert!(snap.tasks[0].done, "子任务全完成时父任务应自动完成");
    }

    #[test]
    fn active_undone_subtask_still_blocks_parent() {
        let mut done = sub("a", "已完成");
        done.done = true;
        let mut snap = Snapshot::new("test");
        snap.tasks.push(task_with_subs(vec![done, sub("b", "未完成")]));
        snap.recalc_all();
        assert!(!snap.tasks[0].done);
    }
}
