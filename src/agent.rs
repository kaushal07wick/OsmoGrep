use std::{
    env, fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::state::PermissionProfile;
use crate::tools::{ToolRegistry, ToolSafety};

#[derive(Debug)]
pub enum AgentEvent {
    ToolCall {
        name: String,
        args: Value,
    },
    ToolResult {
        summary: String,
    },
    ToolDiff {
        tool: String,
        target: String,
        before: String,
        after: String,
    },
    PreviewDiff {
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
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    model: Option<ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-5.2".to_string(),
            api_key_env: Some("OPENAI_API_KEY".to_string()),
            base_url: None,
        }
    }
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

fn repo_instruction_suffix(repo_root: &std::path::Path) -> String {
    let path = repo_root.join(".osmogrep.md");
    if let Ok(text) = fs::read_to_string(path) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return format!(
                "\n\nRepository-specific instructions (.osmogrep.md):\n{}",
                trimmed
            );
        }
    }
    String::new()
}

fn system_prompt(repo_root: &std::path::Path) -> Value {
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
            - Prefer high-leverage workflows over many tiny manual steps.\n\
            - If the user asks for repository triage/review (PRs/issues), proactively use GitHub CLI (`gh`) and local triage tooling when available.\n\
            - For triage-style tasks, gather evidence first, then produce ranked recommendations with clear rationale.\n\
            \n\
            Triage defaults (when user intent is PR/issue triage):\n\
            - Resolve repository via `gh repo view`.\n\
            - Fetch PR/issue snapshots with `gh pr list` / `gh issue list` JSON output.\n\
            - Run `osmogrep triage` for dedupe, scoring, and scope-drift checks.\n\
            - Prefer incremental mode and durable state files under `.context/`.\n\
            - Apply triage actions only when user explicitly asks for auto-apply.\n\
            \n\
            Final response style:\n\
            - Use concise markdown sections with concrete findings.\n\
            - Include top candidates, duplicates, drift/reject risks, and explicit next actions.\n\
            - Be specific with issue/PR IDs and command outputs where relevant.\n\
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
    model_cfg: ModelConfig,
    api_key: Option<String>,
}

pub struct RunControl {
    pub cancel: CancelToken,
    pub steer_tx: Sender<String>,
}

impl Agent {
    pub fn new() -> Self {
        let cfg = load_config();
        let mut model_cfg = cfg
            .as_ref()
            .and_then(|c| c.model.clone())
            .unwrap_or_default();

        if let Ok(model) = env::var("OSMOGREP_MODEL") {
            if !model.trim().is_empty() {
                model_cfg.model = model;
            }
        }
        if let Ok(provider) = env::var("OSMOGREP_PROVIDER") {
            if !provider.trim().is_empty() {
                model_cfg.provider = provider;
            }
        }
        if let Ok(base_url) = env::var("OSMOGREP_BASE_URL") {
            if !base_url.trim().is_empty() {
                model_cfg.base_url = Some(base_url);
            }
        }

        let cfg_api_key = cfg.as_ref().and_then(|c| c.api_key.clone());
        let env_key_name = model_cfg
            .api_key_env
            .clone()
            .unwrap_or_else(default_api_key_env);
        let api_key = env::var(&env_key_name).ok().or(cfg_api_key);

        Self { model_cfg, api_key }
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
        let _ = save_config(&Config {
            api_key: Some(key),
            model: Some(self.model_cfg.clone()),
        });
    }

    pub fn model_config(&self) -> &ModelConfig {
        &self.model_cfg
    }

    pub fn api_key(&self) -> Option<String> {
        self.api_key.clone()
    }

    pub fn set_model_config(&mut self, provider: String, model: String, base_url: Option<String>) {
        self.model_cfg.provider = provider;
        self.model_cfg.model = model;
        self.model_cfg.base_url = base_url;
        self.model_cfg.api_key_env = Some(default_api_key_env_for(&self.model_cfg.provider));

        if let Some(key_name) = self.model_cfg.api_key_env.clone() {
            self.api_key = env::var(key_name).ok().or(self.api_key.clone());
        }

        let _ = save_config(&Config {
            api_key: self.api_key.clone(),
            model: Some(self.model_cfg.clone()),
        });
    }

    pub fn spawn(
        &self,
        repo_root: PathBuf,
        user_text: String,
        prior_messages: Vec<Value>,
        steer: Option<String>,
        permission_profile: PermissionProfile,
        auto_approve: bool,
        tx: Sender<AgentEvent>,
    ) -> RunControl {
        let model_cfg = self.model_cfg.clone();
        let api_key = self.api_key.clone();
        let cancel = CancelToken::new();
        let cancel_worker = cancel.clone();
        let (steer_tx, steer_rx) = mpsc::channel::<String>();

        thread::spawn(move || {
            let runner = RunAgent {
                tools: ToolRegistry::new(),
                model_cfg,
                api_key,
                auto_approve,
                permission_profile,
                cancel: cancel_worker.clone(),
            };

            if let Err(e) = runner.run(repo_root, &user_text, prior_messages, steer, steer_rx, &tx)
            {
                if e == "cancelled" {
                    let _ = tx.send(AgentEvent::Cancelled);
                } else {
                    let _ = tx.send(AgentEvent::Error(e));
                }
            }

            let _ = tx.send(AgentEvent::Done);
        });

        RunControl { cancel, steer_tx }
    }

    pub fn run_swarm(&self, user_text: &str) -> Result<Vec<(String, String)>, String> {
        let api_key = self.api_key.as_ref().ok_or("OPENAI_API_KEY not set")?;
        run_swarm_with(self.model_cfg.clone(), api_key, user_text)
    }
}

struct RunAgent {
    tools: ToolRegistry,
    model_cfg: ModelConfig,
    api_key: Option<String>,
    auto_approve: bool,
    permission_profile: PermissionProfile,
    cancel: CancelToken,
}

impl RunAgent {
    fn run(
        &self,
        repo_root: PathBuf,
        user_text: &str,
        prior_messages: Vec<Value>,
        steer: Option<String>,
        steer_rx: Receiver<String>,
        tx: &Sender<AgentEvent>,
    ) -> Result<(), String> {
        let api_key = self.api_key.as_ref().ok_or("OPENAI_API_KEY not set")?;

        let mut persisted = if prior_messages.is_empty() {
            vec![system_prompt(&repo_root)]
        } else {
            prior_messages
        };
        if let Some(Value::Object(obj)) = persisted.get_mut(0) {
            if let Some(Value::String(content)) = obj.get_mut("content") {
                let suffix = repo_instruction_suffix(&repo_root);
                if !suffix.is_empty() && !content.contains("Repository-specific instructions") {
                    content.push_str(&suffix);
                }
            }
        }
        if let Some(steer_text) = steer.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            persisted.push(json!({
                "role": "system",
                "content": format!("[steer]\n{}", steer_text),
            }));
        }
        persisted.push(json!({ "role": "user", "content": user_text }));

        let mut input = Value::Array(persisted.clone());

        loop {
            apply_pending_steer(&steer_rx, &mut persisted, &mut input, tx);
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
                apply_pending_steer(&steer_rx, &mut persisted, &mut input, tx);
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

                        let dangerous = self.tools.safety(&name) == Some(ToolSafety::Dangerous);
                        if dangerous && self.permission_profile == PermissionProfile::ReadOnly {
                            next_messages.push(item.clone());
                            next_messages.push(json!({
                                "type": "function_call_output",
                                "call_id": call_id,
                                "output": "{\"error\":\"permission profile is read-only\"}"
                            }));
                            continue;
                        }

                        let should_prompt = dangerous
                            && self.permission_profile != PermissionProfile::FullAccess
                            && !self.auto_approve;
                        if should_prompt {
                            if let Some((target, before, after)) =
                                preview_diff_from_args(&name, &args)
                            {
                                let _ = tx.send(AgentEvent::PreviewDiff {
                                    tool: name.clone(),
                                    target,
                                    before,
                                    after,
                                });
                            }

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
                                        persisted
                                            .push(json!({"role": "assistant", "content": text}));
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

    fn call_openai_blocking(&self, api_key: &str, input: &Value) -> Result<Value, String> {
        let payload = json!({
            "model": self.model_cfg.model,
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
            .arg(self.responses_endpoint())
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
            "model": self.model_cfg.model,
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
            .post(self.responses_endpoint())
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

impl RunAgent {
    fn responses_endpoint(&self) -> String {
        let base = self
            .model_cfg
            .base_url
            .clone()
            .unwrap_or_else(|| default_base_url_for(&self.model_cfg.provider));
        format!("{}/responses", base.trim_end_matches('/'))
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
        Ok(val) => matches!(
            val.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn apply_pending_steer(
    steer_rx: &Receiver<String>,
    persisted: &mut Vec<Value>,
    input: &mut Value,
    tx: &Sender<AgentEvent>,
) {
    let mut latest: Option<String> = None;
    while let Ok(s) = steer_rx.try_recv() {
        let t = s.trim();
        if !t.is_empty() {
            latest = Some(t.to_string());
        }
    }

    if let Some(text) = latest {
        persisted.push(json!({
            "role": "system",
            "content": format!("[steer-now]\n{}", text),
        }));
        *input = Value::Array(persisted.clone());
        let preview = if text.chars().count() > 88 {
            let mut s: String = text.chars().take(87).collect();
            s.push('…');
            s
        } else {
            text
        };
        let _ = tx.send(AgentEvent::ToolResult {
            summary: format!("steer applied live: {}", preview),
        });
    }
}

fn run_swarm_with(
    model_cfg: ModelConfig,
    api_key: &str,
    user_text: &str,
) -> Result<Vec<(String, String)>, String> {
    let scopes = [
        (
            "explore",
            "Map relevant files/modules and explain what to inspect first.",
        ),
        (
            "edit",
            "Propose concrete code changes with minimal, safe patches.",
        ),
        (
            "test",
            "Design targeted tests and validation sequence for the requested change.",
        ),
        (
            "review",
            "Find likely regressions, edge-cases, and approval/blocking risks.",
        ),
    ];

    let mut handles = Vec::new();
    for (name, scope_prompt) in scopes {
        let cfg = model_cfg.clone();
        let key = api_key.to_string();
        let user = user_text.to_string();
        let scope = scope_prompt.to_string();
        let role = name.to_string();
        handles.push(thread::spawn(move || {
            let text = one_shot_scoped_call(&cfg, &key, &user, &scope)?;
            Ok::<(String, String), String>((role, text))
        }));
    }

    let mut out = Vec::new();
    for h in handles {
        let res = h
            .join()
            .map_err(|_| "swarm thread panicked".to_string())??;
        out.push(res);
    }
    Ok(out)
}

pub fn run_swarm_job(
    model_cfg: ModelConfig,
    api_key: String,
    user_text: String,
) -> Result<String, String> {
    let mut parts = run_swarm_with(model_cfg, &api_key, &user_text)?;
    parts.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (idx, (role, text)) in parts.iter().enumerate() {
        if idx > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&format!("[{}]\n{}", role, text));
    }
    Ok(out)
}

fn one_shot_scoped_call(
    model_cfg: &ModelConfig,
    api_key: &str,
    user_text: &str,
    scope_prompt: &str,
) -> Result<String, String> {
    let payload = json!({
        "model": model_cfg.model,
        "input": [
            {
                "role": "system",
                "content": format!(
                    "You are a focused coding sub-agent.\nScope: {}\nBe concise and concrete.",
                    scope_prompt
                )
            },
            {"role": "user", "content": user_text}
        ],
        "reasoning": {"effort": "medium"},
        "store": false
    });

    let base = model_cfg
        .base_url
        .clone()
        .unwrap_or_else(|| default_base_url_for(&model_cfg.provider));
    let endpoint = format!("{}/responses", base.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(endpoint)
        .bearer_auth(api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("API error {}", resp.status()));
    }

    let value: Value = resp.json().map_err(|e| e.to_string())?;
    extract_response_text(&value).ok_or_else(|| "no output_text in swarm response".to_string())
}

fn extract_response_text(value: &Value) -> Option<String> {
    let output = value.get("output")?.as_array()?;
    for item in output {
        if item.get("type").and_then(Value::as_str) == Some("output_text") {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                return Some(text.to_string());
            }
        }
        if item.get("type").and_then(Value::as_str) == Some("message") {
            let content = item.get("content").and_then(Value::as_array)?;
            for c in content {
                if c.get("type").and_then(Value::as_str) == Some("output_text") {
                    if let Some(text) = c.get("text").and_then(Value::as_str) {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}

fn preview_diff_from_args(name: &str, args: &Value) -> Option<(String, String, String)> {
    match name {
        "edit_file" => {
            let path = args.get("path").and_then(Value::as_str)?;
            let old = args.get("old").and_then(Value::as_str)?;
            let new = args.get("new").and_then(Value::as_str)?;
            let all = args
                .get("all_occ")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let before = fs::read_to_string(path).ok()?;
            if !before.contains(old) {
                return None;
            }
            let after = if all {
                before.replace(old, new)
            } else {
                before.replacen(old, new, 1)
            };
            Some((path.to_string(), before, after))
        }
        "write_file" => {
            let path = args.get("path").and_then(Value::as_str)?;
            let content = args.get("content").and_then(Value::as_str)?;
            let before = fs::read_to_string(path).unwrap_or_default();
            Some((path.to_string(), before, content.to_string()))
        }
        _ => None,
    }
}

fn default_base_url_for(provider: &str) -> String {
    match provider {
        "openai" => "https://api.openai.com/v1".to_string(),
        "groq" => "https://api.groq.com/openai/v1".to_string(),
        "mistral" => "https://api.mistral.ai/v1".to_string(),
        "ollama" => "http://127.0.0.1:11434/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

fn default_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_api_key_env_for(provider: &str) -> String {
    match provider {
        "openai" => "OPENAI_API_KEY".to_string(),
        "groq" => "GROQ_API_KEY".to_string(),
        "mistral" => "MISTRAL_API_KEY".to_string(),
        "ollama" => "OLLAMA_API_KEY".to_string(),
        _ => "OPENAI_API_KEY".to_string(),
    }
}
