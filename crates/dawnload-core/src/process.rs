//! Runs one command-line tool and hands back its output line by line. Shared by every engine.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;

/// Tells a running job to stop. The core keeps one per running item.
#[derive(Clone, Debug, Default)]
pub struct StopSignal(Arc<StopInner>);

#[derive(Debug, Default)]
struct StopInner {
    stopped: AtomicBool,
    notify: Notify,
}

impl StopSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }

    pub fn is_stopped(&self) -> bool {
        self.0.stopped.load(Ordering::SeqCst)
    }

    /// Resolves once `stop` is called (immediately if it already was).
    pub async fn stopped(&self) {
        loop {
            let notified = self.0.notify.notified();
            if self.is_stopped() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub exit_code: i32,
    /// The last 64 KB of stdout and stderr together.
    pub output: String,
}

#[derive(Debug)]
pub enum RunError {
    /// The stop signal fired; the tool was interrupted.
    Stopped,
    /// The tool couldn't be started.
    Spawn(std::io::Error),
}

const TAIL_BYTES: usize = 64 * 1024;

/// Shell-style preview of a command, for the log: `yt-dlp -P '/x y' https://…`.
pub fn shell_preview(executable: &Path, arguments: &[String]) -> String {
    let quote = |s: &str| {
        if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=,@%+".contains(c)) {
            s.to_string()
        } else {
            format!("'{}'", s.replace('\'', "'\\''"))
        }
    };
    let name = executable.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    std::iter::once(quote(&name)).chain(arguments.iter().map(|a| quote(a))).collect::<Vec<_>>().join(" ")
}

/// Runs to completion. A non-zero exit is a result, not an error: the engine decides what it means.
/// On stop, the tool gets an interrupt (Ctrl-C) so it can save its partial file, then is killed
/// after two seconds. On Unix the whole process group is signalled, so ffmpeg children stop too.
pub async fn run_tool(
    executable: &Path,
    arguments: &[String],
    env: &[(String, String)],
    cwd: &Path,
    stop: &StopSignal,
    mut on_line: impl FnMut(&str),
) -> Result<ProcessOutput, RunError> {
    if stop.is_stopped() {
        return Err(RunError::Stopped);
    }
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console flashes up from a GUI app.
        command.creation_flags(0x0800_0000);
    }

    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(256);
    let out_task = tokio::spawn(read_lines(stdout, tx.clone()));
    let err_task = tokio::spawn(read_lines(stderr, tx));

    let mut tail: VecDeque<String> = VecDeque::new();
    let mut tail_bytes = 0usize;
    let mut stopped = false;
    let mut streams_open = true;

    let status = loop {
        tokio::select! {
            line = rx.recv(), if streams_open => match line {
                Some(line) => {
                    on_line(&line);
                    tail_bytes += line.len() + 1;
                    tail.push_back(line);
                    while tail_bytes > TAIL_BYTES {
                        if let Some(old) = tail.pop_front() { tail_bytes -= old.len() + 1; }
                    }
                }
                None => streams_open = false,
            },
            status = child.wait(), if !streams_open => break status,
            _ = stop.stopped(), if !stopped => {
                stopped = true;
                interrupt(pid);
                let deadline = tokio::time::sleep(Duration::from_secs(2));
                tokio::pin!(deadline);
                tokio::select! {
                    _ = child.wait() => {}
                    _ = &mut deadline => { kill_tree(pid); let _ = child.start_kill(); }
                }
            }
        }
    };
    let _ = out_task.await;
    let _ = err_task.await;
    if stopped {
        return Err(RunError::Stopped);
    }
    let status = status.map_err(RunError::Spawn)?;
    let output = tail.into_iter().collect::<Vec<_>>().join("\n");
    Ok(ProcessOutput { exit_code: status.code().unwrap_or(-1), output })
}

/// Lines split on `\n` and `\r`, so tools that redraw a line still report each update.
async fn read_lines(stream: impl AsyncRead + Unpin, tx: tokio::sync::mpsc::Sender<String>) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                for part in buf.split(|b| *b == b'\n' || *b == b'\r') {
                    if part.is_empty() {
                        continue;
                    }
                    if tx.send(String::from_utf8_lossy(part).into_owned()).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
fn interrupt(pid: Option<u32>) {
    if let Some(pid) = pid {
        // SAFETY: plain signal send to the child's process group, which we created.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGINT);
        }
    }
}

#[cfg(unix)]
fn kill_tree(pid: Option<u32>) {
    if let Some(pid) = pid {
        // SAFETY: as above.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
fn interrupt(_pid: Option<u32>) {
    // Windows has no Ctrl-C for a console-less child; the deadline below terminates it.
    // yt-dlp keeps its .part file either way, so Resume still continues.
}

#[cfg(not(unix))]
fn kill_tree(pid: Option<u32>) {
    if let Some(pid) = pid {
        // taskkill /T ends the tool's children (ffmpeg) too.
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn streams_lines_and_exit_code() {
        let stop = StopSignal::new();
        let mut lines = Vec::new();
        let out = run_tool(
            Path::new("/bin/sh"),
            &["-c".into(), "echo one; printf 'two\\rthree\\n'; echo err >&2; exit 3".into()],
            &[],
            Path::new("/"),
            &stop,
            |l| lines.push(l.to_string()),
        )
        .await
        .unwrap();
        assert_eq!(out.exit_code, 3);
        lines.sort();
        assert_eq!(lines, vec!["err", "one", "three", "two"]);
    }

    #[tokio::test]
    async fn stop_interrupts_the_tool() {
        let stop = StopSignal::new();
        let stopper = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            stopper.stop();
        });
        let started = std::time::Instant::now();
        let result = run_tool(Path::new("/bin/sh"), &["-c".into(), "sleep 30".into()], &[], Path::new("/"), &stop, |_| {}).await;
        assert!(matches!(result, Err(RunError::Stopped)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn previews_commands() {
        let preview = shell_preview(Path::new("/opt/bin/yt-dlp"), &["-P".into(), "/a b".into(), "it's".into()]);
        assert_eq!(preview, "yt-dlp -P '/a b' 'it'\\''s'");
    }
}
