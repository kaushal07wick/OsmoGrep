//! Provider secrets use OS protection; account login uses an explicitly chosen private file.
mod account_file;
#[cfg(not(target_os = "macos"))]
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn service() -> String {
    format!(
        "com.osmogrep.credentials.{}",
        crate::file_guard::digest(
            crate::persistence::config_dir()
                .to_string_lossy()
                .as_bytes()
        )
    )
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    #[ignore = "writes and removes a unique synthetic Keychain test entry"]
    fn native_secure_store_round_trip_and_delete() {
        let account = format!("osmogrep-test-{}", uuid::Uuid::new_v4());
        assert!(super::get(&account).unwrap().is_none());
        super::set(&account, "synthetic-fixture-only").unwrap();
        let result = super::get(&account);
        super::delete(&account).unwrap();
        assert_eq!(result.unwrap().as_deref(), Some("synthetic-fixture-only"));
        assert!(super::get(&account).unwrap().is_none());
    }
}
pub fn set(provider: &str, key: &str) -> Result<(), String> {
    if provider == "oauth/osmogrep" {
        return account_file::set(key);
    }
    if key.len() > 64 * 1024 {
        return Err("Credential exceeds 64 KiB".into());
    }
    #[cfg(target_os = "macos")]
    {
        return security_framework::passwords::set_generic_password(
            &service(),
            provider,
            key.as_bytes(),
        )
        .map_err(|error| {
            format!("Could not save to macOS Keychain (OSStatus {}). Unlock your keychain or use an environment variable.", error.code())
        });
    }
    #[cfg(target_os = "linux")]
    {
        let mut command = Command::new("secret-tool");
        command.args([
            "store",
            "--label=OsmoGrep",
            "service",
            &service(),
            "provider",
            provider,
        ]);
        run(command, key.as_bytes())?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let protected=powershell("$s=ConvertTo-SecureString ([Console]::In.ReadToEnd()) -AsPlainText -Force; ConvertFrom-SecureString $s",key)?;
        let path = windows_path(provider);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        return crate::persistence::atomic_write(&path, protected.as_bytes())
            .map_err(|e| e.to_string());
    }
    #[allow(unreachable_code)]
    Err(
        "No secure credential backend for this platform; use the provider environment variable"
            .into(),
    )
}
pub fn get(provider: &str) -> Result<Option<String>, String> {
    if provider == "oauth/osmogrep" {
        return account_file::get();
    }
    #[cfg(target_os = "macos")]
    {
        return match security_framework::passwords::get_generic_password(&service(), provider) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| "Invalid keychain credential encoding".into()),
            Err(e) if e.code() == -25300 => Ok(None),
            Err(_) => Err(
                "Could not read macOS Keychain; unlock it or use the provider environment variable"
                    .into(),
            ),
        };
    }
    #[cfg(target_os = "linux")]
    {
        let mut command = Command::new("secret-tool");
        command.args([
            "lookup",
            "service",
            service().as_str(),
            "provider",
            provider,
        ]);
        return match run(command, b"") {
            Ok(text) => Ok(Some(text.trim_end_matches('\n').into())),
            Err(_) => Ok(None),
        };
    }
    #[cfg(target_os = "windows")]
    {
        let value = match std::fs::read_to_string(windows_path(provider)) {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        return powershell("$s=ConvertTo-SecureString ([Console]::In.ReadToEnd()); $c=New-Object System.Management.Automation.PSCredential('osmogrep',$s); [Console]::Write($c.GetNetworkCredential().Password)",&value).map(Some);
    }
    #[allow(unreachable_code)]
    Ok(None)
}
pub fn delete(provider: &str) -> Result<(), String> {
    if provider == "oauth/osmogrep" {
        return account_file::delete();
    }
    #[cfg(target_os = "macos")]
    {
        return match security_framework::passwords::delete_generic_password(&service(), provider) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()),
            Err(_) => Err("Could not remove keychain credential".into()),
        };
    }
    #[cfg(target_os = "linux")]
    {
        let mut c = Command::new("secret-tool");
        c.args(["clear", "service", service().as_str(), "provider", provider]);
        run(c, b"")?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let p = windows_path(provider);
        if p.exists() {
            std::fs::remove_file(p).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn run(mut command: Command, input: &[u8]) -> Result<String, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child=command.spawn().map_err(|_|"Secure credential backend unavailable; Linux requires secret-tool/libsecret and an unlocked Secret Service, or use an environment variable")?;
    child
        .stdin
        .take()
        .ok_or("Credential input unavailable")?
        .write_all(input)
        .map_err(|e| e.to_string())?;
    let result = child.wait_with_output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err("Secure credential operation failed; unlock the OS credential store or use an environment variable".into());
    }
    String::from_utf8(result.stdout).map_err(|_| "Invalid credential encoding".into())
}
#[cfg(target_os = "windows")]
fn windows_path(provider: &str) -> std::path::PathBuf {
    crate::persistence::config_dir()
        .join("credentials")
        .join(format!(
            "{}.dpapi",
            crate::file_guard::digest(provider.as_bytes())
        ))
}
#[cfg(target_os = "windows")]
fn powershell(script: &str, input: &str) -> Result<String, String> {
    let mut c = Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    run(c, input.as_bytes())
}
