//! Explicit, checksummed companion downloads; device runtimes are never installed here.
use super::{Device, Event};
use crate::{
    logger::log,
    state::{AgentState, LogLevel},
    updater::InstallProgress,
};
use std::{
    path::{Component, Path, PathBuf},
    process::Command,
    sync::mpsc::Sender,
};

struct Package {
    repo: &'static str,
    tag: &'static str,
    asset: &'static str,
    root: &'static str,
    binary: &'static str,
}

fn package(device: Device) -> Package {
    match device {
        Device::Mac => Package {
            repo: "govijr/terminal-mac",
            tag: "v0.2.0-6",
            asset: "terminal-mac-0.2.0-6-macos-arm64.zip",
            root: "terminal-mac-0.2.0-6-macos-arm64",
            binary: "TerminalMac.app/Contents/MacOS/terminal-mac",
        },
        Device::Phone => Package {
            repo: "govijr/terminal-phone",
            tag: "v0.4.0",
            asset: "terminal-phone-0.4.0-macos-arm64.tar.gz",
            root: "terminal-phone",
            binary: "terminal-phone",
        },
    }
}

fn run(command: &mut Command) -> Result<String, String> {
    command.stdin(std::process::Stdio::null());
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1000)
            .collect());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn validate_entries(names: &str, details: &str, root: &str) -> Result<(), String> {
    if names.lines().count() == 0 || names.lines().count() > 20_000 {
        return Err("Companion archive has an invalid number of entries".into());
    }
    for name in names.lines() {
        let path = Path::new(name);
        if name.contains('\\')
            || name.chars().any(char::is_control)
            || !path
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
            || !path.starts_with(root)
        {
            return Err("Companion archive contains an unsafe path".into());
        }
    }
    // These upstream packages contain only regular files and directories, never archive links.
    if details
        .lines()
        .any(|line| !line.starts_with(['-', 'd']) || line.contains(" link to "))
    {
        return Err("Companion archive contains an unsupported entry or link".into());
    }
    Ok(())
}

fn unpack(package: &Package, archive: &Path, directory: &Path) -> Result<PathBuf, String> {
    let names = run(Command::new("/usr/bin/tar").arg("-tf").arg(archive))?;
    let details = run(Command::new("/usr/bin/tar").arg("-tvf").arg(archive))?;
    validate_entries(&names, &details, package.root)?;
    run(Command::new("/usr/bin/tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(directory))?;
    let payload = directory.join(package.root);
    let binary = payload.join(package.binary);
    if !super::executable(&binary) {
        return Err("Companion package is missing its executable".into());
    }
    Ok(binary)
}

fn download(device: Device, directory: &Path, sender: &Sender<Event>) -> Result<PathBuf, String> {
    let package = package(device);
    let archive = directory.join(package.asset);
    let checksum = directory.join(format!("{}.sha256", package.asset));
    let prefix = format!(
        "https://github.com/{}/releases/download/{}",
        package.repo, package.tag
    );
    let mut progress = |p: InstallProgress| {
        let label = match p.stage {
            "downloading" => "Downloading",
            "verifying" => "Verifying",
            "installing" => "Installing",
            _ => "Fetching checksum for",
        };
        let bytes = if p.stage == "downloading" {
            match p.total.filter(|n| *n > 0) {
                Some(total) => format!(
                    " · {}% · {:.1}/{:.1} MB",
                    p.received.saturating_mul(100) / total,
                    p.received as f64 / 1e6,
                    total as f64 / 1e6
                ),
                None => format!(" · {:.1} MB", p.received as f64 / 1e6),
            }
        } else {
            String::new()
        };
        let _ = sender.send(Event::Progress(format!(
            "↓ {label} {}{bytes}",
            device.program()
        )));
    };
    crate::updater::download(
        &format!("{prefix}/{}.sha256", package.asset),
        &checksum,
        "checksum",
        &mut progress,
    )?;
    crate::updater::download(
        &format!("{prefix}/{}", package.asset),
        &archive,
        "downloading",
        &mut progress,
    )?;
    let binary = install_archive(device, directory, &archive, &checksum, &mut progress)?;
    let _ = std::fs::remove_file(archive);
    let _ = std::fs::remove_file(checksum);
    Ok(binary)
}

fn install_archive(
    device: Device,
    directory: &Path,
    archive: &Path,
    checksum: &Path,
    progress: &mut impl FnMut(InstallProgress),
) -> Result<PathBuf, String> {
    let package = package(device);
    progress(InstallProgress {
        stage: "verifying",
        ..Default::default()
    });
    crate::updater::verify_checksum(archive, checksum, package.asset)?;
    let binary = unpack(&package, archive, directory)?;
    let signed = if device == Device::Mac {
        directory.join(package.root).join("TerminalMac.app")
    } else {
        binary.clone()
    };
    let mut signature = Command::new("/usr/bin/codesign");
    signature.args(["--verify", "--strict"]);
    if device == Device::Mac {
        signature.arg("--deep");
    } else {
        // The bare CLI has no app resource envelope; the archive checksum covers its adjacent workers.
        signature.arg("--ignore-resources");
    }
    run(signature.arg(&signed))?;
    if device == Device::Mac {
        let entitlements = run(Command::new("/usr/bin/codesign")
            .args(["-d", "--entitlements", "-"])
            .arg(&signed))?;
        if !has_virtualization_entitlement(&entitlements) {
            return Err("Mac companion is missing its virtualization entitlement".into());
        }
    }
    let license = match device {
        Device::Mac => include_str!("../../third_party/terminal-mac/LICENSE"),
        Device::Phone => include_str!("../../third_party/terminal-phone/LICENSE"),
    };
    crate::storage::atomic_write(
        &directory
            .join(package.root)
            .join("LICENSE.osmogrep-companion"),
        license.as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    progress(InstallProgress {
        stage: "installing",
        ..Default::default()
    });
    let binary = binary.canonicalize().map_err(|e| e.to_string())?;
    crate::persistence::update_config(|config| {
        let workspace = config
            .as_table_mut()
            .ok_or("Configuration must be a table")?
            .entry("workspace")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .ok_or("Workspace settings must be a table")?;
        workspace.insert(
            device.setting().into(),
            toml::Value::String(binary.to_string_lossy().into_owned()),
        );
        Ok(())
    })?;
    Ok(binary)
}

fn has_virtualization_entitlement(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<String>();
    compact.contains("<key>com.apple.security.virtualization</key><true/>")
        || compact.contains("[Key]com.apple.security.virtualization[Value][Bool]true")
}

pub fn start(state: &mut AgentState, device: Device) {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        log(
            state,
            LogLevel::Error,
            "Mac and iPhone companions require macOS on Apple silicon.",
        );
        return;
    }
    if state.ui.workspace_task.is_some() {
        log(
            state,
            LogLevel::Info,
            "A workspace action is already running.",
        );
        return;
    }
    if let Some(path) = std::env::var_os(device.environment()) {
        if !super::executable(Path::new(&path)) {
            log(
                state,
                LogLevel::Error,
                format!(
                    "{} points to a missing executable. Correct or unset it before installing.",
                    device.environment()
                ),
            );
            return;
        }
    }
    if let Ok(path) = super::resolve(device, &state.repo_root) {
        log(
            state,
            LogLevel::Info,
            format!(
                "{} is ready at {}. Choose Open or Setup in Settings → Workspace.",
                device.program(),
                path.display()
            ),
        );
        return;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    state.ui.workspace_task = Some(receiver);
    state.ui.workspace_status = Some(format!("Preparing optional {} download…", device.program()));
    std::thread::spawn(move || {
        let directory = crate::persistence::config_dir()
            .join("companions")
            .join(format!("{}-{}", device.program(), uuid::Uuid::new_v4()));
        let result = (|| {
            let version = run(Command::new("/usr/bin/sw_vers").arg("-productVersion"))?;
            let major = version
                .split('.')
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or("Cannot determine macOS version")?;
            let required = if device == Device::Mac { 14 } else { 15 };
            if major < required {
                return Err(format!(
                    "{} requires macOS {required} or newer.",
                    device.program()
                ));
            }
            std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                    .map_err(|e| e.to_string())?;
            }
            download(device, &directory, &sender)?;
            Ok(format!("✓ {} installed. Open it from Settings → Workspace. Device setup is separate; no macOS or iOS runtime was downloaded.",device.program()))
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(directory);
        }
        let _ = sender.send(Event::Done(result));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "requires downloaded official packages and an isolated HOME"]
    fn published_packages_preserve_helpers_and_save_paths() {
        let _cleanup = crate::mcp::Cleanup;
        let fixture = std::env::var_os("OSMOGREP_INSTALL_TEST_ROOT")
            .map(PathBuf::from)
            .expect("Set an isolated test root");
        let archives = std::env::var_os("OSMOGREP_COMPANION_ARCHIVES")
            .map(PathBuf::from)
            .expect("Set the downloaded archive directory");
        let config = crate::persistence::config_dir();
        assert!(
            config.starts_with(&fixture),
            "Never overwrite personal configuration in an install test"
        );
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("config.toml"), "[custom]\nuntouched = 'yes'\n").unwrap();
        for (device, archive, checksum) in [
            (Device::Mac, "mac.zip", "mac.sha256"),
            (Device::Phone, "phone.tar.gz", "phone.sha256"),
        ] {
            let destination = fixture.join(device.program());
            std::fs::create_dir_all(&destination).unwrap();
            let mut stages = Vec::new();
            let binary = install_archive(
                device,
                &destination,
                &archives.join(archive),
                &archives.join(checksum),
                &mut |p| stages.push(p.stage),
            )
            .unwrap();
            assert_eq!(super::super::resolve(device, &fixture).unwrap(), binary);
            assert_eq!(stages, vec!["verifying", "installing"]);
            let payload = destination.join(package(device).root);
            assert!(payload.join("LICENSE.osmogrep-companion").is_file());
            match device {
                Device::Mac=>assert!(payload.join("TerminalMac.app/Contents/Resources/guest-tools/TerminalMacGuest.app/Contents/MacOS/terminal-mac").is_file()),
                Device::Phone=>assert!(payload.join("resources/phone-agent-worker.py").is_file()),
            }
            let saved: toml::Value = std::fs::read_to_string(config.join("config.toml"))
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(saved["custom"]["untouched"].as_str(), Some("yes"));
            crate::tools::in_root(&fixture, || {
                crate::mcp::register_local(
                    &fixture,
                    device.program(),
                    binary.clone(),
                    if device == Device::Mac {
                        vec!["mcp".into(), "default".into()]
                    } else {
                        vec!["mcp".into()]
                    },
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
                let expected = if device == Device::Mac {
                    "screenshot"
                } else {
                    "phone_observe"
                };
                assert!(
                    tools
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|tool| tool["name"] == expected),
                    "{tools}"
                );
            });
            println!(
                "{}: checksum, archive safety, signature, resources, license and config passed",
                device.program()
            );
        }
    }
    #[test]
    fn virtualization_entitlements_accept_xml_and_modern_der_display_but_never_false() {
        assert!(has_virtualization_entitlement(
            "<key>com.apple.security.virtualization</key>\n<true/>"
        ));
        assert!(has_virtualization_entitlement(
            "[Dict]\n\t[Key] com.apple.security.virtualization\n\t[Value]\n\t\t[Bool] true\n"
        ));
        assert!(!has_virtualization_entitlement("[Key] com.apple.security.virtualization [Value] [Bool] false [Key] unrelated [Value] [Bool] true"));
        assert!(!has_virtualization_entitlement(
            "<key>com.apple.security.virtualization</key><false/><key>unrelated</key><true/>"
        ));
    }
    #[test]
    fn archives_reject_traversal_absolute_paths_and_links() {
        assert!(validate_entries(
            "terminal-phone/\nterminal-phone/terminal-phone\n",
            "drwx root\n-rwx file\n",
            "terminal-phone"
        )
        .is_ok());
        for path in [
            "/terminal-phone/file",
            "terminal-phone/../escape",
            "terminal-phone-evil/file",
            "terminal-phone/dir\\file",
        ] {
            assert!(
                validate_entries(path, "-rwx file", "terminal-phone").is_err(),
                "{path}"
            );
        }
        assert!(
            validate_entries("terminal-phone/link", "lrwx link -> /tmp", "terminal-phone").is_err()
        );
        assert!(validate_entries(
            "terminal-phone/link",
            "-rwx link link to /tmp",
            "terminal-phone"
        )
        .is_err());
    }
}
