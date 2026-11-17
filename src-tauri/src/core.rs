use crate::{
    auth::Auth,
    database::Database,
    engine::{self, ProgressState},
    models::*,
    process,
    tools::{self, Tools},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::watch;

type Emitter = Arc<dyn Fn(CoreEvent) + Send + Sync>;
struct Active {
    stop: watch::Sender<Option<StopReason>>,
    pid: Arc<AtomicU32>,
    site: String,
}
pub struct Core {
    pub db: Database,
    pub tools: Tools,
    pub auth: Auth,
    emit: Mutex<Option<Emitter>>,
    active: Mutex<HashMap<String, Active>>,
    sources: Mutex<HashMap<String, Active>>,
    writes: Mutex<()>,
    enqueue_lock: Mutex<()>,
    cooldowns: Mutex<HashMap<String, i64>>,
    restricted: Mutex<HashMap<String, i64>>,
    auth_busy: AtomicBool,
    install_busy: AtomicBool,
    shutdown: AtomicBool,
}
impl Core {
    pub fn new(root: PathBuf) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        }
        // Only application-generated temporary cookie files live in this directory.
        let runtime = root.join("runtime");
        if runtime.is_dir() {
            for entry in std::fs::read_dir(&runtime)?.flatten() {
                if entry.file_name().to_string_lossy().starts_with("youtube-") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        let db = Database::open(&root.join("library.sqlite"))?;
        db.recover()?;
        for mut source in db.sources()? {
            if source.status == "loading" {
                source.status = "cancelled".into();
                source.error = Some(AppError::new("interrupted", "Parsing was interrupted"));
                db.save_source(&source)?;
            }
        }
        let auth = Auth::new(root.clone(), db.get("auth")?.unwrap_or_default());
        Ok(Arc::new(Self {
            db,
            tools: Tools::new(root),
            auth,
            emit: Mutex::new(None),
            active: Mutex::new(HashMap::new()),
            sources: Mutex::new(HashMap::new()),
            writes: Mutex::new(()),
            enqueue_lock: Mutex::new(()),
            cooldowns: Mutex::new(HashMap::new()),
            restricted: Mutex::new(HashMap::new()),
            auth_busy: AtomicBool::new(false),
            install_busy: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        }))
    }
    pub fn subscribe(&self, emit: Emitter) {
        *self.emit.lock().unwrap() = Some(emit);
    }
    fn event(&self, event: CoreEvent) {
        if let Some(emit) = self.emit.lock().unwrap().clone() {
            emit(event);
        }
    }
    pub fn settings(&self) -> Result<Settings> {
        Ok(self.db.get("settings")?.unwrap_or_default())
    }
    pub fn save_settings(&self, settings: Settings) -> Result<Settings> {
        settings.validate()?;
        self.db.set("settings", &settings)?;
        Ok(settings)
    }
    pub async fn bootstrap(&self) -> Result<Bootstrap> {
        Ok(Bootstrap {
            settings: self.settings()?,
            auth: self.auth.snapshot(),
            jobs: self.db.jobs()?,
            sources: self.db.sources()?,
            tools: self.tools.all().await,
            version: env!("CARGO_PKG_VERSION").into(),
        })
    }
    fn publish_job(&self, job: &Job) -> Result<()> {
        self.db.save_job(job)?;
        self.event(CoreEvent::Job(Box::new(job.clone())));
        Ok(())
    }
    fn publish_source(&self, source: &Source) -> Result<()> {
        self.db.save_source(source)?;
        self.event(CoreEvent::Source(source.clone()));
        Ok(())
    }
    fn update_job(&self, id: &str, f: impl FnOnce(&mut Job)) -> Result<Job> {
        let _write = self.writes.lock().unwrap();
        let mut job = self.db.job(id)?;
        f(&mut job);
        job.updated_at = now();
        self.publish_job(&job)?;
        Ok(job)
    }
    pub fn enqueue(&self, request: EnqueueRequest) -> Result<EnqueueResult> {
        if self.install_busy.load(Ordering::SeqCst) {
            return Err(AppError::new("busy", "An update is running"));
        }
        let _enqueue = self.enqueue_lock.lock().unwrap();
        validate_options(&request.options)?;
        if request.items.is_empty()
            || request.items.len() > 20_000
            || !Path::new(&request.destination).is_absolute()
        {
            return Err(AppError::new(
                "invalid_request",
                "Select 1–20,000 videos and an absolute destination",
            ));
        }
        let group = tools::safe_name(&request.source_title);
        let profile = if request.options.format == "audio" {
            request.options.audio_format.clone()
        } else {
            format!(
                "{}-{}",
                if request.options.max_height == 0 {
                    "best".into()
                } else {
                    format!("{}p", request.options.max_height)
                },
                request.options.format
            )
        };
        let destination = Path::new(&request.destination)
            .join(group)
            .join(profile)
            .to_string_lossy()
            .into_owned();
        let batch_id = id();
        let mut jobs = vec![];
        let mut skipped = 0;
        for item in request.items {
            tools::validate_url(&item.url)?;
            if item.id.is_empty() || item.id.len() > 512 || item.title.len() > 10_000 {
                return Err(AppError::new("invalid_request", "Invalid video metadata"));
            }
            let key = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&(
                    &item.site,
                    &item.id,
                    &request.options,
                    &destination
                ))?)
            );
            let existing = self.db.existing(&key)?;
            if existing.iter().any(|j| {
                j.is_active()
                    || ["queued", "retry_wait", "waiting_auth", "paused"]
                        .contains(&j.status.as_str())
                    || (request.options.skip_existing
                        && j.status == "completed"
                        && j.file_path.as_ref().is_some_and(|p| Path::new(p).is_file()))
            }) {
                skipped += 1;
                continue;
            }
            let job_id = id();
            let template = if request.options.skip_existing {
                "%(title).140B [%(id)s].%(ext)s".into()
            } else {
                format!("%(title).120B [%(id)s] {}.%(ext)s", &job_id[..8])
            };
            let job = Job {
                id: job_id,
                batch_id: batch_id.clone(),
                item,
                options: request.options.clone(),
                destination: destination.clone(),
                output_template: template,
                status: "queued".into(),
                progress: None,
                bytes_downloaded: 0,
                bytes_total: None,
                speed: None,
                eta: None,
                attempt: 0,
                retry_at: None,
                error: None,
                file_path: None,
                file_size: None,
                duration: None,
                resolution: None,
                created_at: now(),
                updated_at: now(),
                dedupe_key: key,
                fallback_used: false,
            };
            self.publish_job(&job)?;
            jobs.push(job);
        }
        Ok(EnqueueResult {
            jobs,
            skipped,
            batch_id,
        })
    }
    pub fn control(&self, ids: Vec<String>, action: &str) -> Result<Vec<Job>> {
        if !["pause", "resume", "cancel", "retry"].contains(&action) {
            return Err(AppError::new("invalid_request", "Unknown task action"));
        }
        let mut result = vec![];
        for id in ids {
            let _write = self.writes.lock().unwrap();
            let mut job = self.db.job(&id)?;
            match action {
                "pause"
                    if job.is_active()
                        || ["queued", "retry_wait", "waiting_auth"]
                            .contains(&job.status.as_str()) =>
                {
                    job.status = "paused".into();
                    if let Some(active) = self.active.lock().unwrap().get(&id) {
                        let _ = active.stop.send(Some(StopReason::Pause));
                    }
                }
                "cancel" if job.status != "completed" => {
                    job.status = "cancelled".into();
                    if let Some(active) = self.active.lock().unwrap().get(&id) {
                        let _ = active.stop.send(Some(StopReason::Cancel));
                    }
                }
                "resume" | "retry"
                    if [
                        "paused",
                        "failed",
                        "cancelled",
                        "waiting_auth",
                        "retry_wait",
                    ]
                    .contains(&job.status.as_str()) =>
                {
                    job.status = "queued".into();
                    job.attempt = 0;
                    job.retry_at = None;
                    job.error = None;
                }
                _ => continue,
            }
            job.speed = None;
            job.eta = None;
            job.updated_at = now();
            self.publish_job(&job)?;
            result.push(job);
        }
        Ok(result)
    }
    pub fn start(self: &Arc<Self>) {
        let core = self.clone();
        tauri::async_runtime::spawn(async move {
            while !core.shutdown.load(Ordering::SeqCst) {
                if let Err(error) = core.tick() {
                    core.event(CoreEvent::Notice(error));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }
    fn tick(self: &Arc<Self>) -> Result<()> {
        if self.install_busy.load(Ordering::SeqCst) || self.auth_busy.load(Ordering::SeqCst) {
            return Ok(());
        }
        let settings = self.settings()?;
        for candidate in self.db.eligible()? {
            let _write = self.writes.lock().unwrap();
            if self.active.lock().unwrap().len() >= settings.concurrency {
                break;
            }
            let mut job = self.db.job(&candidate.id)?;
            if !["queued", "retry_wait"].contains(&job.status.as_str())
                || self.active.lock().unwrap().contains_key(&job.id)
            {
                continue;
            }
            if job.item.site == "youtube" && self.auth.snapshot().status == "expired" {
                job.status = "waiting_auth".into();
                job.error = Some(AppError::new(
                    "auth_required",
                    "Reconnect YouTube to continue",
                ));
                self.publish_job(&job)?;
                continue;
            }
            let until = self
                .cooldowns
                .lock()
                .unwrap()
                .get(&job.item.site)
                .copied()
                .unwrap_or(0);
            if until > now() {
                job.status = "retry_wait".into();
                job.retry_at = Some(until);
                self.publish_job(&job)?;
                continue;
            }
            // Keep a rate-limited site at one concurrent task during recovery.
            if self
                .restricted
                .lock()
                .unwrap()
                .get(&job.item.site)
                .is_some_and(|until| *until > now())
                && self
                    .active
                    .lock()
                    .unwrap()
                    .values()
                    .any(|a| a.site == job.item.site)
            {
                continue;
            }
            // Retry a throttled task only when another task is not already active.
            if job.error.as_ref().is_some_and(|e| e.code == "rate_limited")
                && !self.active.lock().unwrap().is_empty()
            {
                continue;
            }
            let (stop, rx) = watch::channel(None);
            let pid = Arc::new(AtomicU32::new(0));
            self.active.lock().unwrap().insert(
                job.id.clone(),
                Active {
                    stop,
                    pid: pid.clone(),
                    site: job.item.site.clone(),
                },
            );
            job.status = "downloading".into();
            job.attempt += 1;
            job.retry_at = None;
            job.updated_at = now();
            self.publish_job(&job)?;
            let core = self.clone();
            tauri::async_runtime::spawn(async move {
                let id = job.id.clone();
                let result = core.execute(job, rx, pid).await;
                if let Err(error) = result {
                    core.finish_error(&id, error);
                }
                core.active.lock().unwrap().remove(&id);
            });
        }
        Ok(())
    }
    async fn execute(
        &self,
        mut job: Job,
        rx: watch::Receiver<Option<StopReason>>,
        pid: Arc<AtomicU32>,
    ) -> Result<()> {
        let settings = self.settings()?;
        let program = self.tools.require("yt-dlp")?;
        self.tools.require("ffprobe")?;
        std::fs::create_dir_all(&job.destination)?;
        let mut args = engine::download_args(&job, &settings, &self.tools, job.fallback_used)?;
        let (auth_args, _secret_file) = self.auth.arguments(&job.item.url)?;
        args.extend(auth_args);
        args.push(job.item.url.clone());
        let mut progress = ProgressState::default();
        let result = process::run(&program, &args, rx, Some(pid), |line, err| {
            let changed = progress.line(line, &mut job);
            let processing = err
                && (line.starts_with("[Merger]")
                    || line.starts_with("[VideoRemuxer]")
                    || line.starts_with("[ExtractAudio]"));
            if changed || processing {
                let _ = self.update_job(&job.id, |current| {
                    if current.is_active() {
                        current.progress = job.progress;
                        current.bytes_downloaded = job.bytes_downloaded;
                        current.bytes_total = job.bytes_total;
                        current.speed = job.speed;
                        current.eta = job.eta;
                        if processing {
                            current.status = "processing".into();
                            current.speed = None;
                        }
                    }
                });
            }
        })
        .await?;
        if result.stopped.is_some() {
            return Ok(());
        }
        if !result.success {
            return Err(process::classify(&result.stderr));
        }
        let file = progress.file.ok_or_else(|| {
            AppError::new("verification", "Downloader did not report an output file")
        })?;
        // Downloader output must be inside the selected task directory.
        let path = std::fs::canonicalize(&file)?;
        let base = std::fs::canonicalize(&job.destination)?;
        if !path.starts_with(&base) {
            return Err(AppError::new(
                "verification",
                "Output path is outside the task directory",
            ));
        }
        self.update_job(&job.id, |j| {
            if j.is_active() {
                j.status = "verifying".into();
                j.speed = None;
                j.eta = None;
            }
        })?;
        let verified = engine::verify(
            &self.tools,
            &path,
            job.options.format == "audio",
            progress.expected_duration.or(job.item.duration),
            progress.expect_audio,
        )
        .await?;
        self.update_job(&job.id, |j| {
            if j.is_active() {
                j.status = "completed".into();
                j.progress = Some(100.);
                j.bytes_downloaded = verified.size;
                j.bytes_total = Some(verified.size);
                j.file_size = Some(verified.size);
                j.duration = Some(verified.duration);
                j.resolution = verified.resolution;
                j.file_path = Some(path.to_string_lossy().into());
                j.error = None;
            }
        })?;
        Ok(())
    }
    fn finish_error(&self, id: &str, error: AppError) {
        let settings = self.settings().unwrap_or_default();
        if error.code == "auth_required" || error.code == "browser_permission" {
            self.auth.expire(error.clone());
            let auth = self.auth.snapshot();
            let _ = self.db.set("auth", &auth);
            self.event(CoreEvent::Auth(auth));
        }
        let _ = self.update_job(id, |job| {
            if !job.is_active() {
                return;
            }
            job.speed = None;
            job.eta = None;
            job.error = Some(error.clone());
            if ["auth_required", "browser_permission"].contains(&error.code.as_str()) {
                job.status = "waiting_auth".into();
            } else if error.code == "rate_limited" {
                let delay = 60 * (2i64.pow(job.attempt.min(5)));
                let until = now() + delay;
                self.cooldowns
                    .lock()
                    .unwrap()
                    .insert(job.item.site.clone(), until);
                self.restricted
                    .lock()
                    .unwrap()
                    .insert(job.item.site.clone(), until + 600);
                job.retry_at = Some(until);
                job.status = if job.attempt <= settings.retries {
                    "retry_wait"
                } else {
                    "failed"
                }
                .into();
            } else if ["network", "format_unavailable"].contains(&error.code.as_str())
                && job.attempt <= settings.retries
            {
                if job.options.compatibility && job.options.format == "mp4" {
                    job.fallback_used = true;
                }
                job.status = "retry_wait".into();
                job.retry_at = Some(now() + 5 * 2i64.pow(job.attempt.min(5)));
            } else {
                job.status = "failed".into();
            }
        });
    }
    pub fn resolve(self: &Arc<Self>, urls: Vec<String>) -> Result<Source> {
        if self.install_busy.load(Ordering::SeqCst) {
            return Err(AppError::new("busy", "An update is running"));
        }
        if urls.is_empty() || urls.len() > 100 {
            return Err(AppError::new(
                "invalid_url",
                "Enter 1–100 HTTP/HTTPS addresses",
            ));
        }
        for url in &urls {
            tools::validate_url(url)?;
        }
        self.tools.require("yt-dlp")?;
        let source = Source::new(urls);
        self.publish_source(&source)?;
        let (stop, rx) = watch::channel(None);
        let pid = Arc::new(AtomicU32::new(0));
        self.sources.lock().unwrap().insert(
            source.id.clone(),
            Active {
                stop,
                pid: pid.clone(),
                site: String::new(),
            },
        );
        let core = self.clone();
        let initial = source.clone();
        tauri::async_runtime::spawn(async move {
            core.parse_source(initial, rx, pid).await;
        });
        Ok(source)
    }
    async fn parse_source(
        &self,
        mut source: Source,
        rx: watch::Receiver<Option<StopReason>>,
        pid: Arc<AtomicU32>,
    ) {
        let result: Result<()> = async {
            let program = self.tools.require("yt-dlp")?;
            let settings = self.settings()?;
            let mut seen = HashSet::new();
            for url in source.urls.clone() {
                let mut args = engine::base_args(&settings, &self.tools)?;
                let (auth_args, _secret_file) = self.auth.arguments(&url)?;
                args.extend(auth_args);
                args.extend(
                    [
                        "--flat-playlist",
                        "--lazy-playlist",
                        "--dump-json",
                        "--skip-download",
                        "--ignore-errors",
                        "--playlist-end",
                        "20001",
                    ]
                    .map(str::to_string),
                );
                args.push(url.clone());
                let mut limit = false;
                let outcome = process::run(
                    &program,
                    &args,
                    rx.clone(),
                    Some(pid.clone()),
                    |line, err| {
                        if err {
                            return;
                        }
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                            if source.entries.len() >= 20_000 {
                                limit = true;
                                return;
                            }
                            if let Ok(item) = engine::media_item(&v, &url) {
                                if seen.insert(format!("{}:{}", item.site, item.id)) {
                                    if source.title.is_empty() {
                                        source.title = v["playlist_title"]
                                            .as_str()
                                            .or(item.uploader.as_deref())
                                            .unwrap_or(&item.title)
                                            .to_string();
                                    }
                                    source.total = v["playlist_count"]
                                        .as_u64()
                                        .map(|n| n as usize)
                                        .or(source.total);
                                    source.entries.push(item);
                                    if source.entries.len() == 1
                                        || source.entries.len().is_multiple_of(20)
                                    {
                                        source.updated_at = now();
                                        let _ = self.publish_source(&source);
                                    }
                                }
                            }
                        }
                    },
                )
                .await?;
                if outcome.stopped.is_some() {
                    return Err(AppError::new("cancelled", "Parsing cancelled"));
                }
                if limit {
                    return Err(AppError::new(
                        "list_limit",
                        "The list is limited to 20,000 videos; narrow the source",
                    ));
                }
                if !outcome.success {
                    return Err(process::classify(&outcome.stderr));
                }
            }
            if source.entries.is_empty() {
                return Err(AppError::new(
                    "empty_list",
                    "No downloadable videos were found",
                ));
            }
            Ok(())
        }
        .await;
        source.status = if result.is_ok() {
            "ready"
        } else if result.as_ref().err().is_some_and(|e| e.code == "cancelled") {
            "cancelled"
        } else {
            "failed"
        }
        .into();
        source.error = result.err();
        source.updated_at = now();
        if source.status == "ready" {
            source.total = Some(source.entries.len());
        }
        let _ = self.publish_source(&source);
        self.sources.lock().unwrap().remove(&source.id);
    }
    pub fn cancel_source(&self, id: &str) {
        if let Some(active) = self.sources.lock().unwrap().get(id) {
            let _ = active.stop.send(Some(StopReason::Cancel));
        }
    }
    pub async fn connect(&self, request: AuthRequest) -> Result<AuthConnection> {
        if self.auth_busy.swap(true, Ordering::SeqCst) {
            return Err(AppError::new(
                "busy",
                "Connection verification is already running",
            ));
        }
        let result = async {
            let args = engine::base_args(&self.settings()?, &self.tools)?;
            let auth = self.auth.connect(request, &self.tools, args).await?;
            self.db.set("auth", &auth)?;
            self.event(CoreEvent::Auth(auth.clone()));
            let waiting = self
                .db
                .jobs()?
                .into_iter()
                .filter(|j| j.status == "waiting_auth")
                .map(|j| j.id)
                .collect();
            self.control(waiting, "resume")?;
            Ok(auth)
        }
        .await;
        self.auth_busy.store(false, Ordering::SeqCst);
        result
    }
    pub fn disconnect(&self) -> Result<AuthConnection> {
        if self.auth_busy.load(Ordering::SeqCst) {
            return Err(AppError::new("busy", "Connection verification is running"));
        }
        self.auth.disconnect();
        let auth = self.auth.snapshot();
        self.db.set("auth", &auth)?;
        self.event(CoreEvent::Auth(auth.clone()));
        Ok(auth)
    }
    pub async fn install(&self, update: bool) -> Result<Vec<ToolInfo>> {
        if self.install_busy.swap(true, Ordering::SeqCst) {
            return Err(AppError::new("busy", "Component installation is running"));
        }
        let result = async {
            if !self.active.lock().unwrap().is_empty() || !self.sources.lock().unwrap().is_empty() {
                return Err(AppError::new(
                    "busy",
                    "Pause downloads and finish parsing before updating components",
                ));
            }
            let tools = self.tools.install(&self.settings()?, update).await?;
            self.event(CoreEvent::Tools(tools.clone()));
            Ok(tools)
        }
        .await;
        self.install_busy.store(false, Ordering::SeqCst);
        result
    }
    pub fn completed_path(&self, id: &str) -> Result<PathBuf> {
        let job = self.db.job(id)?;
        if job.status != "completed" {
            return Err(AppError::new("not_found", "Task has no completed file"));
        }
        let file = job
            .file_path
            .ok_or_else(|| AppError::new("not_found", "File path is missing"))?;
        let path = PathBuf::from(file);
        if !path.is_file() {
            return Err(AppError::new(
                "not_found",
                "The local file has been moved or deleted",
            ));
        }
        Ok(path)
    }
    pub fn export_csv(&self, path: &Path) -> Result<usize> {
        if !path.is_absolute() {
            return Err(AppError::new(
                "invalid_request",
                "Choose an absolute export path",
            ));
        }
        let jobs: Vec<_> = self
            .db
            .jobs()?
            .into_iter()
            .filter(|j| j.status == "completed")
            .collect();
        let mut csv = String::from("\u{feff}Title,URL,File,Bytes,Duration seconds\r\n");
        for j in &jobs {
            let cells = [
                j.item.title.clone(),
                j.item.url.clone(),
                j.file_path.clone().unwrap_or_default(),
                j.file_size.unwrap_or(0).to_string(),
                j.duration.unwrap_or(0.).to_string(),
            ];
            csv.push_str(
                &cells
                    .iter()
                    .map(|v| csv_cell(v))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            csv.push_str("\r\n");
        }
        std::fs::write(path, csv)?;
        Ok(jobs.len())
    }
    pub async fn prepare_update(&self) -> Result<()> {
        let ids = self
            .db
            .jobs()?
            .into_iter()
            .filter(|j| {
                j.is_active()
                    || ["queued", "retry_wait", "waiting_auth"].contains(&j.status.as_str())
            })
            .map(|j| j.id)
            .collect();
        if self.install_busy.swap(true, Ordering::SeqCst) {
            return Err(AppError::new("busy", "A component update is running"));
        }
        if let Err(e) = self.control(ids, "pause") {
            self.finish_update();
            return Err(e);
        }
        for active in self.sources.lock().unwrap().values() {
            let _ = active.stop.send(Some(StopReason::Pause));
        }
        for _ in 0..100 {
            if self.active.lock().unwrap().is_empty() && self.sources.lock().unwrap().is_empty() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.finish_update();
        Err(AppError::new(
            "busy",
            "Wait for download processes to stop before installing",
        ))
    }
    pub fn finish_update(&self) {
        self.install_busy.store(false, Ordering::SeqCst);
    }
    pub fn stop_all(&self) {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        for active in self
            .active
            .lock()
            .unwrap()
            .values()
            .chain(self.sources.lock().unwrap().values())
        {
            let _ = active.stop.send(Some(StopReason::Shutdown));
            process::kill_pid(active.pid.load(Ordering::SeqCst));
        }
        let _ = self.db.recover();
    }
}
fn csv_cell(v: &str) -> String {
    let safe = if v.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{v}")
    } else {
        v.into()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn item() -> MediaItem {
        MediaItem {
            id: "test".into(),
            title: "Test".into(),
            url: "https://example.com/video".into(),
            thumbnail: None,
            duration: None,
            uploader: None,
            site: "example.com".into(),
        }
    }
    #[test]
    fn deduplicates_and_recovers_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let c = Core::new(dir.path().into()).unwrap();
        let req = EnqueueRequest {
            items: vec![item(), item()],
            options: DownloadOptions::default(),
            destination: dir.path().join("output").to_string_lossy().into(),
            source_title: "../CON".into(),
        };
        let r = c.enqueue(req).unwrap();
        assert_eq!(r.jobs.len(), 1);
        assert_eq!(r.skipped, 1);
        let id = &r.jobs[0].id;
        c.update_job(id, |j| j.status = "downloading".into())
            .unwrap();
        drop(c);
        let c = Core::new(dir.path().into()).unwrap();
        assert_eq!(c.db.job(id).unwrap().status, "paused");
        assert!(c.completed_path(id).is_err());
    }
    #[test]
    fn scheduler_can_start_from_a_native_ipc_thread() {
        let root = tempfile::tempdir().unwrap();
        let core = Core::new(root.path().into()).unwrap();
        core.start();
        core.stop_all();
    }
    #[test]
    fn csv_does_not_execute_formulas() {
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
    }
}
