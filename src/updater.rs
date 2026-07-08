use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const REPO: &str = "kaushal07wick/OsmoGrep";

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub asset_url: String,
    pub asset_name: String,
    pub checksum_url: String,
}

#[derive(Debug)]
pub enum UpdateEvent {
    Available(UpdateInfo),
    UpToDate(String),
    CheckFailed(String),
    InstallStarted(UpdateInfo),
    Installed(UpdateInfo),
    InstallFailed(String),
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub fn spawn_update_check(tx: Sender<UpdateEvent>) {
    if env_flag("OSMOGREP_DISABLE_UPDATE_CHECK") {
        return;
    }
    thread::spawn(move || {
        let event = match check_for_update() {
            Ok(Some(info)) => UpdateEvent::Available(info),
            Ok(None) => UpdateEvent::UpToDate(current_version()),
            Err(error) => UpdateEvent::CheckFailed(error),
        };
        let _ = tx.send(event);
    });
}

pub fn spawn_update_install(info: UpdateInfo, tx: Sender<UpdateEvent>) {
    thread::spawn(move || {
        let _ = tx.send(UpdateEvent::InstallStarted(info.clone()));
        let event = match install_update(&info) {
            Ok(()) => UpdateEvent::Installed(info),
            Err(error) => UpdateEvent::InstallFailed(error),
        };
        let _ = tx.send(event);
    });
}

fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let latest = fetch_latest_release()?;
    let latest_version = normalize_version(&latest.tag_name);
    let current_version = current_version();
    if !is_newer_version(&latest_version, &current_version) {
        return Ok(None);
    }
    let target = current_target()?;
    let asset = latest
        .assets
        .iter()
        .find(|asset| asset.name.contains(&target) && !asset.name.ends_with(".sha256"))
        .ok_or_else(|| format!("no release asset found for target {target}"))?;
    let checksum_name = format!("{}.sha256", asset.name);
    let checksum_asset = latest
        .assets
        .iter()
        .find(|asset| asset.name == checksum_name)
        .ok_or_else(|| format!("no checksum asset found for {}", asset.name))?;

    Ok(Some(UpdateInfo {
        current_version,
        latest_version,
        asset_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        checksum_url: checksum_asset.browser_download_url.clone(),
    }))
}

fn fetch_latest_release() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .header("User-Agent", "osmogrep-updater")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Release>()
        .map_err(|e| e.to_string())
}

fn install_update(info: &UpdateInfo) -> Result<(), String> {
    let current = env::current_exe().map_err(|e| e.to_string())?;
    let install_path = canonical_install_path(&current);
    let tmp_dir = env::temp_dir().join(format!("osmogrep-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let archive_path = tmp_dir.join(&info.asset_name);
    download(&info.asset_url, &archive_path)?;
    let checksum_path = tmp_dir.join(format!("{}.sha256", info.asset_name));
    download(&info.checksum_url, &checksum_path)?;
    verify_checksum(&archive_path, &checksum_path, &info.asset_name)?;
    let binary_path = unpack_archive(&archive_path, &tmp_dir)?;
    replace_binary(&binary_path, &install_path)?;
    let _ = fs::remove_dir_all(tmp_dir);
    Ok(())
}

fn download(url: &str, path: &Path) -> Result<(), String> {
    let bytes = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .header("User-Agent", "osmogrep-updater")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}

fn verify_checksum(archive: &Path, checksum_file: &Path, asset_name: &str) -> Result<(), String> {
    let expected = expected_checksum(checksum_file, asset_name)?;
    let actual = sha256_file(archive)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch for {asset_name}: expected {expected}, got {actual}"
        ))
    }
}

fn expected_checksum(checksum_file: &Path, asset_name: &str) -> Result<String, String> {
    let text = fs::read_to_string(checksum_file).map_err(|e| e.to_string())?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        if !is_sha256_hex(hash) {
            continue;
        }
        let filename = parts.next().unwrap_or(asset_name).trim_start_matches('*');
        if filename == asset_name {
            return Ok(hash.to_ascii_lowercase());
        }
        if line.split_whitespace().count() == 1 {
            return Ok(hash.to_ascii_lowercase());
        }
    }
    Err(format!(
        "checksum file did not contain a sha256 for {asset_name}"
    ))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(bytes);
    Ok(hex::encode(digest))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn unpack_archive(archive: &Path, tmp_dir: &Path) -> Result<PathBuf, String> {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if name.ends_with(".tar.gz") {
        let out = Command::new("tar")
            .arg("-xzf")
            .arg(archive)
            .arg("-C")
            .arg(tmp_dir)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "tar failed: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        find_binary(tmp_dir, "osmogrep")
    } else if name.ends_with(".zip") {
        let out = Command::new("unzip")
            .arg("-o")
            .arg(archive)
            .arg("-d")
            .arg(tmp_dir)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "unzip failed: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        find_binary(tmp_dir, "osmogrep.exe")
    } else {
        Err(format!("unsupported update archive: {}", archive.display()))
    }
}

fn replace_binary(src: &Path, dst: &Path) -> Result<(), String> {
    let backup = dst.with_extension("old");
    let tmp = dst.with_extension("new");
    fs::copy(src, &tmp).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    if dst.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(dst, &backup).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, dst).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(backup);
    Ok(())
}

fn find_binary(root: &Path, name: &str) -> Result<PathBuf, String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = fs::read_dir(&path).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|name| name.to_str()) == Some(name) {
                return Ok(path);
            }
        }
    }
    Err(format!("archive did not contain {name}"))
}

fn canonical_install_path(current: &Path) -> PathBuf {
    fs::canonicalize(current).unwrap_or_else(|_| current.to_path_buf())
}

fn current_target() -> Result<String, String> {
    let os = match env::consts::OS {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        "windows" => "pc-windows-msvc",
        other => return Err(format!("unsupported update OS: {other}")),
    };
    let arch = match env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => return Err(format!("unsupported update architecture: {other}")),
    };
    Ok(format!("{arch}-{os}"))
}

fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

fn parse_version(version: &str) -> Vec<u64> {
    version
        .split(|ch| ch == '.' || ch == '-')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_like_versions() {
        assert!(is_newer_version("0.3.3", "0.3.2"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(!is_newer_version("0.3.2", "0.3.2"));
        assert!(!is_newer_version("0.3.1", "0.3.2"));
    }

    #[test]
    fn normalizes_release_tag_versions() {
        assert_eq!(normalize_version("v0.4.0"), "0.4.0");
        assert_eq!(normalize_version("0.4.0"), "0.4.0");
    }

    #[test]
    fn verifies_matching_sha256_sidecar() {
        let dir = env::temp_dir().join(format!("osmogrep-checksum-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("osmogrep-test.tar.gz");
        let checksum = dir.join("osmogrep-test.tar.gz.sha256");
        fs::write(&archive, b"hello").unwrap();
        fs::write(
            &checksum,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  osmogrep-test.tar.gz\n",
        )
        .unwrap();

        verify_checksum(&archive, &checksum, "osmogrep-test.tar.gz").unwrap();

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_mismatched_sha256_sidecar() {
        let dir = env::temp_dir().join(format!("osmogrep-checksum-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("osmogrep-test.tar.gz");
        let checksum = dir.join("osmogrep-test.tar.gz.sha256");
        fs::write(&archive, b"hello").unwrap();
        fs::write(
            &checksum,
            "0000000000000000000000000000000000000000000000000000000000000000  osmogrep-test.tar.gz\n",
        )
        .unwrap();

        let error = verify_checksum(&archive, &checksum, "osmogrep-test.tar.gz").unwrap_err();

        assert!(error.contains("checksum mismatch"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parses_bare_sha256_sidecar() {
        let dir = env::temp_dir().join(format!("osmogrep-checksum-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let checksum = dir.join("osmogrep-test.tar.gz.sha256");
        fs::write(
            &checksum,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\n",
        )
        .unwrap();

        let expected = expected_checksum(&checksum, "osmogrep-test.tar.gz").unwrap();

        assert_eq!(
            expected,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        let _ = fs::remove_dir_all(dir);
    }
}
