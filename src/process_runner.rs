use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct ProcessRun {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub timed_out: bool,
    pub cancelled: bool,
}

pub fn run_shell_command(
    cmd: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<ProcessRun, String> {
    run_shell_command_cancellable(cmd, cwd, timeout, || false)
}

pub fn run_shell_command_cancellable(
    cmd: &str,
    cwd: Option<&Path>,
    timeout: Duration,
    is_cancelled: impl Fn() -> bool,
) -> Result<ProcessRun, String> {
    let mut command = Command::new("sh");
    command.arg("-c").arg(cmd);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    run_command_cancellable(command, timeout, is_cancelled)
}

pub fn run_command(command: Command, timeout: Duration) -> Result<ProcessRun, String> {
    run_command_cancellable(command, timeout, || false)
}

pub fn run_command_cancellable(
    mut command: Command,
    timeout: Duration,
    is_cancelled: impl Fn() -> bool,
) -> Result<ProcessRun, String> {
    let started = Instant::now();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut timed_out = false;
    let mut cancelled = false;

    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if is_cancelled() => {
                cancelled = true;
                let _ = child.kill();
                break;
            }
            Ok(None) if started.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }

    let mut out = child.wait_with_output().map_err(|e| e.to_string())?;
    if timed_out {
        append_stderr_line(
            &mut out.stderr,
            &format!("[osmogrep] command timed out after {}s", timeout.as_secs()),
        );
    }
    if cancelled {
        append_stderr_line(&mut out.stderr, "[osmogrep] command cancelled");
    }

    Ok(ProcessRun {
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.status.code().unwrap_or(-1),
        duration_ms: started.elapsed().as_millis(),
        timed_out,
        cancelled,
    })
}

pub fn run_command_with_stdin_cancellable(
    mut command: Command,
    stdin: &[u8],
    timeout: Duration,
    is_cancelled: impl Fn() -> bool,
) -> Result<ProcessRun, String> {
    let started = Instant::now();
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut timed_out = false;
    let mut cancelled = false;
    let input = stdin.to_vec();
    let writer = child.stdin.take().map(|mut child_stdin| {
        thread::spawn(move || child_stdin.write_all(&input).map_err(|e| e.to_string()))
    });

    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if is_cancelled() => {
                cancelled = true;
                let _ = child.kill();
                break;
            }
            Ok(None) if started.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }

    let mut out = child.wait_with_output().map_err(|e| e.to_string())?;
    if let Some(writer) = writer {
        match writer.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => append_stderr_line(
                &mut out.stderr,
                &format!("[osmogrep] stdin write failed: {e}"),
            ),
            Err(_) => append_stderr_line(&mut out.stderr, "[osmogrep] stdin writer panicked"),
        }
    }
    if timed_out {
        append_stderr_line(
            &mut out.stderr,
            &format!("[osmogrep] command timed out after {}s", timeout.as_secs()),
        );
    }
    if cancelled {
        append_stderr_line(&mut out.stderr, "[osmogrep] command cancelled");
    }

    Ok(ProcessRun {
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.status.code().unwrap_or(-1),
        duration_ms: started.elapsed().as_millis(),
        timed_out,
        cancelled,
    })
}

fn append_stderr_line(stderr: &mut Vec<u8>, line: &str) {
    if !stderr.ends_with(b"\n") && !stderr.is_empty() {
        stderr.push(b'\n');
    }
    stderr.extend_from_slice(line.as_bytes());
    stderr.push(b'\n');
}

pub fn timeout_from_env(key: &str, default_secs: u64) -> Duration {
    let secs = std::env::var(key)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(default_secs);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_shell_command_successfully() {
        let run = run_shell_command("printf ok", None, Duration::from_secs(5)).unwrap();

        assert_eq!(run.exit_code, 0);
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok");
        assert!(!run.timed_out);
    }

    #[test]
    fn runs_command_with_args_successfully() {
        let mut command = Command::new("printf");
        command.arg("ok");
        let run = run_command(command, Duration::from_secs(5)).unwrap();

        assert_eq!(run.exit_code, 0);
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok");
        assert!(!run.timed_out);
    }

    #[test]
    fn runs_command_with_stdin_successfully() {
        let command = Command::new("cat");
        let run =
            run_command_with_stdin_cancellable(command, b"ok", Duration::from_secs(5), || false)
                .unwrap();

        assert_eq!(run.exit_code, 0);
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok");
        assert!(!run.timed_out);
    }

    #[test]
    fn times_out_shell_command() {
        let run = run_shell_command("sleep 2", None, Duration::from_millis(100)).unwrap();

        assert!(run.timed_out);
        assert_ne!(run.exit_code, 0);
        assert!(String::from_utf8_lossy(&run.stderr).contains("timed out"));
    }

    #[test]
    fn times_out_command_with_stdin() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("sleep 2");
        let run =
            run_command_with_stdin_cancellable(command, b"ok", Duration::from_millis(100), || {
                false
            })
            .unwrap();

        assert!(run.timed_out);
        assert_ne!(run.exit_code, 0);
        assert!(String::from_utf8_lossy(&run.stderr).contains("timed out"));
    }

    #[test]
    fn cancels_command_with_stdin() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("sleep 2");
        let run =
            run_command_with_stdin_cancellable(command, b"ok", Duration::from_secs(5), || true)
                .unwrap();

        assert!(run.cancelled);
        assert_ne!(run.exit_code, 0);
        assert!(String::from_utf8_lossy(&run.stderr).contains("cancelled"));
    }

    #[test]
    fn cancels_shell_command_without_stdin() {
        let run = run_shell_command_cancellable("sleep 2", None, Duration::from_secs(5), || true)
            .unwrap();

        assert!(run.cancelled);
        assert_ne!(run.exit_code, 0);
        assert!(String::from_utf8_lossy(&run.stderr).contains("cancelled"));
    }
}
