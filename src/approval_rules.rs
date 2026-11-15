use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
#[derive(Default, Deserialize)]
#[serde(default)]
struct Rules {
    allow_commands: Vec<Vec<String>>,
    write_paths: Vec<String>,
    allow_tools: Vec<String>,
}
pub fn allows(root: &Path, tool: &str, args: &Value) -> bool {
    let config = std::fs::read_to_string(crate::persistence::config_dir().join("config.toml"))
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok());
    let rules = config
        .and_then(|c| c.get("permissions").cloned())
        .and_then(|c| c.try_into::<Rules>().ok())
        .unwrap_or_default();
    matches(&rules, root, tool, args)
}
fn matches(rules: &Rules, root: &Path, tool: &str, args: &Value) -> bool {
    if rules.allow_tools.iter().any(|name| name == tool) {
        return true;
    }
    if tool == "run_shell" {
        let Some(command) = args["cmd"].as_str() else {
            return false;
        };
        // Prefix grants apply only to literal argv; shell syntax always asks again.
        if !command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./:=+@%- ".contains(&b))
        {
            return false;
        }
        let words = command.split_whitespace().collect::<Vec<_>>();
        return rules.allow_commands.iter().any(|prefix| {
            !prefix.is_empty()
                && words.len() >= prefix.len()
                && prefix.iter().zip(&words).all(|(a, b)| a == b)
        });
    }
    let paths = crate::file_guard::edit_paths(tool, args);
    !paths.is_empty()
        && paths.iter().all(|path| {
            crate::file_guard::workspace_path(root, Path::new(path), true).is_ok_and(|target| {
                rules.write_paths.iter().any(|prefix| {
                    crate::file_guard::workspace_path(root, Path::new(prefix), true)
                        .is_ok_and(|prefix| target.starts_with(prefix))
                })
            })
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_rules_do_not_authorize_shell_expansion_or_multiple_commands() {
        let rules = Rules {
            allow_commands: vec![vec!["cargo".into(), "test".into()]],
            ..Default::default()
        };
        for command in ["cargo test", "cargo test --locked parser"] {
            assert!(matches(
                &rules,
                Path::new("."),
                "run_shell",
                &serde_json::json!({"cmd":command})
            ));
        }
        for command in [
            "cargo test; rm file",
            "cargo test $(id)",
            "cargo test > file",
            "cargo testing",
            "cargo test\nwhoami",
        ] {
            assert!(!matches(
                &rules,
                Path::new("."),
                "run_shell",
                &serde_json::json!({"cmd":command})
            ));
        }
    }
}
