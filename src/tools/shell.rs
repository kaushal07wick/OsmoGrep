// src/tools/shell.rs

use super::{Tool, ToolResult, ToolSafety};
use serde_json::{json, Value};
pub struct Shell;

impl Tool for Shell {
    fn name(&self) -> &'static str {
        "run_shell"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "run_shell",
            "description": "Start a shell command, or poll/interact with a running command using session_id. Returns incremental output and cursors. Poll running sessions until completion when their result is needed. PTY supports interactive programs on macOS/Linux. Background sessions live until cancelled, timed out, closed, or OsmoGrep exits.",
            "parameters": {
                "type": "object",
                "properties": {
                    "cmd": { "type": "string" },
                    "session_id": { "type": "string" },
                    "input": { "type": "string", "description":"Literal stdin bytes, including newline when needed" },
                    "close_stdin": { "type":"boolean" },
                    "cancel": { "type":"boolean" },
                    "close": { "type":"boolean" },
                    "pty": { "type":"boolean" },
                    "yield_ms": { "type":"integer","minimum":0,"maximum":10000 },
                    "timeout_secs": { "type":"integer","minimum":1,"maximum":86400 },
                    "stdout_cursor": { "type":"integer","minimum":0 },
                    "stderr_cursor": { "type":"integer","minimum":0 }
                },
                "required": [],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }
    fn safety_for(&self, args: &Value) -> ToolSafety {
        if args["session_id"]
            .as_str()
            .is_some_and(|id| !id.trim().is_empty())
            && args["input"].as_str().is_none()
            && args["cancel"] != true
            && args["close"] != true
            && args["close_stdin"] != true
        {
            ToolSafety::Safe
        } else {
            ToolSafety::Dangerous
        }
    }

    fn call(&self, args: Value) -> ToolResult {
        self.call_cancellable(args, &|| false)
    }

    fn call_cancellable(&self, args: Value, is_cancelled: &dyn Fn() -> bool) -> ToolResult {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        call_at(&root, args, is_cancelled)
    }
}

fn call_at(root: &std::path::Path, args: Value, is_cancelled: &dyn Fn() -> bool) -> ToolResult {
    if is_cancelled() {
        return Ok(
            json!({"cancelled":true,"running":false,"exit_code":null,"stdout":"","stderr":"cancelled before command start"}),
        );
    }
    if args["session_id"]
        .as_str()
        .is_some_and(|id| !id.trim().is_empty())
    {
        if args["cmd"]
            .as_str()
            .is_some_and(|cmd| !cmd.trim().is_empty())
        {
            return Err("Use cmd to start OR session_id to interact, not both".into());
        }
        return crate::command_sessions::interact(root, &args, is_cancelled);
    }
    let cmd = args
        .get("cmd")
        .and_then(Value::as_str)
        .filter(|cmd| !cmd.trim().is_empty())
        .ok_or("missing cmd")?;
    if let Err(e) = crate::shell_guard::check_shell_command(cmd) {
        return Ok(json!({
            "error": e,
            "blocked": true,
            "exit_code": null
        }));
    }

    let pre_hook =
        crate::hooks::run_hook_cancellable("pre_shell", &[("cmd", cmd)], root, is_cancelled)?;
    if is_cancelled() {
        return Ok(
            json!({"cancelled":true,"running":false,"exit_code":null,"stdout":"","stderr":"cancelled before command start"}),
        );
    }

    let timeout = args["timeout_secs"]
        .as_u64()
        .map(|n| std::time::Duration::from_secs(n.clamp(1, 86400)))
        .unwrap_or_else(|| {
            crate::process_runner::timeout_from_env("OSMOGREP_SHELL_TIMEOUT_SECS", 120)
        });
    let id = crate::command_sessions::start(root, cmd, args["pty"] == true, timeout)?;
    let mut poll = args.clone();
    poll["session_id"] = json!(id);
    let mut result = crate::command_sessions::interact(root, &poll, is_cancelled)?;
    let stdout_raw = result["stdout"].as_str().unwrap_or("").to_string();
    let stderr_raw = result["stderr"].as_str().unwrap_or("").to_string();
    let stdout = crate::tool_budget::budget_text(root, "run_shell_stdout", &stdout_raw);
    let stderr = crate::tool_budget::budget_text(root, "run_shell_stderr", &stderr_raw);

    result["stdout"] = json!(stdout.text);
    result["stderr"] = json!(stderr.text);
    result["hook"] = json!(pre_hook);
    result["output_budget"] = json!({
            "stdout": stdout,
            "stderr": stderr
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nullable_optional_fields_start_and_poll_a_command() {
        let root = std::env::temp_dir().join(format!("shell-nullable-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(
            call_at(
                &root,
                json!({"cmd":"printf bad > cancelled-marker"}),
                &|| true
            )
            .unwrap()["cancelled"]
                == true
        );
        assert!(!root.join("cancelled-marker").exists());
        let started = call_at(
            &root,
            json!({"cmd":"printf NULLABLE_OK","session_id":null,"input":null,"yield_ms":1000}),
            &|| false,
        )
        .unwrap();
        assert_eq!(started["exit_code"], 0);
        assert_eq!(started["stdout"], "NULLABLE_OK");
        let poll = json!({"cmd":null,"session_id":started["session_id"],"input":null,"yield_ms":0});
        assert_eq!(Shell.safety_for(&poll), ToolSafety::Safe);
        assert_eq!(call_at(&root, poll, &|| false).unwrap()["exit_code"], 0);
        let empty = call_at(
            &root,
            json!({"cmd":"printf EMPTY_OK","session_id":"","input":null,"yield_ms":1000}),
            &|| false,
        )
        .unwrap();
        assert_eq!(empty["stdout"], "EMPTY_OK");
        assert_eq!(
            call_at(
                &root,
                json!({"session_id":started["session_id"],"yield_ms":0}),
                &|| false
            )
            .unwrap()["exit_code"],
            0
        );
        assert_eq!(
            call_at(
                &root,
                json!({"cmd":"","session_id":empty["session_id"],"yield_ms":0}),
                &|| false
            )
            .unwrap()["exit_code"],
            0
        );
        for session in [&started["session_id"], &empty["session_id"]] {
            let _ = call_at(
                &root,
                json!({"session_id":session,"close":true,"yield_ms":0}),
                &|| false,
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
