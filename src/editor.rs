//! Neovim uses an isolated tmux server when launched from a standalone terminal.
use crate::{
    commands::shell_escape,
    logger::log,
    state::{AgentState, LogLevel},
};
use std::{env, path::Path, process::Command};
#[path = "editor/follow.rs"]
mod follow;
pub use follow::Follower;
mod native;
#[derive(Clone)]
pub struct Session {
    pub socket: String,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Stacked,
    Split(u16),
}
pub fn layout(columns: u16) -> Layout {
    layout_width(columns, 42)
}
fn layout_width(columns: u16, percent: u16) -> Layout {
    if columns < 100 {
        Layout::Stacked
    } else {
        Layout::Split(
            ((u32::from(columns) * u32::from(percent.clamp(20, 80)) / 100) as u16)
                .max(28)
                .min(columns - 28),
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
fn editor_command(
    root: &Path,
    entry: &str,
    owner: &str,
    bridge: &Path,
    socket: Option<&Path>,
) -> Result<String, String> {
    let entry = if root.join(entry).is_dir() { "" } else { entry };
    let configuration = bridge.with_extension("lua");
    crate::storage::atomic_write(
        &configuration,
        format!(
            "{}\n{}",
            include_str!("editor.lua"),
            include_str!("editor/workspace.lua")
        )
        .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(format!(
        "cd {} && OSMOGREP_AGENT_PANE={} OSMOGREP_EDITOR_BRIDGE={} exec nvim --clean -i NONE {} -u {} -- {}",
        shell_escape(&root.display().to_string()),
        shell_escape(owner),
        shell_escape(&bridge.display().to_string()),
        socket.map(|path|format!("--listen {}",shell_escape(&path.display().to_string()))).unwrap_or_default(),
        shell_escape(&configuration.display().to_string()),
        if entry.is_empty() { String::new() } else { shell_escape(entry) }
    ))
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
    percent: u16,
    bridge: &Path,
) -> Result<String, String> {
    let cwd = root.display().to_string();
    let command = editor_command(root, entry, owner, bridge, None)?;
    let pane = match layout_width(columns, percent) {
        Layout::Stacked => run(
            socket,
            &[
                "split-window",
                "-v",
                "-b",
                "-l",
                "50%",
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
        Layout::Split(size) => run(
            socket,
            &[
                "split-window",
                "-h",
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
        log(state,LogLevel::Info,"Neovim: :q closes; unsaved files prompt. Space a returns to chat; Space e browses files; Space f pauses agent follow. Editor and chat stay visible together; narrow terminals stack them. /nv toggle requests a safe close.");
        return Ok(());
    }
    let native = crate::workspace::ghostty::available()
        && std::env::var("OSMOGREP_EDITOR_BACKEND").as_deref() != Ok("tmux");
    for executable in if native {
        &["nvim"][..]
    } else {
        &["tmux", "nvim"][..]
    } {
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
        state.ui.active_edit_target.clone().unwrap_or_else(|| {
            if state.repo_root.join("README.md").is_file() {
                "README.md".into()
            } else {
                ".".into()
            }
        })
    } else {
        entry.to_string()
    };
    let bridge = crate::persistence::config_dir()
        .join("editor")
        .join(format!("{}.json", state.session.id));
    if state.ui.editor_follow.is_none() {
        state.ui.editor_follow = Some(Follower::start(bridge.clone(), state.repo_root.clone())?);
    }
    if native {
        return native::open(state, entry, bridge, raw.trim() == "/nv toggle");
    }
    if env::var_os("TMUX").is_some() {
        let owner = env::var("TMUX_PANE").map_err(|_| "Cannot identify this tmux pane")?;
        if let Some(pane) = owned_editor(&owner) {
            let owner_window = run(
                None,
                &["display-message", "-p", "-t", &owner, "#{window_id}"],
            )?;
            let editor_window = run(
                None,
                &["display-message", "-p", "-t", &pane, "#{window_id}"],
            )?;
            if owner_window != editor_window {
                run(None, &["join-pane", "-h", "-b", "-s", &pane, "-t", &owner])?;
            }
            focus(None, &pane)?;
            if raw.trim() == "/nv toggle" {
                run(
                    None,
                    &["send-keys", "-t", &pane, "Escape", ":confirm qall", "Enter"],
                )?;
            }
        } else {
            open_pane(
                None,
                &owner,
                &state.repo_root,
                &entry,
                columns,
                state.preferences.editor_percent,
                &bridge,
            )?;
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
        "tmux -L {} wait-for agent-ready && OSMOGREP_MANAGED_TMUX=1 OSMOGREP_DISABLE_UPDATE_CHECK=1 OSMOGREP_TMUX_GRAPHICS={} OSMOGREP_EDITOR_BRIDGE={} OSMOGREP_NV_SESSION={} {} --resume {}",
        shell_escape(&session.socket),
        if crate::ui::terminal::kitty_terminal() { "1" } else { "0" },
        shell_escape(&bridge.display().to_string()),
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
        open_pane(
            socket,
            &owner,
            &state.repo_root,
            &entry,
            columns,
            state.preferences.editor_percent,
            &bridge,
        )?;
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
            "Editor and chat stacked · Space a returns to chat"
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
    fn both_views_stay_visible_and_splits_keep_the_agent_readable() {
        for width in [20, 40, 59, 80, 96] {
            assert_eq!(layout(width), Layout::Stacked)
        }
        for width in [100, 120, 144, 160, 240] {
            let Layout::Split(left) = layout(width) else {
                panic!()
            };
            assert!(left >= 28);
            assert!(width - left >= 28)
        }
    }
    #[test]
    fn launch_preserves_user_config_quotes_paths_and_never_syncs_plugins() {
        let directory =
            std::env::temp_dir().join(format!("editor-launch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let bridge = directory.join("bridge.json");
        let command = editor_command(
            Path::new("/tmp/my repo"),
            "file ' name.rs",
            "%2",
            &bridge,
            None,
        )
        .unwrap();
        assert!(command.contains("exec nvim --clean -i NONE"));
        assert!(command.contains("OSMOGREP_AGENT_PANE="));
        assert!(std::fs::read_to_string(bridge.with_extension("lua"))
            .unwrap()
            .contains("space a: chat"));
        assert!(
            command.len() < 4096,
            "tmux commands must not embed the full editor configuration"
        );
        assert!(!command.contains("Lazy"));
        assert!(!command.contains("NvimTree"));
        assert!(!command.contains("headless"));
        assert!(command.contains("'\"'\"'"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}

pub fn open_location(state: &mut AgentState, path: &str, line: usize) -> Result<(), String> {
    let target = crate::file_guard::workspace_path(&state.repo_root, Path::new(path), false)?;
    if target.is_dir() {
        return open_inner(state, &format!("/nv {}", target.display()));
    }
    if !target.is_file() || line == 0 || line > u32::MAX as usize {
        return Err("Select a regular workspace file and a valid line.".into());
    }
    open_inner(state, &format!("/nv {}", target.display()))?;
    if let Some(follower) = &state.ui.editor_follow {
        follower.focus(
            &target.to_string_lossy(),
            Some(line as u64),
            None,
            "inspect",
        );
    }
    Ok(())
}

pub fn apply_layout(state: &mut AgentState, mode: &str) -> Result<(), String> {
    match mode {
        "review" => {
            crate::ui::experience::open(state, crate::ui::experience::Page::Changes);
            return Ok(());
        }
        "preview" => {
            crate::ui::workbench::open(state, 13, None);
            return Ok(());
        }
        "auto" | "chat" | "code" => {}
        _ => return Err("Unknown workspace layout".into()),
    }
    if state.ui.workspace_task.is_some() {
        return Err("Wait for the workspace pane to finish opening.".into());
    }
    let previous = state.preferences.workspace_layout.clone();
    state.preferences.workspace_layout = mode.into();
    let result = (|| {
        if crate::workspace::ghostty::available() {
            if !native::exists(state) && mode != "chat" {
                open_inner(state, "/nv")?;
            } else {
                native::focus_layout(state, mode, &previous)?;
            }
        } else if env::var_os("TMUX").is_some() {
            if mode != "chat" {
                open_inner(state, "/nv")?;
            }
            let owner = env::var("TMUX_PANE").map_err(|_| "Cannot identify the chat pane")?;
            let pane = if mode == "chat" {
                owner.clone()
            } else {
                owned_editor(&owner).ok_or("Editor pane not found")?
            };
            let zoomed = run(
                None,
                &[
                    "display-message",
                    "-p",
                    "-t",
                    &owner,
                    "#{window_zoomed_flag}",
                ],
            )? == "1";
            if zoomed {
                run(None, &["resize-pane", "-Z", "-t", &owner])?;
            }
            focus(None, &pane)?;
            if mode != "auto" {
                run(None, &["resize-pane", "-Z", "-t", &pane])?;
            }
        } else if mode == "chat" { /* Already a full-width chat. */
        } else {
            open_inner(state, "/nv")?;
        }
        crate::persistence::save(state)
    })();
    if result.is_err() {
        state.preferences.workspace_layout = previous;
    }
    result
}
