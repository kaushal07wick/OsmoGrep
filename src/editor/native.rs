//! A native Ghostty editor leaves the live agent process and its graphics in place.
use super::editor_command;
use crate::{
    commands::shell_escape,
    state::{AgentState, LogLevel},
    workspace::ghostty,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn rpc(socket: &Path, expression: &str) -> Result<String, String> {
    let mut command = Command::new("nvim");
    command.stdin(std::process::Stdio::null());
    command.args([
        "--server",
        &socket.display().to_string(),
        "--remote-expr",
        expression,
    ]);
    let result = crate::process_runner::run_command(command, Duration::from_secs(2))?;
    if result.exit_code != 0 {
        return Err(String::from_utf8_lossy(&result.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().into())
}

fn socket(session: &str) -> Result<PathBuf, String> {
    #[cfg(unix)]
    let user = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let user = 0;
    let parent = if cfg!(target_os = "macos") {
        PathBuf::from("/private/tmp")
    } else {
        std::env::temp_dir()
    };
    let directory = parent.join(format!("osmogrep-editor-{user}"));
    if std::fs::symlink_metadata(&directory).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Refusing a linked editor socket directory".into());
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if std::fs::metadata(&directory)
            .map_err(|e| e.to_string())?
            .uid()
            != user
        {
            return Err("Editor socket directory belongs to another user".into());
        }
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let id: String = session
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(16)
        .collect();
    Ok(directory.join(format!("{}-{id}.sock", std::process::id())))
}

fn open_expression(entry: &str) -> String {
    let json = serde_json::to_string(entry).unwrap();
    format!(
        "luaeval('osmogrep_workspace.open_path(vim.json.decode(_A))', '{}')",
        json.replace('\'', "''")
    )
}

pub fn open(
    state: &mut AgentState,
    entry: String,
    bridge: PathBuf,
    toggle: bool,
) -> Result<(), String> {
    if state.ui.workspace_task.is_some() {
        return Err("A workspace pane is already opening".into());
    }
    let owner = ghostty::caller_tty()?;
    let root = state.repo_root.clone();
    let socket = socket(&state.session.id)?;
    let script = bridge
        .parent()
        .ok_or("Editor bridge has no directory")?
        .join("ghostty.js");
    crate::storage::atomic_write(&script, ghostty::SCRIPT.as_bytes()).map_err(|e| e.to_string())?;
    let pixels = crossterm::terminal::window_size()
        .map(|s| s.width / 12)
        .unwrap_or(96)
        .clamp(60, 180);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let pane_file = socket.with_extension("pane");
            if rpc(&socket, "getpid()").is_ok() {
                if toggle {
                    let mut command = Command::new("nvim");
                    command.stdin(std::process::Stdio::null());
                    command.args([
                        "--server",
                        &socket.display().to_string(),
                        "--remote-send",
                        "<Esc>:confirm qall<CR>",
                    ]);
                    let result =
                        crate::process_runner::run_command(command, Duration::from_secs(2))?;
                    if result.exit_code != 0 {
                        return Err("Could not request editor close".into());
                    }
                } else {
                    rpc(&socket, &open_expression(&entry))?;
                }
                if let Ok(pane) = std::fs::read_to_string(&pane_file) {
                    // A closing editor may already be gone; focus errors still matter on ordinary opens.
                    let focus = ghostty::action(&root, pane.trim(), "focus", "", 0);
                    if !toggle {
                        focus?;
                    }
                }
                return Ok(if toggle {
                    "Editor close requested · resolve unsaved files in Neovim".into()
                } else {
                    "Editor ready · Space a chat · Space e files · Space s split · Space f follow"
                        .into()
                });
            }
            let command = format!("unset TMUX TMUX_PANE; export PATH={} OSMOGREP_AGENT_TTY={} OSMOGREP_GHOSTTY_SCRIPT={}; {}",
                shell_escape(&std::env::var("PATH").unwrap_or_default()), shell_escape(&owner), shell_escape(&script.display().to_string()),
                editor_command(&root, &entry, "", &bridge, Some(&socket))?);
            let pane = ghostty::action(
                &root,
                &owner,
                "GSrt",
                &format!("/bin/sh -c {}", shell_escape(&command)),
                pixels,
            )?;
            crate::storage::atomic_write(&pane_file, pane.as_bytes()).map_err(|e| e.to_string())?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let process_id = loop {
                let ready = rpc(&socket, "getpid()").and_then(|value| {
                    value
                        .parse::<i32>()
                        .map_err(|e| format!("Invalid editor PID {value:?}: {e}"))
                });
                if let Ok(pid) = ready {
                    if pid > 0 {
                        break pid;
                    }
                }
                if std::time::Instant::now() >= deadline {
                    return Err(format!(
                        "Editor pane opened, but Neovim did not become ready: {}",
                        ready.err().unwrap_or_else(|| "Invalid editor PID".into())
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            };
            std::thread::spawn(move || {
                // Ghostty may retain a finished command surface. Close it only after Neovim has exited.
                #[cfg(unix)]
                while unsafe { libc::kill(process_id, 0) } == 0 {
                    std::thread::sleep(Duration::from_millis(250));
                }
                let _ = process_id;
                let _ = ghostty::action(&root, &pane, "close", "", 0);
            });
            Ok(
                "Editor workspace · chat left, files and code right · Space a returns to chat"
                    .into(),
            )
        })();
        let _ = sender.send(result);
    });
    state.ui.workspace_task = Some(receiver);
    crate::logger::log(state, LogLevel::Info, "Opening editor workspace…");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_file_names_remain_data_and_socket_paths_are_short() {
        let expression = open_expression("a'\n<CR>\".rs");
        assert!(expression.contains("a''\\n<CR>\\\".rs"));
        assert!(
            socket("12345678-1234-1234-1234-123456789012")
                .unwrap()
                .as_os_str()
                .len()
                < 104
        );
    }
}
