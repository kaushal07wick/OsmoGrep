//! All reproduction commands run through a configured isolated VM service.
use super::{error, replay::Capsule, Result};
use crate::intelligence::{git, verify::Sandbox};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
pub struct Vm<'a> {
    pub server: &'a str,
    pub deadline: Instant,
    pub cancelled: &'a dyn Fn() -> bool,
}
impl Sandbox for Vm<'_> {
    fn call(&mut self, method: &str, args: Value) -> crate::intelligence::Result<Value> {
        let closing = method == "close";
        let limit = if closing {
            Duration::from_secs(30)
        } else {
            self.deadline.saturating_duration_since(Instant::now())
        };
        let value = crate::mcp::call_tool(self.server, method, &args, limit, &|| {
            !closing && (self.cancelled)()
        })?;
        normalize(value).map_err(Into::into)
    }
}
pub fn normalize(mut value: Value) -> Result<Value> {
    if value.get("exit_code").is_some() && value.get("stdout").is_some() {
        if value["exit_code"] != 0 {
            return Err("VM bridge failed; inspect its diagnostics".into());
        }
        value = serde_json::from_str(
            value["stdout"]
                .as_str()
                .ok_or("VM bridge returned no JSON")?,
        )
        .map_err(error)?;
    }
    if value["isError"] == true || value.get("error").is_some_and(|v| !v.is_null()) {
        return Err(format!(
            "VM service rejected the operation: {}",
            crate::intelligence::cli::redact(&value.to_string())
                .chars()
                .take(1000)
                .collect::<String>()
        ));
    }
    if let Some(data) = value
        .get("structuredContent")
        .or_else(|| value.get("structured_content"))
    {
        return normalize(data.clone());
    }
    if let Some(content) = value.get("content").and_then(Value::as_array) {
        let text = content
            .iter()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return normalize(
            serde_json::from_str(&text).map_err(|_| "VM tool did not return structured JSON")?,
        );
    }
    Ok(value)
}
pub struct Frozen {
    pub path: PathBuf,
    pub base: String,
    pub candidate_tree: String,
}
impl Drop for Frozen {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
#[cfg(test)]
fn command(root: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
    command_cancellable(root, args, input, &|| false)
}
fn command_cancellable(
    root: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.attributesFile=/dev/null",
        "-c",
        "user.name=OsmoGrep snapshot",
        "-c",
        "user.email=snapshot@localhost",
    ])
    .arg("-C")
    .arg(root)
    .args(args)
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    )
    .env_remove("GIT_CONFIG_COUNT")
    .env_remove("GIT_CONFIG_PARAMETERS")
    .env_remove("GIT_OBJECT_DIRECTORY")
    .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .env_remove("GIT_INDEX_FILE");
    let result = match input {
        Some(bytes) => crate::process_runner::run_command_with_stdin_cancellable(
            cmd,
            bytes,
            Duration::from_secs(30),
            cancel,
        ),
        None => {
            crate::process_runner::run_command_cancellable(cmd, Duration::from_secs(30), cancel)
        }
    }?;
    if result.exit_code != 0 || result.timed_out || result.cancelled {
        return Err(format!(
            "Snapshot Git operation failed: {}",
            String::from_utf8_lossy(&result.stderr)
                .chars()
                .take(400)
                .collect::<String>()
        ));
    }
    Ok(result.stdout)
}
impl Frozen {
    pub fn new(root: &Path, capsule: &Capsule, cancel: &dyn Fn() -> bool) -> Result<Self> {
        let command = |root: &Path, args: &[&str], input: Option<&[u8]>| {
            command_cancellable(root, args, input, cancel)
        };
        capsule.validate()?;
        let revision = git::revision(root, &capsule.base).map_err(error)?;
        let entries = git::files(root, &revision).map_err(error)?;
        if entries.len() > 10_000 {
            return Err("Snapshot exceeds 10,000 files".into());
        }
        let sizes = git::run(root, &["ls-tree", "-r", "-l", "-z", &revision]).map_err(error)?;
        let mut total = 0u64;
        for entry in sizes.split(|b| *b == 0).filter(|entry| !entry.is_empty()) {
            let entry = std::str::from_utf8(entry).map_err(error)?;
            let (meta, path) = entry.split_once('\t').ok_or("Invalid Git tree")?;
            super::safe_path(path)?;
            let fields = meta.split_whitespace().collect::<Vec<_>>();
            if !matches!(fields.first().copied(), Some("100644" | "100755")) {
                return Err("Replay snapshots currently require regular tracked files; links and submodules are not copied".into());
            }
            let size: u64 = fields
                .get(3)
                .ok_or("Missing Git blob size")?
                .parse()
                .map_err(error)?;
            total = total.checked_add(size).ok_or("Snapshot size overflow")?;
        }
        if total > 64 * 1024 * 1024 {
            return Err(
                "Snapshot exceeds 64 MiB; prepare a smaller reproduction repository".into(),
            );
        }
        let parent = crate::file_guard::internal_dir(root, ".context/osmogrep-lab/checkouts")?;
        let path = parent.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&path).map_err(error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(error)?;
        }
        let mut frozen = Self {
            path,
            base: String::new(),
            candidate_tree: String::new(),
        };
        let archive = frozen.path.join("source.tar");
        let canonical_root = root.canonicalize().map_err(error)?;
        let relative_archive = archive.strip_prefix(&canonical_root).map_err(error)?;
        git::run(
            root,
            &[
                "archive",
                "--format=tar",
                "--output",
                relative_archive.to_str().ok_or("Non-UTF8 snapshot path")?,
                &revision,
            ],
        )
        .map_err(error)?;
        if fs::metadata(&archive).map_err(error)?.len() > total + 32 * 1024 * 1024 {
            return Err("Unexpected source archive size".into());
        }
        let mut tar = Command::new("tar");
        tar.current_dir(&frozen.path).args(["-xf", "source.tar"]);
        let extracted =
            crate::process_runner::run_command_cancellable(tar, Duration::from_secs(30), cancel)?;
        if extracted.exit_code != 0 || extracted.timed_out || extracted.cancelled {
            return Err("Could not extract the generated source archive".into());
        }
        fs::remove_file(archive).map_err(error)?;
        let format = git::text(root, &["rev-parse", "--show-object-format"]).map_err(error)?;
        command(
            &frozen.path,
            &["init", "--template=", &format!("--object-format={format}")],
            None,
        )?;
        // The original tracked tree may contain ignored files or text normalization rules.
        fs::create_dir_all(frozen.path.join(".git/info")).map_err(error)?;
        fs::write(
            frozen.path.join(".git/info/attributes"),
            b"* -text -filter -ident -working-tree-encoding\n",
        )
        .map_err(error)?;
        let names = entries
            .iter()
            .map(|entry| format!("./{}\n", entry.path))
            .collect::<String>();
        let hashes = command(
            &frozen.path,
            &["hash-object", "--no-filters", "--stdin-paths"],
            Some(names.as_bytes()),
        )?;
        let actual = String::from_utf8(hashes).map_err(error)?;
        if actual.lines().collect::<Vec<_>>()
            != entries
                .iter()
                .map(|entry| entry.blob.as_str())
                .collect::<Vec<_>>()
        {
            return Err("Archive changed or omitted tracked files (for example export attributes); replay refused".into());
        }
        command(&frozen.path, &["add", "--force", "--all"], None)?;
        command(
            &frozen.path,
            &["commit", "--quiet", "--allow-empty", "-m", "Captured base"],
            None,
        )?;
        frozen.base = git::revision(&frozen.path, "HEAD").map_err(error)?;
        for (path, source) in &capsule.files {
            let path = crate::file_guard::workspace_path(&frozen.path, Path::new(path), true)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(error)?;
            }
            crate::storage::atomic_write(&path, source.content.as_bytes()).map_err(error)?;
        }
        fs::write(frozen.path.join(format!("osmogrep_replay_snapshot_{}.json",uuid::Uuid::new_v4())),json!({"original_base":capsule.base,"capsule_sha256":crate::file_guard::digest(&serde_json::to_vec(capsule).map_err(error)?)}).to_string()).map_err(error)?;
        command(&frozen.path, &["add", "--force", "--all"], None)?;
        frozen.candidate_tree = String::from_utf8(command(&frozen.path, &["write-tree"], None)?)
            .map_err(error)?
            .trim()
            .into();
        Ok(frozen)
    }
}
pub fn replay(
    root: &Path,
    capsule: &Capsule,
    sandbox: &mut dyn Sandbox,
    cancel: &dyn Fn() -> bool,
) -> Result<Value> {
    if cancel() {
        return Err("Replay cancelled before preparation".into());
    }
    let frozen = Frozen::new(root, capsule, cancel)?;
    if cancel() {
        return Err("Replay cancelled before VM allocation".into());
    }
    let session = sandbox
        .call("start", json!({"repo":frozen.path,"base":frozen.base}))
        .map_err(error)?["session"]
        .as_str()
        .ok_or("VM service returned no session")?
        .to_owned();
    let outcome = (|| -> Result<Value> {
        if cancel() {
            return Err("Replay cancelled".into());
        }
        let context = sandbox
            .call("context", json!({"session":session}))
            .map_err(error)?;
        sandbox.call("reproducer",json!({"session":session,"path":capsule.experiment.path,"content":capsule.experiment.source})).map_err(error)?;
        let command = if capsule.environment.is_empty() {
            capsule.experiment.command.clone()
        } else {
            let mut vars = vec![];
            for (key, value) in &capsule.environment {
                if key.is_empty()
                    || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    || key.as_bytes()[0].is_ascii_digit()
                {
                    return Err("Invalid environment variable name".into());
                }
                vars.push(format!("{key}={}", crate::commands::shell_escape(value)));
            }
            format!("env {} {}", vars.join(" "), capsule.experiment.command)
        };
        let comparison = sandbox
            .call("compare", json!({"session":session,"command":command}))
            .map_err(error)?;
        if cancel() {
            return Err("Replay cancelled".into());
        }
        let original_suites = sandbox
            .call("check", json!({"session":session}))
            .map_err(error)?;
        let export = sandbox
            .call("export", json!({"session":session}))
            .map_err(error)?;
        let integrity = original_suites["integrity_preserved"] == true
            && comparison["environment_equivalent"] == true
            && comparison["oracle_validated"] == true
            && comparison["reproducer_identical"] == true
            && comparison["command"] == command
            && comparison["reproducer_sha256"]
                == crate::file_guard::digest(capsule.experiment.source.as_bytes())
            && comparison["oracle_sources"] == json!(capsule.experiment.oracle_sources)
            && comparison["base"]["revision"] == frozen.base
            && comparison["candidate"]["tree"] == frozen.candidate_tree
            && comparison["environment_fingerprint"]
                .as_str()
                .is_some_and(|s| !s.is_empty());
        Ok(
            json!({"session":session,"expected_candidate_tree":frozen.candidate_tree,"comparison":comparison,"original_suites":original_suites,"export":export,"context":context,"integrity_attested":integrity,"scope":"Captured base plus explicitly selected sources; no host reproduction command was executed","original_base":capsule.base,"experiment_sha256":crate::file_guard::digest(capsule.experiment.source.as_bytes()),"environment":capsule.environment}),
        )
    })();
    let cleanup = sandbox.call("close", json!({"session":session}));
    match outcome {
        Ok(mut value) => {
            value["cleanup_error"] = cleanup
                .err()
                .map(|e| json!(e.to_string()))
                .unwrap_or(Value::Null);
            Ok(value)
        }
        Err(error) => Err(match cleanup {
            Ok(_) => error,
            Err(cleanup) => format!("{error}; VM {session} cleanup also failed: {cleanup}"),
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_and_bridge_envelopes_preserve_failures() {
        assert_eq!(
            normalize(json!({"content":[{"type":"text","text":"{\"session\":\"x\"}"}]})).unwrap()
                ["session"],
            "x"
        );
        assert_eq!(
            normalize(json!({"exit_code":0,"stdout":"{\"session\":\"x\"}"})).unwrap()["session"],
            "x"
        );
        assert!(normalize(json!({"content":[{"type":"text","text":"not JSON"}]})).is_err());
        assert!(normalize(json!({"isError":true,"structuredContent":{"ok":true}})).is_err());
    }
}

#[cfg(test)]
pub(crate) mod replay_tests {
    use super::*;
    use crate::{intelligence::verify::Experiment, lab::replay::Source};
    use std::collections::BTreeMap;
    pub(crate) fn fixture() -> (PathBuf, Capsule) {
        let root = std::env::temp_dir().join(format!("lab-frozen-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        command(&root, &["init", "--template="], None).unwrap();
        fs::write(root.join("app.py"), "VALUE = 1\n").unwrap();
        fs::write(root.join("CONTRACT.md"), "The value is one.\n").unwrap();
        fs::write(root.join(".gitignore"), ".context/\n").unwrap();
        command(&root, &["add", "--all"], None).unwrap();
        command(&root, &["commit", "--quiet", "-m", "base"], None).unwrap();
        let content = "VALUE = 2\n".to_string();
        let capsule = Capsule {
            schema_version: 1,
            base: git::revision(&root, "HEAD").unwrap(),
            files: BTreeMap::from([(
                "app.py".into(),
                Source {
                    sha256: crate::file_guard::digest(content.as_bytes()),
                    content,
                    expected_before: Some(crate::file_guard::digest(b"VALUE = 1\n")),
                },
            )]),
            experiment: Experiment {
                path: "osmogrep_repro_value.py".into(),
                source: "from app import VALUE\nassert VALUE == 1\n".into(),
                command: "python3 osmogrep_repro_value.py".into(),
                oracle: "The pre-existing value contract".into(),
                oracle_sources: vec!["CONTRACT.md:1".into()],
            },
            environment: BTreeMap::new(),
            excluded_changes: vec![],
        };
        (root, capsule)
    }
    #[test]
    fn snapshots_retain_tracked_files_even_when_gitignore_matches_them() {
        let (root, mut capsule) = fixture();
        fs::write(root.join(".gitignore"), ".context/\n*.py\n").unwrap();
        command(&root, &["add", ".gitignore"], None).unwrap();
        command(
            &root,
            &["commit", "--quiet", "-m", "Ignore new Python files"],
            None,
        )
        .unwrap();
        capsule.base = git::revision(&root, "HEAD").unwrap();
        {
            let frozen = Frozen::new(&root, &capsule, &|| false).unwrap();
            assert_eq!(
                git::text(&frozen.path, &["show", "HEAD:app.py"]).unwrap(),
                "VALUE = 1"
            );
            assert_eq!(
                git::text(&frozen.path, &["show", ":app.py"]).unwrap(),
                "VALUE = 2"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn snapshots_preserve_the_host_and_cleanup_even_when_vm_comparison_fails() {
        struct Fake {
            calls: Vec<String>,
            snapshot: Option<PathBuf>,
        }
        impl Sandbox for Fake {
            fn call(&mut self, method: &str, args: Value) -> crate::intelligence::Result<Value> {
                self.calls.push(method.into());
                if method == "start" {
                    let path = PathBuf::from(args["repo"].as_str().unwrap());
                    assert_eq!(
                        fs::read_to_string(path.join("app.py")).unwrap(),
                        "VALUE = 2\n"
                    );
                    self.snapshot = Some(path);
                    return Ok(json!({"session":"isolated-test"}));
                }
                if method == "compare" {
                    return Err("controlled comparison failure".into());
                }
                Ok(json!({}))
            }
        }
        let (root, capsule) = fixture();
        let mut fake = Fake {
            calls: vec![],
            snapshot: None,
        };
        assert!(replay(&root, &capsule, &mut fake, &|| false)
            .unwrap_err()
            .contains("controlled comparison failure"));
        assert_eq!(fake.calls.last().unwrap(), "close");
        assert!(!fake.snapshot.unwrap().exists());
        assert_eq!(
            fs::read_to_string(root.join("app.py")).unwrap(),
            "VALUE = 1\n"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
