//! App-owned command sessions. Original output is spooled; polling uses byte cursors.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

struct Session {
    root: PathBuf,
    child: Option<Child>,
    input: Option<Box<dyn Write + Send>>,
    stdout: PathBuf,
    stderr: PathBuf,
    started: Instant,
    timeout: Duration,
    stopped: Option<&'static str>,
    exit: Option<i32>,
    readers: Vec<thread::JoinHandle<()>>,
    delivered: bool,
    command: Option<String>,
    verification: Option<Value>,
}
type Sessions = HashMap<String, Arc<Mutex<Session>>>;
static SESSIONS: OnceLock<Mutex<Sessions>> = OnceLock::new();
fn sessions() -> &'static Mutex<Sessions> {
    SESSIONS.get_or_init(Default::default)
}

pub fn start(root: &Path, cmd: &str, pty: bool, timeout: Duration) -> Result<String, String> {
    crate::shell_guard::check_shell_command(cmd)?;
    let command = if pty {
        pty_command(cmd)?
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    start_command(root, command, timeout, false, Some(cmd.to_string()))
}
pub fn start_harness(root: &Path, command: Command, timeout: Duration) -> Result<String, String> {
    if Path::new(command.get_program()) != std::env::current_exe().map_err(|e| e.to_string())?
        || command.get_args().next() != Some(std::ffi::OsStr::new("rpc"))
    {
        return Err("Only the current OsmoGrep RPC executable can use the harness launcher".into());
    }
    start_command(root, command, timeout, true, None)
}
fn start_command(
    root: &Path,
    mut command: Command,
    timeout: Duration,
    trusted_harness: bool,
    source: Option<String>,
) -> Result<String, String> {
    let mut registry = sessions()
        .lock()
        .map_err(|_| "Command registry unavailable")?;
    if registry.len() >= 64 {
        let oldest = registry
            .iter()
            .filter_map(|(id, s)| {
                s.lock()
                    .ok()
                    .filter(|s| s.delivered)
                    .map(|s| (id.clone(), s.started))
            })
            .min_by_key(|(_, started)| *started)
            .map(|(id, _)| id);
        if let Some(id) = oldest {
            registry.remove(&id);
        }
    }
    if registry.len() >= 64 {
        return Err(
            "Command session limit reached (64); close a completed session before starting another"
                .into(),
        );
    }
    let id = uuid::Uuid::new_v4().to_string();
    let directory =
        crate::file_guard::internal_dir(root, &format!(".context/osmogrep-processes/{id}"))?;
    let stdout = directory.join("stdout");
    let stderr = directory.join("stderr");
    let out = private_file(&stdout)?;
    let err = private_file(&stderr)?;
    command.current_dir(root);
    if !trusted_harness {
        command = crate::sandbox::wrap(command)?;
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let readers = vec![
        drain(child.stdout.take().ok_or("No command stdout")?, out),
        drain(child.stderr.take().ok_or("No command stderr")?, err),
    ];
    let input = child
        .stdin
        .take()
        .map(|s| Box::new(s) as Box<dyn Write + Send>);
    let session = Arc::new(Mutex::new(Session {
        root: root.to_path_buf(),
        child: Some(child),
        input,
        stdout,
        stderr,
        started: Instant::now(),
        timeout,
        stopped: None,
        exit: None,
        readers,
        delivered: false,
        command: source,
        verification: None,
    }));
    registry.insert(id.clone(), session.clone());
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(50));
        let Ok(mut s) = session.lock() else {
            break;
        };
        if s.refresh().unwrap_or(true) {
            break;
        }
        if s.started.elapsed() >= s.timeout {
            s.stop("timed_out");
        }
    });
    Ok(id)
}

fn private_file(path: &Path) -> Result<File, String> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|e| e.to_string())
}
fn drain(mut pipe: impl Read + Send + 'static, mut file: File) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut bytes = [0u8; 8192];
        let mut total = 0usize;
        while let Ok(n) = pipe.read(&mut bytes) {
            if n == 0 {
                break;
            }
            if total.saturating_add(n) <= 64 * 1024 * 1024 {
                if file.write_all(&bytes[..n]).is_err() {
                    break;
                }
                total += n;
            }
            // Continue draining after the disk budget to prevent child pipe deadlocks.
        }
    })
}
fn pty_command(cmd: &str) -> Result<Command, String> {
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("script");
        c.args(["-q", "/dev/null", "sh", "-c", cmd]);
        return Ok(c);
    }
    #[cfg(target_os = "linux")]
    {
        let mut c = Command::new("script");
        c.args(["-q", "-e", "-f", "-c", cmd, "/dev/null"]);
        return Ok(c);
    }
    #[allow(unreachable_code)]
    Err("PTY sessions are supported on macOS and Linux; use pty=false for pipe sessions".into())
}
impl Session {
    fn refresh(&mut self) -> Result<bool, String> {
        if self.exit.is_none() {
            if let Some(status) = self
                .child
                .as_mut()
                .ok_or("Command handle is closed")?
                .try_wait()
                .map_err(|e| e.to_string())?
            {
                self.exit = Some(status.code().unwrap_or(-1));
                self.input = None;
            }
        }
        let finished = self.exit.is_some() && self.readers.iter().all(|r| r.is_finished());
        if finished {
            self.child.take();
        }
        Ok(finished)
    }
    fn stop(&mut self, reason: &'static str) {
        if self.exit.is_some() && self.readers.iter().all(|r| r.is_finished()) {
            self.child.take();
            return;
        }
        self.stopped = Some(reason);
        self.input = None;
        let Some(child) = self.child.as_mut() else {
            return;
        };
        #[cfg(unix)]
        crate::process_runner::kill_process_group(child.id());
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/T", "/F", "/PID", &child.id().to_string()])
                .output();
        }
        let _ = child.kill();
        if let Ok(status) = child.wait() {
            self.exit = Some(status.code().unwrap_or(-1));
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop("closed");
    }
}

pub fn interact(root: &Path, args: &Value, cancel: &dyn Fn() -> bool) -> Result<Value, String> {
    let id = args["session_id"].as_str().ok_or("Missing session_id")?;
    let session = sessions()
        .lock()
        .map_err(|_| "Command registry unavailable")?
        .get(id)
        .cloned()
        .ok_or("Unknown command session; sessions end when OsmoGrep exits")?;
    let wait = Duration::from_millis(args["yield_ms"].as_u64().unwrap_or(500).min(10_000));
    let until = Instant::now() + wait;
    {
        let mut s = session.lock().map_err(|_| "Command session unavailable")?;
        if s.root != root {
            return Err("Command session belongs to another workspace".into());
        }
        s.refresh()?;
        if args["cancel"] == true {
            s.stop("cancelled");
        }
    }
    if let Some(text) = args["input"].as_str() {
        if text.len() > 64 * 1024 {
            return Err("Command input exceeds 64 KiB".into());
        }
        let mut input = session
            .lock()
            .map_err(|_| "Command session unavailable")?
            .input
            .take()
            .ok_or("Command stdin is closed")?;
        let text = text.as_bytes().to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let result = input.write_all(&text).and_then(|_| input.flush());
            let _ = tx.send((input, result));
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok((input, result)) => {
                    session
                        .lock()
                        .map_err(|_| "Command session unavailable")?
                        .input = Some(input);
                    result.map_err(|e| e.to_string())?;
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    if !cancel() && Instant::now() < deadline => {}
                _ => {
                    session
                        .lock()
                        .map_err(|_| "Command session unavailable")?
                        .stop("stdin_timeout");
                    return Err("Command stdin blocked or was cancelled; command stopped".into());
                }
            }
        }
    }
    if args["close_stdin"] == true {
        session
            .lock()
            .map_err(|_| "Command session unavailable")?
            .input = None;
    }
    loop {
        let mut s = session.lock().map_err(|_| "Command session unavailable")?;
        if cancel() {
            s.stop("cancelled");
            break;
        }
        if s.refresh()? || Instant::now() >= until {
            break;
        }
        drop(s);
        thread::sleep(Duration::from_millis(20));
    }
    let mut s = session.lock().map_err(|_| "Command session unavailable")?;
    let finished = s.refresh()?;
    if finished && s.verification.is_none() {
        let mut output = String::new();
        for path in [&s.stdout, &s.stderr] {
            let mut bytes = Vec::new();
            File::open(path)
                .map_err(|e| e.to_string())?
                .take(128 * 1024)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            output.push_str(&String::from_utf8_lossy(&bytes));
            output.push('\n');
        }
        s.verification = Some(if s.stopped.is_none() {
            crate::verification::to_json(&s.command.as_ref().and_then(|cmd| {
                crate::verification::record_command(root, cmd, s.exit.unwrap_or(-1), &output)
            }))
        } else {
            Value::Null
        });
    }
    let (stdout, out_cursor) = read_chunk(&s.stdout, args["stdout_cursor"].as_u64().unwrap_or(0))?;
    let (mut stderr, err_cursor) =
        read_chunk(&s.stderr, args["stderr_cursor"].as_u64().unwrap_or(0))?;
    if let Some(reason) = s.stopped {
        stderr.push_str(&format!("\n[osmogrep] command {reason}\n"));
    }
    s.delivered = finished
        && out_cursor >= fs::metadata(&s.stdout).map_err(|e| e.to_string())?.len()
        && err_cursor >= fs::metadata(&s.stderr).map_err(|e| e.to_string())?.len();
    let value = json!({"session_id":id,"running":!finished,"exit_code":s.exit,"stdout":stdout,"stderr":stderr,
        "cancelled":s.stopped==Some("cancelled"),"timed_out":s.stopped==Some("timed_out"),
        "stdout_cursor":out_cursor,"stderr_cursor":err_cursor,"duration_ms":s.started.elapsed().as_millis(),"stop_reason":s.stopped,
        "output_files":{"stdout":s.stdout,"stderr":s.stderr},"output_limit_bytes":64*1024*1024,"verification":s.verification});
    if args["close"] == true {
        s.stop("closed");
        drop(s);
        sessions()
            .lock()
            .map_err(|_| "Command registry unavailable")?
            .remove(id);
    }
    Ok(value)
}
fn read_chunk(path: &Path, cursor: u64) -> Result<(String, u64), String> {
    use std::io::{Seek, SeekFrom};
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if cursor > size {
        return Err("Output cursor is beyond the end of the stream".into());
    }
    file.seek(SeekFrom::Start(cursor))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(16 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    // Leave a split UTF-8 character for the next poll instead of corrupting it.
    let used = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => bytes.len(),
    };
    Ok((
        String::from_utf8_lossy(&bytes[..used]).into_owned(),
        cursor + used as u64,
    ))
}
pub struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Ok(mut all) = sessions().lock() {
            for session in all.values() {
                if let Ok(mut s) = session.lock() {
                    s.stop("application_exit");
                }
            }
            all.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_stops_background_commands_and_polled_checks_record_evidence_once() {
        let root = std::env::temp_dir().join(format!("command-deadline-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let id = start(&root, "sleep 5", false, Duration::from_millis(100)).unwrap();
        let result = interact(&root, &json!({"session_id":id,"yield_ms":1000}), &|| false).unwrap();
        assert_eq!(result["timed_out"], true);
        assert_eq!(result["running"], false);
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 0.1; printf '1 passed\\n'"]);
        let id = start_command(
            &root,
            command,
            Duration::from_secs(5),
            false,
            Some("pytest".into()),
        )
        .unwrap();
        let first = interact(&root, &json!({"session_id":id,"yield_ms":0}), &|| false).unwrap();
        assert_eq!(first["running"], true);
        let final_result =
            interact(&root, &json!({"session_id":id,"yield_ms":1000}), &|| false).unwrap();
        assert!(final_result["verification"].is_object());
        interact(&root, &json!({"session_id":id,"yield_ms":0}), &|| false).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(".context/osmogrep-verification.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn pty_command_has_a_real_terminal() {
        let root = std::env::temp_dir().join(format!("command-pty-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let id = start(
            &root,
            "test -t 0 && printf PTY_OK",
            true,
            Duration::from_secs(5),
        )
        .unwrap();
        let result = interact(&root, &json!({"session_id":id,"yield_ms":2000}), &|| false).unwrap();
        assert_eq!(result["exit_code"], 0);
        assert!(result["stdout"].as_str().unwrap().contains("PTY_OK"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn incremental_output_stdin_workspace_boundary_and_cleanup() {
        let root = std::env::temp_dir().join(format!("command-session-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let id = start(
            &root,
            "printf ready; read answer; printf ':%s' \"$answer\"",
            false,
            Duration::from_secs(10),
        )
        .unwrap();
        // Loaded runners can need more than one poll to start the shell.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut first =
            interact(&root, &json!({"session_id":id,"yield_ms":100}), &|| false).unwrap();
        while first["stdout"] == "" && first["running"] == true && Instant::now() < deadline {
            first = interact(&root, &json!({"session_id":id,"yield_ms":100}), &|| false).unwrap();
        }
        assert_eq!(first["stdout"], "ready");
        assert_eq!(first["running"], true);
        assert!(interact(
            Path::new("/wrong-workspace"),
            &json!({"session_id":id}),
            &|| false
        )
        .is_err());
        let second=interact(&root,&json!({"session_id":id,"input":"hello\n","stdout_cursor":first["stdout_cursor"],"yield_ms":1000}),&||false).unwrap();
        assert_eq!(second["stdout"], ":hello");
        assert_eq!(second["exit_code"], 0);
        interact(&root, &json!({"session_id":id,"close":true}), &|| false).unwrap();
        assert!(interact(&root, &json!({"session_id":id}), &|| false).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
