use std::{
    path::{Path, PathBuf},
    process::Command,
};

use super::Result;

pub fn run(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    // These are Git plumbing operations only; never use a shell or repository hooks.
    let mut command = Command::new("git");
    command
        .args([
            "--no-optional-locks",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");
    let output = crate::process_runner::run_command(command, std::time::Duration::from_secs(30))?;
    if output.exit_code != 0 || output.timed_out || output.cancelled {
        return Err(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(2000)
                .collect::<String>()
        )
        .into());
    }
    Ok(output.stdout)
}

pub fn text(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(run(root, args)?)?.trim().to_owned())
}

pub fn root(path: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(text(path, &["rev-parse", "--show-toplevel"])?).canonicalize()?)
}

pub fn revision(root: &Path, reference: &str) -> Result<String> {
    if reference.starts_with('-') || reference.len() > 256 || reference.contains(['\n', '\0']) {
        return Err("invalid Git revision".into());
    }
    text(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub blob: String,
}

pub fn files(root: &Path, revision: &str) -> Result<Vec<Entry>> {
    let raw = run(root, &["ls-tree", "-r", "-z", "--full-tree", revision])?;
    let mut entries = Vec::new();
    for record in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = std::str::from_utf8(record)?;
        let (meta, path) = record.split_once('\t').ok_or("invalid Git tree entry")?;
        let mut parts = meta.split_whitespace();
        let mode = parts.next();
        let kind = parts.next();
        let blob = parts.next().ok_or("missing blob")?;
        // Symlinks and submodules are metadata, never followed or executed.
        if kind == Some("blob") && matches!(mode, Some("100644" | "100755")) {
            entries.push(Entry {
                path: path.into(),
                blob: blob.into(),
            });
        }
        if entries.len() > 100_000 {
            return Err("index file budget exceeded".into());
        }
    }
    Ok(entries)
}

pub fn blob(root: &Path, oid: &str) -> Result<Option<String>> {
    let size: usize = text(root, &["cat-file", "-s", oid])?.parse()?;
    if size > 1024 * 1024 {
        return Ok(None);
    }
    Ok(String::from_utf8(run(root, &["cat-file", "blob", oid])?).ok())
}

pub fn changed(root: &Path, base: &str, candidate: &str) -> Result<Vec<String>> {
    Ok(run(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-ext-diff",
            "--no-renames",
            base,
            candidate,
            "--",
        ],
    )?
    .split(|b| *b == 0)
    .filter(|p| !p.is_empty())
    .map(|p| String::from_utf8(p.to_vec()))
    .collect::<std::result::Result<_, _>>()?)
}

pub fn source(root: &Path, revision: &str, path: &str) -> Result<Option<String>> {
    let spec = format!("{revision}:{path}");
    if run(root, &["cat-file", "-e", &spec]).is_err() {
        return Ok(None);
    }
    blob(root, &spec)
}

pub fn common_base(root: &Path, left: &str, right: &str) -> Result<String> {
    let base = text(root, &["merge-base", "--all", left, right])?;
    if base.lines().count() != 1 {
        return Err("multiple merge bases require an explicit integration base".into());
    }
    Ok(base)
}

pub fn combined_tree(root: &Path, left: &str, right: &str) -> Result<String> {
    let keys = text(root, &["config", "--name-only", "--list"])?;
    if keys
        .lines()
        .any(|key| key.starts_with("merge.") && key.ends_with(".driver"))
    {
        return Err("custom Git merge drivers are not executed during analysis".into());
    }
    // merge-tree writes Git objects, never checks out code or changes the index/worktree.
    let output = text(root, &["merge-tree", "--write-tree", left, right])?;
    let tree = output.lines().next().ok_or("merge-tree returned no tree")?;
    if !tree.bytes().all(|b| b.is_ascii_hexdigit()) || !matches!(tree.len(), 40 | 64) {
        return Err("merge-tree returned an invalid tree".into());
    }
    Ok(tree.into())
}
