use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REPO: &str = "kaushal07wick/OsmoGrep";

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub asset_url: String,
    pub asset_name: String,
    pub checksum_url: String,
    pub notes: Vec<String>,
}

#[derive(Debug)]
pub enum UpdateEvent {
    Available(UpdateInfo),
    UpToDate(String),
    CheckFailed(String),
    InstallStarted(UpdateInfo),
    Progress(InstallProgress),
    Installed(UpdateInfo),
    InstallFailed(String),
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct InstallProgress {
    pub stage: &'static str,
    pub received: u64,
    pub total: Option<u64>,
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub(crate) fn homebrew_managed(path: &Path) -> bool {
    let parts = path.components().map(|p| p.as_os_str()).collect::<Vec<_>>();
    parts
        .windows(2)
        .any(|pair| pair[0] == "Cellar" && pair[1] == "osmogrep")
        && path.file_name().is_some_and(|n| n == "osmogrep")
}

pub fn spawn_update_check(tx: Sender<UpdateEvent>) -> bool {
    if env_flag("OSMOGREP_DISABLE_UPDATE_CHECK")
        || env::current_exe().is_ok_and(|p| homebrew_managed(&p))
    {
        return false;
    }
    thread::spawn(move || {
        let mut offered = String::new();
        loop {
            let event = match check_for_update() {
                Ok(Some(info)) if info.latest_version != offered => {
                    offered = info.latest_version.clone();
                    Some(UpdateEvent::Available(info))
                }
                Ok(Some(_)) => None,
                Ok(None) => Some(UpdateEvent::UpToDate(current_version())),
                Err(error) => Some(UpdateEvent::CheckFailed(error)),
            };
            if let Some(event) = event {
                if tx.send(event).is_err() {
                    break;
                }
            }
            thread::sleep(Duration::from_secs(6 * 60 * 60));
        }
    });
    true
}

pub fn spawn_update_install(info: UpdateInfo, tx: Sender<UpdateEvent>) {
    thread::spawn(move || {
        let _ = tx.send(UpdateEvent::InstallStarted(info.clone()));
        let event = match install_update(&info, |progress| {
            let _ = tx.send(UpdateEvent::Progress(progress));
        }) {
            Ok(()) => UpdateEvent::Installed(info),
            Err(error) => UpdateEvent::InstallFailed(error),
        };
        let _ = tx.send(event);
    });
}

pub(crate) fn check_for_update() -> Result<Option<UpdateInfo>, String> {
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
    let release_prefix = format!("https://github.com/{REPO}/releases/download/");
    if Path::new(&asset.name).file_name().and_then(|s| s.to_str()) != Some(asset.name.as_str())
        || asset.name.contains(['/', '\\'])
        || !asset.browser_download_url.starts_with(&release_prefix)
    {
        return Err("invalid release asset".into());
    }
    let checksum_name = format!("{}.sha256", asset.name);
    let checksum_asset = latest
        .assets
        .iter()
        .find(|asset| asset.name == checksum_name)
        .ok_or_else(|| format!("no checksum asset found for {}", asset.name))?;

    if !checksum_asset
        .browser_download_url
        .starts_with(&release_prefix)
    {
        return Err("invalid release checksum URL".into());
    }
    Ok(Some(UpdateInfo {
        current_version,
        latest_version,
        asset_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        checksum_url: checksum_asset.browser_download_url.clone(),
        notes: release_notes(latest.body.as_deref().unwrap_or("")),
    }))
}

pub fn skipped(version: &str) -> bool {
    fs::read_to_string(crate::persistence::config_dir().join("skipped-update"))
        .is_ok_and(|saved| saved.trim() == version)
}
pub fn skip(version: &str) -> Result<(), String> {
    let root = crate::persistence::config_dir();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    crate::persistence::atomic_write(&root.join("skipped-update"), version.as_bytes())
        .map_err(|e| e.to_string())
}
fn release_notes(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with(['#', '<', '`'])
                && !line.starts_with("**Full Changelog")
        })
        .map(|line| {
            line.trim_start_matches(['-', '*', '+', ' '])
                .chars()
                .filter(|c| !c.is_control())
                .take(180)
                .collect::<String>()
        })
        .filter(|line| !line.is_empty())
        .take(5)
        .collect()
}

fn fetch_latest_release() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
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

pub(crate) fn install_update(
    info: &UpdateInfo,
    mut progress: impl FnMut(InstallProgress),
) -> Result<(), String> {
    let current = env::current_exe().map_err(|e| e.to_string())?;
    let install_path = canonical_install_path(&current);
    if homebrew_managed(&install_path) {
        return Err(
            "Homebrew manages this installation. Run: brew upgrade kaushal07wick/tap/osmogrep"
                .into(),
        );
    }
    let tmp_dir = env::temp_dir().join(format!("osmogrep-update-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let result = (|| {
        let archive_path = tmp_dir.join(&info.asset_name);
        let checksum_path = tmp_dir.join(format!("{}.sha256", info.asset_name));
        download(
            &info.checksum_url,
            &checksum_path,
            "checksum",
            &mut progress,
        )?;
        expected_checksum(&checksum_path, &info.asset_name)?;
        download(&info.asset_url, &archive_path, "downloading", &mut progress)?;
        progress(InstallProgress {
            stage: "verifying",
            ..Default::default()
        });
        verify_checksum(&archive_path, &checksum_path, &info.asset_name)?;
        let binary_path = unpack_archive(&archive_path, &tmp_dir)?;
        progress(InstallProgress {
            stage: "installing",
            ..Default::default()
        });
        replace_binary(&binary_path, &install_path)
    })();
    let _ = fs::remove_dir_all(tmp_dir);
    result
}

pub(crate) fn download(
    url: &str,
    path: &Path,
    stage: &'static str,
    progress: &mut impl FnMut(InstallProgress),
) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;
    for attempt in 0..3 {
        progress(InstallProgress {
            stage,
            detail: (attempt > 0).then(|| format!("Connection interrupted · retry {attempt}/2")),
            ..Default::default()
        });
        let result = (|| {
            let response = client
                .get(url)
                .header("User-Agent", "osmogrep-updater")
                .send();
            let mut response = response
                .and_then(|response| response.error_for_status())
                .map_err(|error| {
                    let retry = error.is_connect()
                        || error.is_timeout()
                        || error.is_body()
                        || error.status().is_some_and(|s| {
                            s.is_server_error() || matches!(s.as_u16(), 408 | 429)
                        });
                    let message = error.without_url();
                    let mut message_text = message.to_string();
                    let mut source = std::error::Error::source(&message);
                    while let Some(cause) = source {
                        message_text.push_str(&format!(": {cause}"));
                        source = cause.source();
                    }
                    (message_text, retry)
                })?;
            let total = response.content_length();
            let mut file = fs::File::create(path).map_err(|e| (e.to_string(), false))?;
            transfer(&mut response, &mut file, total, |received| {
                progress(InstallProgress {
                    stage,
                    received,
                    total,
                    detail: None,
                })
            })
            .map_err(|e| {
                (
                    e.to_string(),
                    e.kind() != std::io::ErrorKind::PermissionDenied,
                )
            })
        })();
        match result {
            Ok(()) => return Ok(()),
            Err((error, retry)) if !retry || attempt == 2 => {
                return Err(format!("{stage}: {error}"))
            }
            Err(_) => thread::sleep(Duration::from_millis(500 * (attempt + 1))),
        }
    }
    unreachable!()
}

fn transfer(
    reader: &mut impl Read,
    writer: &mut impl Write,
    total: Option<u64>,
    mut progress: impl FnMut(u64),
) -> std::io::Result<()> {
    let mut buffer = [0; 64 * 1024];
    let mut received = 0;
    let mut last = std::time::Instant::now();
    progress(0);
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        writer.write_all(&buffer[..count])?;
        received += count as u64;
        if last.elapsed() >= Duration::from_millis(100) {
            progress(received);
            last = std::time::Instant::now();
        }
    }
    if total.is_some_and(|expected| expected != received) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "download was incomplete",
        ));
    }
    writer.flush()?;
    progress(received);
    Ok(())
}

pub(crate) fn verify_checksum(
    archive: &Path,
    checksum_file: &Path,
    asset_name: &str,
) -> Result<(), String> {
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

#[cfg(test)]
fn install_verified_archive(
    archive: &Path,
    checksum_file: &Path,
    asset_name: &str,
    install_path: &Path,
    tmp_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(tmp_dir).map_err(|e| e.to_string())?;
    verify_checksum(archive, checksum_file, asset_name)?;
    let binary_path = unpack_archive(archive, tmp_dir)?;
    replace_binary(&binary_path, install_path)
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
    if let Err(error) = fs::rename(&tmp, dst) {
        if backup.exists() {
            fs::rename(&backup, dst).map_err(|restore| format!("Installation failed: {error}; could not restore previous executable: {restore}. Backup: {}", backup.display()))?;
        }
        return Err(error.to_string());
    }
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
    fn download_transfer_reports_bytes_and_rejects_truncation() {
        let input = vec![42; 180_000];
        let mut output = Vec::new();
        let mut counts = Vec::new();
        transfer(
            &mut input.as_slice(),
            &mut output,
            Some(input.len() as u64),
            |n| counts.push(n),
        )
        .unwrap();
        assert_eq!(input, output);
        assert_eq!(counts.first(), Some(&0));
        assert_eq!(counts.last(), Some(&(input.len() as u64)));
        assert!(counts.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(
            transfer(&mut &b"short"[..], &mut Vec::new(), Some(20), |_| {})
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        transfer(&mut &b"unknown length"[..], &mut Vec::new(), None, |_| {}).unwrap();
    }

    #[test]
    fn download_retries_transient_failure_before_reporting_complete() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let handle = thread::spawn(move || {
            for status in ["503 Service Unavailable", "200 OK"] {
                let (mut socket, _) = server.accept().unwrap();
                let mut request = [0; 4096];
                socket.read(&mut request).unwrap();
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"
                )
                .unwrap();
            }
        });
        let path = env::temp_dir().join(format!("osmogrep-download-{}", uuid::Uuid::new_v4()));
        let mut updates = Vec::new();
        download(
            &format!("http://{address}/release"),
            &path,
            "downloading",
            &mut |p| updates.push(p),
        )
        .unwrap();
        handle.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        assert!(updates
            .iter()
            .any(|p| p.detail.as_ref().is_some_and(|d| d.contains("retry 1/2"))));
        assert_eq!(updates.last().unwrap().received, 5);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn release_highlights_are_bounded_and_cannot_inject_terminal_controls() {
        let notes = release_notes(
            "## What's new\n\n- Better sessions\n* Safer updates\n+ Fix \u{1b}[2J reset\n```sh\n\n",
        );
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0], "Better sessions");
        assert!(!notes.join("").contains('\u{1b}'));
        assert_eq!(release_notes(&"x".repeat(500))[0].len(), 180);
        assert!(release_notes("").is_empty());
    }
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

    #[cfg(unix)]
    #[test]
    fn installs_verified_tarball_over_existing_binary() {
        use std::os::unix::fs::PermissionsExt;

        let dir = env::temp_dir().join(format!("osmogrep-install-test-{}", uuid::Uuid::new_v4()));
        let package_dir = dir.join("package");
        let extract_dir = dir.join("extract");
        fs::create_dir_all(&package_dir).unwrap();
        let packaged_binary = package_dir.join("osmogrep");
        fs::write(&packaged_binary, b"new binary").unwrap();
        fs::set_permissions(&packaged_binary, fs::Permissions::from_mode(0o755)).unwrap();

        let archive = dir.join("osmogrep-test.tar.gz");
        let output = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&package_dir)
            .arg("osmogrep")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let checksum = dir.join("osmogrep-test.tar.gz.sha256");
        fs::write(
            &checksum,
            format!("{}  osmogrep-test.tar.gz\n", sha256_file(&archive).unwrap()),
        )
        .unwrap();
        let install_path = dir.join("installed-osmogrep");
        fs::write(&install_path, b"old binary").unwrap();

        install_verified_archive(
            &archive,
            &checksum,
            "osmogrep-test.tar.gz",
            &install_path,
            &extract_dir,
        )
        .unwrap();

        assert_eq!(fs::read(&install_path).unwrap(), b"new binary");
        assert_eq!(
            fs::metadata(&install_path).unwrap().permissions().mode() & 0o111,
            0o111
        );
        let _ = fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod homebrew_tests {
    use super::*;
    #[test]
    fn package_manager_paths_are_never_self_replaced() {
        assert!(homebrew_managed(Path::new(
            "/opt/homebrew/Cellar/osmogrep/0.4.9/bin/osmogrep"
        )));
        assert!(homebrew_managed(Path::new(
            "/home/linuxbrew/.linuxbrew/Cellar/osmogrep/0.4.9/bin/osmogrep"
        )));
        assert!(!homebrew_managed(Path::new(
            "/Users/me/.local/bin/osmogrep"
        )));
        assert!(!homebrew_managed(Path::new(
            "/tmp/Cellar/other/0.4.9/bin/osmogrep"
        )));
    }
}
