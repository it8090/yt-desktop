use crate::{
    models::*,
    process,
    tools::{validate_url, youtube, Tools},
};
use std::{io::Write, path::PathBuf, sync::Mutex, time::Duration};
use tokio::sync::watch;

pub struct Auth {
    pub connection: Mutex<AuthConnection>,
    cookies: Mutex<Option<String>>,
    root: PathBuf,
    service: String,
}
impl Auth {
    pub fn new(root: PathBuf, mut connection: AuthConnection) -> Self {
        use sha2::{Digest, Sha256};
        let suffix = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
        let service = format!("io.github.it8090.ytdesktop.{}", &suffix[..12]);
        let mut cookies = None;
        if connection.method == "cookies" {
            if connection.remember {
                cookies = keyring::Entry::new(&service, "youtube")
                    .and_then(|e| e.get_password())
                    .ok();
            }
            if cookies.is_none() {
                connection.status = "expired".into();
                connection.error =
                    Some(AppError::new("auth_required", "Re-import the cookie file"));
            }
        } else if connection.method == "browser" {
            connection.status = "unverified".into();
        }
        Self {
            connection: Mutex::new(connection),
            cookies: Mutex::new(cookies),
            root,
            service,
        }
    }
    pub fn snapshot(&self) -> AuthConnection {
        self.connection.lock().unwrap().clone()
    }
    pub fn disconnect(&self) {
        let _ = keyring::Entry::new(&self.service, "youtube").and_then(|e| e.delete_credential());
        *self.cookies.lock().unwrap() = None;
        *self.connection.lock().unwrap() = AuthConnection::default();
    }
    pub fn expire(&self, error: AppError) {
        let mut c = self.connection.lock().unwrap();
        c.status = "expired".into();
        c.error = Some(error);
    }
    pub fn arguments(&self, target: &str) -> Result<(Vec<String>, Option<tempfile::TempPath>)> {
        if !youtube(target) {
            return Ok((vec![], None));
        }
        let c = self.snapshot();
        if c.method == "browser" {
            if ![
                "chrome", "chromium", "firefox", "edge", "brave", "vivaldi", "opera", "safari",
            ]
            .contains(&c.browser.as_str())
            {
                return Err(AppError::new("invalid_browser", "Unsupported browser"));
            }
            let profile = if c.profile.trim().is_empty() {
                c.browser
            } else {
                format!("{}:{}", c.browser, c.profile.trim())
            };
            return Ok((vec!["--cookies-from-browser".into(), profile], None));
        }
        if c.method == "cookies" {
            let cookies =
                self.cookies.lock().unwrap().clone().ok_or_else(|| {
                    AppError::new("auth_required", "Cookie session is unavailable")
                })?;
            let directory = self.root.join("runtime");
            std::fs::create_dir_all(&directory)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
            }
            let mut file = tempfile::Builder::new()
                .prefix("youtube-")
                .suffix(".txt")
                .tempfile_in(directory)?;
            file.write_all(cookies.as_bytes())?;
            file.flush()?;
            let path = file.into_temp_path();
            return Ok((
                vec!["--cookies".into(), path.to_string_lossy().into()],
                Some(path),
            ));
        }
        Ok((vec![], None))
    }
    pub async fn connect(
        &self,
        request: AuthRequest,
        tools: &Tools,
        base_args: Vec<String>,
    ) -> Result<AuthConnection> {
        if !request.consent {
            return Err(AppError::new(
                "consent_required",
                "Explicit consent is required before reading browser credentials",
            ));
        }
        if !["browser", "cookies"].contains(&request.method.as_str()) {
            return Err(AppError::new(
                "invalid_auth",
                "Choose a browser or a cookie file",
            ));
        }
        if request.profile.contains(['\n', '\r', '\0']) {
            return Err(AppError::new("invalid_auth", "Invalid browser profile"));
        }
        let old = self.snapshot();
        let old_cookies = self.cookies.lock().unwrap().clone();
        let imported = if request.method == "cookies" {
            let path = request
                .cookie_path
                .as_ref()
                .ok_or_else(|| AppError::new("cookie_file", "Select a cookie file"))?;
            let meta = std::fs::metadata(path)?;
            if meta.len() > 4_000_000 {
                return Err(AppError::new("cookie_file", "Cookie file is too large"));
            }
            Some(filter_cookies(&std::fs::read_to_string(path)?)?)
        } else {
            None
        };
        let pending = AuthConnection {
            method: request.method.clone(),
            browser: request.browser,
            profile: request.profile,
            status: "verifying".into(),
            remember: request.remember,
            verified_at: None,
            error: None,
        };
        *self.connection.lock().unwrap() = pending;
        *self.cookies.lock().unwrap() = imported.clone();
        let url = request
            .test_url
            .filter(|u| is_video_probe(u))
            .unwrap_or_else(|| "https://www.youtube.com/watch?v=jNQXAC9IVRw".into());
        let attempt = async {
            let program = tools.require("yt-dlp")?;
            let (auth_args, _secret_file) = self.arguments(&url)?;
            let mut args = base_args;
            args.extend(auth_args);
            args.extend(
                [
                    "--simulate",
                    "--skip-download",
                    "--no-playlist",
                    "--playlist-end",
                    "1",
                    "--dump-json",
                ]
                .map(str::to_string),
            );
            args.push(url);
            let (keep, rx) = watch::channel(None);
            let mut valid = false;
            let result = tokio::time::timeout(
                Duration::from_secs(100),
                process::run(&program, &args, rx, None, |line, err| {
                    if !err {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                            valid = v
                                .get("formats")
                                .and_then(|f| f.as_array())
                                .is_some_and(|f| !f.is_empty())
                                || v.get("url").and_then(|u| u.as_str()).is_some();
                        }
                    }
                }),
            )
            .await
            .map_err(|_| AppError::new("timeout", "Connection verification timed out"))??;
            drop(keep);
            if !result.success
                || !valid
                || result
                    .stderr
                    .to_ascii_lowercase()
                    .contains("cookies are no longer valid")
            {
                return Err(process::classify(&result.stderr));
            }
            if request.remember && request.method == "cookies" {
                keyring::Entry::new(&self.service, "youtube")
                    .and_then(|e| e.set_password(imported.as_deref().unwrap_or_default()))
                    .map_err(|_| {
                        AppError::new(
                            "credential_store",
                            "Secure storage is unavailable; retry without saving credentials",
                        )
                    })?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = attempt {
            *self.connection.lock().unwrap() = old;
            *self.cookies.lock().unwrap() = old_cookies;
            return Err(error);
        }
        if !request.remember {
            let _ =
                keyring::Entry::new(&self.service, "youtube").and_then(|e| e.delete_credential());
        }
        let mut c = self.connection.lock().unwrap();
        c.status = "connected".into();
        c.error = None;
        c.verified_at = Some(now());
        Ok(c.clone())
    }
}
pub fn is_video_probe(input: &str) -> bool {
    if !youtube(input) {
        return false;
    }
    validate_url(input).ok().is_some_and(|u| {
        u.host_str() == Some("youtu.be")
            || u.path().starts_with("/shorts/")
            || (u.path() == "/watch" && u.query_pairs().any(|(k, _)| k == "v"))
    })
}
pub fn filter_cookies(input: &str) -> Result<String> {
    if !input
        .trim_start_matches('\u{feff}')
        .lines()
        .take(3)
        .any(|l| l.contains("Netscape HTTP Cookie File") || l.contains("HTTP Cookie File"))
    {
        return Err(AppError::new(
            "cookie_file",
            "Expected a Netscape cookie file",
        ));
    }
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    let mut auth = false;
    let mut kept = 0;
    for line in input.lines() {
        let value = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if value.starts_with('#') || value.trim().is_empty() {
            continue;
        }
        let fields: Vec<_> = value.split('\t').collect();
        if fields.len() != 7 {
            continue;
        }
        let domain = fields[0].trim_start_matches('.').to_ascii_lowercase();
        if !(domain == "youtube.com"
            || domain.ends_with(".youtube.com")
            || domain == "google.com"
            || domain == "accounts.google.com")
        {
            continue;
        }
        let expires = fields[4].parse::<i64>().unwrap_or(-1);
        if expires < 0 || (expires > 0 && expires < now()) {
            continue;
        }
        if [
            "SAPISID",
            "__Secure-3PAPISID",
            "__Secure-1PAPISID",
            "SID",
            "LOGIN_INFO",
        ]
        .contains(&fields[5])
        {
            auth = true;
        }
        out.push_str(line);
        out.push('\n');
        kept += 1;
    }
    if kept == 0 || !auth {
        return Err(AppError::new(
            "cookie_file",
            "No unexpired YouTube/Google login cookies were found",
        ));
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cookie_import_is_scoped() {
        let raw="# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tsecret\n.example.com\tTRUE\t/\tTRUE\t0\tPASSWORD\tprivate\n";
        let v = filter_cookies(raw).unwrap();
        assert!(v.contains("SAPISID"));
        assert!(!v.contains("private"));
    }
    #[test]
    fn rejects_expired_or_non_cookie_files() {
        assert!(filter_cookies("password=abc").is_err());
        assert!(filter_cookies(
            "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t1\tSAPISID\texpired"
        )
        .is_err());
    }
    #[test]
    fn probe_cannot_enumerate_private_feeds() {
        assert!(!is_video_probe("https://www.youtube.com/feed/history"));
        assert!(!is_video_probe("https://www.youtube.com/@test/shorts"));
    }
}
