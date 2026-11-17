use crate::{
    models::*,
    tools::{self, Tools},
};
use serde_json::Value;
use std::path::Path;

pub const PROGRESS: &str = "__YT_PROGRESS__";
pub const FILE: &str = "__YT_FILE__";
pub const META: &str = "__YT_META__";
pub fn base_args(settings: &Settings, tools: &Tools) -> Result<Vec<String>> {
    let mut args = [
        "--ignore-config",
        "--no-colors",
        "--encoding",
        "utf-8",
        "--socket-timeout",
        "20",
        "--retries",
        "2",
        "--fragment-retries",
        "2",
        "--extractor-retries",
        "1",
        "--retry-sleep",
        "2",
        "--sleep-requests",
        "1",
    ]
    .map(str::to_string)
    .to_vec();
    if let Some(proxy) = tools::effective_proxy(settings)? {
        args.extend(["--proxy".into(), proxy]);
    }
    if let Some(deno) = tools.find("deno") {
        args.extend([
            "--js-runtimes".into(),
            format!("deno:{}", deno.to_string_lossy()),
        ]);
    }
    args.extend(["--extractor-args".into(), "youtube:skip=hls,dash".into()]);
    Ok(args)
}
pub fn download_args(
    job: &Job,
    settings: &Settings,
    tools: &Tools,
    fallback: bool,
) -> Result<Vec<String>> {
    validate_options(&job.options)?;
    tools::validate_url(&job.item.url)?;
    let mut args = base_args(settings, tools)?;
    let ffmpeg = tools.require("ffmpeg")?;
    args.extend([
        "--ffmpeg-location".into(),
        ffmpeg.to_string_lossy().into(),
        "--no-playlist".into(),
        "--no-simulate".into(),
        "--continue".into(),
        "--no-overwrites".into(),
        "--windows-filenames".into(),
        "--abort-on-unavailable-fragments".into(),
        "--newline".into(),
        "--progress-delta".into(),
        "0.5".into(),
        "--sleep-interval".into(),
        "2".into(),
        "--max-sleep-interval".into(),
        "4".into(),
    ]);
    let limit = if job.options.max_height > 0 {
        format!("[height<=?{}]", job.options.max_height)
    } else {
        String::new()
    };
    if job.options.format == "audio" {
        args.extend([
            "-f".into(),
            "bestaudio/best".into(),
            "-x".into(),
            "--audio-format".into(),
            job.options.audio_format.clone(),
        ]);
    } else {
        args.extend([
            "-f".into(),
            format!("bestvideo*{limit}+bestaudio/best{limit}"),
        ]);
        if job.options.format != "original" {
            args.extend([
                "--merge-output-format".into(),
                job.options.format.clone(),
                "--remux-video".into(),
                job.options.format.clone(),
            ]);
        }
        if job.options.compatibility && job.options.format == "mp4" && !fallback {
            args.extend(["-S".into(), "vcodec:h264,acodec:aac".into()]);
        }
    }
    if job.options.subtitles {
        args.extend(
            [
                "--write-subs",
                "--write-auto-subs",
                "--sub-langs",
                "zh.*,en.*",
                "--convert-subs",
                "srt",
            ]
            .map(str::to_string),
        );
    }
    if job.options.thumbnail {
        args.push("--write-thumbnail".into());
    }
    args.extend(["-P".into(),job.destination.clone(),"-o".into(),job.output_template.clone(),"--print".into(),format!("before_dl:{META}{{\"duration\":%(duration)j,\"formats\":%(requested_formats.:.{{vcodec,acodec,filesize,filesize_approx}})j,\"vcodec\":%(vcodec)j,\"acodec\":%(acodec)j}}"),"--print".into(),format!("after_move:{FILE}%(filepath)j"),"--progress-template".into(),format!("download:{PROGRESS}{{\"filename\":%(progress.filename)j,\"downloaded\":%(progress.downloaded_bytes)j,\"total\":%(progress.total_bytes)j,\"estimate\":%(progress.total_bytes_estimate)j,\"speed\":%(progress.speed)j,\"eta\":%(progress.eta)j,\"status\":%(progress.status)j}}")]);
    Ok(args)
}
pub fn media_item(v: &Value, input: &str) -> Result<MediaItem> {
    let id = v["id"].as_str().unwrap_or("").to_string();
    if id.is_empty() {
        return Err(AppError::new("metadata", "Video has no identifier"));
    }
    let site = if tools::youtube(input)
        || v["ie_key"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("youtube"))
    {
        "youtube".to_string()
    } else {
        url::Url::parse(input)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "generic".into())
    };
    let url = if site == "youtube" {
        format!("https://www.youtube.com/watch?v={id}")
    } else {
        v["webpage_url"]
            .as_str()
            .or_else(|| v["url"].as_str())
            .unwrap_or(input)
            .to_string()
    };
    tools::validate_url(&url)?;
    let thumb = v["thumbnail"]
        .as_str()
        .or_else(|| {
            v["thumbnails"]
                .as_array()
                .and_then(|a| a.last())
                .and_then(|t| t["url"].as_str())
        })
        .filter(|u| u.starts_with("https://"))
        .map(str::to_string);
    Ok(MediaItem {
        id,
        title: v["title"].as_str().unwrap_or("Video").to_string(),
        url,
        thumbnail: thumb,
        duration: v["duration"].as_f64(),
        uploader: v["uploader"]
            .as_str()
            .or_else(|| v["channel"].as_str())
            .map(str::to_string),
        site,
    })
}
#[derive(Default)]
pub struct ProgressState {
    parts: std::collections::HashMap<String, (u64, Option<u64>)>,
    pub expected_total: Option<u64>,
    pub expected_duration: Option<f64>,
    pub expect_audio: bool,
    pub file: Option<String>,
}
impl ProgressState {
    pub fn line(&mut self, line: &str, job: &mut Job) -> bool {
        if let Some(raw) = line.strip_prefix(META) {
            if let Ok(v) = serde_json::from_str::<Value>(raw) {
                self.expected_duration = v["duration"].as_f64();
                job.duration = self.expected_duration;
                let formats = v["formats"].as_array();
                let list: Vec<&Value> = formats
                    .map(|a| a.iter().collect())
                    .unwrap_or_else(|| vec![&v]);
                self.expect_audio = list.iter().any(|f| {
                    f["acodec"]
                        .as_str()
                        .is_some_and(|s| s != "none" && s != "NA")
                });
                let sizes: Vec<_> = list
                    .iter()
                    .map(|f| {
                        f["filesize"]
                            .as_u64()
                            .or_else(|| f["filesize_approx"].as_u64())
                    })
                    .collect();
                if !sizes.is_empty() && sizes.iter().all(Option::is_some) {
                    self.expected_total = Some(sizes.iter().flatten().sum());
                }
            }
            return false;
        }
        if let Some(raw) = line.strip_prefix(FILE) {
            if let Ok(file) = serde_json::from_str::<String>(raw) {
                self.file = Some(file);
            }
            return false;
        }
        if let Some(raw) = line.strip_prefix(PROGRESS) {
            if let Ok(v) = serde_json::from_str::<Value>(raw) {
                let filename = v["filename"].as_str().unwrap_or("media").to_string();
                let downloaded = v["downloaded"].as_u64().unwrap_or(0);
                let total = v["total"].as_u64().or_else(|| v["estimate"].as_u64());
                self.parts.insert(filename, (downloaded, total));
                job.bytes_downloaded = self.parts.values().map(|p| p.0).sum();
                job.bytes_total = self.expected_total;
                job.progress = job
                    .bytes_total
                    .filter(|v| *v > 0)
                    .map(|n| (job.bytes_downloaded as f64 / n as f64 * 100.).min(99.));
                job.speed = v["speed"].as_f64();
                job.eta = v["eta"].as_f64();
                return true;
            }
        }
        false
    }
}
pub struct VerifiedFile {
    pub size: u64,
    pub duration: f64,
    pub resolution: Option<String>,
}
pub async fn verify(
    tools: &Tools,
    file: &Path,
    audio_only: bool,
    expected: Option<f64>,
    expect_audio: bool,
) -> Result<VerifiedFile> {
    let meta = std::fs::metadata(file).map_err(|e| AppError::new("verification", e))?;
    if meta.len() == 0 {
        return Err(AppError::new("verification", "Output file is empty"));
    }
    let program = tools.require("ffprobe")?;
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        crate::process::command(&program)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration:stream=codec_type,width,height",
                "-of",
                "json",
            ])
            .arg(file)
            .output(),
    )
    .await
    .map_err(|_| AppError::new("verification", "ffprobe timed out"))??;
    if !output.status.success() {
        return Err(AppError::new(
            "verification",
            "ffprobe could not read the output file",
        ));
    }
    let data: Value = serde_json::from_slice(&output.stdout)?;
    let duration = data["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.);
    let streams = data["streams"]
        .as_array()
        .ok_or_else(|| AppError::new("verification", "Missing media streams"))?;
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let audio = streams.iter().any(|s| s["codec_type"] == "audio");
    if duration <= 0.
        || !duration.is_finite()
        || (!audio_only && video.is_none())
        || ((audio_only || expect_audio) && !audio)
    {
        return Err(AppError::new(
            "verification",
            "Output streams or duration are incomplete",
        ));
    }
    if let Some(expected) = expected {
        if expected > 0. && (duration - expected).abs() > (expected * 0.05).max(4.) {
            return Err(AppError::new(
                "verification",
                "Output duration differs from the source",
            ));
        }
    }
    let resolution = video.and_then(|v| {
        Some(format!(
            "{} × {}",
            v["width"].as_u64()?,
            v["height"].as_u64()?
        ))
    });
    Ok(VerifiedFile {
        size: meta.len(),
        duration,
        resolution,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_does_not_keep_signed_stream_urls() {
        let v = serde_json::json!({"id":"abc","title":"Video","extractor_key":"Youtube","webpage_url":"https://www.youtube.com/watch?v=abc","formats":[{"url":"https://cdn.example/a?sig=secret"}]});
        let item = media_item(&v, "https://www.youtube.com/watch?v=abc").unwrap();
        assert!(!serde_json::to_string(&item).unwrap().contains("secret"));
    }
}
