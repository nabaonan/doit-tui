//! 检查更新 + 本地下载安装（GitHub Releases，公开仓库匿名可读）。
//!
//! 与桌面端 `doit-desktop/src/services/update.ts` 同口径：直接查 Releases API，
//! 不依赖 Tauri updater 签名。产物是各平台的裸二进制，下载后替换当前可执行文件，
//! 提示用户重启生效。所有错误文案均为安全描述，不输出网络底层错误。

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

#[allow(dead_code)] // 仅测试与文档 URL 断言使用
const REPO: &str = "nabaonan/doit-tui";
const API_LATEST: &str = "https://api.github.com/repos/nabaonan/doit-tui/releases/latest";
const MAX_ASSET_BYTES: u64 = 200 * 1024 * 1024;
const USER_AGENT: &str = concat!("doit-tui/", env!("CARGO_PKG_VERSION"));

/// 当前 Release 版本信息（仅展示与下载所需字段）。
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    #[allow(dead_code)] // 展示与后续扩展用
    pub tag: String,
    pub asset_name: String,
    pub download_url: String,
    #[allow(dead_code)]
    pub size: Option<u64>,
    #[allow(dead_code)]
    pub notes: String,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    body: Option<String>,
    assets: Vec<ReleaseAsset>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build()
}

/// 当前平台在 Release 产物名里的标识（与 .github/workflows/release.yml 一致）。
pub fn platform_token() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "_win-x64.exe"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "_linux-x64"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        ""
    }
}

/// 按平台后缀从 assets 里挑产物；名字带 query 的历史脏数据不影响匹配。
pub fn select_asset<'a>(assets: &'a [ReleaseAsset], token: &str) -> Option<&'a ReleaseAsset> {
    if token.is_empty() {
        return None;
    }
    assets.iter().find(|a| a.name.contains(token))
}

/// 语义版本比较：忽略前导 v 和 -pre 后缀，缺位按 0；返回 true 表示 newer 更新。
pub fn is_newer(current: &str, newer: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        let s = s.trim().trim_start_matches(['v', 'V']);
        let core = s.split(['-', '+']).next().unwrap_or("");
        core.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let a = parse(current);
    let b = parse(newer);
    let n = a.len().max(b.len());
    for i in 0..n {
        let av = *a.get(i).unwrap_or(&0);
        let bv = *b.get(i).unwrap_or(&0);
        if av != bv {
            return bv > av;
        }
    }
    false
}

/// 查询 GitHub Releases 最新版本；已是最新时返回 None。
pub fn check_update() -> Result<Option<UpdateInfo>, String> {
    if platform_token().is_empty() {
        return Err("当前平台没有预编译产物，请从源码构建".into());
    }
    let resp = agent()
        .get(API_LATEST)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| "检查更新失败：网络不可用或 GitHub 访问受限".to_string())?;
    let release: Release = resp
        .into_json()
        .map_err(|_| "检查更新失败：版本信息格式异常".to_string())?;
    let version = release
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    if !is_newer(env!("CARGO_PKG_VERSION"), &version) {
        return Ok(None);
    }
    let asset = select_asset(&release.assets, platform_token())
        .ok_or_else(|| "发现新版本，但没有适合当前平台的安装包".to_string())?;
    Ok(Some(UpdateInfo {
        version,
        tag: release.tag_name.clone(),
        asset_name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        size: asset.size,
        notes: release.body.unwrap_or_default(),
    }))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn write_executable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// 下载并替换当前可执行文件。返回给 UI 的安全提示文案。
/// 失败时保留 `.doit-update.new` 便于手动替换，不删除用户数据。
pub fn install_update(info: &UpdateInfo) -> Result<String, String> {
    let resp = agent()
        .get(&info.download_url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|_| "下载失败：网络不可用或下载链接失效".to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(MAX_ASSET_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "下载失败：读取安装包出错".to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err("下载失败：安装包大小为 0 或超过 200 MB".into());
    }

    let current = std::env::current_exe().map_err(|_| "无法定位当前程序路径".to_string())?;
    let staged = sibling(&current, ".doit-update.new");
    write_executable(&staged, &bytes)
        .map_err(|_| "无法写入更新文件（请检查安装目录权限）".to_string())?;

    #[cfg(windows)]
    {
        // Windows 不允许覆盖正在运行的 exe：留待退出后手动替换。
        return Ok(format!(
            "已下载到 {}；退出后用它替换当前程序即可完成安装",
            staged.display()
        ));
    }

    #[cfg(not(windows))]
    {
        let backup = sibling(&current, ".doit-update.bak");
        let _ = fs::copy(&current, &backup); // 备份失败不阻断更新
        fs::rename(&staged, &current).map_err(|_| {
            format!(
                "已下载到 {}，但无法自动替换当前程序；请退出后手动替换",
                staged.display()
            )
        })?;
        Ok(format!(
            "已安装 v{}（原程序备份为 {}），重启后生效",
            info.version,
            backup.display()
        ))
    }
}

/// Releases 页面地址（供 UI 提示用户手动下载）。
pub fn releases_url() -> &'static str {
    "https://github.com/nabaonan/doit-tui/releases"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.1.3", "0.1.4"));
        assert!(is_newer("0.1.3", "0.2.0"));
        assert!(is_newer("0.1.3", "1.0.0"));
        assert!(!is_newer("0.1.3", "v0.1.3"));
        assert!(!is_newer("0.1.3", "0.1.2"));
        assert!(!is_newer("0.1.3", "0.1.3-pre"));
        assert!(is_newer("0.1.3", "0.1.4-rc1"));
    }

    #[test]
    fn asset_pick_by_platform_token() {
        let assets = vec![
            ReleaseAsset {
                name: "doit_v0.1.4-aarch64-apple-darwin".into(),
                browser_download_url: "https://example.invalid/a".into(),
                size: Some(1),
            },
            ReleaseAsset {
                name: "doit_v0.1.4_linux-x64".into(),
                browser_download_url: "https://example.invalid/l".into(),
                size: None,
            },
        ];
        assert_eq!(
            select_asset(&assets, "aarch64-apple-darwin").unwrap().name,
            assets[0].name
        );
        assert_eq!(
            select_asset(&assets, "_linux-x64").unwrap().name,
            assets[1].name
        );
        assert!(select_asset(&assets, "x86_64-unknown-none").is_none());
        assert!(select_asset(&assets, "").is_none());
    }

    #[test]
    fn repo_and_endpoint_are_public() {
        // 契约锚点：检查更新走公开仓库的 Releases API，不需要 Token。
        assert!(API_LATEST.contains(REPO));
        assert!(releases_url().contains(REPO));
    }
}
