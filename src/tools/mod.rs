use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod code_graph;
mod device_workspace;
mod diagnostics;
mod dynamic_workflow;
mod edit;
mod find_definition;
mod find_references;
mod git_commit;
mod git_diff;
mod git_log;
mod glob;
mod list_dir;
mod mcp_call;
mod notebook_edit;
mod patch;
mod plan;
mod read;
mod regex_search;
mod search;
mod shell;
mod test;
mod tool_search;
mod web_fetch;
mod web_search;
mod worktree_swarm;
mod write;

pub use code_graph::CodeGraph;
pub use diagnostics::Diagnostics;
pub use dynamic_workflow::DynamicWorkflow;
pub use edit::Edit;
pub use find_definition::FindDefinition;
pub use find_references::FindReferences;
pub use git_commit::GitCommit;
pub use git_diff::GitDiff;
pub use git_log::GitLog;
pub use glob::Glob;
pub use list_dir::ListDir;
pub use mcp_call::McpCall;
pub use notebook_edit::NotebookEdit;
pub use patch::Patch;
pub use plan::Plan;
pub use read::Read;
pub use regex_search::RegexSearch;
pub use search::Search;
pub use shell::Shell;
pub use test::Test;
pub use web_fetch::WebFetch;
pub use web_search::WebSearch;
pub use worktree_swarm::WorktreeSwarm;
pub use write::Write;

pub type ToolResult = Result<Value, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSafety {
    Safe,
    Dangerous,
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn schema(&self) -> Value;
    fn safety(&self) -> ToolSafety;
    fn safety_for(&self, _args: &Value) -> ToolSafety {
        self.safety()
    }
    fn call(&self, args: Value) -> ToolResult;
    fn call_cancellable(&self, args: Value, _is_cancelled: &dyn Fn() -> bool) -> ToolResult {
        self.call(args)
    }
}

pub struct ToolRegistry {
    tools: HashMap<&'static str, Box<dyn Tool>>,
    repo_root: PathBuf,
    read_receipts: Mutex<HashMap<PathBuf, String>>,
    enabled: std::sync::Arc<Mutex<std::collections::BTreeSet<String>>>,
}

#[derive(Clone, Debug)]
pub struct ToolScope {
    include_web: bool,
    include_mcp: bool,
    include_notebooks: bool,
    include_git_commit: bool,
    include_worktree_swarm: bool,
    include_dynamic_workflow: bool,
    include_intelligence: bool,
    include_team: bool,
    include_devices: bool,
    read_only: bool,
    chat_only: bool,
}

impl ToolScope {
    pub fn for_prompt(prompt: &str) -> Self {
        let lower = prompt.to_ascii_lowercase();
        let ultracode = has_ultracode_suffix(&lower);
        let include_devices = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|word| {
                matches!(
                    word,
                    "mac"
                        | "macos"
                        | "iphone"
                        | "iphones"
                        | "ios"
                        | "swiftui"
                        | "xcode"
                        | "simulator"
                        | "simulators"
                        | "device_workspace"
                )
            });
        Self {
            include_devices,
            include_team: crate::team::enabled() || crate::team::mentions_peer(prompt),
            chat_only: is_small_talk(prompt),
            include_web: contains_any(
                &lower,
                &[
                    "http://",
                    "https://",
                    "url",
                    "web",
                    "online",
                    "internet",
                    "latest",
                    "current docs",
                    "documentation",
                    "search online",
                    "fetch",
                ],
            ),
            include_mcp: crate::mcp::is_enabled() || include_devices,
            include_intelligence: contains_any(
                &lower,
                &[
                    "code graph",
                    "query_code_graph",
                    "runtime risk",
                    "predict",
                    "regression history",
                    "change impact",
                    "dependency impact",
                ],
            ),
            include_notebooks: contains_any(&lower, &["notebook", ".ipynb", "jupyter"]),
            include_git_commit: contains_any(
                &lower,
                &[
                    "commit",
                    "push",
                    "branch",
                    "git ",
                    "git-",
                    "pull request",
                    " pr ",
                ],
            ),
            include_worktree_swarm: std::env::var("OSMOGREP_DELEGATED").as_deref() != Ok("1")
                && contains_any(
                    &lower,
                    &["subagent", "sub-agent", "swarm", "delegate", "multi-agent"],
                ),
            include_dynamic_workflow: ultracode
                || contains_any(
                    &lower,
                    &[
                        "dynamic workflow",
                        "workflow",
                        "ultracode",
                        "deep research",
                        "research online",
                        "search online",
                        "cross-check",
                        "latest",
                        "current docs",
                    ],
                ),
            read_only: false,
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn chat_only(&self) -> bool {
        self.chat_only
    }

    fn allows(&self, name: &str) -> bool {
        if name == "task_progress" {
            return !self.chat_only;
        }
        if name == "team" {
            return !self.chat_only && (self.include_team || crate::team::enabled());
        }
        if self.chat_only {
            return false;
        }
        if self.read_only && is_mutating_tool(name) {
            return false;
        }
        match name {
            "web_fetch" | "web_search" => self.include_web,
            "mcp_call" => self.include_mcp,
            "device_workspace" => self.include_devices,
            "notebook_edit" => self.include_notebooks,
            "git_commit" => self.include_git_commit,
            "worktree_swarm" | "subagent" => self.include_worktree_swarm,
            "dynamic_workflow" => self.include_dynamic_workflow,
            "query_code_graph" => self.include_intelligence,
            "read_file" | "write_file" | "edit_file" | "run_shell" | "search" | "update_plan"
            | "tool_search" => true,
            _ => false,
        }
    }
}

fn is_small_talk(prompt: &str) -> bool {
    let words = prompt
        .to_lowercase()
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()).to_string())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    matches!(
        words.as_str(),
        "hi" | "hello"
            | "hey"
            | "hello bro"
            | "hey bro"
            | "hi bro"
            | "thanks"
            | "thank you"
            | "good morning"
            | "good evening"
    )
}

fn is_mutating_tool(name: &str) -> bool {
    matches!(
        name,
        "run_shell"
            | "write_file"
            | "edit_file"
            | "run_tests"
            | "diagnostics"
            | "patch"
            | "mcp_call"
            | "notebook_edit"
            | "memory"
            | "subagent"
            | "git_commit"
            | "worktree_swarm"
            | "device_workspace"
    )
}

fn has_ultracode_suffix(prompt: &str) -> bool {
    let trimmed = prompt
        .trim()
        .trim_end_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'));
    trimmed
        .split_whitespace()
        .last()
        .map(|word| word == "ultracode")
        .unwrap_or(false)
}

impl ToolRegistry {
    pub fn with_root(repo_root: PathBuf) -> Self {
        let mut tools: HashMap<&'static str, Box<dyn Tool>> = HashMap::new();

        let list: Vec<Box<dyn Tool>> = vec![
            Box::new(Shell),
            Box::new(Read),
            Box::new(Write),
            Box::new(Edit),
            Box::new(Search),
            Box::new(Glob),
            Box::new(Test),
            Box::new(ListDir),
            Box::new(GitDiff),
            Box::new(GitLog),
            Box::new(RegexSearch),
            Box::new(WebFetch),
            Box::new(McpCall),
            Box::new(device_workspace::DeviceWorkspace),
            Box::new(FindDefinition),
            Box::new(FindReferences),
            Box::new(GitCommit),
            Box::new(Patch),
            Box::new(Plan),
            Box::new(NotebookEdit),
            Box::new(WebSearch),
            Box::new(Diagnostics),
            Box::new(WorktreeSwarm),
            Box::new(DynamicWorkflow),
            Box::new(crate::project_context::Memory),
            Box::new(crate::subagents::Subagent),
            Box::new(crate::language_server::LanguageServer),
            Box::new(crate::team::Team),
            Box::new(code_graph::CodeGraph {
                root: repo_root.clone(),
            }),
        ];

        for tool in list {
            tools.insert(tool.name(), tool);
        }
        if std::env::var("OSMOGREP_DELEGATED").as_deref() == Ok("1") {
            tools.remove("subagent");
            tools.remove("worktree_swarm");
        }
        let enabled = std::sync::Arc::new(Mutex::new(std::collections::BTreeSet::new()));
        let search = tool_search::ToolSearch {
            catalog: tools.values().map(|t| t.schema()).collect(),
            enabled: enabled.clone(),
        };
        tools.insert("tool_search", Box::new(search));

        Self {
            tools,
            repo_root,
            read_receipts: Mutex::new(HashMap::new()),
            enabled,
        }
    }

    pub fn with_task(mut self, task: std::sync::Arc<crate::long_runs::Task>) -> Self {
        // Nested model processes need shared reservations before they can obey this budget.
        for name in ["subagent", "worktree_swarm", "dynamic_workflow"] {
            self.tools.remove(name);
        }
        self.tools
            .insert("task_progress", Box::new(crate::long_runs::Progress(task)));
        self.tools.insert(
            "tool_search",
            Box::new(tool_search::ToolSearch {
                catalog: self
                    .tools
                    .iter()
                    .filter(|(name, _)| **name != "tool_search")
                    .map(|(_, tool)| tool.schema())
                    .collect(),
                enabled: self.enabled.clone(),
            }),
        );
        self
    }
    pub fn call_cancellable(
        &self,
        name: &str,
        args: Value,
        is_cancelled: &dyn Fn() -> bool,
    ) -> ToolResult {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("unknown tool: {}", name))?;

        let args = self.guard_args(name, self.resolve_parallel_args(name, args))?;
        let peer = crate::team::for_root(&self.repo_root);
        let paths = crate::file_guard::edit_paths(name, &args);
        let _lease = if !paths.is_empty() {
            peer.as_ref().map(|p| p.claim(&paths, false)).transpose()?
        } else {
            None
        };
        let mut result = self.call_in_repo_root(tool.as_ref(), args.clone(), is_cancelled)?;
        if !crate::tool_guard::tool_failed(name, &result) {
            if let Some(peer) = peer {
                for path in paths {
                    if let Err(error) = peer.publish(
                        "edit",
                        &path,
                        "File tool completed; read the current content before modifying it",
                    ) {
                        if result.is_object() {
                            result["team_update_error"] = serde_json::json!(error);
                        }
                    }
                }
            }
        }
        self.remember_read(name, &args, &result)?;
        Ok(result)
    }

    pub fn parallel_safe(&self, name: &str) -> bool {
        matches!(self.safety(name), Some(ToolSafety::Safe))
            && !matches!(
                name,
                "update_plan" | "dynamic_workflow" | "tool_search" | "team" | "task_progress"
            )
    }

    pub fn call_parallel_safe_cancellable(
        &self,
        name: &str,
        args: Value,
        is_cancelled: &dyn Fn() -> bool,
    ) -> ToolResult {
        if !self.parallel_safe(name) {
            return Err(format!("tool is not parallel safe: {}", name));
        }

        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("unknown tool: {}", name))?;
        let args = self.guard_args(name, self.resolve_parallel_args(name, args))?;
        let result = in_root(&self.repo_root, || {
            tool.call_cancellable(args.clone(), is_cancelled)
        })?;
        self.remember_read(name, &args, &result)?;
        Ok(result)
    }

    fn guard_args(&self, name: &str, mut args: Value) -> Result<Value, String> {
        if !args.is_object() {
            return Err("Tool arguments must be a JSON object".into());
        }
        let editing = matches!(name, "write_file" | "edit_file" | "notebook_edit" | "patch");
        if editing {
            let root = self.repo_root.canonicalize().map_err(|e| e.to_string())?;
            for path in crate::file_guard::edit_paths(name, &args) {
                let path = crate::file_guard::workspace_path(&root, Path::new(&path), true)?;
                if path
                    .strip_prefix(&root)
                    .is_ok_and(|p| p.starts_with(".context/osmogrep-tasks"))
                {
                    return Err("Task supervisor records cannot be edited through file tools; use task_progress or task controls".into());
                }
            }
        }

        if crate::sandbox::mode() == "read-only"
            && (editing
                || (name == "memory" && args["action"].as_str().unwrap_or("read") != "read"))
        {
            return Err("Read-only sandbox denies workspace mutations".into());
        }
        if matches!(
            name,
            "read_file"
                | "write_file"
                | "edit_file"
                | "notebook_edit"
                | "list_dir"
                | "search"
                | "regex_search"
                | "glob_files"
                | "find_definition"
                | "find_references"
        ) {
            if let Some(path) = args["path"].as_str() {
                let path =
                    crate::file_guard::workspace_path(&self.repo_root, Path::new(path), editing)?;
                if editing {
                    self.check_receipt(&path, name == "write_file")?;
                }
                args["path"] = serde_json::json!(path);
            }
        }
        if name == "patch" {
            if let Some(patch) = args["patch"].as_str() {
                for line in patch.lines() {
                    let path = [
                        "*** Update File: ",
                        "*** Add File: ",
                        "*** Delete File: ",
                        "+++ ",
                        "--- ",
                        "rename from ",
                        "rename to ",
                    ]
                    .iter()
                    .find_map(|prefix| line.strip_prefix(prefix));
                    if let Some(path) = path {
                        let path = path.split('\t').next().unwrap_or("");
                        if path == "/dev/null" {
                            continue;
                        }
                        let path = path
                            .strip_prefix("a/")
                            .or_else(|| path.strip_prefix("b/"))
                            .unwrap_or(path);
                        let path = crate::file_guard::workspace_path(
                            &self.repo_root,
                            Path::new(path),
                            true,
                        )?;
                        self.check_receipt(&path, false)?;
                    }
                }
            }
        }
        Ok(args)
    }
    fn check_receipt(&self, path: &Path, require: bool) -> Result<(), String> {
        let receipts = self
            .read_receipts
            .lock()
            .map_err(|_| "Read tracking unavailable")?;
        if let Some(expected) = receipts.get(path) {
            let current = std::fs::read(path).map_err(|e| e.to_string())?;
            if crate::file_guard::digest(&current) != *expected {
                return Err(format!("{} changed since read_file. Read it again before editing; user changes were preserved.",path.display()));
            }
        } else if require && path.exists() {
            return Err("Read an existing file with read_file before replacing it".into());
        }
        Ok(())
    }
    fn remember_read(&self, name: &str, args: &Value, result: &Value) -> Result<(), String> {
        if name == "read_file" {
            if let (Some(path), Some(hash)) = (args["path"].as_str(), result["sha256"].as_str()) {
                self.read_receipts
                    .lock()
                    .map_err(|_| "Read tracking unavailable")?
                    .insert(PathBuf::from(path), hash.into());
            }
        } else if matches!(name, "write_file" | "edit_file" | "patch" | "notebook_edit") {
            let mut receipts = self
                .read_receipts
                .lock()
                .map_err(|_| "Read tracking unavailable")?;
            receipts.retain(|path, hash| {
                std::fs::read(path)
                    .map(|bytes| crate::file_guard::digest(&bytes) == *hash)
                    .unwrap_or(false)
            });
        }
        Ok(())
    }

    pub fn safety(&self, name: &str) -> Option<ToolSafety> {
        self.tools.get(name).map(|t| t.safety())
    }
    pub fn safety_for(&self, name: &str, args: &Value) -> Option<ToolSafety> {
        self.tools.get(name).map(|t| t.safety_for(args))
    }

    pub fn scoped_schema(&self, scope: &ToolScope) -> Vec<Value> {
        let enabled = self.enabled.lock().unwrap_or_else(|e| e.into_inner());
        let mut tools: Vec<_> = self
            .tools
            .iter()
            .filter_map(|(name, tool)| {
                (scope.allows(name)
                    || (!scope.chat_only
                        && !(scope.read_only && is_mutating_tool(name))
                        && enabled.contains(*name)))
                .then(|| tool.schema())
            })
            .collect();
        // Stable tool ordering keeps equivalent requests eligible for prefix caching.
        tools.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        tools
    }

    fn call_in_repo_root(
        &self,
        tool: &dyn Tool,
        args: Value,
        is_cancelled: &dyn Fn() -> bool,
    ) -> ToolResult {
        let result = in_root(&self.repo_root, || {
            tool.call_cancellable(args, is_cancelled)
        });
        if tool.name() == "device_workspace" && result.is_ok() {
            self.enabled
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert("mcp_call".into());
        }
        result
    }

    fn resolve_parallel_args(&self, name: &str, args: Value) -> Value {
        let mut map = match args {
            Value::Object(map) => map,
            other => return other,
        };

        match name {
            "read_file" | "search" | "regex_search" => {
                self.resolve_path_field(&mut map, "path", false);
            }
            "list_dir" | "find_definition" | "find_references" | "glob_files" => {
                self.resolve_path_field(&mut map, "path", true);
            }
            "git_diff" | "git_log" => {
                map.insert(
                    "_repo_root".to_string(),
                    Value::String(self.repo_root.display().to_string()),
                );
            }
            _ => {}
        }

        Value::Object(map)
    }

    fn resolve_path_field(
        &self,
        map: &mut serde_json::Map<String, Value>,
        key: &str,
        default_to_repo_root: bool,
    ) {
        match map.get(key).and_then(Value::as_str).map(str::to_string) {
            Some(path) => {
                map.insert(
                    key.to_string(),
                    Value::String(self.resolve_repo_path(&path)),
                );
            }
            None if default_to_repo_root => {
                map.insert(
                    key.to_string(),
                    Value::String(self.repo_root.display().to_string()),
                );
            }
            None => {}
        }
    }

    fn resolve_repo_path(&self, raw: &str) -> String {
        let path = Path::new(raw);
        if path.is_absolute() {
            raw.to_string()
        } else {
            self.repo_root.join(path).display().to_string()
        }
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

thread_local! {static ROOT_CONTEXT:std::cell::RefCell<Option<PathBuf>>=const{std::cell::RefCell::new(None)};}
pub fn context_root() -> Option<PathBuf> {
    ROOT_CONTEXT.with(|root| root.borrow().clone())
}
pub fn repo_root() -> Result<PathBuf, String> {
    context_root()
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)
        .map_err(|e| e.to_string())
}
pub(crate) fn in_root<T>(root: &Path, call: impl FnOnce() -> T) -> T {
    struct Restore(Option<PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ROOT_CONTEXT.with(|root| *root.borrow_mut() = self.0.take());
        }
    }
    let previous = ROOT_CONTEXT.with(|slot| slot.replace(Some(root.to_path_buf())));
    let _restore = Restore(previous);
    call()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn natural_peer_requests_expose_team_without_searching_for_it() {
        for prompt in [
            "there is one more osmogrep agent working here, can u message him and talk to him",
            "yeah message that peer only",
        ] {
            let scope = ToolScope::for_prompt(prompt);
            assert!(scope.allows("team"), "{prompt}");
            assert!(!scope.chat_only(), "{prompt}");
        }
    }

    #[test]
    fn everyday_tools_are_compact_deterministic_and_specialists_load_on_demand() {
        let root = std::env::temp_dir();
        let registry = ToolRegistry::with_root(root.clone());
        let mut scope = ToolScope::for_prompt("explain closures");
        scope.include_mcp = false;
        scope.include_team = false;
        let schema = registry.scoped_schema(&scope);
        let names = schema_names(&schema);
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(names.contains(&"tool_search".into()));
        assert!(!names.contains(&"find_definition".into()));
        for _ in 0..5 {
            assert_eq!(
                schema,
                ToolRegistry::with_root(root.clone()).scoped_schema(&scope)
            );
        }
        let size = serde_json::to_string(&schema).unwrap().len();
        eprintln!("Everyday tool schemas: {} tools, {size} bytes", names.len());
        assert!(
            size < 6500,
            "Everyday tool definitions grew to {size} bytes"
        );
        registry
            .call_cancellable("tool_search", json!({"query":"find_definition"}), &|| false)
            .unwrap();
        assert!(schema_names(&registry.scoped_schema(&scope)).contains(&"find_definition".into()));
        let protected = scope.read_only();
        registry
            .call_cancellable("tool_search", json!({"query":"write_file"}), &|| false)
            .unwrap();
        assert!(!schema_names(&registry.scoped_schema(&protected)).contains(&"write_file".into()));
    }
    #[test]
    fn invalid_arguments_and_supervisor_record_writes_are_rejected() {
        let root = std::env::temp_dir().join(format!("tool-boundaries-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        assert!(registry
            .guard_args("language_server", json!([]))
            .unwrap_err()
            .contains("JSON object"));
        assert!(registry
            .guard_args(
                "write_file",
                json!({"path":".context/osmogrep-tasks/task.json","content":"reset budget"})
            )
            .unwrap_err()
            .contains("supervisor"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_tool_contexts_never_change_process_directory() {
        struct Probe {
            root: PathBuf,
            process_root: PathBuf,
        }
        impl Tool for Probe {
            fn name(&self) -> &'static str {
                "probe_root"
            }
            fn schema(&self) -> Value {
                json!({})
            }
            fn safety(&self) -> ToolSafety {
                ToolSafety::Safe
            }
            fn call(&self, _: Value) -> ToolResult {
                assert_eq!(repo_root()?, self.root);
                assert_eq!(std::env::current_dir().unwrap(), self.process_root);
                let out = crate::process_runner::run_shell_command(
                    if cfg!(windows) { "pwd -W" } else { "pwd" },
                    None,
                    std::time::Duration::from_secs(3),
                )?;
                assert_eq!(
                    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
                        .canonicalize()
                        .unwrap(),
                    self.root.canonicalize().unwrap()
                );
                Ok(json!({"ok":true}))
            }
        }
        let root = std::env::temp_dir().join(format!("concurrent-context-{}", Uuid::new_v4()));
        let process_root = std::env::current_dir().unwrap();
        std::thread::scope(|scope| {
            for name in ["a", "b"] {
                let path = root.join(name);
                fs::create_dir_all(&path).unwrap();
                let mut registry = ToolRegistry::with_root(path.clone());
                registry.tools.insert(
                    "probe_root",
                    Box::new(Probe {
                        root: path,
                        process_root: process_root.clone(),
                    }),
                );
                scope.spawn(move || {
                    for _ in 0..5 {
                        registry
                            .call_cancellable("probe_root", json!({}), &|| false)
                            .unwrap();
                    }
                });
            }
        });
        assert_eq!(std::env::current_dir().unwrap(), process_root);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tool_discovery_preserves_policy_and_does_not_enable_delegation() {
        let root = std::env::temp_dir();
        let registry = ToolRegistry::with_root(root);
        let scope = ToolScope::for_prompt("fix this function");
        assert!(!schema_names(&registry.scoped_schema(&scope)).contains(&"web_fetch".into()));
        registry
            .call_cancellable("tool_search", json!({"query":"web fetch"}), &|| false)
            .unwrap();
        assert!(schema_names(&registry.scoped_schema(&scope)).contains(&"web_fetch".into()));
        assert!(registry
            .scoped_schema(&ToolScope::for_prompt("hello"))
            .is_empty());
        assert!(!schema_names(
            &registry.scoped_schema(&ToolScope::for_prompt("complex parallel audit"))
        )
        .contains(&"subagent".into()));
        registry
            .call_cancellable("tool_search", json!({"query":"subagent swarm"}), &|| false)
            .unwrap();
        assert!(!schema_names(&registry.scoped_schema(&scope)).contains(&"subagent".into()));
    }
    #[test]
    fn stale_file_receipts_protect_user_edits() {
        let root = std::env::temp_dir().join(format!("read-receipts-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a"), "old").unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        registry
            .call_cancellable("read_file", json!({"path":"a"}), &|| false)
            .unwrap();
        fs::write(root.join("a"), "user change").unwrap();
        assert!(registry
            .call_cancellable(
                "write_file",
                json!({"path":"a","content":"overwrite"}),
                &|| false
            )
            .is_err());
        assert_eq!(fs::read_to_string(root.join("a")).unwrap(), "user change");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn graph_tool_is_available_when_requested_by_exact_name() {
        assert!(
            ToolScope::for_prompt("Use query_code_graph for this revision")
                .allows("query_code_graph")
        );
        assert!(ToolScope::for_prompt("Use query_code_graph")
            .read_only()
            .allows("query_code_graph"));
        assert!(!ToolScope::for_prompt("fix the function").allows("query_code_graph"));
    }

    #[test]
    fn registry_executes_tools_from_configured_repo_root() {
        let root = std::env::temp_dir().join(format!("osmogrep-tools-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("only-in-root.txt"), "root scoped").unwrap();

        let registry = ToolRegistry::with_root(root.clone());
        let result = registry
            .call_cancellable("read_file", json!({ "path": "only-in-root.txt" }), &|| {
                false
            })
            .unwrap();

        assert_eq!(
            result.get("text").and_then(Value::as_str),
            Some("root scoped")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_safe_calls_resolve_paths_without_changing_cwd() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-parallel-tools-root-{}", Uuid::new_v4()));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("readme.txt"), "parallel scoped").unwrap();

        let previous = std::env::current_dir().unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let result = registry
            .call_parallel_safe_cancellable(
                "read_file",
                json!({ "path": "nested/readme.txt" }),
                &|| false,
            )
            .unwrap();

        assert_eq!(
            result.get("text").and_then(Value::as_str),
            Some("parallel scoped")
        );
        assert_eq!(std::env::current_dir().unwrap(), previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_shell_tool_reports_cancelled_process() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-shell-cancel-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());

        let result = registry
            .call_cancellable("run_shell", json!({ "cmd": "sleep 2" }), &|| true)
            .unwrap();

        assert_eq!(result.get("cancelled").and_then(Value::as_bool), Some(true));
        assert_ne!(result.get("exit_code").and_then(Value::as_i64), Some(0));
        assert!(result
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("cancelled"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_safe_calls_can_run_concurrently() {
        let root = std::env::temp_dir().join(format!(
            "osmogrep-parallel-concurrent-root-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "alpha").unwrap();
        fs::write(root.join("b.txt"), "beta").unwrap();

        let registry = ToolRegistry::with_root(root.clone());
        std::thread::scope(|scope| {
            let read = scope.spawn(|| {
                registry.call_parallel_safe_cancellable(
                    "read_file",
                    json!({ "path": "a.txt" }),
                    &|| false,
                )
            });
            let list = scope.spawn(|| {
                registry.call_parallel_safe_cancellable("list_dir", json!({}), &|| false)
            });

            let read = read.join().unwrap().unwrap();
            let list = list.join().unwrap().unwrap();
            assert_eq!(read.get("text").and_then(Value::as_str), Some("alpha"));
            assert_eq!(list.get("count").and_then(Value::as_u64), Some(2));
        });

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_safe_rejects_stateful_and_dangerous_tools() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-parallel-reject-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());

        assert!(!registry.parallel_safe("update_plan"));
        assert!(!registry.parallel_safe("dynamic_workflow"));
        assert!(!registry.parallel_safe("run_shell"));
        assert!(registry
            .call_parallel_safe_cancellable("update_plan", json!({ "action": "show" }), &|| false,)
            .is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_updates_durable_plan_file() {
        let root = std::env::temp_dir().join(format!("osmogrep-plan-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let registry = ToolRegistry::with_root(root.clone());
        let set = registry
            .call_cancellable(
                "update_plan",
                json!({ "action": "set", "items": ["inspect", "fix"] }),
                &|| false,
            )
            .unwrap();
        assert_eq!(
            set.pointer("/items/0/status").and_then(Value::as_str),
            Some("in_progress")
        );
        assert!(root.join(".context").join("osmogrep-plan.json").is_file());

        let done = registry
            .call_cancellable("update_plan", json!({ "action": "done", "id": 1 }), &|| {
                false
            })
            .unwrap();
        assert_eq!(
            done.pointer("/items/0/status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            done.pointer("/items/1/status").and_then(Value::as_str),
            Some("in_progress")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_persists_and_resumes_dynamic_workflow_ledger() {
        let root = std::env::temp_dir().join(format!("osmogrep-workflow-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let registry = ToolRegistry::with_root(root.clone());
        let start = registry
            .call_cancellable(
                "dynamic_workflow",
                json!({
                    "task": "refactor parser",
                    "kind": "coding",
                    "workflow_id": "parser/work",
                }),
                &|| false,
            )
            .unwrap();
        let id = start
            .pointer("/workflow/id")
            .and_then(Value::as_str)
            .unwrap();

        assert_eq!(id, "parser-work");
        assert_eq!(
            start.pointer("/next_step/name").and_then(Value::as_str),
            Some("orient")
        );
        assert!(root
            .join(".context")
            .join("osmogrep-workflows")
            .join("parser-work.json")
            .is_file());

        let checkpoint = registry
            .call_cancellable(
                "dynamic_workflow",
                json!({
                    "action": "checkpoint",
                    "workflow_id": id,
                    "completed_steps": ["orient"],
                    "notes": ["read local instructions"],
                }),
                &|| false,
            )
            .unwrap();

        assert_eq!(
            checkpoint
                .pointer("/workflow/completed_steps")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            checkpoint
                .pointer("/next_step/name")
                .and_then(Value::as_str),
            Some("map_code")
        );

        let resume = registry
            .call_cancellable(
                "dynamic_workflow",
                json!({ "action": "resume", "workflow_id": id }),
                &|| false,
            )
            .unwrap();

        assert_eq!(
            resume.pointer("/workflow/id").and_then(Value::as_str),
            Some(id)
        );
        assert_eq!(
            resume.pointer("/ledger/notes/0").and_then(Value::as_str),
            Some("read local instructions")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn greetings_never_expose_tools_but_requests_that_begin_with_a_greeting_still_do() {
        let registry = ToolRegistry::with_root(std::env::temp_dir());
        for prompt in ["hello", "Hi!", "hey bro", "thanks"] {
            let scope = ToolScope::for_prompt(prompt);
            assert!(scope.chat_only());
            assert!(registry.scoped_schema(&scope).is_empty());
            assert!(!scope.allows("run_shell"));
        }
        for prompt in [
            "hello, fix the parser",
            "hi, run tests",
            "continue the previous task",
        ] {
            assert!(!ToolScope::for_prompt(prompt).chat_only());
        }
    }

    #[test]
    fn apple_app_work_exposes_preparation_and_mcp_but_read_only_never_prepares_devices() {
        let registry = ToolRegistry::with_root(std::env::temp_dir());
        for prompt in [
            "Build and run an iPhone app",
            "Build and run a Mac based app",
            "Inspect this macOS app",
            "Test this SwiftUI screen in the simulator",
        ] {
            let scope = ToolScope::for_prompt(prompt);
            let names = schema_names(&registry.scoped_schema(&scope));
            assert!(names.iter().any(|n| n == "device_workspace"), "{prompt}");
            assert!(names.iter().any(|n| n == "mcp_call"), "{prompt}");
            assert!(!schema_names(&registry.scoped_schema(&scope.read_only()))
                .iter()
                .any(|n| n == "device_workspace"));
        }
    }

    #[test]
    fn default_tool_scope_hides_specialized_tools() {
        let root = std::env::temp_dir().join(format!("osmogrep-scope-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let names =
            schema_names(&registry.scoped_schema(&ToolScope::for_prompt("fix the rust bug")));

        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"run_shell".to_string()));
        assert!(names.contains(&"update_plan".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
        assert!(!names.contains(&"web_fetch".to_string()));
        assert!(!names.contains(&"mcp_call".to_string()));
        assert!(!names.contains(&"device_workspace".to_string()));
        assert!(!names.contains(&"notebook_edit".to_string()));
        assert!(!names.contains(&"git_commit".to_string()));
        assert!(!names.contains(&"worktree_swarm".to_string()));
        assert!(!names.contains(&"dynamic_workflow".to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_scope_exposes_web_tools_for_current_docs_requests() {
        let root = std::env::temp_dir().join(format!("osmogrep-web-scope-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let names = schema_names(&registry.scoped_schema(&ToolScope::for_prompt(
            "fetch the latest docs from https://example.com",
        )));

        assert!(names.contains(&"web_search".to_string()));
        assert!(names.contains(&"web_fetch".to_string()));
        assert!(names.contains(&"dynamic_workflow".to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_scope_exposes_dynamic_workflow_for_deep_research() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-workflow-scope-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let names = schema_names(&registry.scoped_schema(&ToolScope::for_prompt(
            "use a dynamic workflow to do deep research online",
        )));

        assert!(names.contains(&"dynamic_workflow".to_string()));
        assert!(names.contains(&"web_search".to_string()));
        assert!(names.contains(&"web_fetch".to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ultracode_suffix_exposes_dynamic_coding_workflows() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-ultracode-scope-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let names = schema_names(
            &registry.scoped_schema(&ToolScope::for_prompt("refactor the parser ultracode")),
        );

        assert!(names.contains(&"dynamic_workflow".to_string()));
        assert!(!names.contains(&"worktree_swarm".to_string()));
        assert!(!names.contains(&"subagent".to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_only_scope_hides_mutating_tools() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-readonly-scope-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let scope = ToolScope::for_prompt("plan a parser refactor ultracode").read_only();
        let names = schema_names(&registry.scoped_schema(&scope));

        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"search".to_string()));
        assert!(names.contains(&"update_plan".to_string()));
        assert!(names.contains(&"dynamic_workflow".to_string()));
        assert!(!names.contains(&"write_file".to_string()));
        assert!(!names.contains(&"edit_file".to_string()));
        assert!(!names.contains(&"run_shell".to_string()));
        assert!(!names.contains(&"run_tests".to_string()));
        assert!(!names.contains(&"worktree_swarm".to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_scope_exposes_worktree_swarm_for_parallel_audits() {
        let root =
            std::env::temp_dir().join(format!("osmogrep-worktree-scope-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let registry = ToolRegistry::with_root(root.clone());
        let names = schema_names(&registry.scoped_schema(&ToolScope::for_prompt(
            "do a deep audit with parallel subagents",
        )));

        assert!(names.contains(&"worktree_swarm".to_string()));
        let _ = fs::remove_dir_all(root);
    }

    fn schema_names(schema: &[Value]) -> Vec<String> {
        schema
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .map(ToString::to_string)
            .collect()
    }
}
