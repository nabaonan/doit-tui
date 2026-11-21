//! WebDAV 同步：SnapshotV2 契约 + 三方合并（base / local / remote）。
//!
//! 与两端共用远端文件 doit-backup.json；冲突按 updatedAt 新者胜。
//! 未启用或未配置时 `s` 仅提示。

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::Value;

use crate::model::Snapshot;
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

    let remote = match dav_get(cfg, &file)? {
        None => None,
        Some(raw) if raw.trim().is_empty() => None,
        Some(raw) => Some(
            serde_json::from_str::<Snapshot>(&raw)
                .with_context(|| "远端快照解析失败（契约不一致？）".to_string())?,
        ),
    };

    let merged = merge(&base, local, remote.as_ref());
    let mut merged = merged;
    merged.device_id = local.device_id.clone();
    merged.version = crate::model::SNAPSHOT_VERSION;
    merged.revision = merged
        .revision
        .max(local.revision)
        .max(remote.as_ref().map(|r| r.revision).unwrap_or(0))
        + 1;

    dav_put(cfg, &file, &serde_json::to_string(&merged)?)?;
    store.save_sync_base(&merged)?;
    let note = format!("同步完成 · r{} · {} 个任务", merged.revision, merged.tasks.len());
    Ok((merged, note))
}

/// 三方合并。base 为 None 时：本地与远端共存合并（按 id 去重、新者胜）。
pub fn merge(base: &Option<Snapshot>, local: &Snapshot, remote: Option<&Snapshot>) -> Snapshot {
    let Some(remote) = remote else {
        let out = local.clone();
        return out;
    };
    let Some(base) = base else {
        // 无 base：直接做本地/远端双源合并
        return merge_pair(local, remote);
    };

    let mut out = local.clone();
    out.categories = merge_named(
        &base.categories,
        &local.categories,
        &remote.categories,
        |c| &c.name,
    );
    out.tasks = merge_vec3(
        &base.tasks,
        &local.tasks,
        &remote.tasks,
        |t| &t.id,
        |t| t.updated_at,
    );
    out
}

/// 本地 / 远端双源合并（无 base 情况）：按 id 对齐，冲突新者胜，其余并集。
fn merge_pair(a: &Snapshot, b: &Snapshot) -> Snapshot {
    let mut out = a.clone();
    for c in &b.categories {
        if !out.categories.iter().any(|x| x.name == c.name) {
            out.categories.push(c.clone());
        }
    }
    for t in &b.tasks {
        match out.tasks.iter_mut().find(|x| x.id == t.id) {
            Some(x) => {
                if t.updated_at > x.updated_at {
                    *x = t.clone();
                }
            }
            None => out.tasks.push(t.clone()),
        }
    }
    out
}

/// 按 key + updated_at 的通用三方合并（Vec，元素有 id 与 updated_at）。
fn merge_vec3<T, K, U>(
    base: &[T],
    local: &[T],
    remote: &[T],
    key: K,
    updated: U,
) -> Vec<T>
where
    K: Fn(&T) -> &str,
    U: Fn(&T) -> i64,
    T: Clone + serde::Serialize + serde::de::DeserializeOwned,
{
    let val = |t: &T| serde_json::to_value(t).unwrap_or(Value::Null);
    let mut out: Vec<T> = Vec::new();

    let mut all_keys: Vec<String> = Vec::new();
    for e in base.iter().chain(local).chain(remote) {
        let k = key(e).to_string();
        if !all_keys.contains(&k) {
            all_keys.push(k);
        }
    }

    for k in all_keys {
        let b = base.iter().find(|e| key(e) == k);
        let l = local.iter().find(|e| key(e) == k);
        let r = remote.iter().find(|e| key(e) == k);
        let picked = match (b, l, r) {
            (None, None, None) => None,
            // 仅一侧存在：取存在的一侧（新增或删除语义一致）
            (None, Some(l), None) | (None, None, Some(l)) => Some(l.clone()),
            (None, Some(l), Some(r)) => {
                // 双侧新增：新者胜
                Some(if updated(l) >= updated(r) {
                    l.clone()
                } else {
                    r.clone()
                })
            }
            (Some(_), None, None) => None, // base 有、两侧都没有 → 已删除
            (Some(b), Some(l), None) => {
                // 远端删除：本地未改 → 删除；本地改过 → 保留本地
                if val(l) == val(b) {
                    None
                } else {
                    Some(l.clone())
                }
            }
            (Some(b), None, Some(r)) => {
                if val(r) == val(b) {
                    None
                } else {
                    Some(r.clone())
                }
            }
            (Some(b), Some(l), Some(r)) => {
                let lb = val(l) == val(b);
                let rb = val(r) == val(b);
                if lb && rb {
                    Some(b.clone())
                } else if lb {
                    Some(r.clone())
                } else if rb {
                    Some(l.clone())
                } else {
                    // 双侧都改：updatedAt 新者胜
                    Some(if updated(l) >= updated(r) {
                        l.clone()
                    } else {
                        r.clone()
                    })
                }
            }
        };
        if let Some(e) = picked {
            out.push(e);
        }
    }
    out
}

/// 分类合并（按 name 对齐、无时间戳）。
/// 分类实体只有 name/color，用「与 base 是否相同」判断改动，实现删除语义：
/// - 一侧删除、另一侧未改 → 跟随删除（否则被删分类会从远端复活）
/// - 一侧删除、另一侧改过 → 保留改动侧
/// - 双侧都在 → 本地优先；本地与 base 相同则取远端改动
fn merge_named<T, K>(base: &[T], local: &[T], remote: &[T], key: K) -> Vec<T>
where
    K: Fn(&T) -> &str,
    T: Clone + serde::Serialize,
{
    let val = |t: &T| serde_json::to_value(t).unwrap_or(Value::Null);
    let mut out: Vec<T> = Vec::new();

    let mut names: Vec<String> = Vec::new();
    for e in base.iter().chain(local).chain(remote) {
        let k = key(e).to_string();
        if !names.contains(&k) {
            names.push(k);
        }
    }

    for k in names {
        let b = base.iter().find(|e| key(e) == k);
        let l = local.iter().find(|e| key(e) == k);
        let r = remote.iter().find(|e| key(e) == k);
        let picked = match (b, l, r) {
            (None, None, None) => None,
            // 新增（单侧或双侧）：并集保留，双侧冲突本地优先
            (None, Some(l), None) | (None, None, Some(l)) => Some(l.clone()),
            (None, Some(l), Some(_r)) => Some(l.clone()),
            // 双侧都删 → 删除
            (Some(_), None, None) => None,
            // 本地删除：远端未改 → 跟随删除；远端改过（改名/换色）→ 保留远端
            (Some(b), None, Some(r)) => {
                if val(r) == val(b) {
                    None
                } else {
                    Some(r.clone())
                }
            }
            // 远端删除：本地未改 → 跟随删除；本地改过 → 保留本地
            (Some(b), Some(l), None) => {
                if val(l) == val(b) {
                    None
                } else {
                    Some(l.clone())
                }
            }
            // 双侧都在：本地优先；本地未改则接受远端改动
            (Some(b), Some(l), Some(r)) => {
                if val(l) == val(b) && val(r) != val(b) {
                    Some(r.clone())
                } else {
                    Some(l.clone())
                }
            }
        };
        if let Some(e) = picked {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Task};

    fn cat(name: &str, color: &str) -> Category {
        Category {
            name: name.into(),
            color: color.into(),
        }
    }

    #[test]
    fn merge_local_cat_delete_propagates() {
        // 本地删除分类（未改色），远端未动 → 合并后分类应被删除（不复活）
        let base = Snapshot {
            categories: vec![cat("工作", "#4A86D6"), cat("生活", "#4FBF9E")],
            ..snap(vec![])
        };
        let local = Snapshot {
            categories: vec![cat("生活", "#4FBF9E")],
            ..snap(vec![])
        };
        let remote = Snapshot {
            categories: vec![cat("工作", "#4A86D6"), cat("生活", "#4FBF9E")],
            ..snap(vec![])
        };
        let out = merge(&Some(base), &local, Some(&remote));
        assert!(
            !out.categories.iter().any(|c| c.name == "工作"),
            "本地删除的分类不应在合并后复活"
        );
        assert!(out.categories.iter().any(|c| c.name == "生活"));
    }

    #[test]
    fn merge_remote_cat_delete_propagates() {
        // 远端删除分类，本地未动 → 跟随删除
        let base = Snapshot {
            categories: vec![cat("工作", "#4A86D6"), cat("学习", "#EF9F27")],
            ..snap(vec![])
        };
        let local = Snapshot {
            categories: vec![cat("工作", "#4A86D6"), cat("学习", "#EF9F27")],
            ..snap(vec![])
        };
        let remote = Snapshot {
            categories: vec![cat("工作", "#4A86D6")],
            ..snap(vec![])
        };
        let out = merge(&Some(base), &local, Some(&remote));
        assert!(!out.categories.iter().any(|c| c.name == "学习"));
    }

    #[test]
    fn merge_remote_cat_edited_keeps_remote_edit() {
        // 本地删除、远端改色 → 保留远端改动（不同设备仍需要这个分类）
        let base = Snapshot {
            categories: vec![cat("工作", "#4A86D6")],
            ..snap(vec![])
        };
        let local = Snapshot {
            categories: vec![],
            ..snap(vec![])
        };
        let remote = Snapshot {
            categories: vec![cat("工作", "#DE5B55")],
            ..snap(vec![])
        };
        let out = merge(&Some(base), &local, Some(&remote));
        assert_eq!(out.categories.len(), 1);
        assert_eq!(out.categories[0].color, "#DE5B55");
    }

    #[test]
    fn merge_new_cat_union() {
        // 双侧各新增分类 → 并集
        let base = Snapshot {
            categories: vec![],
            ..snap(vec![])
        };
        let local = Snapshot {
            categories: vec![cat("运动", "#3AA6C9")],
            ..snap(vec![])
        };
        let remote = Snapshot {
            categories: vec![cat("阅读", "#7F77DD")],
            ..snap(vec![])
        };
        let out = merge(&Some(base), &local, Some(&remote));
        assert_eq!(out.categories.len(), 2);
    }

    fn task(id: &str, cat: &str, title: &str, updated: i64) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            category: cat.into(),
            priority: 0,
            done: false,
            created_at: 0,
            updated_at: updated,
            completed_at: None,
            subs: vec![],
        }
    }

    fn snap(tasks: Vec<Task>) -> Snapshot {
        Snapshot {
            version: 2,
            revision: 0,
            device_id: "test".into(),
            categories: vec![],
            tasks,
        }
    }

    #[test]
    fn merge_unchanged_both_sides() {
        let base = snap(vec![task("a", "工作", "原文", 1)]);
        let local = snap(vec![task("a", "工作", "原文", 1)]);
        let remote = snap(vec![task("a", "工作", "原文", 1)]);
        let out = merge(&Some(base), &local, Some(&remote));
        assert_eq!(out.tasks.len(), 1);
        assert_eq!(out.tasks[0].title, "原文");
    }

    #[test]
    fn merge_local_wins_when_remote_older() {
        let base = snap(vec![task("a", "工作", "原文", 1)]);
        let local = snap(vec![task("a", "工作", "本地改", 10)]);
        let remote = snap(vec![task("a", "工作", "远端改", 5)]);
        let out = merge(&Some(base), &local, Some(&remote));
        assert_eq!(out.tasks[0].title, "本地改");
    }

    #[test]
    fn merge_remote_wins_when_newer() {
        let base = snap(vec![task("a", "工作", "原文", 1)]);
        let local = snap(vec![task("a", "工作", "本地改", 3)]);
        let remote = snap(vec![task("a", "工作", "远端改", 9)]);
        let out = merge(&Some(base), &local, Some(&remote));
        assert_eq!(out.tasks[0].title, "远端改");
    }

    #[test]
    fn merge_remote_delete_local_untouched() {
        let base = snap(vec![task("a", "工作", "原文", 1)]);
        let local = snap(vec![]); // 本地没动（a 不在）
        let remote = snap(vec![]); // 远端删除了 a
        let out = merge(&Some(base), &local, Some(&remote));
        assert!(out.tasks.is_empty());
    }

    #[test]
    fn merge_keep_local_when_remote_deleted_but_local_edited() {
        let base = snap(vec![task("a", "工作", "原文", 1)]);
        let local = snap(vec![task("a", "工作", "本地改", 10)]);
        let remote = snap(vec![]);
        let out = merge(&Some(base), &local, Some(&remote));
        assert_eq!(out.tasks.len(), 1);
        assert_eq!(out.tasks[0].title, "本地改");
    }

    #[test]
    fn merge_union_of_new_tasks() {
        let base = snap(vec![]);
        let local = snap(vec![task("a", "工作", "本地新增", 2)]);
        let remote = snap(vec![task("b", "生活", "远端新增", 3)]);
        let out = merge(&Some(base), &local, Some(&remote));
        let ids: Vec<&str> = out.tasks.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b"));
    }

    #[test]
    fn merge_pair_no_base() {
        let a = snap(vec![task("a", "工作", "旧", 1)]);
        let b = snap(vec![task("a", "工作", "新", 5), task("c", "学习", "新任务", 2)]);
        let out = merge_pair(&a, &b);
        assert_eq!(out.tasks.len(), 2);
        assert_eq!(out.tasks.iter().find(|t| t.id == "a").unwrap().title, "新");
    }
}
