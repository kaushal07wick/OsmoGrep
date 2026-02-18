use std::{
    env,
    fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
        Arc,
    },
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tools::{ToolRegistry, ToolSafety};

#[derive(Debug)]
pub enum AgentEvent {
    ToolCall { name: String, args: Value },
    ToolResult { summary: String },
    ToolDiff {
        tool: String,
        target: String,
        before: String,
        after: String,
    },
    OutputText(String),
    StreamDelta(String),
    StreamDone,
    PermissionRequest {
        tool_name: String,
        args_summary: String,
        reply_tx: Sender<bool>,
    },
    ConversationUpdate(Vec<Value>),
    Cancelled,
    Error(String),
    Done,
}

#[derive(Clone)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    api_key: String,
}

fn config_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("osmogrep");
    dir.push("config.toml");
    dir
}

fn load_config() -> Option<Config> {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
}

fn save_config(cfg: &Config) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string(cfg).unwrap())
}

fn system_prompt() -> Value {
    json!({
        "role": "system",
        "content":
            "You are Osmogrep, an AI coding agent working inside this repository.\n\
            \n\
            This repository may include a machine-generated index stored as a file named `.context.json` at the repository root.\n\
            This file, if present, describes the structure of the codebase: files, symbols, and call relationships.\n\
            \n\
            The index is a regular file and must be discovered and read using tools.\n\
            It may or may not exist.\n\
            \n\
            When working:\n\
            - Check for the presence of `.context.json` using tools when repository structure matters.\n\
            - If present, read it to understand the repository before acting.\n\
            - Use tools to inspect other files or make changes as needed.\n\
            - If `.context.json` is missing or insufficient, proceed normally and use tools freely.\n\
            \n\
            Philosophy:\n\
            This repository will outlive any single contributor.\n\
            Many people will read, use, and maintain this code over time.\n\
            Leave the codebase better than you found it.\n\
            Prefer clear structure, small deterministic functions, and reliable behavior.\n\
            Make changes that are easy to understand, review, and maintain."
    })
}

pub struct Agent {
    model: String,
    api_key: Option<String>,
}

impl Agent {
    pub fn new() -> Self {
        let api_key = env::var("OPENAI_API_KEY")
            .ok()
            .or_else(|| load_config().map(|c| c.api_key));

        let model = env::var("OSMOGREP_MODEL").unwrap_or_else(|_| "gpt-5.2".to_string());

        Self { model, api_key }
    }

    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    pub fn set_api_key(&mut self, key: String) {
        let key = key.trim().to_string();
        if key.is_empty() {
            return;
        }

        self.api_key = Some(key.clone());
        let _ = save_config(&Config { api_key: key });
    }

    pub fn spawn(
        &self,
        repo_root: PathBuf,
        user_text: String,
        prior_messages: Vec<Value>,
        auto_approve: bool,
        tx: Sender<AgentEvent>,
    ) -> CancelToken {
        let model = self.model.clone();
        let api_key = self.api_key.clone();
        let cancel = CancelToken::new();
        let cancel_worker = cancel.clone();

        thread::spawn(move || {
            let runner = RunAgent {
                tools: ToolRegistry::new(),
                model,
                api_key,
                auto_approve,
                cancel: cancel_worker.clone(),
            };

            if let Err(e) = runner.run(repo_root, &user_text, prior_messages, &tx) {
                if e == "cancelled" {
                    let _ = tx.send(AgentEvent::Cancelled);
                } else {
                    let _ = tx.send(AgentEvent::Error(e));
                }
            }

            let _ = tx.send(AgentEvent::Done);
        });

        cancel
    }
}

struct RunAgent {
    tools: ToolRegistry,
    model: String,
    api_key: Option<String>,
    auto_approve: bool,
    cancel: CancelToken,
}

impl RunAgent {
    fn run(
        &self,
        _repo_root: PathBuf,
        user_text: &str,
        prior_messages: Vec<Value>,
        tx: &Sender<AgentEvent>,
    ) -> Result<(), String> {
        let api_key = self.api_key.as_ref().ok_or("OPENAI_API_KEY not set")?;

        let mut persisted = if prior_messages.is_empty() {
            vec![system_prompt()]
        } else {
            prior_messages
        };
        persisted.push(json!({ "role": "user", "content": user_text }));

        let mut input = Value::Array(persisted.clone());

        loop {
            if self.cancel.is_cancelled() {
                return Err("cancelled".into());
            }

            let resp = self.call_openai_with_retry(api_key, &input, tx)?;

            let output = resp
                .get("output")
                .and_then(Value::as_array)
                .ok_or("missing output array")?;

            let mut next_messages = input
                .as_array()
                .ok_or("input must be message array")?
                .clone();

            let mut saw_tool = false;

            for item in output {
                if self.cancel.is_cancelled() {
                    return Err("cancelled".into());
                }

                match item.get("type").and_then(Value::as_str) {
                    Some("reasoning") => {
                        next_messages.push(item.clone());
                    }

                    Some("function_call") => {
                        saw_tool = true;

                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .ok_or("function_call missing name")?
                            .to_string();

                        let call_id = item
                            .get("call_id")
                            .cloned()
                            .ok_or("function_call missing call_id")?;

                        let args: Value = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or_else(|| json!({}));

                        let _ = tx.send(AgentEvent::ToolCall {
                            name: name.clone(),
                            args: args.clone(),
                        });

                        if self.tools.safety(&name) == Some(ToolSafety::Dangerous)
                            && !self.auto_approve
                        {
                            let (reply_tx, reply_rx) = mpsc::channel::<bool>();
                            let _ = tx.send(AgentEvent::PermissionRequest {
                                tool_name: name.clone(),
                                args_summary: summarize_args(&name, &args),
                                reply_tx,
                            });

                            let allow = reply_rx.recv().map_err(|_| "permission channel closed")?;
                            if !allow {
                                next_messages.push(item.clone());
                                next_messages.push(json!({
                                    "type": "function_call_output",
                                    "call_id": call_id,
                                    "output": "{\"error\":\"user denied permission\"}"
                                }));
                                continue;
                            }
                        }

                        let result = self
                            .tools
                            .call(&name, args)
                            .unwrap_or_else(|e| json!({ "error": e }));

                        if let (Some(before), Some(after), Some(path)) = (
                            result.get("before").and_then(Value::as_str),
                            result.get("after").and_then(Value::as_str),
                            result.get("path").and_then(Value::as_str),
                        ) {
                            let _ = tx.send(AgentEvent::ToolDiff {
                                tool: name.clone(),
                                target: path.to_string(),
                                before: before.to_string(),
                                after: after.to_string(),
                            });
                        }

                        let summary = result
                            .get("mode")
                            .and_then(Value::as_str)
                            .map(|m| format!("edit ({})", m))
                            .unwrap_or_else(|| "ok".into());

                        let _ = tx.send(AgentEvent::ToolResult { summary });

                        next_messages.push(item.clone());
                        next_messages.push(json!({
                            "type": "function_call_output",
                            "call_id": call_id,
                            "output": serde_json::to_string(&result).unwrap()
                        }));
                    }

                    Some("output_text") => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            let _ = tx.send(AgentEvent::OutputText(text.to_string()));
                            persisted.push(json!({"role": "assistant", "content": text}));
                            let _ = tx.send(AgentEvent::ConversationUpdate(persisted));
                            return Ok(());
                        }
                    }

                    Some("message") => {
                        if let Some(content) = item.get("content").and_then(Value::as_array) {
                            for c in content {
                                if c.get("type").and_then(Value::as_str) == Some("output_text") {
                                    if let Some(text) = c.get("text").and_then(Value::as_str) {
                                        let _ = tx.send(AgentEvent::OutputText(text.to_string()));
                                        persisted.push(json!({"role": "assistant", "content": text}));
                                        let _ = tx.send(AgentEvent::ConversationUpdate(persisted));
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }

            if !saw_tool {
                return Err("model returned no tool calls and no output_text".into());
            }

            input = Value::Array(next_messages);
        }
    }

    fn call_openai_with_retry(
        &self,
        api_key: &str,
        input: &Value,
        tx: &Sender<AgentEvent>,
    ) -> Result<Value, String> {
        let mut last_err = None;

        for attempt in 1..=3 {
            if self.cancel.is_cancelled() {
                return Err("cancelled".into());
            }

            let streaming_disabled = env_truthy("OSMOGREP_NO_STREAM", false);
            let result = if streaming_disabled {
                self.call_openai_blocking(api_key, input)
            } else {
                self.call_openai_streaming(api_key, input, tx)
            };

            match result {
                Ok(v) => return Ok(v),
                Err(e) => {
                    last_err = Some(e.clone());
                    if attempt < 3 {
                        thread::sleep(Duration::from_millis(350 * attempt as u64));
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| "unknown API error".into()))
    }

    fn call_openai_blocking(
        &self,
        api_key: &str,
        input: &Value,
    ) -> Result<Value, String> {
        let payload = json!({
            "model": self.model,
            "input": input,
            "tools": self.tools.schema(),
            "tool_choice": "auto",
            "reasoning": { "effort": "medium" },
            "store": true
        });

        let mut child = Command::new("curl")
            .arg("-s")
            .arg("-w")
            .arg("\n%{http_code}")
            .arg("-X")
            .arg("POST")
            .arg("https://api.openai.com/v1/responses")
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", api_key))
            .arg("--data-binary")
            .arg(payload.to_string())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        loop {
            if self.cancel.is_cancelled() {
                let _ = child.kill();
                return Err("cancelled".into());
            }

            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(e.to_string()),
            }
        }

        let mut out = String::new();
        child
            .stdout
            .take()
            .ok_or("missing stdout")?
            .read_to_string(&mut out)
            .map_err(|e| e.to_string())?;

        let (body, status) = out.rsplit_once('\n').ok_or("missing status")?;
        if status != "200" {
            return Err(format!("API error {}", status));
        }

        serde_json::from_str(body).map_err(|e| e.to_string())
    }

    fn call_openai_streaming(
        &self,
        api_key: &str,
        input: &Value,
        tx: &Sender<AgentEvent>,
    ) -> Result<Value, String> {
        let payload = json!({
            "model": self.model,
            "input": input,
            "tools": self.tools.schema(),
            "tool_choice": "auto",
            "reasoning": { "effort": "medium" },
            "store": true,
            "stream": true,
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;

        let mut resp = client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("API error {}", resp.status()));
        }

        let mut pending = String::new();
        let mut buf = [0u8; 8192];
        let mut completed_response: Option<Value> = None;

        loop {
            if self.cancel.is_cancelled() {
                return Err("cancelled".into());
            }

            let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }

            pending.push_str(&String::from_utf8_lossy(&buf[..n]));

            while let Some(idx) = pending.find("\n\n") {
                let block = pending[..idx].to_string();
                pending.drain(..idx + 2);

                let mut data_lines = Vec::new();
                for line in block.lines() {
                    if let Some(rest) = line.strip_prefix("data: ") {
                        data_lines.push(rest);
                    }
                }

                if data_lines.is_empty() {
                    continue;
                }

                let data = data_lines.join("\n");
                if data == "[DONE]" {
                    let _ = tx.send(AgentEvent::StreamDone);
                    if let Some(full) = completed_response {
                        return Ok(full);
                    }
                    return Err("stream ended without completed response".into());
                }

                let event: Value = match serde_json::from_str(&data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                match event.get("type").and_then(Value::as_str) {
                    Some("response.output_text.delta") => {
                        if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                            let _ = tx.send(AgentEvent::StreamDelta(delta.to_string()));
                        }
                    }
                    Some("response.completed") => {
                        if let Some(response) = event.get("response") {
                            completed_response = Some(response.clone());
                        }
                    }
                    Some("error") => {
                        if let Some(msg) = event
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(Value::as_str)
                        {
                            return Err(msg.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        let _ = tx.send(AgentEvent::StreamDone);
        completed_response.ok_or_else(|| "stream ended without response.completed".into())
    }
}

fn summarize_args(tool: &str, args: &Value) -> String {
    match tool {
        "run_shell" => args
            .get("cmd")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "write_file" | "edit_file" | "read_file" => args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string()),
    }
}

fn env_truthy(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(val) => matches!(val.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}
