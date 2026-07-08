use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

mod diagnostics;
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
mod write;

pub use diagnostics::Diagnostics;
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
}

impl ToolScope {
    pub fn for_prompt(prompt: &str) -> Self {
        let lower = prompt.to_ascii_lowercase();
        Self {
            include_web: contains_any(
                &lower,
                &[
                    "http://",
                    "https://",
                    "url",
                    "web",
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
        }
    }

    fn allows(&self, name: &str) -> bool {
        match name {
            "web_fetch" | "web_search" => self.include_web,
            "mcp_call" => self.include_mcp,
            "notebook_edit" => self.include_notebooks,
            "git_commit" => self.include_git_commit,
            _ => true,
        }
    }
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
