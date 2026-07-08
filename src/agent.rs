use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::harness::{clip, RunLedger};
use crate::state::{DiffSnapshot, PermissionProfile};
use crate::tool_guard::ToolLoopGuard;
use crate::tools::{ToolRegistry, ToolSafety, ToolScope};
use crate::worktree::{run_worktree_swarm, WorktreeSwarmResult};

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
    RunStatus {
        phase: String,
        detail: String,
        iteration: usize,
        max_iterations: usize,
    },
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

const REPO_INSTRUCTION_BUDGET: usize = 32 * 1024;
const REPO_INSTRUCTION_FILES: &[&str] = &[".osmogrep.md", "AGENTS.md", "CLAUDE.md", ".cursorrules"];

fn repo_instruction_suffix(repo_root: &std::path::Path) -> String {
    let mut blocks = Vec::new();
    let mut remaining = REPO_INSTRUCTION_BUDGET;

    for name in REPO_INSTRUCTION_FILES {
        if remaining == 0 {
            break;
        }
        let path = repo_root.join(name);
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }

        let header = format!("Repository instructions ({name}):\n");
        if header.len() >= remaining {
            break;
        }
        remaining -= header.len();

        let body = take_chars(trimmed, remaining);
        remaining = remaining.saturating_sub(body.len());
        blocks.push(format!("{header}{body}"));
    }

    if blocks.is_empty() {
        return String::new();
    }

    let mut suffix = format!("\n\n{}", blocks.join("\n\n"));
    if remaining == 0 {
        suffix.push_str(
            "\n\n[Repository instruction budget exhausted; remaining instruction content omitted.]",
        );
    }
    suffix
}

fn take_chars(text: &str, max_bytes: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.len() + ch.len_utf8() > max_bytes {
            break;
        }
        out.push(ch);
    }
    out
}

fn system_prompt(repo_root: &std::path::Path) -> Value {
    let mut content =
        "You are Osmogrep, an AI coding agent working inside this repository.\n\
            \n\
            This repository may include a machine-generated index stored as `.context/context.json` at the repository root.\n\
            This file, if present, describes the structure of the codebase: files, symbols, and call relationships.\n\
            \n\
            The index is a regular file and must be discovered and read using tools.\n\
            It may or may not exist.\n\
            \n\
            When working:\n\
            - Check for the presence of `.context/context.json` using tools when repository structure matters.\n\
            - If present, read it to understand the repository before acting.\n\
            - Use tools to inspect other files or make changes as needed.\n\
            - If `.context/context.json` is missing or insufficient, proceed normally and use tools freely.\n\
            - Prefer high-leverage workflows over many tiny manual steps.\n\
            - For multi-step work, keep a durable progress plan with `update_plan`; treat it as scratchpad memory and verify against real files before acting.\n\
            - For non-trivial coding work, run a disciplined loop: investigate, make a short plan, implement the approved slice, verify, then review residual risk.\n\
            - Treat tests, builds, diffs, and command outputs as evidence. Do not claim the repo is green unless a relevant command actually passed.\n\
            - If verification is not possible, say exactly what was not run and why.\n\
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
            .to_string();

    let workspace = workspace_fact_block(repo_root);
    if !workspace.is_empty() {
        content.push_str("\n\n");
        content.push_str(&workspace);
    }

    json!({
        "role": "system",
        "content": content
    })
}

fn workspace_fact_block(repo_root: &std::path::Path) -> String {
    let manifests = detect_manifests(repo_root);
    let verify_commands = detect_verify_commands(repo_root);
    if manifests.is_empty() && verify_commands.is_empty() {
        return String::new();
    }

    let mut lines = vec!["Workspace facts (snapshot at run start):".to_string()];
    lines.push(format!("- Root: {}", repo_root.display()));
    if !manifests.is_empty() {
        lines.push(format!("- Manifests: {}", manifests.join(", ")));
    }
    if !verify_commands.is_empty() {
        lines.push(format!(
            "- Likely verify commands: {}",
            verify_commands.join("; ")
        ));
    }
    lines.push(
        "- Re-check files before relying on this snapshot; treat it as a starting point."
            .to_string(),
    );
    lines.join("\n")
}

fn detect_manifests(repo_root: &std::path::Path) -> Vec<String> {
    [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "pytest.ini",
        "go.mod",
        "Makefile",
        "Dockerfile",
    ]
    .iter()
    .filter(|name| repo_root.join(name).is_file())
    .map(|name| (*name).to_string())
    .collect()
}

fn detect_verify_commands(repo_root: &std::path::Path) -> Vec<String> {
    let mut commands = Vec::new();
    if repo_root.join("Cargo.toml").is_file() {
        push_unique(&mut commands, "cargo test --color never");
        push_unique(&mut commands, "cargo check");
    }
    if repo_root.join("go.mod").is_file() {
        push_unique(&mut commands, "go test ./...");
    }
    if repo_root.join("pytest.ini").is_file()
        || repo_root.join("pyproject.toml").is_file()
        || repo_root.join("tests").is_dir()
    {
        push_unique(&mut commands, "pytest -q");
    }
    if repo_root.join("package.json").is_file() {
        for command in package_verify_commands(repo_root) {
            push_unique(&mut commands, &command);
        }
    }
    if repo_root.join("Makefile").is_file() {
        for command in make_verify_commands(repo_root) {
            push_unique(&mut commands, &command);
        }
    }
    commands.truncate(10);
    commands
}

fn package_verify_commands(repo_root: &std::path::Path) -> Vec<String> {
    let text = fs::read_to_string(repo_root.join("package.json")).unwrap_or_default();
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = value.get("scripts").and_then(Value::as_object) else {
        return Vec::new();
    };
    let package_manager = if repo_root.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if repo_root.join("bun.lockb").is_file() || repo_root.join("bun.lock").is_file() {
        "bun"
    } else if repo_root.join("yarn.lock").is_file() {
        "yarn"
    } else {
        "npm"
    };
    ["test", "lint", "typecheck", "check", "build"]
        .iter()
        .filter(|name| scripts.contains_key(**name))
        .map(|name| format!("{package_manager} run {name}"))
        .collect()
}

fn make_verify_commands(repo_root: &std::path::Path) -> Vec<String> {
    let text = fs::read_to_string(repo_root.join("Makefile")).unwrap_or_default();
    ["test", "lint", "typecheck", "check", "build"]
        .iter()
        .filter(|target| {
            text.lines().any(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with(&format!("{target}:"))
                    || trimmed.starts_with(&format!("{target} :"))
            })
        })
        .map(|target| format!("make {target}"))
        .collect()
}

fn push_unique(items: &mut Vec<String>, item: &str) {
    if !items.iter().any(|existing| existing == item) {
        items.push(item.to_string());
    }
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
                tools: ToolRegistry::with_root(repo_root.clone()),
                tool_scope: ToolScope::for_prompt(&user_text),
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

    pub fn run_worktree_swarm(
        &self,
        repo_root: &Path,
        user_text: &str,
    ) -> Result<Vec<WorktreeSwarmResult>, String> {
        let _ = self.api_key.as_ref().ok_or("OPENAI_API_KEY not set")?;
        run_worktree_swarm(repo_root, user_text)
    }
}

struct RunAgent {
    tools: ToolRegistry,
    tool_scope: ToolScope,
    model_cfg: ModelConfig,
    api_key: Option<String>,
    auto_approve: bool,
    permission_profile: PermissionProfile,
    cancel: CancelToken,
}

#[derive(Clone, Debug)]
struct ToolInvocation {
    item: Value,
    name: String,
    call_id: Value,
    args: Value,
    args_summary: String,
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
        let max_iterations = max_iterations();
        let mut iteration = 0usize;
        let mut run_notes: Vec<String> = Vec::new();
        let mut tool_guard = ToolLoopGuard::default();
        let mut verify_on_stop_attempts = 0usize;
        let mut ledger = RunLedger::start(
            &repo_root,
            user_text,
            &self.model_cfg,
            self.permission_profile,
            max_iterations,
        );

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

        'agent_loop: loop {
            apply_pending_steer(&steer_rx, &mut persisted, &mut input, tx);
            if self.cancel.is_cancelled() {
                ledger.error("cancelled", iteration);
                return Err("cancelled".into());
            }

            iteration += 1;
            if iteration > max_iterations {
                let msg = format!("iteration budget exceeded ({max_iterations})");
                ledger.error(&msg, iteration);
                return Err(msg);
            }

            send_run_status(
                tx,
                "thinking",
                format!("model turn {iteration}/{max_iterations}"),
                iteration,
                max_iterations,
            );
            ledger.status(
                "thinking",
                format!("model turn {iteration}/{max_iterations}"),
                iteration,
            );

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

            let mut idx = 0usize;
            while idx < output.len() {
                apply_pending_steer(&steer_rx, &mut persisted, &mut input, tx);
                if self.cancel.is_cancelled() {
                    ledger.error("cancelled", iteration);
                    return Err("cancelled".into());
                }

                let batch_start = idx;
                let item = output[idx].clone();
                idx += 1;

                match item.get("type").and_then(Value::as_str) {
                    Some("reasoning") => {
                        next_messages.push(item.clone());
                    }

                    Some("function_call") => {
                        let batch = collect_parallel_safe_batch(output, batch_start, &self.tools)?;
                        if batch.len() > 1 {
                            let batch_len = batch.len();
                            saw_tool = true;
                            self.execute_parallel_safe_batch(
                                batch,
                                tx,
                                &mut ledger,
                                &mut tool_guard,
                                &mut run_notes,
                                &mut next_messages,
                                iteration,
                                max_iterations,
                            );
                            idx = batch_start + batch_len;
                            continue;
                        }

                        saw_tool = true;

                        let invocation = parse_tool_invocation(&item)?;
                        let name = invocation.name;
                        let call_id = invocation.call_id;
                        let args = invocation.args;
                        let args_summary = invocation.args_summary;

                        let _ = tx.send(AgentEvent::ToolCall {
                            name: name.clone(),
                            args: args.clone(),
                        });
                        send_run_status(
                            tx,
                            "tool",
                            format!("{name} {args_summary}"),
                            iteration,
                            max_iterations,
                        );
                        ledger.tool_started(&name, &args_summary, iteration);

                        let dangerous = self.tools.safety(&name) == Some(ToolSafety::Dangerous);
                        if dangerous && self.permission_profile == PermissionProfile::ReadOnly {
                            ledger.permission(&name, "blocked-read-only", iteration);
                            run_notes.push(format!(
                                "- blocked `{name}` ({args_summary}): permission profile is read-only"
                            ));
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
                                args_summary: args_summary.clone(),
                                reply_tx,
                            });

                            let allow = reply_rx.recv().map_err(|_| "permission channel closed")?;
                            if !allow {
                                ledger.permission(&name, "denied", iteration);
                                run_notes.push(format!(
                                    "- denied `{name}` ({args_summary}): user denied permission"
                                ));
                                next_messages.push(item.clone());
                                next_messages.push(json!({
                                    "type": "function_call_output",
                                    "call_id": call_id,
                                    "output": "{\"error\":\"user denied permission\"}"
                                }));
                                continue;
                            }
                            ledger.permission(&name, "approved", iteration);
                        }

                        let started = Instant::now();
                        let mut result = self
                            .tools
                            .call(&name, args.clone())
                            .unwrap_or_else(|e| json!({ "error": e }));
                        let loop_warning = tool_guard.after_call(&name, &args, &result);
                        if let Some(warning) = loop_warning.as_ref() {
                            if let Some(map) = result.as_object_mut() {
                                let warning_value =
                                    serde_json::to_value(warning).unwrap_or_else(|_| json!(null));
                                map.insert("tool_loop_warning".to_string(), warning_value);
                            }
                        }
                        let duration_ms = started.elapsed().as_millis();
                        let ok = result.get("error").is_none();

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

                        let mut summary = tool_result_summary(&name, &result);
                        if let Some(warning) = loop_warning.as_ref() {
                            summary = format!("{summary}; {}", warning.message);
                        }
                        let status = if ok { "ok" } else { "error" };
                        ledger.tool_finished(
                            &name,
                            status,
                            summary.clone(),
                            duration_ms,
                            iteration,
                        );
                        send_run_status(
                            tx,
                            if ok { "tool_done" } else { "tool_error" },
                            format!("{name} {status} in {duration_ms}ms"),
                            iteration,
                            max_iterations,
                        );
                        run_notes.push(format!(
                            "- {status} `{name}` ({args_summary}) in {duration_ms}ms: {}",
                            clip(&summary)
                        ));

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
                            if queue_verify_on_stop(
                                &repo_root,
                                &mut verify_on_stop_attempts,
                                text,
                                iteration,
                                &mut ledger,
                                &mut run_notes,
                                &mut next_messages,
                            ) {
                                input = Value::Array(next_messages);
                                continue 'agent_loop;
                            }
                            let _ = tx.send(AgentEvent::OutputText(text.to_string()));
                            ledger.final_text(text, iteration);
                            persisted.push(json!({
                                "role": "assistant",
                                "content": assistant_memory_text(text, &run_notes, &ledger)
                            }));
                            let _ = tx.send(AgentEvent::ConversationUpdate(persisted));
                            return Ok(());
                        }
                    }

                    Some("message") => {
                        if let Some(content) = item.get("content").and_then(Value::as_array) {
                            for c in content {
                                if c.get("type").and_then(Value::as_str) == Some("output_text") {
                                    if let Some(text) = c.get("text").and_then(Value::as_str) {
                                        if queue_verify_on_stop(
                                            &repo_root,
                                            &mut verify_on_stop_attempts,
                                            text,
                                            iteration,
                                            &mut ledger,
                                            &mut run_notes,
                                            &mut next_messages,
                                        ) {
                                            input = Value::Array(next_messages);
                                            continue 'agent_loop;
                                        }
                                        let _ = tx.send(AgentEvent::OutputText(text.to_string()));
                                        ledger.final_text(text, iteration);
                                        persisted.push(json!({
                                            "role": "assistant",
                                            "content": assistant_memory_text(
                                                text,
                                                &run_notes,
                                                &ledger
                                            )
                                        }));
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
                ledger.error("model returned no tool calls and no output_text", iteration);
                return Err("model returned no tool calls and no output_text".into());
            }

            input = Value::Array(next_messages);
        }
    }

    fn execute_parallel_safe_batch(
        &self,
        batch: Vec<ToolInvocation>,
        tx: &Sender<AgentEvent>,
        ledger: &mut RunLedger,
        tool_guard: &mut ToolLoopGuard,
        run_notes: &mut Vec<String>,
        next_messages: &mut Vec<Value>,
        iteration: usize,
        max_iterations: usize,
    ) {
        send_run_status(
            tx,
            "tool",
            format!("parallel read-only batch ({} tools)", batch.len()),
            iteration,
            max_iterations,
        );

        for invocation in &batch {
            let _ = tx.send(AgentEvent::ToolCall {
                name: invocation.name.clone(),
                args: invocation.args.clone(),
            });
            ledger.tool_started(&invocation.name, &invocation.args_summary, iteration);
        }

        let results = thread::scope(|scope| {
            let mut handles = Vec::new();
            for invocation in &batch {
                let tools = &self.tools;
                let name = invocation.name.clone();
                let args = invocation.args.clone();
                handles.push(scope.spawn(move || {
                    let started = Instant::now();
                    let result = tools
                        .call_parallel_safe(&name, args)
                        .unwrap_or_else(|e| json!({ "error": e }));
                    (result, started.elapsed().as_millis())
                }));
            }

            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| (json!({ "error": "parallel tool panicked" }), 0))
                })
                .collect::<Vec<_>>()
        });

        for (invocation, (mut result, duration_ms)) in batch.into_iter().zip(results) {
            let loop_warning = tool_guard.after_call(&invocation.name, &invocation.args, &result);
            if let Some(warning) = loop_warning.as_ref() {
                if let Some(map) = result.as_object_mut() {
                    let warning_value =
                        serde_json::to_value(warning).unwrap_or_else(|_| json!(null));
                    map.insert("tool_loop_warning".to_string(), warning_value);
                }
            }
            let ok = result.get("error").is_none();

            if let (Some(before), Some(after), Some(path)) = (
                result.get("before").and_then(Value::as_str),
                result.get("after").and_then(Value::as_str),
                result.get("path").and_then(Value::as_str),
            ) {
                let _ = tx.send(AgentEvent::ToolDiff {
                    tool: invocation.name.clone(),
                    target: path.to_string(),
                    before: before.to_string(),
                    after: after.to_string(),
                });
            }

            let mut summary = tool_result_summary(&invocation.name, &result);
            if let Some(warning) = loop_warning.as_ref() {
                summary = format!("{summary}; {}", warning.message);
            }
            let status = if ok { "ok" } else { "error" };
            ledger.tool_finished(
                &invocation.name,
                status,
                summary.clone(),
                duration_ms,
                iteration,
            );
            send_run_status(
                tx,
                if ok { "tool_done" } else { "tool_error" },
                format!("{} {status} in {duration_ms}ms", invocation.name),
                iteration,
                max_iterations,
            );
            run_notes.push(format!(
                "- {status} `{}` ({}) in {duration_ms}ms: {}",
                invocation.name,
                invocation.args_summary,
                clip(&summary)
            ));

            let _ = tx.send(AgentEvent::ToolResult { summary });

            next_messages.push(invocation.item);
            next_messages.push(json!({
                "type": "function_call_output",
                "call_id": invocation.call_id,
                "output": serde_json::to_string(&result).unwrap()
            }));
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
        let payload = self.responses_payload(input, false);

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
            return Err(format_api_error(status, body));
        }

        serde_json::from_str(body).map_err(|e| e.to_string())
    }

    fn responses_payload(&self, input: &Value, stream: bool) -> Value {
        let mut payload = json!({
            "model": self.model_cfg.model,
            "input": input,
            "tools": self.tools.scoped_schema(&self.tool_scope),
            "tool_choice": "auto",
            "store": true
        });

        if let Some(effort) = reasoning_effort_for(&self.model_cfg.model) {
            payload["reasoning"] = json!({ "effort": effort });
        }

        if stream {
            payload["stream"] = json!(true);
        }

        payload
    }

    fn call_openai_streaming(
        &self,
        api_key: &str,
        input: &Value,
        tx: &Sender<AgentEvent>,
    ) -> Result<Value, String> {
        let payload = self.responses_payload(input, true);

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

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(format_api_error(&status.to_string(), &body));
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

fn collect_parallel_safe_batch(
    output: &[Value],
    start: usize,
    tools: &ToolRegistry,
) -> Result<Vec<ToolInvocation>, String> {
    let Some(first) = output.get(start) else {
        return Ok(Vec::new());
    };
    if first.get("type").and_then(Value::as_str) != Some("function_call") {
        return Ok(Vec::new());
    }

    let first = parse_tool_invocation(first)?;
    if !tools.parallel_safe(&first.name) {
        return Ok(Vec::new());
    }

    let mut batch = vec![first];
    let mut idx = start + 1;
    while let Some(item) = output.get(idx) {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            break;
        }

        let Ok(invocation) = parse_tool_invocation(item) else {
            break;
        };
        if !tools.parallel_safe(&invocation.name) {
            break;
        }

        batch.push(invocation);
        idx += 1;
    }

    Ok(batch)
}

fn parse_tool_invocation(item: &Value) -> Result<ToolInvocation, String> {
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
    let args_summary = summarize_args(&name, &args);

    Ok(ToolInvocation {
        item: item.clone(),
        name,
        call_id,
        args,
        args_summary,
    })
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

fn tool_result_summary(tool: &str, result: &Value) -> String {
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        return format!("error: {}", clip(error));
    }
    if let Some(summary) = verification_summary(result) {
        return summary;
    }
    if let Some(summary) = verification_stale_summary(result) {
        return summary;
    }

    match tool {
        "run_shell" => {
            let exit = result
                .get("exit_code")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let stdout = result
                .get("stdout")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let stderr = result
                .get("stderr")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            if detail.is_empty() {
                format!("exit={exit}")
            } else {
                format!("exit={exit}; {}", clip(detail))
            }
        }
        "read_file" => {
            let lines = result
                .get("lines")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            format!("read {lines} lines")
        }
        "write_file" => {
            let bytes = result
                .get("bytes")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            format!("wrote {bytes} bytes")
        }
        "edit_file" => result
            .get("mode")
            .and_then(Value::as_str)
            .map(|m| format!("edit ({m})"))
            .unwrap_or_else(|| "edit ok".to_string()),
        "run_tests" => {
            let timed_out = result
                .get("timed_out")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let success = result
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let passed = result
                .get("passed")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let failed = result
                .get("failed")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            format!(
                "{} passed={passed} failed={failed}",
                if timed_out {
                    "timed out"
                } else if success {
                    "passed"
                } else {
                    "failed"
                }
            )
        }
        _ => "ok".to_string(),
    }
}

fn verification_summary(result: &Value) -> Option<String> {
    let verification = result.get("verification")?;
    if verification.is_null() {
        return None;
    }
    let kind = verification
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("verify");
    let scope = verification
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = verification
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let command = verification
        .get("canonical_command")
        .and_then(Value::as_str)
        .unwrap_or("command");
    Some(format!("verification {kind}:{scope}:{status} ({command})"))
}

fn verification_stale_summary(result: &Value) -> Option<String> {
    let stale = result.get("verification_stale")?;
    if stale.is_null() {
        return None;
    }
    let paths = stale.get("changed_paths").and_then(Value::as_array)?;
    let changed = paths
        .iter()
        .filter_map(Value::as_str)
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
    if changed.is_empty() {
        return None;
    }
    Some(format!(
        "workspace edited; verification stale for {changed}"
    ))
}

fn queue_verify_on_stop(
    repo_root: &std::path::Path,
    attempts: &mut usize,
    final_text: &str,
    iteration: usize,
    ledger: &mut RunLedger,
    run_notes: &mut Vec<String>,
    next_messages: &mut Vec<Value>,
) -> bool {
    let Some(nudge) = crate::verify_stop::build_verify_on_stop_nudge(repo_root, *attempts) else {
        return false;
    };

    *attempts += 1;
    ledger.status(
        "verify_on_stop",
        format!("queued verification follow-up attempt {}", *attempts),
        iteration,
    );
    run_notes.push(format!(
        "- verify-on-stop queued attempt {} before final response",
        *attempts
    ));
    next_messages.push(json!({
        "role": "assistant",
        "content": final_text
    }));
    next_messages.push(json!({
        "role": "user",
        "content": nudge
    }));
    true
}

fn assistant_memory_text(text: &str, run_notes: &[String], ledger: &RunLedger) -> String {
    if run_notes.is_empty() && ledger.path().is_none() {
        return text.to_string();
    }

    let mut out = text.to_string();
    out.push_str("\n\n[run-evidence]");
    out.push_str(&format!("\nrun_id: {}", ledger.run_id()));
    if let Some(path) = ledger.path() {
        out.push_str(&format!("\nledger: {}", path.display()));
    }
    for note in run_notes
        .iter()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        out.push('\n');
        out.push_str(note);
    }
    out
}

fn max_iterations() -> usize {
    env::var("OSMOGREP_MAX_ITERATIONS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(90)
}

fn send_run_status(
    tx: &Sender<AgentEvent>,
    phase: impl Into<String>,
    detail: impl Into<String>,
    iteration: usize,
    max_iterations: usize,
) {
    let _ = tx.send(AgentEvent::RunStatus {
        phase: phase.into(),
        detail: detail.into(),
        iteration,
        max_iterations,
    });
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

fn reasoning_effort_for(model: &str) -> Option<String> {
    if let Ok(raw) = env::var("OSMOGREP_REASONING_EFFORT") {
        let value = raw.trim();
        if value.is_empty()
            || matches!(
                value.to_ascii_lowercase().as_str(),
                "0" | "false" | "none" | "off"
            )
        {
            return None;
        }
        return Some(value.to_string());
    }

    let model = model.to_ascii_lowercase();
    if model.starts_with("gpt-4o") || model.starts_with("gpt-4.1") {
        return None;
    }

    if model.starts_with("gpt-5")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        return Some("medium".to_string());
    }

    None
}

fn format_api_error(status: &str, body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return format!("API error {}", status);
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(message) = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
        {
            return format!("API error {}: {}", status, message);
        }
    }

    let preview: String = trimmed.chars().take(800).collect();
    format!("API error {}: {}", status, preview)
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
    let mut handles = Vec::new();
    for (name, scope_prompt) in swarm_scopes() {
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

fn swarm_scopes() -> [(&'static str, &'static str); 4] {
    [
        (
            "explore",
            "Map relevant files/modules and explain what to inspect first.",
        ),
        (
            "edit",
            "Propose and apply concrete code changes with minimal, safe patches.",
        ),
        (
            "test",
            "Design and run targeted tests and validation for the requested change.",
        ),
        (
            "review",
            "Find likely regressions, edge-cases, and approval/blocking risks.",
        ),
    ]
}

const REVIEW_PROMPT_BUDGET: usize = 48_000;
const REVIEW_FILE_BUDGET: usize = 8_000;

fn build_review_prompt(changes: &[DiffSnapshot]) -> String {
    let mut prompt = String::from(
        "Review the session changes below as a strict coding-agent judge.\n\
         Return findings first, ordered by severity. Include file paths and concrete failure modes.\n\
         Also call out missing verification, edge cases, and any residual risk. If there are no findings, say that clearly.\n",
    );

    for (idx, change) in changes.iter().enumerate() {
        if prompt.chars().count() >= REVIEW_PROMPT_BUDGET {
            prompt.push_str("\n[review input truncated: prompt budget exhausted]\n");
            break;
        }

        let before = clip_review_text(&change.before, REVIEW_FILE_BUDGET);
        let after = clip_review_text(&change.after, REVIEW_FILE_BUDGET);
        let section = format!(
            "\n--- Change {} ---\nTool: {}\nFile: {}\nBefore:\n```text\n{}\n```\nAfter:\n```text\n{}\n```\n",
            idx + 1,
            change.tool,
            change.target,
            before,
            after
        );
        append_with_budget(&mut prompt, &section, REVIEW_PROMPT_BUDGET);
    }

    prompt
}

fn clip_review_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}\n[truncated]")
}

fn append_with_budget(target: &mut String, value: &str, max_chars: usize) {
    let used = target.chars().count();
    if used >= max_chars {
        return;
    }
    let remaining = max_chars - used;
    if value.chars().count() <= remaining {
        target.push_str(value);
    } else {
        let marker = "\n[truncated]";
        let marker_len = marker.chars().count();
        if remaining > marker_len {
            target.extend(value.chars().take(remaining - marker_len));
            target.push_str(marker);
        } else {
            target.extend(value.chars().take(remaining));
        }
    }
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

pub fn run_review_job(
    model_cfg: ModelConfig,
    api_key: String,
    changes: Vec<DiffSnapshot>,
) -> Result<String, String> {
    if changes.is_empty() {
        return Err("no changes to review".to_string());
    }
    let prompt = build_review_prompt(&changes);
    one_shot_scoped_call(
        &model_cfg,
        &api_key,
        &prompt,
        "Act as an independent code-review judge. Find correctness bugs, regressions, missing tests, safety issues, and unclear residual risk. Findings first; be concrete and cite files.",
    )
}

fn one_shot_scoped_call(
    model_cfg: &ModelConfig,
    api_key: &str,
    user_text: &str,
    scope_prompt: &str,
) -> Result<String, String> {
    let mut payload = json!({
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
        "store": false
    });
    if let Some(effort) = reasoning_effort_for(&model_cfg.model) {
        payload["reasoning"] = json!({ "effort": effort });
    }

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

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format_api_error(&status.to_string(), &body));
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn loads_common_repository_instruction_files() {
        let root = temp_root();
        fs::write(root.join(".osmogrep.md"), "osmogrep rule").unwrap();
        fs::write(root.join("AGENTS.md"), "agents rule").unwrap();
        fs::write(root.join("CLAUDE.md"), "claude rule").unwrap();
        fs::write(root.join(".cursorrules"), "cursor rule").unwrap();

        let suffix = repo_instruction_suffix(&root);

        assert!(suffix.contains("Repository instructions (.osmogrep.md):"));
        assert!(suffix.contains("osmogrep rule"));
        assert!(suffix.contains("Repository instructions (AGENTS.md):"));
        assert!(suffix.contains("agents rule"));
        assert!(suffix.contains("Repository instructions (CLAUDE.md):"));
        assert!(suffix.contains("claude rule"));
        assert!(suffix.contains("Repository instructions (.cursorrules):"));
        assert!(suffix.contains("cursor rule"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bounds_repository_instruction_context() {
        let root = temp_root();
        fs::write(
            root.join(".osmogrep.md"),
            "x".repeat(REPO_INSTRUCTION_BUDGET * 2),
        )
        .unwrap();
        fs::write(root.join("AGENTS.md"), "should be omitted").unwrap();

        let suffix = repo_instruction_suffix(&root);

        assert!(suffix.contains("Repository instructions (.osmogrep.md):"));
        assert!(!suffix.contains("should be omitted"));
        assert!(suffix.contains("instruction budget exhausted"));
        assert!(suffix.len() <= REPO_INSTRUCTION_BUDGET + 200);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_workspace_verify_commands() {
        let root = temp_root();
        fs::write(root.join("Cargo.toml"), "[package]\nname='x'\n").unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"scripts":{"test":"vitest","lint":"eslint .","build":"vite build"}}"#,
        )
        .unwrap();
        fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        fs::write(root.join("Makefile"), "check:\n\tcargo check\n").unwrap();

        let commands = detect_verify_commands(&root);

        assert!(commands.contains(&"cargo test --color never".to_string()));
        assert!(commands.contains(&"cargo check".to_string()));
        assert!(commands.contains(&"pnpm run test".to_string()));
        assert!(commands.contains(&"pnpm run lint".to_string()));
        assert!(commands.contains(&"pnpm run build".to_string()));
        assert!(commands.contains(&"make check".to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn system_prompt_includes_workspace_fact_block() {
        let root = temp_root();
        fs::write(root.join("go.mod"), "module example.com/x\n").unwrap();

        let prompt = system_prompt(&root);
        let content = prompt.get("content").and_then(Value::as_str).unwrap();

        assert!(content.contains("Workspace facts (snapshot at run start):"));
        assert!(content.contains("go.mod"));
        assert!(content.contains("go test ./..."));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_batch_collector_stops_before_dangerous_tools() {
        let root = temp_root();
        let registry = ToolRegistry::with_root(root.clone());
        let output = json!([
            {
                "type": "function_call",
                "name": "read_file",
                "call_id": "call_read",
                "arguments": "{\"path\":\"src/main.rs\"}"
            },
            {
                "type": "function_call",
                "name": "list_dir",
                "call_id": "call_list",
                "arguments": "{}"
            },
            {
                "type": "function_call",
                "name": "run_shell",
                "call_id": "call_shell",
                "arguments": "{\"cmd\":\"cargo test\"}"
            }
        ]);
        let output = output.as_array().unwrap();

        let batch = collect_parallel_safe_batch(output, 0, &registry).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].name, "read_file");
        assert_eq!(batch[1].name, "list_dir");

        let dangerous = collect_parallel_safe_batch(output, 2, &registry).unwrap();
        assert!(dangerous.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_prompt_includes_findings_criteria_and_changes() {
        let changes = vec![DiffSnapshot {
            tool: "edit_file".to_string(),
            target: "src/lib.rs".to_string(),
            before: "fn value() -> i32 { 1 }".to_string(),
            after: "fn value() -> i32 { 2 }".to_string(),
        }];

        let prompt = build_review_prompt(&changes);

        assert!(prompt.contains("Return findings first"));
        assert!(prompt.contains("missing verification"));
        assert!(prompt.contains("File: src/lib.rs"));
        assert!(prompt.contains("fn value() -> i32 { 1 }"));
        assert!(prompt.contains("fn value() -> i32 { 2 }"));
    }

    #[test]
    fn review_prompt_caps_large_change_context() {
        let changes = vec![DiffSnapshot {
            tool: "write_file".to_string(),
            target: "src/large.rs".to_string(),
            before: "a".repeat(REVIEW_FILE_BUDGET * 3),
            after: "b".repeat(REVIEW_FILE_BUDGET * 3),
        }];

        let prompt = build_review_prompt(&changes);

        assert!(prompt.chars().count() <= REVIEW_PROMPT_BUDGET);
        assert!(prompt.contains("[truncated]"));
        assert!(prompt.contains("File: src/large.rs"));
    }

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("osmogrep-agent-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
