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
        json!({"type":"function","name":"tool_search","description":format!("Find and enable tools by capability, including {}. Discovery supplies workflow guidance; choose and chain relevant steps within task permissions. It does not execute tools or grant authorization.",crate::lab::workflows::CAPABILITIES),"parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}})
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
                let exact = tool["name"].as_str() == Some(query.trim());
                let workflow = if tool["name"] == "lab" {
                    crate::lab::workflows::search_score(&query)
                } else {
                    0
                };
                let score = usize::from(exact) * 1000
                    + workflow
                    + words.iter().filter(|word| text.contains(**word)).count();
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
        drop(enabled);
        let mut result = json!({"tools":tools,"note":"Schemas enabled for the next request. Normal permission and read-only policies still apply."});
        if result["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "lab"))
        {
            result["workflow"] = crate::lab::workflows::discovery(&super::repo_root()?, &query);
        }
        Ok(result)
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
    #[test]
    fn exact_tool_names_beat_incidental_description_matches() {
        let search = ToolSearch {
            catalog: vec![
                json!({"name":"aaa","description":"lab"}),
                json!({"name":"lab","description":"workflows"}),
            ],
            enabled: Arc::new(Mutex::new(BTreeSet::new())),
        };
        let root = std::env::temp_dir();
        let result = super::super::in_root(&root, || search.call(json!({"query":"lab"}))).unwrap();
        assert_eq!(result["tools"][0]["name"], "lab");
        assert_eq!(result["workflow"]["workflows"].as_array().unwrap().len(), 6);
        assert!(result["workflow"]["policy"]
            .as_str()
            .unwrap()
            .contains("grants no execution"));
    }
}
