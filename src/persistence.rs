use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::state::{
    AgentState, DiffSnapshot, JobRecord, PermissionProfile, PlanItem, UiAccent, UiDensity, UiTheme,
};

#[derive(Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    conversation: Vec<Value>,
    #[serde(default)]
    steer: Option<String>,
    #[serde(default = "default_auto_eval")]
    auto_eval: bool,
    #[serde(default = "default_permission_profile")]
    permission_profile: PermissionProfile,
    #[serde(default)]
    jobs: Vec<JobRecord>,
    #[serde(default)]
    next_job_id: u64,
    #[serde(default)]
    plan_items: Vec<PlanItem>,
    #[serde(default)]
    session_changes: Vec<DiffSnapshot>,
    #[serde(default)]
    reviewed_change_count: usize,
    #[serde(default)]
    undo_stack: Vec<DiffSnapshot>,
    #[serde(default)]
    prompt_tokens: usize,
    #[serde(default)]
    completion_tokens: usize,
    #[serde(default)]
    session_name: Option<String>,
    #[serde(default)]
    theme: UiTheme,
    #[serde(default)]
    accent: UiAccent,
    #[serde(default)]
    density: UiDensity,
    #[serde(default)]
    plan_mode: bool,
}

pub fn load(state: &mut AgentState) {
    let path = state_file(&state.repo_root);
    let Ok(raw) = fs::read_to_string(path) else {
        return;
    };
    let Ok(saved) = serde_json::from_str::<PersistedState>(&raw) else {
        return;
    };

    if !saved.conversation.is_empty() {
        state.conversation.set_messages(saved.conversation);
    }
    state.steer = saved.steer;
    state.auto_eval = saved.auto_eval;
    state.permission_profile = saved.permission_profile;
    state.jobs = saved.jobs;
    state.next_job_id = saved.next_job_id.max(1);
    state.plan_items = saved.plan_items;
    state.session_changes = saved.session_changes;
    state.reviewed_change_count = saved.reviewed_change_count.min(state.session_changes.len());
    state.undo_stack = saved.undo_stack;
    state.usage.prompt_tokens = saved.prompt_tokens;
    state.usage.completion_tokens = saved.completion_tokens;
    state.session_name = saved.session_name;
    state.theme = saved.theme;
    state.accent = saved.accent;
    state.density = saved.density;
    state.plan_mode = saved.plan_mode;
}

pub fn save(state: &AgentState) -> Result<(), String> {
    let path = state_file(&state.repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let payload = PersistedState {
        conversation: state.conversation.messages.clone(),
        steer: state.steer.clone(),
        auto_eval: state.auto_eval,
        permission_profile: state.permission_profile,
        jobs: state.jobs.clone(),
        next_job_id: state.next_job_id,
        plan_items: state.plan_items.clone(),
        session_changes: state.session_changes.clone(),
        reviewed_change_count: state.reviewed_change_count.min(state.session_changes.len()),
        undo_stack: state.undo_stack.clone(),
        prompt_tokens: state.usage.prompt_tokens,
        completion_tokens: state.usage.completion_tokens,
        session_name: state.session_name.clone(),
        theme: state.theme,
        accent: state.accent,
        density: state.density,
        plan_mode: state.plan_mode,
    };

    let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

pub struct SessionSummary {
    pub id: String,
    pub name: Option<String>,
    pub modified: Option<SystemTime>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub plan_items: usize,
    pub jobs: usize,
}

pub fn list_sessions() -> Result<Vec<SessionSummary>, String> {
    list_sessions_in(&session_dir())
}

fn list_sessions_in(dir: &Path) -> Result<Vec<SessionSummary>, String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(Vec::new());
    };

    let mut sessions = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(saved) = serde_json::from_str::<PersistedState>(&raw) else {
            continue;
        };
        let modified = entry.metadata().ok().and_then(|meta| meta.modified().ok());
        let id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string();
        sessions.push(SessionSummary {
            id,
            name: saved.session_name,
            modified,
            prompt_tokens: saved.prompt_tokens,
            completion_tokens: saved.completion_tokens,
            plan_items: saved.plan_items.len(),
            jobs: saved.jobs.len(),
        });
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(sessions)
}

fn default_permission_profile() -> PermissionProfile {
    PermissionProfile::WorkspaceAuto
}

fn default_auto_eval() -> bool {
    true
}

fn state_file(repo_root: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(repo_root.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    session_dir().join(format!("{}.json", hash))
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("osmogrep")
}

pub fn session_dir() -> PathBuf {
    config_dir().join("sessions")
}

#[cfg(test)]
mod tests {
    use super::list_sessions_in;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn lists_saved_session_summaries() {
        let dir = std::env::temp_dir().join(format!("osmogrep-sessions-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("abc123.json"),
            r#"{
              "session_name": "parser work",
              "prompt_tokens": 12,
              "completion_tokens": 30,
              "plan_items": [
                { "text": "inspect", "done": true, "active": false }
              ],
              "jobs": []
            }"#,
        )
        .unwrap();
        fs::write(dir.join("ignore.txt"), "{}").unwrap();

        let sessions = list_sessions_in(&dir).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "abc123");
        assert_eq!(sessions[0].name.as_deref(), Some("parser work"));
        assert_eq!(
            sessions[0].prompt_tokens + sessions[0].completion_tokens,
            42
        );
        assert_eq!(sessions[0].plan_items, 1);

        let _ = fs::remove_dir_all(dir);
    }
}
