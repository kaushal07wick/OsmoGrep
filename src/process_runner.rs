use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(20);

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
    command: Command,
    timeout: Duration,
    is_cancelled: impl Fn() -> bool,
) -> Result<ProcessRun, String> {
    run_captured(command, None, timeout, &is_cancelled)
}

pub fn run_command_with_stdin_cancellable(
    command: Command,
    stdin: &[u8],
    timeout: Duration,
    is_cancelled: impl Fn() -> bool,
) -> Result<ProcessRun, String> {
    run_captured(command, Some(stdin), timeout, &is_cancelled)
}

fn run_captured(
    mut command: Command,
    stdin: Option<&[u8]>,
    timeout: Duration,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<ProcessRun, String> {
    command = crate::sandbox::wrap(command)?;
    let started = Instant::now();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    // Drain both pipes while the process runs; waiting first deadlocks on large logs.
    let stdout = read_pipe(child.stdout.take().unwrap());
    let stderr = read_pipe(child.stderr.take().unwrap());
    let input = stdin.unwrap_or_default().to_vec();
    let writer = child.stdin.take().map(|mut child_stdin| {
        thread::spawn(move || child_stdin.write_all(&input).map_err(|e| e.to_string()))
    });

    let mut stop = match wait_for_child(&mut child, started, timeout, is_cancelled) {
        Ok(stop) => stop,
        Err(error) => {
            #[cfg(unix)]
            kill_process_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let status = child.wait().map_err(|e| e.to_string())?;
    // Keep the deadline while descendants drain inherited pipes after their parent exits.
    while !stdout.is_finished()
        || !stderr.is_finished()
        || writer.as_ref().map(|w| !w.is_finished()).unwrap_or(false)
    {
        if stop.timed_out || stop.cancelled {
            break;
        }
        stop.cancelled = is_cancelled();
        stop.timed_out = started.elapsed() >= timeout;
        if stop.timed_out || stop.cancelled {
            #[cfg(unix)]
            kill_process_group(child.id());
            break;
        }
        thread::sleep(next_poll_delay(started, timeout));
    }
    let stdout = stdout.join().map_err(|_| "stdout reader panicked")??;
    let mut stderr = stderr.join().map_err(|_| "stderr reader panicked")??;
    if let Some(writer) = writer {
        match writer.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                append_stderr_line(&mut stderr, &format!("[osmogrep] stdin write failed: {e}"))
            }
            Err(_) => append_stderr_line(&mut stderr, "[osmogrep] stdin writer panicked"),
        }
    }
    if stop.timed_out {
        append_stderr_line(
            &mut stderr,
            &format!("[osmogrep] command timed out after {}s", timeout.as_secs()),
        );
    }
    if stop.cancelled {
        append_stderr_line(&mut stderr, "[osmogrep] command cancelled");
    }

    Ok(ProcessRun {
        stdout,
        stderr,
        exit_code: if stop.timed_out || stop.cancelled {
            -1
        } else {
            status.code().unwrap_or(-1)
        },
        duration_ms: started.elapsed().as_millis(),
        timed_out: stop.timed_out,
        cancelled: stop.cancelled,
    })
}

fn read_pipe(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Result<Vec<u8>, String>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        let mut overflow = false;
        loop {
            let count = pipe.read(&mut chunk).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            if bytes.len() + count <= 32 * 1024 * 1024 {
                bytes.extend_from_slice(&chunk[..count]);
            } else {
                overflow = true;
            }
        }
        if overflow {
            return Err("command output exceeds 32 MiB per stream".into());
        }
        Ok(bytes)
    })
}

#[cfg(unix)]
pub(crate) fn kill_process_group(id: u32) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // The child was made leader of a new process group before spawn.
    unsafe {
        kill(-(id as i32), 9);
    }
}

#[derive(Debug, Default)]
struct ProcessStop {
    timed_out: bool,
    cancelled: bool,
}

fn wait_for_child(
    child: &mut Child,
    started: Instant,
    timeout: Duration,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<ProcessStop, String> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(ProcessStop::default()),
            Ok(None) if is_cancelled() => {
                #[cfg(unix)]
                kill_process_group(child.id());
                let _ = child.kill();
                return Ok(ProcessStop {
                    timed_out: false,
                    cancelled: true,
                });
            }
            Ok(None) if started.elapsed() >= timeout => {
                #[cfg(unix)]
                kill_process_group(child.id());
                let _ = child.kill();
                return Ok(ProcessStop {
                    timed_out: true,
                    cancelled: false,
                });
            }
            Ok(None) => thread::sleep(next_poll_delay(started, timeout)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn next_poll_delay(started: Instant, timeout: Duration) -> Duration {
    timeout
        .checked_sub(started.elapsed())
        .unwrap_or(Duration::ZERO)
        .min(PROCESS_POLL_INTERVAL)
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

    #[cfg(unix)]
    #[test]
    fn drains_output_larger_than_a_pipe_buffer() {
        let run =
            run_shell_command("head -c 262144 /dev/zero", None, Duration::from_secs(3)).unwrap();
        assert_eq!(run.stdout.len(), 262144);
        assert_eq!(run.exit_code, 0);
        assert!(!run.timed_out);
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_descendants_holding_output_pipes() {
        let started = Instant::now();
        let run = run_shell_command("sleep 10 & wait", None, Duration::from_millis(100)).unwrap();
        assert!(run.timed_out);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn deadline_applies_when_parent_exits_before_inherited_pipes_close() {
        let started = Instant::now();
        let run = run_shell_command("sleep 10 &", None, Duration::from_millis(100)).unwrap();
        assert!(run.timed_out);
        assert_ne!(run.exit_code, 0);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

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
    fn process_poll_delay_is_capped() {
        let started = Instant::now();

        assert_eq!(
            next_poll_delay(started, Duration::from_secs(5)),
            PROCESS_POLL_INTERVAL
        );
    }

    #[test]
    fn process_poll_delay_uses_remaining_timeout() {
        let started = Instant::now() - Duration::from_millis(95);

        assert!(next_poll_delay(started, Duration::from_millis(100)) <= Duration::from_millis(5));
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
