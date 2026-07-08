use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{agent::ModelConfig, state::PermissionProfile};

const MAX_PREVIEW_CHARS: usize = 1200;

pub struct RunLedger {
    run_id: String,
    path: Option<PathBuf>,
}

impl RunLedger {
    pub fn start(
        repo_root: &Path,
        prompt: &str,
        model: &ModelConfig,
        permission_profile: PermissionProfile,
        max_iterations: usize,
    ) -> Self {
        let run_id = Uuid::new_v4().to_string();
        let path = prepare_ledger_path(repo_root, &run_id);
        let mut ledger = Self { run_id, path };
        ledger.record(json!({
            "type": "run_started",
            "prompt_preview": clip(prompt),
            "model": model.model,
            "provider": model.provider,
            "permission_profile": permission_profile.as_str(),
            "max_iterations": max_iterations
        }));
        ledger
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn status(&mut self, phase: &str, detail: impl Into<String>, iteration: usize) {
        self.record(json!({
            "type": "status",
            "phase": phase,
            "detail": clip(&detail.into()),
            "iteration": iteration
        }));
    }

    pub fn tool_started(&mut self, tool: &str, args_summary: impl Into<String>, iteration: usize) {
        self.record(json!({
            "type": "tool_started",
            "tool": tool,
            "args_summary": clip(&args_summary.into()),
            "iteration": iteration
        }));
    }

    pub fn tool_finished(
        &mut self,
        tool: &str,
        status: &str,
        summary: impl Into<String>,
        duration_ms: u128,
        iteration: usize,
    ) {
        self.record(json!({
            "type": "tool_finished",
            "tool": tool,
            "status": status,
            "summary": clip(&summary.into()),
            "duration_ms": duration_ms,
            "iteration": iteration
        }));
    }

    pub fn permission(&mut self, tool: &str, decision: &str, iteration: usize) {
        self.record(json!({
            "type": "permission",
            "tool": tool,
            "decision": decision,
            "iteration": iteration
        }));
    }

    pub fn final_text(&mut self, text: &str, iteration: usize) {
        self.record(json!({
            "type": "final",
            "text_preview": clip(text),
            "iteration": iteration
        }));
    }

    pub fn error(&mut self, message: impl Into<String>, iteration: usize) {
        self.record(json!({
            "type": "error",
            "message": clip(&message.into()),
            "iteration": iteration
        }));
    }

    fn record(&mut self, mut value: Value) {
        let Value::Object(ref mut obj) = value else {
            return;
        };
        obj.insert("run_id".into(), json!(self.run_id));
        obj.insert("ts".into(), json!(Utc::now().to_rfc3339()));

        let Some(path) = &self.path else {
            return;
        };
        let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
            self.path = None;
            return;
        };
        let _ = writeln!(file, "{}", value);
    }
}

fn prepare_ledger_path(repo_root: &Path, run_id: &str) -> Option<PathBuf> {
    let dir = repo_root.join(".context").join("osmogrep-runs");
    fs::create_dir_all(&dir).ok()?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    Some(dir.join(format!("{stamp}-{run_id}.jsonl")))
}

pub fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_PREVIEW_CHARS {
        return s.to_string();
    }
    let mut out: String = s
        .chars()
        .take(MAX_PREVIEW_CHARS.saturating_sub(3))
        .collect();
    out.push_str("...");
    out
}
