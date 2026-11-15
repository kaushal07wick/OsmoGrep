//! Optional OS enforcement. Missing platform support is an error, never a silent fallback.
use std::{path::Path, process::Command};

pub fn mode() -> String {
    settings().0
}
fn settings() -> (String, bool) {
    let config = std::fs::read_to_string(crate::persistence::config_dir().join("config.toml"))
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok());
    let sandbox = config.as_ref().and_then(|c| c.get("sandbox"));
    (
        std::env::var("OSMOGREP_SANDBOX").ok().unwrap_or_else(|| {
            sandbox
                .and_then(|s| s.get("mode"))
                .and_then(toml::Value::as_str)
                .unwrap_or("off")
                .to_string()
        }),
        sandbox
            .and_then(|s| s.get("network"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
    )
}
pub fn wrap(command: Command) -> Result<Command, String> {
    let (mode, network) = settings();
    wrap_with(command, &mode, network)
}
fn wrap_with(command: Command, mode: &str, network: bool) -> Result<Command, String> {
    if mode == "off" {
        return Ok(command);
    }
    if !matches!(mode, "workspace" | "read-only") {
        return Err("Sandbox mode must be off, workspace, or read-only".into());
    }
    let root = command
        .get_current_dir()
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(crate::tools::repo_root)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let mut wrapped: Command;
    #[cfg(target_os = "macos")]
    {
        let quoted = serde_json::to_string(&root.to_string_lossy()).map_err(|e| e.to_string())?;
        let temporary = serde_json::to_string(
            &std::env::temp_dir()
                .canonicalize()
                .map_err(|e| e.to_string())?
                .to_string_lossy(),
        )
        .map_err(|e| e.to_string())?;
        let mut profile=format!("(version 1)(allow default)(deny file-write*)(allow file-write* (subpath {temporary}) (literal \"/dev/null\") (literal \"/dev/tty\"))");
        if mode == "workspace" {
            profile.push_str(&format!("(allow file-write* (subpath {quoted}))"));
        }
        if mode == "read-only" {
            profile.push_str(&format!("(deny file-write* (subpath {quoted}))"));
        }
        if !network {
            profile.push_str("(deny network*)");
        }
        wrapped = Command::new("/usr/bin/sandbox-exec");
        wrapped.arg("-p").arg(profile);
    }
    #[cfg(target_os = "linux")]
    {
        wrapped = Command::new("bwrap");
        wrapped.args([
            "--die-with-parent",
            "--new-session",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--tmpfs",
            "/tmp",
        ]);
        wrapped
            .arg(if mode == "workspace" {
                "--bind"
            } else {
                "--ro-bind"
            })
            .arg(&root)
            .arg(&root);
        if !network {
            wrapped.arg("--unshare-net");
        }
        wrapped.arg("--");
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (root, network);
        return Err("OS sandbox enforcement is currently supported on macOS (sandbox-exec) and Linux (bubblewrap). Keep sandbox off or run OsmoGrep inside an isolated environment on this platform.".into());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        wrapped
            .arg(command.get_program())
            .args(command.get_args())
            .current_dir(root);
        for (key, value) in command.get_envs() {
            match value {
                Some(value) => {
                    wrapped.env(key, value);
                }
                None => {
                    wrapped.env_remove(key);
                }
            }
        }
        Ok(wrapped)
    }
}

pub fn set(mode: &str) -> Result<(), String> {
    if !matches!(mode, "off" | "workspace" | "read-only") {
        return Err("Use /sandbox off|workspace|read-only".into());
    }
    if mode != "off" {
        #[cfg(target_os = "macos")]
        if !Path::new("/usr/bin/sandbox-exec").is_file() {
            return Err("macOS sandbox-exec is unavailable".into());
        }
        #[cfg(target_os = "linux")]
        if !std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join("bwrap").is_file())
        }) {
            return Err("Install bubblewrap before enabling the Linux OS sandbox".into());
        }
        #[cfg(not(any(target_os="macos",target_os="linux")))] return Err("Native OS sandboxing is not yet supported on this platform; use an isolated container/VM".into());
    }
    let path = crate::persistence::config_dir().join("config.toml");
    let mut config = match std::fs::read_to_string(&path) {
        Ok(s) => s.parse::<toml::Value>().map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            toml::Value::Table(Default::default())
        }
        Err(e) => return Err(e.to_string()),
    };
    if config.get("sandbox").is_none() {
        config["sandbox"] = toml::Value::Table(Default::default());
    }
    config["sandbox"]["mode"] = toml::Value::String(mode.into());
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    crate::persistence::atomic_write(
        &path,
        toml::to_string(&config)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires native sandbox-exec outside an enclosing sandbox"]
    fn native_sandbox_enforces_writes_and_network() {
        let dir = std::env::temp_dir().join(format!("osmogrep-sandbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |mode: &str, script: &str| {
            let mut command = Command::new("sh");
            command.args(["-c", script]).current_dir(&dir);
            wrap_with(command, mode, false).unwrap().output().unwrap()
        };
        let writable = run("workspace", "printf ok > inside");
        assert!(
            writable.status.success(),
            "{}",
            String::from_utf8_lossy(&writable.stderr)
        );
        assert!(!run("read-only", "printf changed > inside").status.success());
        assert_eq!(std::fs::read_to_string(dir.join("inside")).unwrap(), "ok");
        let outside = std::path::PathBuf::from("/private/tmp")
            .join(format!("osmogrep-denied-{}", uuid::Uuid::new_v4()));
        let denied = run(
            "workspace",
            &format!("printf bad > '{}'", outside.display()),
        );
        let leaked = outside.exists();
        if leaked {
            std::fs::remove_file(&outside).unwrap();
        }
        assert!(!denied.status.success() && !leaked);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(!run(
            "workspace",
            &format!(
                "/usr/bin/curl --noproxy '*' --max-time 1 http://{}/",
                listener.local_addr().unwrap()
            )
        )
        .status
        .success());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
