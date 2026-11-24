//! WebDAV 同步：SnapshotV2 契约 + 三方合并（base / local / remote）。
//!
//! 与两端共用远端文件 doit-backup.json；冲突按 updatedAt 新者胜。
//! 未启用或未配置时 `s` 仅提示。

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;

use crate::model::{Snapshot, WireCategory, WireSnapshot, WireTask};
use crate::store::{Store, WebDavSettings};

pub fn remote_file_name(cfg: &WebDavSettings) -> String {
    if cfg.remote_file.trim().is_empty() {
        "doit-backup.json".into()
    } else {
        cfg.remote_file.trim().to_string()
    }
}

fn auth_header(cfg: &WebDavSettings) -> Option<String> {
    if cfg.username.is_empty() && cfg.password.is_empty() {
        return None;
    }
    Some(format!(
        "Basic {}",
        B64.encode(format!("{}:{}", cfg.username, cfg.password))
    ))
}

/// GET 远端文件。404 视为远端不存在（返回 None）。
fn dav_get(cfg: &WebDavSettings, file: &str) -> Result<Option<String>> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let url = format!("{}/{}", cfg.url.trim_end_matches('/'), file);
    let mut req = agent.get(&url);
    if let Some(auth) = auth_header(cfg) {
        req = req.set("Authorization", &auth);
    }
    match req.call() {
        Ok(resp) => Ok(Some(resp.into_string()?)),
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(anyhow!("拉取远端快照失败: {e}")),
    }
}

fn dav_put(cfg: &WebDavSettings, file: &str, body: &str) -> Result<()> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let url = format!("{}/{}", cfg.url.trim_end_matches('/'), file);
    let mut req = agent.put(&url);
    if let Some(auth) = auth_header(cfg) {
        req = req.set("Authorization", &auth);
    }
    req.send_string(body)?;
    Ok(())
}

/// 执行一次同步：GET 远端 → 三方合并 → PUT → 更新 base。
/// 返回（合并后的快照, 说明文字）。
pub fn sync(store: &Store, local: &Snapshot, cfg: &WebDavSettings) -> Result<(Snapshot, String)> {
    if !cfg.enabled {
        bail!("同步未启用（设置页开启后再按 s）");
    }
    if cfg.url.trim().is_empty() {
        bail!("未配置 WebDAV 服务器地址");
    }
    let file = remote_file_name(cfg);
    let base = store.load_sync_base();

    // 统一在桌面端 wire 契约上做合并（camelCase + 扁平 + 墓碑）。
    let local_wire = WireSnapshot::from(local.clone());
    let base_wire = base.as_ref().map(|b| WireSnapshot::from(b.clone()));
    let remote_wire: Option<WireSnapshot> = match dav_get(cfg, &file)? {
        None => None,
        Some(raw) if raw.trim().is_empty() => None,
        Some(raw) => {
            // 远端可能是桌面端/手机端的新 wire 格式，也可能是旧格式；统一走兼容解析后转 wire。
            let snap = crate::model::parse_snapshot(&raw)
                .with_context(|| "远端快照解析失败（契约不一致？）".to_string())?;
            Some(WireSnapshot::from(snap))
        }
    };

    let merged_wire = merge_wire(base_wire.as_ref(), &local_wire, remote_wire.as_ref());
    let mut merged_wire = merged_wire;
    merged_wire.device_id = local.device_id.clone();
    merged_wire.version = crate::model::SNAPSHOT_VERSION;
    let task_count = merged_wire.tasks.len();
    let merged: Snapshot = merged_wire.into();

    dav_put(cfg, &file, &serde_json::to_string(&WireSnapshot::from(merged.clone()))?)?;
    store.save_sync_base(&merged)?;
    let note = format!("同步完成 · r{} · {} 个任务", merged.revision, task_count);
    Ok((merged, note))
}

/// 桌面端 domain/merge.ts 的字段级三方合并（在 wire 契约上执行）。
/// 语义：并集保留、字段级合并、同字段冲突按 effectiveTime 取新（平局远程优先）、
/// 墓碑删除（删除后对方真编辑才复活）、陈旧远端检测。

fn effective_time(t: &WireTask) -> i64 {
    [
        t.updated_at,
        t.completed_at.unwrap_or(0),
        t.deleted_at.unwrap_or(0),
        t.created_at,
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

pub fn is_stale_remote(remote: &WireSnapshot, local: &WireSnapshot) -> bool {
    remote.exported_at < local.exported_at && remote.revision <= local.revision
}

/// 稳定序列化用于内容指纹比较（对象键排序，这里 structs 字段固定，直接 serde_json 即可）。
#[allow(dead_code)]
fn same_task(a: &WireTask, b: &WireTask) -> bool {
    a == b
}
#[allow(dead_code)]
fn same_cat(a: &WireCategory, b: &WireCategory) -> bool {
    a == b
}

fn merge_task(local: &WireTask, remote: &WireTask, base: Option<&WireTask>) -> WireTask {
    let mut out = local.clone();
    let le = effective_time(local);
    let re = effective_time(remote);
    // TASK_FIELDS: parentUid, title, category, priority, done, completedAt, createdAt, deletedAt, depth, sortOrder
    // 逐字段合并；这里对每个字段用 (local,remote,base) 的三方规则。
    macro_rules! merge_field {
        ($field:ident) => {{
            let lv = local.$field.clone();
            let rv = remote.$field.clone();
            let bv = base.map(|b| b.$field.clone());
            let local_changed = bv.is_none() || lv != bv.clone().unwrap();
            let remote_changed = bv.is_none() || rv != bv.clone().unwrap();
            if local_changed && !remote_changed {
                out.$field = lv;
            } else if !local_changed && remote_changed {
                out.$field = rv;
            } else if local_changed && remote_changed && lv != rv {
                out.$field = if le > re { lv } else { rv }; // 平局远程优先
            } else if let Some(b) = bv {
                out.$field = b;
            }
        }};
    }
    merge_field!(parent_uid);
    merge_field!(title);
    merge_field!(category);
    merge_field!(priority);
    merge_field!(done);
    merge_field!(completed_at);
    merge_field!(created_at);
    merge_field!(deleted_at);
    merge_field!(depth);
    merge_field!(sort_order);

    // 墓碑 vs 编辑：删除之后对方又编辑过 → 撤销删除（编辑胜），否则删除生效。
    let ldel = local.deleted_at;
    let rdel = remote.deleted_at;
    if ldel.is_some() || rdel.is_some() {
        if ldel.is_some() && rdel.is_none() && re > ldel.unwrap() {
            out.deleted_at = None;
        } else if rdel.is_some() && ldel.is_none() && le > rdel.unwrap() {
            out.deleted_at = None;
        } else {
            out.deleted_at = Some(ldel.unwrap_or(0).max(rdel.unwrap_or(0))).filter(|v| *v != 0);
        }
    }
    // 完成态自洽
    if !out.done { out.completed_at = None; }
    if out.done && out.completed_at.is_none() { out.completed_at = Some(le.max(re)); }
    out.updated_at = le.max(re);
    out.uid = local.uid.clone();
    out
}

fn merge_category(local: &WireCategory, remote: &WireCategory, base: Option<&WireCategory>) -> WireCategory {
    let ldel = local.deleted_at;
    let rdel = remote.deleted_at;
    if ldel.is_some() || rdel.is_some() {
        if ldel.is_some() && rdel.is_some() {
            let newer = if ldel.unwrap() >= rdel.unwrap() { local } else { remote };
            let mut c = newer.clone();
            c.deleted_at = Some(ldel.unwrap().max(rdel.unwrap()));
            c.updated_at = local.updated_at.max(remote.updated_at);
            return c;
        }
        let dead = if ldel.is_some() { local } else { remote };
        let live = if ldel.is_some() { remote } else { local };
        let dead_at = dead.deleted_at.unwrap();
        // 用「可见内容（color/sortOrder）相对 base 是否真变化」判断编辑，不看 updatedAt，
        // 避免同步回写刷新 updatedAt 导致已删分类被误复活。
        let live_edited = base.map(|b| {
            live.color != b.color || live.sort_order != b.sort_order
        }).unwrap_or(false);
        let delete_wins = !live_edited || live.updated_at <= dead_at;
        if delete_wins {
            let mut c = dead.clone();
            c.deleted_at = Some(dead_at);
            c.updated_at = local.updated_at.max(remote.updated_at);
            return c;
        }
        // 删除后真编辑过 → 复活
        let mut c = live.clone();
        c.deleted_at = None;
        c.updated_at = local.updated_at.max(remote.updated_at);
        return c;
    }
    let mut out = local.clone();
    for (lv, rv, bv) in [
        (&local.color, &remote.color, base.map(|b| &b.color)),
    ] {
        let lc = bv.map(|b| lv != b).unwrap_or(true);
        let rc = bv.map(|b| rv != b).unwrap_or(true);
        if lc && !rc { out.color = lv.clone(); }
        else if !lc && rc { out.color = rv.clone(); }
        else if lc && rc && lv != rv { out.color = if local.updated_at > remote.updated_at { lv.clone() } else { rv.clone() }; }
        else if let Some(b) = bv { out.color = b.clone(); }
    }
    // sortOrder 字段
    let (lv, rv, bv) = (local.sort_order, remote.sort_order, base.map(|b| b.sort_order));
    let lc = bv.map(|b| lv != b).unwrap_or(true);
    let rc = bv.map(|b| rv != b).unwrap_or(true);
    if lc && !rc { out.sort_order = lv; }
    else if !lc && rc { out.sort_order = rv; }
    else if lc && rc && lv != rv { out.sort_order = if local.updated_at > remote.updated_at { lv } else { rv }; }
    else if let Some(b) = bv { out.sort_order = b; }
    out.name = local.name.clone();
    out.updated_at = local.updated_at.max(remote.updated_at);
    out
}

/// 三方合并入口。base=None 时：本地与远端做字段级双源合并（base 视作空）。
pub fn merge_wire(base: Option<&WireSnapshot>, local: &WireSnapshot, remote: Option<&WireSnapshot>) -> WireSnapshot {
    let Some(remote) = remote else {
        return local.clone();
    };
    // 陈旧远端短路：远端更旧且 revision 不高于本地 → 直接用本地，跳过合并。
    if is_stale_remote(remote, local) {
        let mut out = local.clone();
        out.revision = local.revision.max(remote.revision);
        return out;
    }

    let empty = WireSnapshot { version: 2, revision: 0, exported_at: 0, device_id: "merge-base".into(), last_sync_at: None, categories: vec![], tasks: vec![] };
    let b = base.unwrap_or(&empty);

    use std::collections::HashMap;
    let lt: HashMap<&str, &WireTask> = local.tasks.iter().map(|t| (t.uid.as_str(), t)).collect();
    let rt: HashMap<&str, &WireTask> = remote.tasks.iter().map(|t| (t.uid.as_str(), t)).collect();
    let bt: HashMap<&str, &WireTask> = b.tasks.iter().map(|t| (t.uid.as_str(), t)).collect();
    let mut uids: Vec<&str> = lt.keys().chain(rt.keys()).copied().collect();
    uids.sort();
    uids.dedup();
    let mut tasks: Vec<WireTask> = Vec::new();
    for uid in uids {
        let l = lt.get(uid).copied();
        let r = rt.get(uid).copied();
        let bb = bt.get(uid).copied();
        match (l, r) {
            (Some(l), Some(r)) => tasks.push(merge_task(l, r, bb)),
            (Some(l), None) => tasks.push(l.clone()),
            (None, Some(r)) => tasks.push(r.clone()),
            (None, None) => {}
        }
    }
    tasks.sort_by(|a, b| (a.depth, a.sort_order).cmp(&(b.depth, b.sort_order)).then_with(|| a.uid.cmp(&b.uid)));

    let lc: HashMap<&str, &WireCategory> = local.categories.iter().map(|c| (c.name.as_str(), c)).collect();
    let rc: HashMap<&str, &WireCategory> = remote.categories.iter().map(|c| (c.name.as_str(), c)).collect();
    let bc: HashMap<&str, &WireCategory> = b.categories.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut names: Vec<&str> = lc.keys().chain(rc.keys()).chain(bc.keys()).copied().collect();
    names.sort();
    names.dedup();
    let mut categories: Vec<WireCategory> = Vec::new();
    for name in names {
        let l = lc.get(name).copied();
        let r = rc.get(name).copied();
        let bb = bc.get(name).copied();
        match (l, r) {
            (Some(l), Some(r)) => categories.push(merge_category(l, r, bb)),
            (Some(l), None) => categories.push(l.clone()),
            (None, Some(r)) if bb.is_some() => {
                // base 有、本地无 → 本地删除；仅当远端相对 base 更新过才恢复。
                if r.updated_at > bb.unwrap().updated_at { categories.push(r.clone()); }
            }
            (None, Some(r)) => categories.push(r.clone()),
            (None, None) => {}
        }
    }
    let now = local.exported_at.max(remote.exported_at);
    WireSnapshot {
        version: 2,
        revision: local.revision.max(remote.revision) + 1,
        exported_at: now,
        device_id: local.device_id.clone(),
        last_sync_at: Some(now),
        categories,
        tasks,
    }
}

// 兼容旧测试入口：把内部 Snapshot 转成 wire 后调用 merge_wire。
#[allow(dead_code)]
pub fn merge(base: &Option<Snapshot>, local: &Snapshot, remote: Option<&Snapshot>) -> Snapshot {
    let lw = WireSnapshot::from(local.clone());
    let bw = base.as_ref().map(|s| WireSnapshot::from(s.clone()));
    let rw = remote.map(|s| WireSnapshot::from(s.clone()));
    merge_wire(bw.as_ref(), &lw, rw.as_ref()).into()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{WireCategory, WireTask};

    fn cat(name: &str, color: &str) -> WireCategory {
        WireCategory { name: name.into(), color: color.into(), sort_order: 0, updated_at: 100, deleted_at: None }
    }
    fn task(uid: &str, cat: &str, title: &str, updated: i64) -> WireTask {
        WireTask {
            uid: uid.into(), parent_uid: None, title: title.into(), category: cat.into(),
            priority: 0, done: false, completed_at: None, created_at: 1, updated_at: updated,
            deleted_at: None, depth: 0, sort_order: 0,
        }
    }
    fn snap(tasks: Vec<WireTask>, cats: Vec<WireCategory>) -> WireSnapshot {
        WireSnapshot {
            version: 2, revision: 1, exported_at: 200, device_id: "test".into(),
            last_sync_at: None, categories: cats, tasks,
        }
    }

    #[test]
    fn category_delete_does_not_revive_from_stale_peer() {
        // 桌面删分类 → 墓碑；手机端旧快照仍含活分类 → 合并后墓碑胜，不复活。
        let mut deleted = cat("工作", "#5AA9E6");
        deleted.deleted_at = Some(300);
        deleted.updated_at = 300;
        let base = snap(vec![], vec![cat("工作", "#5AA9E6")]);
        let local = snap(vec![], vec![deleted]);
        let remote = base_snap_with_cat();
        let out = merge_wire(Some(&base), &local, Some(&remote));
        let c = out.categories.iter().find(|c| c.name == "工作").unwrap();
        assert!(c.deleted_at.is_some(), "桌面已删除的分类不应被手机端旧快照复活");
    }

    fn base_snap_with_cat() -> WireSnapshot {
        snap(vec![], vec![cat("工作", "#5AA9E6")])
    }

    #[test]
    fn category_real_edit_after_delete_revives() {
        // 删除后对方真改了颜色（内容相对 base 变化）→ 编辑胜，分类恢复。
        let mut deleted = cat("工作", "#5AA9E6");
        deleted.deleted_at = Some(300);
        deleted.updated_at = 300;
        let base = snap(vec![], vec![cat("工作", "#5AA9E6")]);
        let local = snap(vec![], vec![deleted]);
        let mut remote = snap(vec![], vec![cat("工作", "#A78BFA")]);
        remote.categories[0].updated_at = 400; // 删除后真编辑
        let out = merge_wire(Some(&base), &local, Some(&remote));
        let c = out.categories.iter().find(|c| c.name == "工作").unwrap();
        assert!(c.deleted_at.is_none(), "删除后真编辑过应恢复分类");
        assert_eq!(c.color, "#A78BFA");
    }

    #[test]
    fn task_field_level_merge() {
        // 本地改标题、远端改优先级 → 字段级合并两者都保留。
        let base = snap(vec![task("a", "工作", "原文", 1)], vec![]);
        let local = snap(vec![task("a", "工作", "本地标题", 10)], vec![]);
        let mut remote = snap(vec![task("a", "工作", "原文", 20)], vec![]);
        remote.tasks[0].priority = 4;
        let out = merge_wire(Some(&base), &local, Some(&remote));
        let t = out.tasks.iter().find(|t| t.uid == "a").unwrap();
        assert_eq!(t.title, "本地标题");
        assert_eq!(t.priority, 4);
    }

    #[test]
    fn task_tombstone_wins_unless_edited_after() {
        let base = snap(vec![task("a", "工作", "原文", 1)], vec![]);
        let mut local = snap(vec![], vec![]); // 本地删除 a
        let _ = &mut local;
        let mut local_t = snap(vec![task("a", "工作", "原文", 1)], vec![]);
        local_t.tasks[0].deleted_at = Some(300);
        local_t.tasks[0].updated_at = 300;
        let remote = snap(vec![task("a", "工作", "原文", 1)], vec![]); // 远端未动
        let out = merge_wire(Some(&base), &local_t, Some(&remote));
        let t = out.tasks.iter().find(|t| t.uid == "a").unwrap();
        assert!(t.deleted_at.is_some(), "删除后对方未编辑，墓碑应生效");
    }

    #[test]
    fn stale_remote_short_circuits() {
        let local = WireSnapshot { revision: 10, exported_at: 500, ..snap(vec![task("a","工作","本地",10)], vec![]) };
        let remote = WireSnapshot { revision: 5, exported_at: 100, ..snap(vec![task("a","工作","旧",5)], vec![]) };
        assert!(is_stale_remote(&remote, &local));
        let out = merge_wire(None, &local, Some(&remote));
        assert_eq!(out.tasks[0].title, "本地", "陈旧远端不应覆盖本地");
    }

    #[test]
    fn union_of_new_tasks() {
        let base = snap(vec![], vec![]);
        let local = snap(vec![task("a", "工作", "本地新增", 2)], vec![]);
        let remote = snap(vec![task("b", "生活", "远端新增", 3)], vec![]);
        let out = merge_wire(Some(&base), &local, Some(&remote));
        let ids: Vec<&str> = out.tasks.iter().map(|t| t.uid.as_str()).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b"));
    }
}
