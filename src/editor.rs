//! Neovim uses an isolated tmux server when launched from a standalone terminal.
use crate::{
    commands::shell_escape,
    logger::log,
    state::{AgentState, LogLevel},
};
use std::{env, path::Path, process::Command};
#[derive(Clone)]
pub struct Session {
    pub socket: String,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Window,
    Split(u16),
}
pub fn layout(columns: u16) -> Layout {
    if columns < 96 {
        Layout::Window
    } else {
        Layout::Split(
            ((u32::from(columns) * 52 / 100) as u16)
                .max(44)
                .min(columns - 44),
        )
    }
}
fn run(socket: Option<&str>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("tmux");
    if let Some(socket) = socket {
        cmd.args(["-L", socket]);
    }
    let out = cmd.args(args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
fn editor_command(root: &Path, entry: &str, owner: &str) -> String {
    format!(
        "cd {} && OSMOGREP_AGENT_PANE={} exec nvim --clean -i NONE --cmd {} -- {}",
        shell_escape(&root.display().to_string()),
        shell_escape(owner),
        shell_escape(&format!("lua {}", include_str!("editor.lua"))),
        shell_escape(entry)
    )
}

fn owned_editor(owner: &str) -> Option<String> {
    let session = run(
        None,
        &["display-message", "-p", "-t", owner, "#{session_id}"],
    )
    .ok()?;
    let panes = run(
        None,
        &[
            "list-panes",
            "-s",
            "-t",
            &session,
            "-F",
            "#{pane_id} #{@osmogrep-editor}",
        ],
    )
    .ok()?;
    panes.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let pane = parts.next()?;
        if parts.next() == Some(owner) {
            Some(pane.to_string())
        } else {
            None
        }
    })
}
fn focus(socket: Option<&str>, pane: &str) -> Result<(), String> {
    let window = run(
        socket,
        &["display-message", "-p", "-t", pane, "#{window_id}"],
    )?;
    run(socket, &["select-window", "-t", &window])?;
    run(socket, &["select-pane", "-t", pane])?;
    Ok(())
}
fn open_pane(
    socket: Option<&str>,
    owner: &str,
    root: &Path,
    entry: &str,
    columns: u16,
) -> Result<String, String> {
    let cwd = root.display().to_string();
    let command = editor_command(root, entry, owner);
    let pane = match layout(columns) {
        Layout::Window => {
            let window = run(
                socket,
                &["display-message", "-p", "-t", owner, "#{window_id}"],
            )?;
            run(
                socket,
                &[
                    "new-window",
                    "-a",
                    "-d",
                    "-P",
                    "-F",
                    "#{pane_id}",
                    "-n",
                    "editor",
                    "-t",
                    &window,
                    "-c",
                    &cwd,
                    &command,
                ],
            )?
        }
        Layout::Split(size) => run(
            socket,
            &[
                "split-window",
                "-h",
                "-b",
                "-l",
                &size.to_string(),
                "-P",
                "-F",
                "#{pane_id}",
                "-t",
                owner,
                "-c",
                &cwd,
                &command,
            ],
        )?,
    };
    run(
        socket,
        &["set-option", "-p", "-t", &pane, "@osmogrep-editor", owner],
    )?;
    focus(socket, &pane)?;
    Ok(pane)
}
pub fn open(state: &mut AgentState, raw: &str) {
    if let Err(e) = open_inner(state, raw) {
        log(state, LogLevel::Error, format!("Editor: {e}"));
    }
}
fn open_inner(state: &mut AgentState, raw: &str) -> Result<(), String> {
    if raw.trim() == "/nv help" {
        log(state,LogLevel::Info,"Neovim: :q closes; unsaved files prompt. Space a returns to chat; Space e browses files. Alt+Left/Right switches managed panes. Below 96 columns, Ctrl+b p switches editor/chat windows. /nv toggle requests a safe close.");
        return Ok(());
    }
    for executable in ["tmux", "nvim"] {
        if !Command::new("sh")
            .args(["-c", &format!("command -v {executable} >/dev/null 2>&1")])
            .status()
            .is_ok_and(|s| s.success())
        {
            return Err(format!(
                "{executable} is missing; install it before using /nv"
            ));
        }
    }
    let (columns, rows) = crossterm::terminal::size().unwrap_or((100, 30));
    let entry = raw.strip_prefix("/nv").unwrap_or("").trim();
    let entry = if entry.is_empty() || entry == "toggle" {
        state
            .ui
            .active_edit_target
            .clone()
            .unwrap_or_else(|| ".".into())
    } else {
        entry.to_string()
    };
    if env::var_os("TMUX").is_some() {
        let owner = env::var("TMUX_PANE").map_err(|_| "Cannot identify this tmux pane")?;
        if let Some(pane) = owned_editor(&owner) {
            focus(None, &pane)?;
            if raw.trim() == "/nv toggle" {
                run(
                    None,
                    &["send-keys", "-t", &pane, "Escape", ":confirm qall", "Enter"],
                )?;
            }
        } else {
            open_pane(None, &owner, &state.repo_root, &entry, columns)?;
        }
        return Ok(());
    }
    if state.ui.agent_running {
        return Err("Finish or cancel the current run before switching a standalone session into the editor workspace".into());
    }
    crate::persistence::save(state)?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let session = Session {
        socket: format!("osmogrep-{}", &token[..12]),
        name: "osmogrep".into(),
    };
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let command = format!(
        "tmux -L {} wait-for agent-ready && OSMOGREP_MANAGED_TMUX=1 OSMOGREP_NV_SESSION={} {} --resume {}",
        shell_escape(&session.socket),
        shell_escape(&session.name),
        shell_escape(&executable.display().to_string()),
        shell_escape(&state.session.id)
    );
    let socket = Some(session.socket.as_str());
    let owner = run(
        socket,
        &[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "-s",
            &session.name,
            "-x",
            &columns.to_string(),
            "-y",
            &rows.to_string(),
            "-c",
            &state.repo_root.display().to_string(),
            &command,
        ],
    )?;
    let configure = (|| {
        run(
            socket,
            &["set-option", "-t", &session.name, "status", "off"],
        )?;
        run(socket, &["set-option", "-t", &session.name, "mouse", "on"])?;
        run(
            socket,
            &[
                "set-window-option",
                "-g",
                "pane-border-style",
                "fg=#292724,bg=#000000",
            ],
        )?;
        run(
            socket,
            &[
                "set-window-option",
                "-g",
                "pane-active-border-style",
                "fg=#805638,bg=#000000",
            ],
        )?;
        run(socket, &["bind-key", "-n", "M-Left", "select-pane", "-L"])?;
        run(socket, &["bind-key", "-n", "M-Right", "select-pane", "-R"])?;
        let _ = run(
            socket,
            &["set-option", "-w", "-t", &owner, "allow-passthrough", "on"],
        );
        open_pane(socket, &owner, &state.repo_root, &entry, columns)?;
        Ok::<_, String>(())
    })();
    if let Err(error) = configure {
        let _ = run(socket, &["kill-session", "-t", &session.name]);
        return Err(error);
    }
    log(
        state,
        LogLevel::Info,
        if matches!(layout(columns), Layout::Split(_)) {
            "Editor workspace · Alt+Left/Right switches panes · Space a returns to chat"
        } else {
            "Editor workspace · Space a returns to chat · Ctrl+b p returns to the editor"
        },
    );
    state.ui.editor_session = Some(session);
    state.ui.should_exit = true;
    Ok(())
}
pub fn attach(session: &Session) {
    if let Err(error) = run(Some(&session.socket), &["wait-for", "-S", "agent-ready"]) {
        eprintln!("Editor workspace: {error}");
        return;
    }
    let _ = Command::new("tmux")
        .args(["-L", &session.socket, "attach-session", "-t", &session.name])
        .status();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_terminals_use_windows_and_splits_keep_the_agent_readable() {
        for width in [40, 80, 95] {
            assert_eq!(layout(width), Layout::Window)
        }
        for width in [96, 100, 120, 144, 160, 240] {
            let Layout::Split(left) = layout(width) else {
                panic!()
            };
            assert!(left >= 44);
            assert!(width - left >= 44)
        }
    }
    #[test]
    fn launch_preserves_user_config_quotes_paths_and_never_syncs_plugins() {
        let command = editor_command(Path::new("/tmp/my repo"), "file ' name.rs", "%2");
        assert!(command.contains("exec nvim --clean -i NONE"));
        assert!(command.contains("OSMOGREP_AGENT_PANE="));
        assert!(command.contains("space a: chat"));
        assert!(!command.contains("Lazy"));
        assert!(!command.contains("NvimTree"));
        assert!(!command.contains("headless"));
        assert!(command.contains("'\"'\"'"));
    }
}
