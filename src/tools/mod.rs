use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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
mod web_fetch;
mod web_search;
mod worktree_swarm;
mod write;

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
    fn call(&self, args: Value) -> ToolResult;
}

pub struct ToolRegistry {
    tools: HashMap<&'static str, Box<dyn Tool>>,
    repo_root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ToolScope {
    include_web: bool,
    include_mcp: bool,
    include_notebooks: bool,
    include_git_commit: bool,
    include_worktree_swarm: bool,
    include_dynamic_workflow: bool,
}

impl ToolScope {
    pub fn for_prompt(prompt: &str) -> Self {
        let lower = prompt.to_ascii_lowercase();
        let ultracode = has_ultracode_suffix(&lower);
        Self {
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
            include_mcp: lower.contains("mcp"),
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
            include_worktree_swarm: ultracode
                || contains_any(
                    &lower,
                    &[
                        "subagent",
                        "sub-agent",
                        "swarm",
                        "worktree",
                        "parallel",
                        "deep audit",
                        "audit",
                        "complex",
                        "large refactor",
                        "multi-agent",
                    ],
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
        }
    }

    fn allows(&self, name: &str) -> bool {
        match name {
            "web_fetch" | "web_search" => self.include_web,
            "mcp_call" => self.include_mcp,
            "notebook_edit" => self.include_notebooks,
            "git_commit" => self.include_git_commit,
            "worktree_swarm" => self.include_worktree_swarm,
            "dynamic_workflow" => self.include_dynamic_workflow,
            _ => true,
        }
    }
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
        ];

        for tool in list {
            tools.insert(tool.name(), tool);
        }

        Self { tools, repo_root }
    }

    pub fn call(&self, name: &str, args: Value) -> ToolResult {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("unknown tool: {}", name))?;

        self.call_in_repo_root(tool.as_ref(), args)
    }

    pub fn parallel_safe(&self, name: &str) -> bool {
        matches!(self.safety(name), Some(ToolSafety::Safe)) && !matches!(name, "update_plan")
    }

    pub fn call_parallel_safe(&self, name: &str, args: Value) -> ToolResult {
        if !self.parallel_safe(name) {
            return Err(format!("tool is not parallel safe: {}", name));
        }

        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("unknown tool: {}", name))?;
        tool.call(self.resolve_parallel_args(name, args))
    }

    pub fn safety(&self, name: &str) -> Option<ToolSafety> {
        self.tools.get(name).map(|t| t.safety())
    }

    pub fn scoped_schema(&self, scope: &ToolScope) -> Vec<Value> {
        self.tools
            .iter()
            .filter_map(|(name, tool)| scope.allows(name).then(|| tool.schema()))
            .collect()
    }

    fn call_in_repo_root(&self, tool: &dyn Tool, args: Value) -> ToolResult {
        static TOOL_CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

        let _guard = TOOL_CWD_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| "tool cwd lock poisoned".to_string())?;
        let previous = std::env::current_dir().map_err(|e| e.to_string())?;
        std::env::set_current_dir(&self.repo_root).map_err(|e| {
            format!(
                "failed to enter tool repo root {}: {}",
                self.repo_root.display(),
                e
            )
        })?;

        let result = tool.call(args);
        let restore = std::env::set_current_dir(&previous).map_err(|e| {
            format!(
                "failed to restore working directory {}: {}",
                previous.display(),
                e
            )
        });

        match (result, restore) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(err), Ok(())) => Err(err),
            (Ok(_), Err(err)) => Err(err),
            (Err(err), Err(restore_err)) => Err(format!("{err}; {restore_err}")),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn registry_executes_tools_from_configured_repo_root() {
        let root = std::env::temp_dir().join(format!("osmogrep-tools-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("only-in-root.txt"), "root scoped").unwrap();

        let registry = ToolRegistry::with_root(root.clone());
        let result = registry
            .call("read_file", json!({ "path": "only-in-root.txt" }))
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
            .call_parallel_safe("read_file", json!({ "path": "nested/readme.txt" }))
            .unwrap();

        assert_eq!(
            result.get("text").and_then(Value::as_str),
            Some("parallel scoped")
        );
        assert_eq!(std::env::current_dir().unwrap(), previous);
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
            let read = scope
                .spawn(|| registry.call_parallel_safe("read_file", json!({ "path": "a.txt" })));
            let list = scope.spawn(|| registry.call_parallel_safe("list_dir", json!({})));

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
        assert!(!registry.parallel_safe("run_shell"));
        assert!(registry
            .call_parallel_safe("update_plan", json!({ "action": "show" }))
            .is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_updates_durable_plan_file() {
        let root = std::env::temp_dir().join(format!("osmogrep-plan-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let registry = ToolRegistry::with_root(root.clone());
        let set = registry
            .call(
                "update_plan",
                json!({ "action": "set", "items": ["inspect", "fix"] }),
            )
            .unwrap();
        assert_eq!(
            set.pointer("/items/0/status").and_then(Value::as_str),
            Some("in_progress")
        );
        assert!(root.join(".context").join("osmogrep-plan.json").is_file());

        let done = registry
            .call("update_plan", json!({ "action": "done", "id": 1 }))
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
        let names = schema_names(&registry.scoped_schema(&ToolScope::for_prompt(
            "refactor the parser ultracode",
        )));

        assert!(names.contains(&"dynamic_workflow".to_string()));
        assert!(names.contains(&"worktree_swarm".to_string()));
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
