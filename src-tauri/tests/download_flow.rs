//! Opt-in integration test using a local HTTP media fixture, never an account.
use std::time::Duration;
use yt_desktop_lib::{core::Core, models::*};
#[tokio::test]
#[ignore = "requires yt-dlp, ffmpeg, ffprobe and YT_SMOKE_URL"]
async fn real_download_pause_resume_and_restart() {
    let url = std::env::var("YT_SMOKE_URL").expect("Set YT_SMOKE_URL to a local MP4 URL");
    let root = tempfile::tempdir().unwrap();
    let core = Core::new(root.path().join("data")).unwrap();
    let mut settings = core.settings().unwrap();
    settings.proxy_mode = "none".into();
    settings.concurrency = 1;
    settings.retries = 0;
    core.save_settings(settings).unwrap();
    let source = core.resolve(vec![url]).unwrap();
    let mut parsed = None;
    for _ in 0..120 {
        let sources = core.db.sources().unwrap();
        if let Some(s) = sources
            .into_iter()
            .find(|s| s.id == source.id && s.status != "loading")
        {
            parsed = Some(s);
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let parsed = parsed.expect("Parsing timed out");
    assert_eq!(parsed.status, "ready", "{:?}", parsed.error);
    assert_eq!(parsed.entries.len(), 1);
    let queued = core
        .enqueue(EnqueueRequest {
            items: parsed.entries,
            options: DownloadOptions::default(),
            destination: root.path().join("output").to_string_lossy().into(),
            source_title: "Smoke".into(),
        })
        .unwrap();
    let id = &queued.jobs[0].id;
    core.control(vec![id.clone()], "pause").unwrap();
    assert_eq!(core.db.job(id).unwrap().status, "paused");
    core.start();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(core.db.job(id).unwrap().status, "paused");
    core.control(vec![id.clone()], "resume").unwrap();
    let mut result = None;
    for _ in 0..240 {
        let j = core.db.job(id).unwrap();
        if ["completed", "failed"].contains(&j.status.as_str()) {
            result = Some(j);
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let job = result.expect("Download timed out");
    assert_eq!(job.status, "completed", "{:?}", job.error);
    assert!(job.file_size.unwrap() > 0);
    assert!(job.duration.unwrap() > 0.);
    assert!(core.completed_path(id).unwrap().is_file());
    core.export_csv(&root.path().join("library.csv")).unwrap();
    core.stop_all();
    let restored = Core::new(root.path().join("data")).unwrap();
    assert_eq!(restored.db.job(id).unwrap().status, "completed");
    assert_eq!(restored.db.job(id).unwrap().progress, Some(100.));
}
