use serde_json::Value;
use std::collections::HashMap;

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
}

impl ToolRegistry {
    pub fn new() -> Self {
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

        Self { tools }
    }

    pub fn call(&self, name: &str, args: Value) -> ToolResult {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("unknown tool: {}", name))?;

        tool.call(args)
    }

    pub fn safety(&self, name: &str) -> Option<ToolSafety> {
        self.tools.get(name).map(|t| t.safety())
    }

    pub fn schema(&self) -> Vec<Value> {
        self.tools.values().map(|t| t.schema()).collect()
    }
}
