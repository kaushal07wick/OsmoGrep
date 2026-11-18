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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}
pub struct Follower {
    selection: mpsc::Receiver<Result<Selection, String>>,
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
        let (selection_tx, selection) = mpsc::sync_channel(8);
        std::thread::spawn(move || loop {
            match receiver.recv_timeout(std::time::Duration::from_millis(150)) {
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => {}
            }
            let selected = path.with_extension("selection.json");
            if let Ok(meta) = std::fs::symlink_metadata(&selected) {
                let result = (|| {
                    if !meta.file_type().is_file() || meta.len() > 2 * 1024 * 1024 {
                        return Err("Invalid editor selection file".into());
                    }
                    let data = std::fs::read(&selected).map_err(|e| e.to_string())?;
                    let selection: Selection =
                        serde_json::from_slice(&data).map_err(|e| e.to_string())?;
                    crate::file_guard::workspace_path(
                        &root,
                        std::path::Path::new(&selection.path),
                        false,
                    )?;
                    if selection.start == 0
                        || selection.end < selection.start
                        || selection.text.is_empty()
                        || selection.text.len() > 256 * 1024
                    {
                        return Err(
                            "Select between 1 byte and 256 KiB from a workspace file".into()
                        );
                    }
                    Ok(selection)
                })();
                if selection_tx.try_send(result).is_ok() {
                    let _ = std::fs::remove_file(&selected);
                }
            }
            let Some(mut event) = pending.lock().ok().and_then(|mut value| value.take()) else {
                continue;
            };
            let Ok(target) =
                crate::file_guard::workspace_path(&root, std::path::Path::new(&event.path), false)
            else {
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
        });
        Ok(Self {
            selection,
            latest,
            wake,
            error,
        })
    }

    pub fn take_selection(&self) -> Option<Result<Selection, String>> {
        self.selection.try_recv().ok()
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

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn explicit_selections_are_delivered_once_and_outside_paths_are_rejected() {
        let root = std::env::temp_dir().join(format!("selection-{}", uuid::Uuid::new_v4()));
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("a.rs"), "saved").unwrap();
        let bridge = root.join("private/focus.json");
        let follower = Follower::start(bridge.clone(), project).unwrap();
        let selected = bridge.with_extension("selection.json");
        std::fs::write(
            &selected,
            br#"{"path":"a.rs","start":2,"end":3,"text":"unsaved selected text"}"#,
        )
        .unwrap();
        let wait = || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if let Some(selection) = follower.take_selection() {
                    return selection;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        assert_eq!(wait().unwrap().text, "unsaved selected text");
        assert!(!selected.exists());
        assert!(follower.take_selection().is_none());
        std::fs::write(
            &selected,
            br#"{"path":"../outside","start":1,"end":1,"text":"outside"}"#,
        )
        .unwrap();
        assert!(wait().is_err());
        drop(follower);
        std::fs::remove_dir_all(root).unwrap();
    }
}
