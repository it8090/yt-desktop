use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub code: String,
    pub detail: String,
}
impl AppError {
    pub fn new(code: &str, detail: impl ToString) -> Self {
        Self {
            code: code.into(),
            detail: detail.to_string(),
        }
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for AppError {}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io", e)
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("database", e)
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("data", e)
    }
}
pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadOptions {
    pub max_height: u32,
    pub format: String,
    pub audio_format: String,
    pub compatibility: bool,
    pub subtitles: bool,
    pub thumbnail: bool,
    pub skip_existing: bool,
}
impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            max_height: 1080,
            format: "mp4".into(),
            audio_format: "mp3".into(),
            compatibility: true,
            subtitles: false,
            thumbnail: false,
            skip_existing: true,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: String,
    pub theme: String,
    pub destination: String,
    pub concurrency: usize,
    pub retries: u32,
    pub proxy_mode: String,
    pub proxy_url: String,
    pub close_to_tray: bool,
    pub auto_update: bool,
    pub options: DownloadOptions,
}
impl Default for Settings {
    fn default() -> Self {
        let base = directories::UserDirs::new()
            .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(std::env::temp_dir);
        Self {
            language: "system".into(),
            theme: "system".into(),
            destination: base.join("YT-Desktop").to_string_lossy().into(),
            concurrency: 2,
            retries: 3,
            proxy_mode: "system".into(),
            proxy_url: String::new(),
            close_to_tray: true,
            auto_update: true,
            options: DownloadOptions::default(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !["system", "zh-CN", "en"].contains(&self.language.as_str())
            || !["system", "light", "dark"].contains(&self.theme.as_str())
        {
            return Err(AppError::new(
                "invalid_settings",
                "Unsupported language or theme",
            ));
        }
        if !(1..=4).contains(&self.concurrency)
            || self.retries > 10
            || !std::path::Path::new(&self.destination).is_absolute()
        {
            return Err(AppError::new(
                "invalid_settings",
                "Invalid concurrency, retry count, or directory",
            ));
        }
        if !["system", "none", "custom"].contains(&self.proxy_mode.as_str()) {
            return Err(AppError::new("invalid_settings", "Invalid proxy mode"));
        }
        if self.proxy_mode == "custom" {
            let u =
                url::Url::parse(&self.proxy_url).map_err(|e| AppError::new("invalid_proxy", e))?;
            if !["http", "https", "socks5", "socks5h"].contains(&u.scheme())
                || u.host_str().is_none()
            {
                return Err(AppError::new(
                    "invalid_proxy",
                    "Use an HTTP or SOCKS5 proxy",
                ));
            }
        }
        validate_options(&self.options)
    }
}
pub fn validate_options(o: &DownloadOptions) -> Result<()> {
    if !["mp4", "mkv", "original", "audio"].contains(&o.format.as_str())
        || !["mp3", "m4a", "opus"].contains(&o.audio_format.as_str())
        || ![0, 360, 480, 720, 1080, 1440, 2160, 4320].contains(&o.max_height)
    {
        return Err(AppError::new(
            "invalid_settings",
            "Unsupported download options",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: String,
    pub title: String,
    pub url: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub site: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub urls: Vec<String>,
    pub title: String,
    pub status: String,
    pub entries: Vec<MediaItem>,
    pub total: Option<usize>,
    pub error: Option<AppError>,
    pub updated_at: i64,
}
impl Source {
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            id: id(),
            urls,
            title: String::new(),
            status: "loading".into(),
            entries: vec![],
            total: None,
            error: None,
            updated_at: now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuthConnection {
    pub method: String,
    pub browser: String,
    pub profile: String,
    pub status: String,
    pub remember: bool,
    pub verified_at: Option<i64>,
    pub error: Option<AppError>,
}
impl Default for AuthConnection {
    fn default() -> Self {
        Self {
            method: "none".into(),
            browser: "chrome".into(),
            profile: String::new(),
            status: "disconnected".into(),
            remember: false,
            verified_at: None,
            error: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    pub method: String,
    pub browser: String,
    pub profile: String,
    pub cookie_path: Option<String>,
    pub remember: bool,
    pub consent: bool,
    pub test_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub batch_id: String,
    pub item: MediaItem,
    pub options: DownloadOptions,
    pub destination: String,
    pub output_template: String,
    pub status: String,
    pub progress: Option<f64>,
    pub bytes_downloaded: u64,
    pub bytes_total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub attempt: u32,
    pub retry_at: Option<i64>,
    pub error: Option<AppError>,
    pub file_path: Option<String>,
    pub file_size: Option<u64>,
    pub duration: Option<f64>,
    pub resolution: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub dedupe_key: String,
    #[serde(default)]
    pub fallback_used: bool,
}
impl Job {
    pub fn is_active(&self) -> bool {
        ["downloading", "processing", "verifying"].contains(&self.status.as_str())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub managed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub settings: Settings,
    pub auth: AuthConnection,
    pub jobs: Vec<Job>,
    pub sources: Vec<Source>,
    pub tools: Vec<ToolInfo>,
    pub version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueRequest {
    pub items: Vec<MediaItem>,
    pub options: DownloadOptions,
    pub destination: String,
    pub source_title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueResult {
    pub jobs: Vec<Job>,
    pub skipped: usize,
    pub batch_id: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Pause,
    Cancel,
    Shutdown,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum CoreEvent {
    Job(Box<Job>),
    Source(Source),
    Auth(AuthConnection),
    Tools(Vec<ToolInfo>),
    Notice(AppError),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_settings() {
        let mut s = Settings {
            destination: "relative".into(),
            ..Settings::default()
        };
        assert!(s.validate().is_err());
        s.destination = std::env::temp_dir().to_string_lossy().into();
        s.proxy_mode = "custom".into();
        s.proxy_url = "file:///etc/passwd".into();
        assert!(s.validate().is_err());
    }
    #[test]
    fn format_options_are_bounded() {
        let o = DownloadOptions {
            format: "--exec".into(),
            ..DownloadOptions::default()
        };
        assert!(validate_options(&o).is_err());
    }
}
