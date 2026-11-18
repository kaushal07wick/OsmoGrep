//! Account login persists locally by user choice; provider secrets keep their OS store.
use std::{fs, io::Read, path::Path};

fn directory() -> std::path::PathBuf {
    crate::persistence::config_dir().join("account")
}
fn private(path: &Path, directory: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(
            "Account storage must be a regular private file or directory, not a link".into(),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(format!("Account storage is not private: {}. Restrict it to your OS user before signing in.", path.display()));
        }
    }
    Ok(())
}
fn prepare(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Account storage needs a parent")?)
        .map_err(|e| e.to_string())?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.to_string()),
    }
    private(path, true)?;
    Ok(())
}
fn read(path: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => (),
    }
    private(path.parent().ok_or("Account storage needs a parent")?, true)?;
    private(path, false)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 {
        return Err("Account credential exceeds 64 KiB".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "Invalid account credential encoding".into())
}
pub(super) fn exists() -> bool {
    std::fs::symlink_metadata(directory().join("login.json"))
        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}
pub(super) fn get() -> Result<Option<String>, String> {
    read(&directory().join("login.json"))
}
pub(super) fn set(value: &str) -> Result<(), String> {
    if value.len() > 64 * 1024 {
        return Err("Account credential exceeds 64 KiB".into());
    }
    let directory = directory();
    prepare(&directory)?;
    write(&directory.join("login.json"), value)
}
fn write(path: &Path, value: &str) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok() {
        private(path, false)?;
    }
    crate::storage::atomic_write(path, value.as_bytes()).map_err(|e| e.to_string())
}
pub(super) fn delete() -> Result<(), String> {
    let directory = directory();
    if directory.exists() {
        private(&directory, true)?;
    }
    match fs::remove_file(directory.join("login.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_account_file_round_trips_and_rejects_unsafe_storage() {
        let root = std::env::temp_dir().join(format!("account-store-{}", uuid::Uuid::new_v4()));
        let directory = root.join("account");
        prepare(&directory).unwrap();
        let path = directory.join("login.json");
        assert!(read(&path).unwrap().is_none());
        write(&path, "synthetic-token").unwrap();
        assert_eq!(read(&path).unwrap().as_deref(), Some("synthetic-token"));
        write(&path, "rotated-token").unwrap();
        assert_eq!(read(&path).unwrap().as_deref(), Some("rotated-token"));
        fs::write(&path, vec![b'x'; 64 * 1024 + 1]).unwrap();
        assert!(read(&path).unwrap_err().contains("64 KiB"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read(&path).unwrap_err().contains("not private"));
            assert!(write(&path, "new-secret").is_err());
            fs::remove_file(&path).unwrap();
            let target = root.join("untouched");
            fs::write(&target, "unchanged").unwrap();
            symlink(&target, &path).unwrap();
            assert!(write(&path, "new-secret").is_err());
            assert!(read(&path).is_err());
            assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
