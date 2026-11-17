use crate::{models::*, process};
use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read, Write},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const TOOL_NAMES: [&str; 4] = ["yt-dlp", "ffmpeg", "ffprobe", "deno"];
pub struct Tools {
    pub root: PathBuf,
    installing: AtomicBool,
}
impl Tools {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            installing: AtomicBool::new(false),
        }
    }
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        if !TOOL_NAMES.contains(&name) {
            return None;
        }
        let executable = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        };
        let mut candidates = vec![self.root.join("tools").join(&executable)];
        if let Some(paths) = std::env::var_os("PATH") {
            candidates.extend(std::env::split_paths(&paths).map(|p| p.join(&executable)));
        }
        for base in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
            candidates.push(PathBuf::from(base).join(&executable));
        }
        candidates.into_iter().find(|p| {
            if !p.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                p.metadata()
                    .map(|m| m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            }
            #[cfg(not(unix))]
            {
                true
            }
        })
    }
    pub fn require(&self, name: &str) -> Result<PathBuf> {
        self.find(name)
            .ok_or_else(|| AppError::new("engine_missing", format!("Missing component: {name}")))
    }
    pub async fn info(&self, name: &str) -> ToolInfo {
        let path = self.find(name);
        let mut version = None;
        if let Some(p) = &path {
            if let Ok(Ok(output)) = tokio::time::timeout(
                Duration::from_secs(8),
                process::command(p)
                    .arg(if ["ffmpeg", "ffprobe"].contains(&name) {
                        "-version"
                    } else {
                        "--version"
                    })
                    .output(),
            )
            .await
            {
                if output.status.success() {
                    version = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .map(|s| s.chars().take(120).collect());
                }
            }
        }
        ToolInfo {
            name: name.into(),
            managed: path
                .as_ref()
                .is_some_and(|p| p.starts_with(self.root.join("tools"))),
            path: path.map(|p| p.to_string_lossy().into()),
            version,
        }
    }
    pub async fn all(&self) -> Vec<ToolInfo> {
        let (a, b, c, d) = tokio::join!(
            self.info("yt-dlp"),
            self.info("ffmpeg"),
            self.info("ffprobe"),
            self.info("deno")
        );
        vec![a, b, c, d]
    }
    pub async fn install(&self, settings: &Settings, update: bool) -> Result<Vec<ToolInfo>> {
        if self.installing.swap(true, Ordering::SeqCst) {
            return Err(AppError::new(
                "busy",
                "Component installation is already running",
            ));
        }
        let result = self.install_inner(settings, update).await;
        self.installing.store(false, Ordering::SeqCst);
        result
    }
    async fn install_inner(&self, settings: &Settings, update: bool) -> Result<Vec<ToolInfo>> {
        let mut builder = reqwest::Client::builder()
            .user_agent("YT-Desktop/0.1.0")
            .timeout(Duration::from_secs(600));
        if let Some(proxy) = effective_proxy(settings)? {
            if proxy.is_empty() {
                builder = builder.no_proxy();
            } else {
                builder = builder.proxy(
                    reqwest::Proxy::all(&proxy).map_err(|e| AppError::new("invalid_proxy", e))?,
                );
            }
        }
        let client = builder.build().map_err(|e| AppError::new("network", e))?;
        let root = self.root.join("tools");
        std::fs::create_dir_all(&root)?;
        let mut release_cache = std::collections::HashMap::new();
        for name in TOOL_NAMES {
            if !update && self.find(name).is_some() {
                continue;
            }
            let (repo, asset_name, zip_entry) = asset_for(name)?;
            if !release_cache.contains_key(repo) {
                let release: Value = client
                    .get(format!(
                        "https://api.github.com/repos/{repo}/releases/latest"
                    ))
                    .send()
                    .await
                    .map_err(|e| AppError::new("network", e))?
                    .error_for_status()
                    .map_err(|e| AppError::new("network", e))?
                    .json()
                    .await
                    .map_err(|e| AppError::new("data", e))?;
                release_cache.insert(repo, release);
            }
            let release = &release_cache[repo];
            let asset = release["assets"]
                .as_array()
                .and_then(|v| v.iter().find(|a| a["name"].as_str() == Some(&asset_name)))
                .ok_or_else(|| {
                    AppError::new("component_unavailable", format!("{repo}: {asset_name}"))
                })?;
            let url = asset["browser_download_url"]
                .as_str()
                .ok_or_else(|| AppError::new("data", "Missing component URL"))?;
            if !url.starts_with(&format!("https://github.com/{repo}/releases/download/")) {
                return Err(AppError::new(
                    "untrusted_component",
                    "Unexpected download source",
                ));
            }
            let digest = asset["digest"]
                .as_str()
                .and_then(|s| s.strip_prefix("sha256:"))
                .ok_or_else(|| {
                    AppError::new(
                        "checksum_missing",
                        "Publisher has not provided a SHA-256 digest",
                    )
                })?;
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|e| AppError::new("network", e))?
                .error_for_status()
                .map_err(|e| AppError::new("network", e))?;
            if response.content_length().is_some_and(|n| n > 400_000_000) {
                return Err(AppError::new(
                    "component_size",
                    "Component exceeds the size limit",
                ));
            }
            let mut stream = response.bytes_stream();
            let mut staging = tempfile::NamedTempFile::new_in(&root)?;
            let mut hash = Sha256::new();
            let mut size = 0usize;
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(|e| AppError::new("network", e))?;
                size += bytes.len();
                if size > 400_000_000 {
                    return Err(AppError::new(
                        "component_size",
                        "Component exceeds the size limit",
                    ));
                }
                hash.update(&bytes);
                staging.write_all(&bytes)?;
            }
            if format!("{:x}", hash.finalize()) != digest {
                return Err(AppError::new(
                    "checksum_mismatch",
                    "Downloaded component failed SHA-256 verification",
                ));
            }
            staging.flush()?;
            let mut binary = tempfile::NamedTempFile::new_in(&root)?;
            if let Some(entry) = zip_entry {
                let mut file = std::fs::File::open(staging.path())?;
                let mut archive = Vec::new();
                file.read_to_end(&mut archive)?;
                let mut zip = zip::ZipArchive::new(Cursor::new(archive))
                    .map_err(|e| AppError::new("archive", e))?;
                let mut file = zip
                    .by_name(&entry)
                    .map_err(|e| AppError::new("archive", e))?;
                if file.size() > 400_000_000 {
                    return Err(AppError::new("component_size", "Invalid archive size"));
                }
                std::io::copy(&mut file, &mut binary)?;
            } else {
                let mut file = std::fs::File::open(staging.path())?;
                std::io::copy(&mut file, &mut binary)?;
            }
            binary.flush()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(binary.path(), std::fs::Permissions::from_mode(0o700))?;
            }
            let target = root.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            });
            let backup = target.with_extension("previous");
            if backup.exists() {
                std::fs::remove_file(&backup)?;
            }
            if target.exists() {
                std::fs::rename(&target, &backup)?;
            }
            if let Err(error) = std::fs::rename(binary.path(), &target) {
                if backup.exists() {
                    let _ = std::fs::rename(&backup, &target);
                }
                return Err(error.into());
            }
            if backup.exists() {
                let _ = std::fs::remove_file(&backup);
            }
            let record = serde_json::json!({"component":name,"release":release["tag_name"],"sha256":digest,"installedAt":now()});
            std::fs::write(
                root.join(format!("{name}.json")),
                serde_json::to_vec_pretty(&record)?,
            )?;
        }
        Ok(self.all().await)
    }
}
pub fn effective_proxy(settings: &Settings) -> Result<Option<String>> {
    match settings.proxy_mode.as_str() {
        "none" => Ok(Some(String::new())),
        "custom" => Ok(Some(settings.proxy_url.clone())),
        _ => {
            for key in ["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"] {
                if let Ok(v) = std::env::var(key) {
                    if !v.is_empty() {
                        return Ok(Some(v));
                    }
                }
            }
            if let Ok(proxy) = sysproxy::Sysproxy::get_system_proxy() {
                if proxy.enable && !proxy.host.is_empty() {
                    let host = if proxy.host.contains(':') && !proxy.host.starts_with('[') {
                        format!("[{}]", proxy.host)
                    } else {
                        proxy.host
                    };
                    return Ok(Some(format!("http://{host}:{}", proxy.port)));
                }
            }
            Ok(None)
        }
    }
}
fn asset_for(name: &str) -> Result<(&'static str, String, Option<String>)> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    if name == "yt-dlp" {
        let asset = match (os, arch) {
            ("macos", _) => "yt-dlp_macos",
            ("windows", "aarch64") => "yt-dlp_arm64.exe",
            ("windows", _) => "yt-dlp.exe",
            ("linux", "aarch64") => "yt-dlp_linux_aarch64",
            ("linux", "x86_64") => "yt-dlp_linux",
            _ => {
                return Err(AppError::new(
                    "unsupported_platform",
                    format!("{os}/{arch}"),
                ))
            }
        };
        return Ok(("yt-dlp/yt-dlp", asset.into(), None));
    }
    if name == "deno" {
        let target = match (os, arch) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("windows", "aarch64") => "aarch64-pc-windows-msvc",
            ("windows", _) => "x86_64-pc-windows-msvc",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            _ => {
                return Err(AppError::new(
                    "unsupported_platform",
                    format!("{os}/{arch}"),
                ))
            }
        };
        return Ok((
            "denoland/deno",
            format!("deno-{target}.zip"),
            Some(if os == "windows" { "deno.exe" } else { "deno" }.into()),
        ));
    }
    let os_tag = match os {
        "macos" => "darwin",
        "windows" => "win32",
        "linux" => "linux",
        _ => return Err(AppError::new("unsupported_platform", os)),
    };
    let arch_tag = match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return Err(AppError::new("unsupported_platform", arch)),
    };
    Ok((
        "eugeneware/ffmpeg-static",
        format!("{name}-{os_tag}-{arch_tag}"),
        None,
    ))
}
pub fn validate_url(input: &str) -> Result<url::Url> {
    let u = url::Url::parse(input.trim()).map_err(|e| AppError::new("invalid_url", e))?;
    if !["http", "https"].contains(&u.scheme())
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err(AppError::new(
            "invalid_url",
            "Only HTTP/HTTPS URLs without embedded credentials are allowed",
        ));
    }
    Ok(u)
}
pub fn safe_name(input: &str) -> String {
    let mut out: String = input
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| {
            if ['<', '>', ':', '"', '/', '\\', '|', '?', '*'].contains(&c) {
                '_'
            } else {
                c
            }
        })
        .take(70)
        .collect();
    out = out.trim().trim_matches('.').trim().to_string();
    if out.is_empty() {
        return "Videos".into();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        out.insert(0, '_');
    }
    out
}
pub fn youtube(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .is_some_and(|h| h == "youtu.be" || h == "youtube.com" || h.ends_with(".youtube.com"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_local_protocols_and_credentials() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("https://u:p@example.com").is_err());
        assert!(validate_url("https://www.youtube.com/watch?v=abc&list=def").is_ok());
    }
    #[test]
    fn safe_cross_platform_names() {
        assert_eq!(safe_name("../CON"), "_CON");
        assert!(!safe_name("a/b:c?").contains('/'));
        assert_eq!(safe_name("NUL.txt"), "_NUL.txt");
    }
}
