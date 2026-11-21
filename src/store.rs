//! 本地持久化：数据目录 ~/.doit-tui/（可用 DOIT_TUI_DIR 覆盖）。
//!
//! - data.json     —— SnapshotV2 快照（本机数据）
//! - settings.json —— 主题 / WebDAV 配置
//! - sync-base.json —— 三方合并的 base 快照

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::model::Snapshot;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub theme: String, // "light" | "dark"，空 = 跟随默认
    #[serde(default)]
    pub webdav: WebDavSettings,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WebDavSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub remote_file: String, // 默认 doit-backup.json
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncBase {
    #[serde(default)]
    pub snapshot: Option<Snapshot>,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new() -> Self {
        let dir = std::env::var("DOIT_TUI_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".doit-tui")
            });
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("创建数据目录失败: {}", self.dir.display()))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn load_snapshot(&self) -> Result<Snapshot> {
        let p = self.path("data.json");
        if !p.exists() {
            return Ok(self.seed());
        }
        let raw = std::fs::read_to_string(&p)?;
        let snap: Snapshot = serde_json::from_str(&raw)
            .with_context(|| format!("解析 data.json 失败: {}", p.display()))?;
        Ok(snap)
    }

    pub fn save_snapshot(&self, snap: &Snapshot) -> Result<()> {
        self.ensure_dir()?;
        let tmp = self.path("data.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(snap)?)?;
        std::fs::rename(&tmp, self.path("data.json"))?;
        Ok(())
    }

    pub fn load_settings(&self) -> Settings {
        std::fs::read_to_string(self.path("settings.json"))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.ensure_dir()?;
        let tmp = self.path("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(s)?)?;
        std::fs::rename(&tmp, self.path("settings.json"))?;
        Ok(())
    }

    pub fn load_sync_base(&self) -> Option<Snapshot> {
        std::fs::read_to_string(self.path("sync-base.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<SyncBase>(&raw).ok())
            .and_then(|b| b.snapshot)
    }

    pub fn save_sync_base(&self, snap: &Snapshot) -> Result<()> {
        self.ensure_dir()?;
        let wrapper = SyncBase {
            snapshot: Some(snap.clone()),
        };
        let tmp = self.path("sync-base.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&wrapper)?)?;
        std::fs::rename(&tmp, self.path("sync-base.json"))?;
        Ok(())
    }

    /// 首次启动的种子数据：三个默认分类 + 示例任务，保证界面不为空。
    fn seed(&self) -> Snapshot {
        let now = crate::model::now_ms();
        let mut snap = Snapshot::new(&device_id());
        snap.categories = vec![
            ("工作", "#4A86D6"),
            ("生活", "#4FBF9E"),
            ("学习", "#EF9F27"),
        ]
        .into_iter()
        .map(|(name, color)| crate::model::Category {
            name: name.into(),
            color: color.into(),
        })
        .collect();

        let sub = |title: &str, pri: u8, done: bool| crate::model::Subtask {
            id: new_id("s"),
            title: title.into(),
            priority: pri,
            done,
            created_at: now,
            updated_at: now,
            completed_at: if done { Some(now) } else { None },
            subs: Vec::new(),
        };

        let mk = |id: &str, title: &str, cat: &str, pri: u8, done: bool, subs: Vec<crate::model::Subtask>| crate::model::Task {
            id: id.into(),
            title: title.into(),
            category: cat.into(),
            priority: pri,
            done,
            created_at: now,
            updated_at: now,
            completed_at: if done { Some(now) } else { None },
            subs,
        };

        snap.tasks = vec![
            mk(
                "t1", "完成 doit-tui 工程骨架", "工作", 3, false,
                vec![sub("搭建 ratatui 事件循环", 2, true), sub("实现卡片列表渲染", 2, false), sub("接入 WebDAV 同步", 3, false)],
            ),
            mk("t2", "撰写设计文档评审纪要", "工作", 2, false, vec![]),
            mk("t3", "周末采购食材", "生活", 0, false, vec![sub("蔬菜水果", 0, false), sub("咖啡豆", 1, false)]),
            mk("t4", "阅读 ratatui 官方文档", "学习", 1, false, vec![]),
            mk("t5", "整理桌面文件", "生活", 0, true, vec![]),
        ];
        snap.touch();
        snap
    }
}

pub fn device_id() -> String {
    let id_file = dirs::home_dir()
        .map(|h| h.join(".doit-tui").join("device-id"))
        .unwrap_or_else(|| PathBuf::from(".doit-tui-device-id"));
    if let Ok(s) = std::fs::read_to_string(&id_file) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    let id = format!("tui-{}", &uuid_like());
    let _ = std::fs::create_dir_all(id_file.parent().unwrap_or(Path::new(".")));
    let _ = std::fs::write(&id_file, &id);
    id
}

/// 简易随机 id（无 uuid 依赖）。
fn uuid_like() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:x}", t, std::process::id())
}

pub fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(1);
    format!(
        "{}{}-{}",
        prefix,
        uuid_like(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}
