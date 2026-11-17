//! Optional native viewers. The companions own device capture, input and lifecycle.
use crate::{
    commands::shell_escape,
    logger::log,
    state::{AgentState, LogLevel},
};
use std::path::{Path, PathBuf};

pub mod ghostty;
mod install;

pub enum Event {
    Progress(String),
    Done(Result<String, String>),
}

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
    find_executable(device, root, &std::env::var_os("PATH").unwrap_or_default())
        .ok_or_else(|| format!("{} is optional and is not installed. Open the device in Settings → Workspace to prepare it automatically, or run /workspace install {} to download only the companion. Nothing downloads at app startup.", device.program(), if device == Device::Mac {"mac"} else {"phone"}))?
        .canonicalize().map_err(|e| e.to_string())
}

fn find_executable(device: Device, root: &Path, paths: &std::ffi::OsStr) -> Option<PathBuf> {
    let mut candidates: Vec<_> = std::env::split_paths(paths)
        .map(|dir| dir.join(device.program()))
        .collect();
    // A workspace can be the clones' parent directory or a project nested below it.
    candidates.extend(root.ancestors().map(|parent| {
        parent
            .join(device.program())
            .join("dist")
            .join(device.program())
    }));
    candidates.into_iter().find(|path| executable(path))
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

fn pane_command(program: &Path, name: &str) -> String {
    format!(
        "env -u TMUX -u TMUX_PANE {} up {} --protocol kitty || {{ osmogrep_view_status=$?; printf '\\nWorkspace stopped. Press Return to close this pane. '; read -r osmogrep_view_reply; exit \"$osmogrep_view_status\"; }}",
        shell_escape(&program.display().to_string()),
        shell_escape(name),
    )
}

fn launch(
    device: Device,
    root: &Path,
    target: &str,
    tty: &str,
    sender: Option<&std::sync::mpsc::Sender<Event>>,
) -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err("Mac and iPhone companions require macOS on Apple silicon.".into());
    }
    validate_name(target)?;
    let program = install::ensure(device, root, sender)?;
    launch_program(device, root, target, tty, &program)
}

fn launch_program(
    device: Device,
    root: &Path,
    target: &str,
    tty: &str,
    program: &Path,
) -> Result<String, String> {
    if !ghostty::available() {
        return Err(format!("Automatic companion panes currently use Ghostty 1.3+. Open a native terminal split and run: {}", pane_command(program, target)));
    }
    let viewer = format!(
        "/bin/sh -c {}",
        shell_escape(&pane_command(program, target))
    );
    ghostty::action(root, tty, "GSrt", &viewer, 0)?;
    register_controls(device, root, target, program)?;
    Ok(format!("Preparing {} · {target} beside chat. Missing runtime and device setup run automatically; existing devices are reused. Follow progress in that pane; first-time guest login or permissions may need you. Device control tools are registered; verify readiness before use.",device.program()))
}

fn register_controls(
    device: Device,
    root: &Path,
    target: &str,
    program: &Path,
) -> Result<(), String> {
    register_named_controls(device, root, target, program, device.program())
}
pub(crate) fn register_named_controls(
    device: Device,
    root: &Path,
    target: &str,
    program: &Path,
    name: &str,
) -> Result<(), String> {
    let arguments = if device == Device::Mac {
        vec!["mcp".into(), target.into()]
    } else {
        vec!["mcp".into()]
    };
    crate::mcp::register_local(
        root,
        name,
        program.to_path_buf(),
        arguments,
        if device == Device::Phone {
            Some(target.into())
        } else {
            None
        },
    )
}

pub fn open_for_agent(
    root: &Path,
    device: Device,
    target: &str,
    cancel: &dyn Fn() -> bool,
) -> Result<serde_json::Value, String> {
    validate_name(target)?;
    if cancel() {
        return Err("Device setup cancelled before launch".into());
    }
    if crate::sandbox::mode() != "off" {
        return Err("Device setup needs host access outside the project; it cannot run inside the configured OS sandbox.".into());
    }
    let program = install::ensure(device, root, None)?;
    if cancel() {
        return Err("Device setup cancelled before launch".into());
    }
    let tty = ghostty::available()
        .then(ghostty::caller_tty)
        .and_then(Result::ok);
    let mut result = serde_json::json!({"status":"starting","platform":if device==Device::Mac {"mac"} else {"iphone"},"name":target,"server":device.program(),"viewer":tty.is_some(),"next":"Discover the server with mcp_call tools/list and check device readiness. Runtime downloads and first-time guest account or permission steps may still be in progress; do not claim the app was tested until it actually ran."});
    if let Some(tty) = tty {
        result["message"] =
            serde_json::json!(launch_program(device, root, target, &tty, &program)?);
    } else {
        let command = format!(
            "{} up {} --no-view",
            shell_escape(&program.display().to_string()),
            shell_escape(target)
        );
        let id = crate::command_sessions::start(
            root,
            &command,
            false,
            std::time::Duration::from_secs(86400),
        )?;
        register_controls(device, root, target, &program)?;
        result["session_id"] = serde_json::json!(id);
        result["next"]=serde_json::json!("Poll this session_id with run_shell for setup progress; it runs independently while you code. Then discover this server with mcp_call tools/list and verify readiness before interacting. A first-time guest login or permission step may need the user.");
    }
    Ok(result)
}

pub fn handle(state: &mut AgentState, command: &str) {
    let parts: Vec<_> = command.split_whitespace().collect();
    if let [_, "install", kind] = parts.as_slice() {
        if matches!(*kind, "mac" | "phone") {
            install::start(
                state,
                if *kind == "mac" {
                    Device::Mac
                } else {
                    Device::Phone
                },
            );
            return;
        }
    }
    let (kind, target) = match parts.as_slice() {
        [_, kind] | [_, "setup", kind] if matches!(*kind, "mac" | "phone") => (*kind, "default"),
        [_, kind, name] | [_, "setup", kind, name] if matches!(*kind, "mac" | "phone") => {
            (*kind, *name)
        }
        _ => {
            log(
                state,
                LogLevel::Error,
                "Use /workspace mac|phone [name], /workspace install mac|phone, or /workspace setup mac|phone [name].",
            );
            return;
        }
    };
    let device = if kind == "mac" {
        Device::Mac
    } else {
        Device::Phone
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
        let _ = sender.send(Event::Done(launch(
            device,
            &root,
            &target,
            &tty,
            Some(&sender),
        )));
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
        Ok(Event::Progress(message)) => {
            state.ui.workspace_status = Some(message);
            true
        }
        Ok(Event::Done(result)) => {
            state.ui.workspace_task = None;
            state.ui.workspace_status = None;
            match result {
                Ok(message) => log(state, LogLevel::Info, message),
                Err(error) => log(state, LogLevel::Error, error),
            }
            true
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            state.ui.workspace_task = None;
            state.ui.workspace_status = None;
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
    fn companions_are_found_from_their_parent_projects_and_nested_directories() {
        let root =
            std::env::temp_dir().join(format!("companion-discovery-{}", uuid::Uuid::new_v4()));
        for device in [Device::Mac, Device::Phone] {
            let binary = root
                .join(device.program())
                .join("dist")
                .join(device.program());
            std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
            std::fs::write(&binary, b"fixture").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            for cwd in [
                root.clone(),
                root.join("project"),
                root.join("project/packages/app"),
            ] {
                assert_eq!(
                    find_executable(device, &cwd, std::ffi::OsStr::new("")),
                    Some(binary.clone())
                );
            }
            let paths = std::env::join_paths([binary.parent().unwrap()]).unwrap();
            assert_eq!(
                find_executable(device, Path::new("/unrelated"), &paths),
                Some(binary)
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn names_and_shell_arguments_cannot_turn_into_commands() {
        for name in ["", "-evil", "a;b", "$(touch x)", "a/b", "with space"] {
            assert!(validate_name(name).is_err());
        }
        assert!(validate_name("osmogrep-iphone_1").is_ok());
        let command = pane_command(Path::new("/tmp/my clone/phone's binary"), "work");
        assert!(command.contains("'\"'\"'"));
        assert!(command.contains(" up work --protocol kitty"));
        assert!(!command.contains(" view "));
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
