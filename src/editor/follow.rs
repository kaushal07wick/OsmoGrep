//! A coalescing bridge keeps editor I/O off the terminal input thread.
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
};

#[derive(Serialize)]
struct Focus {
    path: String,
    line: Option<u64>,
    end_line: Option<u64>,
    phase: String,
    sequence: String,
}

pub struct Follower {
    latest: Arc<Mutex<Option<Focus>>>,
    wake: SyncSender<()>,
    error: Arc<Mutex<Option<String>>>,
}

impl Follower {
    pub fn start(path: PathBuf, root: PathBuf) -> Result<Self, String> {
        let parent = path
            .parent()
            .ok_or("Editor bridge needs a parent directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let latest = Arc::new(Mutex::new(None::<Focus>));
        let pending = latest.clone();
        let error = Arc::new(Mutex::new(None));
        let write_error = error.clone();
        let (wake, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while receiver.recv().is_ok() {
                let Some(mut event) = pending.lock().ok().and_then(|mut value| value.take()) else {
                    continue;
                };
                let Ok(target) = crate::file_guard::workspace_path(
                    &root,
                    std::path::Path::new(&event.path),
                    false,
                ) else {
                    continue;
                };
                if target.is_dir() {
                    continue;
                }
                event.path = target.to_string_lossy().into_owned();
                if let Ok(bytes) = serde_json::to_vec(&event) {
                    if let Err(error) = crate::persistence::atomic_write(&path, &bytes) {
                        if let Ok(mut current) = write_error.lock() {
                            *current = Some(error.to_string());
                        }
                    }
                }
            }
        });
        Ok(Self {
            latest,
            wake,
            error,
        })
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|mut error| error.take())
    }

    pub fn focus(&self, path: &str, line: Option<u64>, end_line: Option<u64>, phase: &str) {
        if let Ok(mut latest) = self.latest.lock() {
            *latest = Some(Focus {
                path: path.into(),
                line,
                end_line,
                phase: phase.into(),
                sequence: uuid::Uuid::new_v4().to_string(),
            });
            let _ = self.wake.try_send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coalesces_to_latest_focus_and_keeps_bridge_outside_project() {
        let dir = std::env::temp_dir().join(format!("editor-follow-{}", uuid::Uuid::new_v4()));
        let root = dir.join("project");
        std::fs::create_dir_all(&root).unwrap();
        let bridge = dir.join("private/focus.json");
        let follower = Follower::start(bridge.clone(), root.clone()).unwrap();
        for line in 1..=1000 {
            follower.focus("new.rs", Some(line), None, "editing");
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let value = std::fs::read(&bridge)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
            if value.as_ref().is_some_and(|v| v["line"] == 1000) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!root.join("focus.json").exists());
        drop(follower);
        // The worker exits after its sender; leave directory cleanup to the OS test temp area.
    }
}
