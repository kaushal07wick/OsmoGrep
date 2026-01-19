use std::{
    env,
    fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::Sender,
    thread,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tools::ToolRegistry;

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
    Error(String),
    Done,
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

pub struct Agent {
    model: String,
    api_key: Option<String>,
}

impl Agent {
    pub fn new() -> Self {
        let api_key = env::var("OPENAI_API_KEY")
            .ok()
            .or_else(|| load_config().map(|c| c.api_key));

        Self {
            model: "gpt-5.2".to_string(),
            api_key,
        }
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
        tx: Sender<AgentEvent>,
    ) {
        let model = self.model.clone();
        let api_key = self.api_key.clone();

        thread::spawn(move || {
            let runner = RunAgent {
                tools: ToolRegistry::new(),
                model,
                api_key,
            };

            if let Err(e) = runner.run(repo_root, &user_text, &tx) {
                let _ = tx.send(AgentEvent::Error(e));
            }

            let _ = tx.send(AgentEvent::Done);
        });
    }
}

struct RunAgent {
    tools: ToolRegistry,
    model: String,
    api_key: Option<String>,
}

impl RunAgent {
    fn run(
        &self,
        _repo_root: PathBuf,
        user_text: &str,
        tx: &Sender<AgentEvent>,
    ) -> Result<(), String> {
        let api_key = self.api_key.as_ref().ok_or("OPENAI_API_KEY not set")?;

        let mut input = json!([
            {
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
            },
            {
                "role": "user",
                "content": user_text
            }
        ]);

        loop {
            let resp = self.call_openai(api_key, &input)?;

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

                        // Keep ToolResult for logs / status line
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
                            return Ok(());
                        }
                    }

                    Some("message") => {
                        if let Some(content) = item.get("content").and_then(Value::as_array) {
                            for c in content {
                                if c.get("type").and_then(Value::as_str) == Some("output_text") {
                                    if let Some(text) = c.get("text").and_then(Value::as_str) {
                                        let _ = tx.send(AgentEvent::OutputText(text.to_string()));
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

    fn call_openai(
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

        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();

        let (body, status) = out.rsplit_once('\n').ok_or("missing status")?;
        if status != "200" {
            return Err(format!("API error {}", status));
        }

        serde_json::from_str(body).map_err(|e| e.to_string())
    }
}
