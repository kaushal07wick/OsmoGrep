use std::collections::HashMap;
use serde_json::Value;

mod shell;
mod read;
mod write;
mod edit;
mod search;
mod glob;

pub use shell::Shell;
pub use read::Read;
pub use write::Write;
pub use edit::Edit;
pub use search::Search;
pub use glob::Glob;

pub type ToolResult = Result<Value, String>;

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn schema(&self) -> Value;
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

    pub fn schema(&self) -> Vec<Value> {
        self.tools.values().map(|t| t.schema()).collect()
    }
}
