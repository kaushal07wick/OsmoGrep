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

    pub fn schema(&self) -> Vec<Value> {
        self.tools.values().map(|t| t.schema()).collect()
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
}
