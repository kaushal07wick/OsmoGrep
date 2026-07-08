use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const EXACT_FAILURE_WARN_AFTER: usize = 2;
const SAME_TOOL_FAILURE_WARN_AFTER: usize = 3;
const NO_PROGRESS_WARN_AFTER: usize = 2;

#[derive(Debug, Clone, Serialize)]
pub struct ToolLoopWarning {
    pub code: &'static str,
    pub message: String,
    pub tool_name: String,
    pub count: usize,
}

#[derive(Default)]
pub struct ToolLoopGuard {
    exact_failure_counts: HashMap<String, usize>,
    same_tool_failure_counts: HashMap<String, usize>,
    idempotent_no_progress: HashMap<String, (String, usize)>,
}

impl ToolLoopGuard {
    pub fn after_call(
        &mut self,
        tool_name: &str,
        args: &Value,
        result: &Value,
    ) -> Option<ToolLoopWarning> {
        let signature = tool_signature(tool_name, args);
        if tool_failed(tool_name, result) {
            let exact_count = increment(&mut self.exact_failure_counts, &signature);
            self.idempotent_no_progress.remove(&signature);

            let same_count = increment(&mut self.same_tool_failure_counts, tool_name);
            if exact_count >= EXACT_FAILURE_WARN_AFTER {
                return Some(ToolLoopWarning {
                    code: "repeated_exact_failure",
                    message: format!(
                        "{tool_name} has failed {exact_count} times with identical arguments. Inspect the latest error and change strategy instead of retrying unchanged."
                    ),
                    tool_name: tool_name.to_string(),
                    count: exact_count,
                });
            }
            if same_count >= SAME_TOOL_FAILURE_WARN_AFTER {
                return Some(ToolLoopWarning {
                    code: "same_tool_failure",
                    message: format!(
                        "{tool_name} has failed {same_count} times in this run. Run one diagnostic or use a different tool/path instead of looping."
                    ),
                    tool_name: tool_name.to_string(),
                    count: same_count,
                });
            }
            return None;
        }

        self.exact_failure_counts.remove(&signature);
        self.same_tool_failure_counts.remove(tool_name);

        if !is_idempotent_tool(tool_name) {
            self.idempotent_no_progress.remove(&signature);
            return None;
        }

        let result_hash = hash_value(result);
        let count = match self.idempotent_no_progress.get_mut(&signature) {
            Some((previous_hash, previous_count)) if *previous_hash == result_hash => {
                *previous_count += 1;
                *previous_count
            }
            Some((previous_hash, previous_count)) => {
                *previous_hash = result_hash;
                *previous_count = 1;
                1
            }
            None => {
                self.idempotent_no_progress
                    .insert(signature, (result_hash, 1));
                1
            }
        };

        (count >= NO_PROGRESS_WARN_AFTER).then(|| ToolLoopWarning {
            code: "idempotent_no_progress",
            message: format!(
                "{tool_name} returned the same result {count} times for identical arguments. Use the result already provided or change the query."
            ),
            tool_name: tool_name.to_string(),
            count,
        })
    }
}

fn increment(counts: &mut HashMap<String, usize>, key: &str) -> usize {
    let count = counts.entry(key.to_string()).or_insert(0);
    *count += 1;
    *count
}

fn tool_failed(tool_name: &str, result: &Value) -> bool {
    if result.get("error").is_some() {
        return true;
    }
    match tool_name {
        "run_shell" | "patch" => result
            .get("exit_code")
            .and_then(Value::as_i64)
            .map(|code| code != 0)
            .unwrap_or(false),
        "run_tests" => result
            .get("success")
            .and_then(Value::as_bool)
            .map(|success| !success)
            .unwrap_or(false),
        _ => false,
    }
}

fn is_idempotent_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file"
            | "search"
            | "glob"
            | "list_dir"
            | "git_diff"
            | "git_log"
            | "regex_search"
            | "web_fetch"
            | "web_search"
            | "find_definition"
            | "find_references"
            | "diagnostics"
    )
}

fn tool_signature(tool_name: &str, args: &Value) -> String {
    format!("{tool_name}:{}", hash_value(args))
}

fn hash_value(value: &Value) -> String {
    let canonical = canonical_json(value);
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonical_json).collect()),
        Value::Object(map) => {
            let sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>();
            let mut out = serde_json::Map::new();
            for (key, value) in sorted {
                out.insert(key, value);
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn warns_on_repeated_identical_failure() {
        let mut guard = ToolLoopGuard::default();
        let args = json!({ "cmd": "false" });
        let result = json!({ "exit_code": 1, "stdout": "", "stderr": "" });

        assert!(guard.after_call("run_shell", &args, &result).is_none());
        let warning = guard.after_call("run_shell", &args, &result).unwrap();

        assert_eq!(warning.code, "repeated_exact_failure");
        assert_eq!(warning.count, 2);
    }

    #[test]
    fn warns_on_idempotent_no_progress() {
        let mut guard = ToolLoopGuard::default();
        let args = json!({ "path": "src/main.rs" });
        let result = json!({ "text": "same", "lines": 1 });

        assert!(guard.after_call("read_file", &args, &result).is_none());
        let warning = guard.after_call("read_file", &args, &result).unwrap();

        assert_eq!(warning.code, "idempotent_no_progress");
        assert_eq!(warning.count, 2);
    }

    #[test]
    fn canonicalizes_argument_order() {
        let mut guard = ToolLoopGuard::default();
        let result = json!({ "error": "missing" });

        assert!(guard
            .after_call("read_file", &json!({ "a": 1, "b": 2 }), &result)
            .is_none());
        let warning = guard
            .after_call("read_file", &json!({ "b": 2, "a": 1 }), &result)
            .unwrap();

        assert_eq!(warning.code, "repeated_exact_failure");
    }
}
