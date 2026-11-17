use crate::{core::Core, models::*, process, tools};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;
#[derive(Default)]
pub struct Updates {
    pending: Mutex<Option<Update>>,
    busy: AtomicBool,
}
struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
    date: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}
fn error(e: impl ToString) -> AppError {
    AppError::new("update_failed", process::redact(&e.to_string()))
}
#[tauri::command]
pub async fn check_update(
    app: tauri::AppHandle,
    core: State<'_, Arc<Core>>,
    updates: State<'_, Updates>,
) -> Result<Option<UpdateInfo>> {
    if updates.busy.swap(true, Ordering::SeqCst) {
        return Err(AppError::new("busy", "An update operation is running"));
    }
    let _busy = Busy(&updates.busy);
    let mut builder = app.updater_builder().timeout(Duration::from_secs(600));
    if let Some(proxy) = tools::effective_proxy(&core.settings()?)? {
        builder = if proxy.is_empty() {
            builder.no_proxy()
        } else {
            builder.proxy(url::Url::parse(&proxy).map_err(error)?)
        };
    }
    let updater = builder.build().map_err(error)?;
    let pending = tokio::time::timeout(Duration::from_secs(25), updater.check())
        .await
        .map_err(|_| AppError::new("timeout", "Update check timed out"))?
        .map_err(error)?;
    if let Some(update) = &pending {
        if !trusted_url(update.download_url.as_str()) {
            return Err(AppError::new(
                "update_failed",
                "The update package must come from this project's HTTPS releases",
            ));
        }
    }
    let info = pending.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    });
    *updates.pending.lock().await = pending;
    Ok(info)
}
#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    core: State<'_, Arc<Core>>,
    updates: State<'_, Updates>,
) -> Result<()> {
    if updates.busy.swap(true, Ordering::SeqCst) {
        return Err(AppError::new("busy", "An update operation is running"));
    }
    let _busy = Busy(&updates.busy);
    let pending = updates.pending.lock().await;
    let update = pending
        .as_ref()
        .ok_or_else(|| AppError::new("update_failed", "Check for an update first"))?;
    let mut downloaded = 0u64;
    let mut last = Instant::now();
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                if last.elapsed() >= Duration::from_millis(150) || total == Some(downloaded) {
                    last = Instant::now();
                    let _ = app.emit(
                        "update-progress",
                        Progress {
                            phase: "downloading",
                            downloaded,
                            total,
                        },
                    );
                }
            },
            || {},
        )
        .await
        .map_err(error)?;
    // The plugin verifies the signature before this point. Pause media tasks before any installer can exit the process.
    core.prepare_update().await?;
    let _ = app.emit(
        "update-progress",
        Progress {
            phase: "installing",
            downloaded,
            total: Some(downloaded),
        },
    );
    if let Err(e) = update.install(&bytes) {
        core.finish_update();
        return Err(error(e));
    }
    app.state::<Arc<Core>>().stop_all();
    app.restart();
}
pub fn trusted_url(input: &str) -> bool {
    input.starts_with("https://github.com/it8090/yt-desktop/releases/download/")
        && url::Url::parse(input)
            .ok()
            .is_some_and(|u| u.username().is_empty() && u.password().is_none())
}
#[cfg(test)]
mod tests {
    #[test]
    fn rejects_other_update_publishers() {
        assert!(super::trusted_url(
            "https://github.com/it8090/yt-desktop/releases/download/v1/app.zip"
        ));
        assert!(!super::trusted_url(
            "http://github.com/it8090/yt-desktop/releases/download/v1/app.zip"
        ));
        assert!(!super::trusted_url(
            "https://github.com/another/app/releases/download/v1/app.zip"
        ));
    }
}
