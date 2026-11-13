use serde_json::{json, Value};
use std::path::PathBuf;

use super::{Tool, ToolResult, ToolSafety};

pub struct CodeGraph {
    pub root: PathBuf,
}

impl Tool for CodeGraph {
    fn name(&self) -> &'static str {
        "query_code_graph"
    }
    fn schema(&self) -> Value {
        json!({"type":"function","name":self.name(),
            "description":"Query the optional revision-aware code/runtime graph or structural regression history. Read-only; returns an error if not indexed. Static name matches and declarations are not runtime proof.",
            "parameters":{"type":"object","properties":{
                "revision":{"type":"string"},"seeds":{"type":"array","items":{"type":"string"}},
                "resource_history":{"type":"string"},"depth":{"type":"integer","minimum":0,"maximum":4},
                "limit":{"type":"integer","minimum":1,"maximum":500}},"required":["revision"],"additionalProperties":false}})
    }
    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }
    fn call(&self, args: Value) -> ToolResult {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Query {
            revision: String,
            #[serde(default)]
            seeds: Vec<String>,
            resource_history: Option<String>,
            depth: Option<usize>,
            limit: Option<usize>,
        }
        let query: Query = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let run = || -> crate::intelligence::Result<Value> {
            let revision = crate::intelligence::git::revision(&self.root, &query.revision)?;
            let store = crate::context::graph::Store::read(&self.root)?;
            let graph = store.neighborhood(
                &revision,
                &query.seeds,
                query.depth.unwrap_or(2),
                query.limit.unwrap_or(100),
            )?;
            let (history, observations) = match query.resource_history {
                Some(resource) => (
                    store.history(&resource, 5)?,
                    store.observations(&revision, &resource)?,
                ),
                None => (vec![], vec![]),
            };
            Ok(
                json!({"revision":revision,"graph":graph,"history":history,"runtime_observations":observations}),
            )
        };
        run().map_err(|e| e.to_string())
    }
}
