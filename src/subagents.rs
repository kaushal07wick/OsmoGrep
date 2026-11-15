//! Explicit worker processes share the public RPC contract, never the parent's mutable runtime.
use crate::tools::{Tool, ToolResult, ToolSafety};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::Duration,
};

struct Child {
    parent: PathBuf,
    root: PathBuf,
    process: String,
    cursor: u64,
    partial: String,
    events: Vec<Value>,
    status: String,
    baseline: BTreeMap<String, Option<Vec<u8>>>,
}
static CHILDREN: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
fn children() -> &'static Mutex<HashMap<String, Child>> {
    CHILDREN.get_or_init(Default::default)
}

pub fn overview(parent: &Path) -> Result<Vec<Value>, String> {
    let mut all = children()
        .lock()
        .map_err(|_| "Subagent registry unavailable")?;
    let mut rows = Vec::new();
    for (id, child) in all.iter_mut().filter(|(_, s)| s.parent == parent) {
        if let Err(error) = poll(child, &|| false) {
            child.status = format!("unavailable: {error}");
        }
        rows.push(json!({"id":id,"status":child.status,"workspace":child.root,"latest":child.events.last()}));
    }
    Ok(rows)
}

pub struct Subagent;
impl Tool for Subagent {
    fn name(&self) -> &'static str {
        "subagent"
    }
    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }
    fn safety_for(&self, args: &Value) -> ToolSafety {
        if matches!(args["action"].as_str(), Some("list" | "status" | "changes")) {
            ToolSafety::Safe
        } else {
            ToolSafety::Dangerous
        }
    }
    fn schema(&self) -> Value {
        json!({"type":"function","name":"subagent","description":"Manage individually scoped workers only when the user explicitly requests delegation. Default read-only workers share current files; writable workers use isolated worktrees containing the current workspace snapshot. They never auto-approve actions. Inspect status, deliver follow-ups, respond to explicit permission prompts, cancel, and review changes before applying them through normal file tools.","parameters":{"type":"object","properties":{
        "action":{"type":"string","enum":["spawn","list","status","send","steer","permission","cancel","changes","close"]},
        "id":{"type":"string"},"task":{"type":"string"},"scope":{"type":"string"},"writable":{"type":"boolean"},
        "max_iterations":{"type":"integer","minimum":1,"maximum":90},"timeout_secs":{"type":"integer","minimum":1,"maximum":3600},
        "request_id":{"type":"string"},"allow":{"type":"boolean"},"wait_ms":{"type":"integer","minimum":0,"maximum":10000}
    },"required":["action"],"additionalProperties":false}})
    }
    fn call(&self, args: Value) -> ToolResult {
        self.call_cancellable(args, &|| false)
    }
    fn call_cancellable(&self, args: Value, cancel: &dyn Fn() -> bool) -> ToolResult {
        if std::env::var("OSMOGREP_DELEGATED").as_deref() == Ok("1") {
            return Err("Delegated workers cannot recursively delegate".into());
        }
        let parent = std::env::current_dir().map_err(|e| e.to_string())?;
        if args["action"] == "spawn" {
            return spawn(&parent, &args, cancel);
        }
        let mut all = children()
            .lock()
            .map_err(|_| "Subagent registry unavailable")?;
        if args["action"] == "list" {
            return Ok(json!(all
                .iter()
                .filter(|(_, s)| s.parent == parent)
                .map(|(id, s)| json!({"id":id,"status":s.status,"workspace":s.root}))
                .collect::<Vec<_>>()));
        }
        let id = args["id"].as_str().ok_or("Missing worker ID")?;
        let child = all.get_mut(id).ok_or("Unknown worker ID")?;
        if child.parent != parent {
            return Err("Worker belongs to another workspace".into());
        }
        match args["action"].as_str().ok_or("Missing action")? {
            "status" => {}
            "send" | "steer" => {
                let text = args["task"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Missing follow-up task")?;
                let method = if args["action"] == "steer" {
                    "steer"
                } else {
                    "prompt"
                };
                send(
                    child,
                    json!({"id":uuid::Uuid::new_v4().to_string(),"method":method,"params":if method=="steer"{json!({"text":text})}else{json!({"prompt":text})}}),
                    cancel,
                )?;
            }
            "permission" => send(
                child,
                json!({"id":uuid::Uuid::new_v4().to_string(),"method":"permission","params":{"request_id":args["request_id"],"allow":args["allow"]}}),
                cancel,
            )?,
            "cancel" => send(
                child,
                json!({"id":uuid::Uuid::new_v4().to_string(),"method":"cancel"}),
                cancel,
            )?,
            "close" => {
                crate::command_sessions::interact(
                    &child.root,
                    &json!({"session_id":child.process,"cancel":true,"yield_ms":0}),
                    cancel,
                )?;
                child.status = "closed".into();
            }
            "changes" => return changes(child),
            _ => return Err("Unknown worker action".into()),
        }
        poll(child, cancel)?;
        if args["action"] == "status" {
            let deadline = std::time::Instant::now()
                + Duration::from_millis(args["wait_ms"].as_u64().unwrap_or(500).min(10_000));
            while !matches!(
                child.status.as_str(),
                "idle" | "waiting_permission" | "error" | "cancelled" | "exited" | "closed"
            ) && std::time::Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(20));
                poll(child, cancel)?;
            }
        }
        Ok(
            json!({"id":id,"status":child.status,"workspace":child.root,"events":child.events,"note":"Read permission_request events; an explicit permission action is required to answer them. close preserves the worktree."}),
        )
    }
}
fn send(child: &Child, message: Value, cancel: &dyn Fn() -> bool) -> Result<(), String> {
    crate::command_sessions::interact(
        &child.root,
        &json!({"session_id":child.process,"input":format!("{message}\n"),"stdout_cursor":child.cursor,"yield_ms":0}),
        cancel,
    )?;
    Ok(())
}
fn poll(child: &mut Child, cancel: &dyn Fn() -> bool) -> Result<(), String> {
    let result = crate::command_sessions::interact(
        &child.root,
        &json!({"session_id":child.process,"stdout_cursor":child.cursor,"yield_ms":0}),
        cancel,
    )?;
    child.cursor = result["stdout_cursor"].as_u64().unwrap_or(child.cursor);
    child
        .partial
        .push_str(result["stdout"].as_str().unwrap_or(""));
    while let Some(end) = child.partial.find('\n') {
        let line = child.partial.drain(..=end).collect::<String>();
        if let Ok(event) = serde_json::from_str::<Value>(&line) {
            if let Some(kind) = event["type"].as_str() {
                child.status = match kind {
                    "done" => "idle",
                    "permission_request" => "waiting_permission",
                    "error" => "error",
                    "cancelled" => "cancelled",
                    "run_status" => "working",
                    _ => &child.status,
                }
                .to_string();
            }
            child.events.push(event);
            if child.events.len() > 40 {
                child.events.remove(0);
            }
        }
    }
    if child.partial.len() > 1024 * 1024 {
        return Err("Worker emitted an oversized RPC record".into());
    }
    if result["running"] == false {
        child.status = "exited".into();
    }
    Ok(())
}
fn spawn(parent: &Path, args: &Value, cancel: &dyn Fn() -> bool) -> ToolResult {
    let task = args["task"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("Missing worker task")?;
    let scope = args["scope"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("Give the worker a specific scope")?;
    let mut all = children()
        .lock()
        .map_err(|_| "Subagent registry unavailable")?;
    if all
        .values()
        .filter(|s| !matches!(s.status.as_str(), "exited" | "closed"))
        .count()
        >= 8
    {
        return Err("Eight workers are already open; close an idle worker first".into());
    }
    let writable = args["writable"] == true;
    let root = if writable {
        crate::worktree::create_role_worktree(parent, "worker")?.path
    } else {
        parent.to_path_buf()
    };
    let baseline = if writable {
        copy_workspace(parent, &root)?
    } else {
        BTreeMap::new()
    };
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg("rpc")
        .arg("--repo-root")
        .arg(&root)
        .args([
            "--permission-profile",
            if writable {
                "workspace-auto"
            } else {
                "read-only"
            },
        ])
        .env("OSMOGREP_DELEGATED", "1")
        .env(
            "OSMOGREP_MAX_ITERATIONS",
            args["max_iterations"]
                .as_u64()
                .unwrap_or(20)
                .clamp(1, 90)
                .to_string(),
        );
    let timeout = Duration::from_secs(args["timeout_secs"].as_u64().unwrap_or(900).clamp(1, 3600));
    let process = crate::command_sessions::start_harness(&root, command, timeout)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut child = Child {
        parent: parent.into(),
        root,
        process,
        cursor: 0,
        partial: String::new(),
        events: Vec::new(),
        status: "starting".into(),
        baseline,
    };
    let prompt=format!("Delegated scope: {scope}\nTask: {task}\nWork only on this scope. Do not delegate, push, or alter unrelated files. Report completed work and relevant evidence concisely. A permission prompt is not permission to approve yourself.");
    send(
        &child,
        json!({"id":"initial","method":"prompt","params":{"prompt":prompt}}),
        cancel,
    )?;
    poll(&mut child, cancel)?;
    let result = json!({"id":id,"status":child.status,"workspace":child.root,"writable":writable,"max_iterations":args["max_iterations"].as_u64().unwrap_or(20).clamp(1,90),"timeout_secs":timeout.as_secs()});
    all.insert(id, child);
    Ok(result)
}
fn paths(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Worktree file discovery failed".into());
    }
    if output.stdout.len() > 2 * 1024 * 1024 {
        return Err("Workspace file list exceeds 2 MiB".into());
    }
    let text = String::from_utf8(output.stdout).map_err(|_| "Workspace has non-UTF-8 paths")?;
    let mut paths = text
        .split('\0')
        .filter(|s| !s.is_empty() && !s.starts_with(".context/"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    if paths.len() > 10_000 {
        return Err("Workspace snapshot exceeds 10,000 files; use read-only workers".into());
    }
    Ok(paths)
}
fn copy_workspace(parent: &Path, root: &Path) -> Result<BTreeMap<String, Option<Vec<u8>>>, String> {
    let mut baseline = BTreeMap::new();
    let mut size = 0u64;
    for path in paths(parent)? {
        let source = crate::file_guard::workspace_path(parent, Path::new(&path), false)?;
        let target = crate::file_guard::workspace_path(root, Path::new(&path), true)?;
        if source.is_dir() {
            continue;
        }
        let contents = match fs::read(&source) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        size += contents.as_ref().map_or(0, |b| b.len() as u64);
        if size > 50 * 1024 * 1024 {
            return Err("Workspace snapshot exceeds 50 MiB; use read-only workers".into());
        }
        if let Some(bytes) = &contents {
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::write(&target, bytes).map_err(|e| e.to_string())?;
            if let Ok(metadata) = fs::metadata(source) {
                fs::set_permissions(&target, metadata.permissions()).map_err(|e| e.to_string())?;
            }
        } else if target.exists() {
            fs::remove_file(target).map_err(|e| e.to_string())?;
        }
        baseline.insert(path, contents);
    }
    Ok(baseline)
}
fn changes(child: &Child) -> ToolResult {
    if child.root == child.parent {
        return Ok(json!({"note":"Read-only worker shares the workspace; no isolated edit diff"}));
    }
    let mut all = child
        .baseline
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    all.extend(paths(&child.root)?);
    let mut patch = String::new();
    let mut binaries = Vec::new();
    let mut count = 0;
    for path in all {
        let before_file = child.baseline.get(&path).and_then(Option::as_deref);
        let absolute = crate::file_guard::workspace_path(&child.root, Path::new(&path), false)?;
        let after_file = match fs::read(&absolute) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("Cannot read worker change {path}: {error}")),
        };
        if before_file == after_file.as_deref() {
            continue;
        }
        count += 1;
        let before = before_file.unwrap_or_default();
        let after = after_file.as_deref().unwrap_or_default();
        if before.is_empty() && after.is_empty() {
            binaries.push(format!(
                "{path} (empty file {}; inspect worktree)",
                if after_file.is_some() {
                    "created"
                } else {
                    "deleted"
                }
            ));
            continue;
        }
        if let (Ok(before), Ok(after)) = (std::str::from_utf8(before), std::str::from_utf8(&after))
        {
            let old = if child.baseline.get(&path).is_some_and(Option::is_some) {
                format!("a/{path}")
            } else {
                "/dev/null".into()
            };
            let new = if absolute.exists() {
                format!("b/{path}")
            } else {
                "/dev/null".into()
            };
            patch.push_str(
                &similar::TextDiff::from_lines(before, after)
                    .unified_diff()
                    .context_radius(3)
                    .header(&old, &new)
                    .to_string(),
            );
        } else {
            binaries.push(path);
        }
        if patch.len() > 2 * 1024 * 1024 {
            return Err("Worker diff exceeds 2 MiB; inspect the worktree directly".into());
        }
    }
    let artifact = crate::file_guard::internal_dir(&child.parent, ".context/osmogrep-subagents")?
        .join(format!("{}.patch", child.process));
    crate::persistence::atomic_write(&artifact, patch.as_bytes()).map_err(|e| e.to_string())?;
    Ok(
        json!({"changed_files":count,"patch_file":artifact,"patch":if patch.len()<=24*1024{patch}else{String::from("Diff saved to patch_file; read it in chunks")},"binary_changes":binaries,"note":"Diff is relative to the parent's workspace at spawn, including its uncommitted changes. Review and apply through normal edit/patch tools; parent edits are not overwritten automatically."}),
    )
}
