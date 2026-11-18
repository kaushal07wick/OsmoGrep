//! Shared atomic writes and locked configuration updates; no UI or agent dependencies.
use std::{fs, io::Write, path::Path};

pub fn lock_file(path: &Path) -> Result<fs::File, String> {
    lock_file_with_timeout(path, std::time::Duration::from_millis(250))
}
fn lock_file_with_timeout(path: &Path, timeout: std::time::Duration) -> Result<fs::File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let lock = options.open(path).map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(error) => return Err(error.to_string()),
        }
        if std::time::Instant::now() >= deadline {
            return Err("Another instance is saving this file. Retry this change.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub fn update_toml(
    path: &Path,
    update: impl FnOnce(&mut toml::Value) -> Result<(), String>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Settings path needs a parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let _lock = lock_file_with_timeout(
        &path.with_extension("lock"),
        std::time::Duration::from_secs(2),
    )?;
    let mut config = match fs::read_to_string(path) {
        Ok(text) => text.parse::<toml::Value>().map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            toml::Value::Table(Default::default())
        }
        Err(e) => return Err(e.to_string()),
    };
    update(&mut config)?;
    atomic_write(
        path,
        toml::to_string(&config)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_settings_updates_preserve_every_field_and_leave_invalid_config_untouched() {
        let root = std::env::temp_dir().join(format!("storage-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");
        let threads: Vec<_> = (0..12)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    update_toml(&path, |config| {
                        config
                            .as_table_mut()
                            .unwrap()
                            .insert(format!("field_{n}"), toml::Value::Integer(n));
                        Ok(())
                    })
                    .unwrap()
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let value: toml::Value = fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(value.as_table().unwrap().len(), 12);
        fs::write(&path, "invalid [toml").unwrap();
        assert!(update_toml(&path, |_| Ok(())).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "invalid [toml");
    }
}
