use super::{Tool, ToolResult, ToolSafety};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
pub struct ToolSearch {
    pub catalog: Vec<Value>,
    pub enabled: Arc<Mutex<BTreeSet<String>>>,
}
impl Tool for ToolSearch {
    fn name(&self) -> &'static str {
        "tool_search"
    }
    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }
    fn schema(&self) -> Value {
        json!({"type":"function","name":"tool_search","description":"Find additional tools by capability and enable their schemas for the next model turn. Use when the currently visible tools do not cover the user's task. This discovers tools; it does not execute them or grant permissions.","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}})
    }
    fn call(&self, args: Value) -> ToolResult {
        let query = args["query"]
            .as_str()
            .ok_or("Missing tool query")?
            .to_lowercase();
        let words = query.split_whitespace().collect::<Vec<_>>();
        if words.is_empty() {
            return Err("Describe the capability you need".into());
        }
        let mut ranked = self
            .catalog
            .iter()
            .filter(|tool| {
                !matches!(
                    tool["name"].as_str(),
                    Some("subagent" | "worktree_swarm" | "team")
                )
            })
            .map(|tool| {
                let text = format!("{} {}", tool["name"], tool["description"]).to_lowercase();
                let score = words.iter().filter(|word| text.contains(**word)).count();
                (score, tool)
            })
            .filter(|(score, _)| *score > 0)
            .collect::<Vec<_>>();
        ranked.sort_by(|(a_score, a), (b_score, b)| {
            b_score
                .cmp(a_score)
                .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
        });
        let tools = ranked
            .into_iter()
            .take(6)
            .map(|(_, tool)| tool.clone())
            .collect::<Vec<_>>();
        let mut enabled = self
            .enabled
            .lock()
            .map_err(|_| "Tool discovery unavailable")?;
        for tool in &tools {
            if let Some(name) = tool["name"].as_str() {
                enabled.insert(name.into());
            }
        }
        Ok(
            json!({"tools":tools,"note":"Schemas enabled for the next request. Normal permission and read-only policies still apply."}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tied_search_results_ignore_catalog_insertion_order() {
        let catalog: Vec<_> = ["zeta", "alpha", "beta", "delta", "epsilon", "gamma", "eta"]
            .into_iter()
            .map(|name| json!({"name":name,"description":"source lookup"}))
            .collect();
        let search = |catalog| ToolSearch {
            catalog,
            enabled: Arc::new(Mutex::new(BTreeSet::new())),
        };
        let first = search(catalog.clone())
            .call(json!({"query":"source"}))
            .unwrap();
        let second = search(catalog.into_iter().rev().collect())
            .call(json!({"query":"source"}))
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first["tools"][0]["name"], "alpha");
        assert_eq!(first["tools"].as_array().unwrap().len(), 6);
    }
}
