//! Optional native viewers. The companions own device capture, input and lifecycle.
use crate::{
    commands::shell_escape,
    logger::log,
    state::{AgentState, LogLevel},
};
use std::path::{Path, PathBuf};

pub mod ghostty;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Device {
    Mac,
    Phone,
}
impl Device {
    pub fn program(self) -> &'static str {
        match self {
            Self::Mac => "terminal-mac",
            Self::Phone => "terminal-phone",
        }
    }
    fn setting(self) -> &'static str {
        match self {
            Self::Mac => "terminal_mac",
            Self::Phone => "terminal_phone",
        }
    }
    fn environment(self) -> &'static str {
        match self {
            Self::Mac => "OSMOGREP_TERMINAL_MAC",
            Self::Phone => "OSMOGREP_TERMINAL_PHONE",
        }
    }
}

fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn resolve(device: Device, root: &Path) -> Result<PathBuf, String> {
    let configured = std::env::var_os(device.environment())
        .map(PathBuf::from)
        .or_else(|| {
            let config =
                std::fs::read_to_string(crate::persistence::config_dir().join("config.toml"))
                    .ok()?;
            let config: toml::Value = config.parse().ok()?;
            config
                .get("workspace")?
                .get(device.setting())?
                .as_str()
                .map(PathBuf::from)
        });
    if let Some(path) = configured {
        return if executable(&path) {
            path.canonicalize().map_err(|e| e.to_string())
        } else {
            Err(format!(
                "Configured {} is not an executable: {}",
                device.program(),
                path.display()
            ))
        };
    }
    let paths = std::env::var_os("PATH").unwrap_or_default();
    let mut candidates: Vec<_> = std::env::split_paths(&paths)
        .map(|dir| dir.join(device.program()))
        .collect();
    // Developer checkouts may keep the companion clones beside their projects.
    if let Some(parent) = root.parent() {
        candidates.push(
            parent
                .join(device.program())
                .join("dist")
                .join(device.program()),
        );
    }
    candidates.into_iter().find(|path| executable(path))
        .ok_or_else(|| format!("{} is not built or on PATH. Build its clone, or set [workspace].{} to its executable.", device.program(), device.setting()))?
        .canonicalize().map_err(|e| e.to_string())
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("Use a device name containing 1–64 letters, digits, '-' or '_'; it cannot start with '-'.".into());
    }
    Ok(())
}

fn pane_command(program: &Path, name: &str, setup: bool) -> String {
    let verb = if setup { "up" } else { "view" };
    format!(
        "env -u TMUX -u TMUX_PANE {} {verb} {}{} || {{ osmogrep_view_status=$?; printf '\\nViewer exited. Press Return to close this pane. '; read -r osmogrep_view_reply; exit \"$osmogrep_view_status\"; }}",
        shell_escape(&program.display().to_string()),
        shell_escape(name),
        if setup { "" } else { " --protocol kitty" }
    )
}

fn launch(
    device: Device,
    root: &Path,
    target: &str,
    setup: bool,
    tty: &str,
) -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err("Mac and iPhone companions require macOS on Apple silicon.".into());
    }
    validate_name(target)?;
    let program = resolve(device, root)?;
    if !ghostty::available() {
        return Err(format!("Automatic companion panes currently use Ghostty 1.3+. Open a native terminal split and run: {}", pane_command(&program, target, setup)));
    }
    let viewer = format!(
        "/bin/sh -c {}",
        shell_escape(&pane_command(&program, target, setup))
    );
    ghostty::action(root, tty, "GSrt", &viewer, 0)?;
    let arguments = if device == Device::Mac {
        vec!["mcp".into(), target.into()]
    } else {
        vec!["mcp".into()]
    };
    crate::mcp::register_local(
        root,
        device.program(),
        program,
        arguments,
        if device == Device::Phone {
            Some(target.into())
        } else {
            None
        },
    )?;
    Ok(format!(
        "{} · {target}: native viewer pane opened; agent controls available through MCP.{}",
        device.program(),
        if setup {
            " Complete first-run setup in that pane; it may download the required OS runtime."
        } else {
            " The viewer reports if this target still needs setup."
        }
    ))
}

pub fn handle(state: &mut AgentState, command: &str) {
    let parts: Vec<_> = command.split_whitespace().collect();
    let (device, target, setup) = match parts.as_slice() {
        [_, kind] | [_, kind, "default"] if matches!(*kind, "mac" | "phone") => (
            if *kind == "mac" {
                Device::Mac
            } else {
                Device::Phone
            },
            "default",
            false,
        ),
        [_, kind, name] if matches!(*kind, "mac" | "phone") => (
            if *kind == "mac" {
                Device::Mac
            } else {
                Device::Phone
            },
            *name,
            false,
        ),
        [_, "setup", kind] | [_, "setup", kind, "default"] if matches!(*kind, "mac" | "phone") => (
            if *kind == "mac" {
                Device::Mac
            } else {
                Device::Phone
            },
            "default",
            true,
        ),
        [_, "setup", kind, name] if matches!(*kind, "mac" | "phone") => (
            if *kind == "mac" {
                Device::Mac
            } else {
                Device::Phone
            },
            *name,
            true,
        ),
        _ => {
            log(
                state,
                LogLevel::Error,
                "Use /workspace mac|phone [name], or /workspace setup mac|phone [name].",
            );
            return;
        }
    };
    if state.ui.workspace_task.is_some() {
        log(
            state,
            LogLevel::Info,
            "A workspace pane is already opening.",
        );
        return;
    }
    if let Err(error) = validate_name(target) {
        log(state, LogLevel::Error, error);
        return;
    }
    let tty = match ghostty::caller_tty() {
        Ok(tty) => tty,
        Err(error) => {
            log(state, LogLevel::Error, error);
            return;
        }
    };
    let root = state.repo_root.clone();
    let target = target.to_string();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(launch(device, &root, &target, setup, &tty));
    });
    state.ui.workspace_task = Some(receiver);
    log(
        state,
        LogLevel::Info,
        format!("Opening {} workspace…", device.program()),
    );
}

pub fn poll(state: &mut AgentState) -> bool {
    let Some(task) = &state.ui.workspace_task else {
        return false;
    };
    match task.try_recv() {
        Ok(result) => {
            state.ui.workspace_task = None;
            match result {
                Ok(message) => log(state, LogLevel::Info, message),
                Err(error) => log(state, LogLevel::Error, error),
            }
            true
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            state.ui.workspace_task = None;
            log(
                state,
                LogLevel::Error,
                "The workspace launcher stopped before reporting a result.",
            );
            true
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_shell_arguments_cannot_turn_into_commands() {
        for name in ["", "-evil", "a;b", "$(touch x)", "a/b", "with space"] {
            assert!(validate_name(name).is_err());
        }
        assert!(validate_name("osmogrep-iphone_1").is_ok());
        let command = pane_command(Path::new("/tmp/my clone/phone's binary"), "work", false);
        assert!(command.contains("'\"'\"'"));
        assert!(command.contains(" view work --protocol kitty"));
        assert!(!pane_command(Path::new("/tmp/phone"), "work", true).contains("--protocol"));
    }

    #[test]
    #[ignore = "requires the separately built macOS companion applications"]
    fn built_companions_expose_their_real_mcp_controls() {
        let root = std::env::current_dir().unwrap();
        let _cleanup = crate::mcp::Cleanup;
        crate::tools::in_root(&root, || {
            for (device, expected) in [
                (Device::Mac, "screenshot"),
                (Device::Phone, "phone_observe"),
            ] {
                let arguments = if device == Device::Mac {
                    vec!["mcp".into(), "default".into()]
                } else {
                    vec!["mcp".into()]
                };
                crate::mcp::register_local(
                    &root,
                    device.program(),
                    resolve(device, &root).unwrap(),
                    arguments,
                    (device == Device::Phone).then(|| "default".into()),
                )
                .unwrap();
                let tools = crate::mcp::call_cancellable(
                    Some(device.program()),
                    "tools/list",
                    &serde_json::json!({}),
                    &|| false,
                )
                .unwrap();
                let tools = tools.as_array().unwrap();
                let names: Vec<_> = tools
                    .iter()
                    .filter_map(|tool| tool["name"].as_str())
                    .collect();
                assert!(names.contains(&expected), "{names:?}");
                println!("{}: {}", device.program(), names.join(", "));
            }
        });
    }
}
