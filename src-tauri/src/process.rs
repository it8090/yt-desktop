use crate::models::*;
use std::{
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::watch,
};

pub struct ProcessResult {
    pub success: bool,
    pub stderr: String,
    pub stopped: Option<StopReason>,
}
pub fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.as_std_mut().creation_flags(0x08000000);
    }
    c
}
async fn terminate(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
    }
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .output()
            .await;
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
}
pub fn kill_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
struct ProcessGuard {
    pid: u32,
    output: Option<Arc<AtomicU32>>,
    armed: bool,
}
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if self.armed {
            kill_pid(self.pid);
        }
        if let Some(o) = &self.output {
            o.store(0, Ordering::SeqCst);
        }
    }
}
pub async fn run<F>(
    program: &Path,
    args: &[String],
    mut stop: watch::Receiver<Option<StopReason>>,
    pid_output: Option<Arc<AtomicU32>>,
    mut on_line: F,
) -> Result<ProcessResult>
where
    F: FnMut(&str, bool) + Send,
{
    if let Some(reason) = *stop.borrow() {
        return Ok(ProcessResult {
            success: false,
            stderr: String::new(),
            stopped: Some(reason),
        });
    }
    let mut child = command(program)
        .args(args)
        .spawn()
        .map_err(|e| AppError::new("engine_start", e))?;
    let pid = child.id().unwrap_or(0);
    if let Some(output) = &pid_output {
        output.store(pid, Ordering::SeqCst);
    }
    let mut guard = ProcessGuard {
        pid,
        output: pid_output,
        armed: true,
    };
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let mut out_done = false;
    let mut err_done = false;
    let mut cancel_open = true;
    let mut diagnostics = String::new();
    loop {
        tokio::select! {
            changed=stop.changed(),if cancel_open=>{if changed.is_ok(){let reason=*stop.borrow();if let Some(reason)=reason{terminate(&mut child).await;return Ok(ProcessResult{success:false,stderr:diagnostics,stopped:Some(reason)});}}else{cancel_open=false}},
            line=stdout.next_line(),if !out_done=>{match line{Ok(Some(line))=>on_line(&line,false),_=>out_done=true}},
            line=stderr.next_line(),if !err_done=>{match line{Ok(Some(line))=>{on_line(&line,true);if diagnostics.len()>14000{let start=diagnostics.char_indices().find(|(i,_)|*i>=7000).map(|(i,_)|i).unwrap_or(0);diagnostics=diagnostics[start..].into();}diagnostics.push_str(&redact(&line));diagnostics.push('\n');},_=>err_done=true}},
        }
        if out_done && err_done {
            break;
        }
    }
    let status = child.wait().await?;
    guard.armed = false;
    Ok(ProcessResult {
        success: status.success(),
        stderr: diagnostics,
        stopped: None,
    })
}
pub fn redact(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if lower.contains("authorization:")
        || lower.contains("cookie:")
        || lower.contains("set-cookie:")
    {
        return "[redacted authentication header]".into();
    }
    static URLS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    URLS.get_or_init(|| regex::Regex::new(r#"https?://[^\s"<>]+"#).unwrap())
        .replace_all(input, |caps: &regex::Captures| {
            let value = &caps[0];
            if let Ok(mut url) = url::Url::parse(value) {
                let _ = url.set_username("");
                let _ = url.set_password(None);
                if url.query().is_some() {
                    url.set_query(Some("[redacted]"));
                }
                url.set_fragment(None);
                url.to_string()
            } else {
                "[redacted URL]".into()
            }
        })
        .into_owned()
}
pub fn classify(stderr: &str) -> AppError {
    let s = stderr.to_ascii_lowercase();
    let code = if s.contains("429") || s.contains("too many requests") {
        "rate_limited"
    } else if s.contains("sign in")
        || s.contains("cookies are no longer valid")
        || s.contains("login required")
        || s.contains("confirm your age")
    {
        "auth_required"
    } else if s.contains("keyring")
        || s.contains("decrypt")
        || s.contains("cookie database")
        || s.contains("permission denied") && s.contains("cookie")
    {
        "browser_permission"
    } else if s.contains("no space left") || s.contains("disk full") {
        "disk_full"
    } else if s.contains("permission denied") || s.contains("access is denied") {
        "permission"
    } else if s.contains("requested format is not available") {
        "format_unavailable"
    } else if s.contains("private video")
        || s.contains("video unavailable")
        || s.contains("not available in your country")
        || s.contains("removed by")
    {
        "unavailable"
    } else if s.contains("unsupported url") {
        "unsupported"
    } else if s.contains("ssl")
        || s.contains("timed out")
        || s.contains("connection")
        || s.contains("http error 5")
        || s.contains("temporary failure")
        || s.contains("network")
    {
        "network"
    } else {
        "download_failed"
    };
    let detail = stderr
        .lines()
        .rev()
        .find(|l| l.contains("ERROR:"))
        .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()))
        .unwrap_or("Download process exited unsuccessfully");
    AppError::new(code, redact(detail))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_redacted() {
        assert!(!redact("http://user:secret@example.com/video").contains("secret"));
        assert_eq!(
            redact("Authorization: Bearer abc"),
            "[redacted authentication header]"
        );
        assert!(!redact("ERROR https://video.example/play?sig=secret").contains("secret"));
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn pause_terminates_a_running_process_group() {
        let (stop, rx) = watch::channel(None);
        let pid = Arc::new(AtomicU32::new(0));
        let output = pid.clone();
        let worker = tokio::spawn(async move {
            run(
                Path::new("/bin/sh"),
                &["-c".into(), "echo ready; sleep 30".into()],
                rx,
                Some(output),
                |_, _| {},
            )
            .await
        });
        for _ in 0..100 {
            if pid.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(pid.load(Ordering::SeqCst) > 0);
        stop.send(Some(StopReason::Pause)).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result.stopped, Some(StopReason::Pause));
        assert_eq!(pid.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn rate_limits_and_login_are_distinct() {
        assert_eq!(
            classify("HTTP Error 429: Too Many Requests").code,
            "rate_limited"
        );
        assert_eq!(
            classify("Sign in to confirm you're not a bot").code,
            "auth_required"
        );
    }
}
