//! Native terminal surfaces keep graphics and input local to each pane.
use std::{path::Path, process::Command, time::Duration};

pub const SCRIPT: &str = include_str!("ghostty-split.js");

pub fn available() -> bool {
    cfg!(target_os = "macos")
        && (std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some()
            || std::env::var("TERM_PROGRAM").as_deref() == Ok("ghostty"))
}

pub fn caller_tty() -> Result<String, String> {
    if std::env::var_os("TMUX").is_some() {
        let pane = std::env::var("TMUX_PANE").map_err(|_| "Cannot identify the agent pane")?;
        let output = Command::new("tmux")
            .args(["display-message", "-p", "-t", &pane, "#{client_tty}"])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().into());
        }
    }
    #[cfg(unix)]
    {
        let mut buffer = [0u8; 1024];
        if unsafe { libc::ttyname_r(0, buffer.as_mut_ptr().cast(), buffer.len()) } == 0 {
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            return Ok(String::from_utf8_lossy(&buffer[..end]).into());
        }
    }
    Err("Open a workspace pane from the interactive terminal.".into())
}

pub fn action(
    root: &Path,
    target: &str,
    direction: &str,
    command: &str,
    grow: u16,
) -> Result<String, String> {
    let mut process = Command::new("/usr/bin/osascript");
    process.current_dir(root).args([
        "-l",
        "JavaScript",
        "-e",
        SCRIPT,
        target,
        &root.display().to_string(),
        direction,
        command,
        &grow.to_string(),
    ]);
    let result = crate::process_runner::run_command(process, Duration::from_secs(20))?;
    if result.exit_code != 0 {
        return Err(format!(
            "Native pane: {}",
            String::from_utf8_lossy(&result.stderr)
                .chars()
                .take(1000)
                .collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().into())
}
