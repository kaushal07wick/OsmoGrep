//! Cancellable local file/symbol indexing and saved-session search.
use super::*;
use std::path::{Path, PathBuf};
pub(super) fn index_files(root: PathBuf) -> Job {
    let (sender, receiver) = mpsc::sync_channel(8);
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    std::thread::spawn(move || {
        let mut command = std::process::Command::new("git");
        command.arg("-C").arg(&root).args([
            "-c",
            "core.fsmonitor=false",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ]);
        let paths = crate::process_runner::run_command(command, std::time::Duration::from_secs(5))
            .ok()
            .filter(|r| r.exit_code == 0 && !r.timed_out)
            .map(|r| {
                r.stdout
                    .split(|b| *b == 0)
                    .filter_map(|p| {
                        std::str::from_utf8(p)
                            .ok()
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                walkdir::WalkDir::new(&root)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(|e| {
                        !e.file_type().is_dir()
                            || !matches!(
                                e.file_name().to_str(),
                                Some(
                                    ".git"
                                        | "node_modules"
                                        | "target"
                                        | ".next"
                                        | ".venv"
                                        | "vendor"
                                )
                            )
                    })
                    .filter_map(Result::ok)
                    .filter(|e| e.file_type().is_file())
                    .filter_map(|e| {
                        e.path()
                            .strip_prefix(&root)
                            .ok()
                            .map(|p| p.to_string_lossy().into_owned())
                    })
                    .collect()
            });
        let mut folders = std::collections::BTreeSet::new();
        for path in &paths {
            let mut parent = Path::new(path).parent();
            while let Some(p) = parent {
                if p.as_os_str().is_empty() {
                    break;
                }
                folders.insert(format!("{}/", p.display()));
                parent = p.parent();
            }
        }
        for group in folders.into_iter().collect::<Vec<_>>().chunks(128) {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let _ = sender.send(IndexEvent::Files(
                group
                    .iter()
                    .map(|path| Source {
                        path: path.clone(),
                        line: None,
                        end: None,
                        symbol: None,
                    })
                    .collect(),
            ));
        }
        let mut chunk = Vec::new();
        for path in &paths {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if path.chars().any(char::is_control) {
                continue;
            }
            if crate::file_guard::workspace_path(&root, Path::new(path), false).is_err() {
                continue;
            }
            chunk.push(Source {
                path: path.clone(),
                line: None,
                end: None,
                symbol: None,
            });
            if chunk.len() >= 128
                && sender
                    .send(IndexEvent::Files(std::mem::take(&mut chunk)))
                    .is_err()
            {
                return;
            }
        }
        if !chunk.is_empty() {
            let _ = sender.send(IndexEvent::Files(std::mem::take(&mut chunk)));
        }
        for path in paths {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if !matches!(
                Path::new(&path).extension().and_then(|s| s.to_str()),
                Some("rs" | "py")
            ) {
                continue;
            }
            let Ok(text) = super::references::read_file(&root, &path) else {
                continue;
            };
            for symbol in crate::context::indexer::extract_symbols(&text, &path) {
                chunk.push(Source {
                    path: path.clone(),
                    line: Some(symbol.line_start),
                    end: Some(symbol.line_end),
                    symbol: Some(symbol.name),
                });
            }
            if chunk.len() >= 128
                && sender
                    .send(IndexEvent::Files(std::mem::take(&mut chunk)))
                    .is_err()
            {
                return;
            }
        }
        if !chunk.is_empty() {
            let _ = sender.send(IndexEvent::Files(chunk));
        }
        let _ = sender.send(IndexEvent::Done);
    });
    Job { receiver, cancel }
}
pub(super) fn search_saved(root: PathBuf, query: String) -> Job {
    let (sender, receiver) = mpsc::sync_channel(8);
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    std::thread::spawn(move || {
        let sessions = match crate::persistence::list_sessions() {
            Ok(s) => s,
            Err(e) => {
                let _ = sender.send(IndexEvent::Error(e));
                return;
            }
        };
        for session in sessions
            .into_iter()
            .filter(|s| Path::new(&s.repo_root) == root)
        {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let logs = match crate::persistence::session_logs(&root, &session.id) {
                Ok(logs) => logs,
                Err(e) => {
                    let _ = sender.send(IndexEvent::Error(format!("Saved search: {e}")));
                    return;
                }
            };
            for (index, (_, text)) in logs.into_iter().enumerate() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                if matches(&query, &text) {
                    let title = format!(
                        "{} · {}",
                        session.name.as_deref().unwrap_or(&session.title),
                        text.lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(100)
                            .collect::<String>()
                    );
                    if sender
                        .send(IndexEvent::Saved {
                            session: session.id.clone(),
                            index,
                            title,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
        let _ = sender.send(IndexEvent::Done);
    });
    Job { receiver, cancel }
}
