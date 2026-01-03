use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::llm::prompt::LlmPrompt;

pub struct Ollama;

impl Ollama {
    pub fn run(prompt: LlmPrompt, model: &str) -> io::Result<String> {
        let script = ollama_script_path()?;

        let mut child = Command::new(python_bin())
            .arg(script)
            .env("OSMOGREP_OLLAMA_MODEL", model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let input = format!(
            "[SYSTEM]\n{}\n\n[USER]\n{}\n",
            prompt.system,
            prompt.user
        );

        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "failed to open stdin")
            })?;
            stdin.write_all(input.as_bytes())?;
        }

        let output = wait_with_timeout(child, Duration::from_secs(120))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                err.trim().to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

fn ollama_script_path() -> io::Result<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("llm_py")
        .join("ollama.py");

    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("ollama script not found: {}", path.display()),
        ));
    }

    Ok(path)
}

fn python_bin() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> io::Result<std::process::Output> {
    use std::thread;
    use std::time::Instant;

    let start = Instant::now();
    loop {
        if let Some(_) = child.try_wait()? {
            return child.wait_with_output();
        }

        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ollama process timed out",
            ));
        }

        thread::sleep(Duration::from_millis(25));
    }
}
