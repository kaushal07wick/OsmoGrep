use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::state::{AgentState, DiffSnapshot};

#[derive(Serialize, Deserialize)]
struct PersistedState {
    conversation: Vec<Value>,
    session_changes: Vec<DiffSnapshot>,
    undo_stack: Vec<DiffSnapshot>,
    prompt_tokens: usize,
    completion_tokens: usize,
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
    state.session_changes = saved.session_changes;
    state.undo_stack = saved.undo_stack;
    state.usage.prompt_tokens = saved.prompt_tokens;
    state.usage.completion_tokens = saved.completion_tokens;
}

pub fn save(state: &AgentState) -> Result<(), String> {
    let path = state_file(&state.repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let payload = PersistedState {
        conversation: state.conversation.messages.clone(),
        session_changes: state.session_changes.clone(),
        undo_stack: state.undo_stack.clone(),
        prompt_tokens: state.usage.prompt_tokens,
        completion_tokens: state.usage.completion_tokens,
    };

    let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

fn state_file(repo_root: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(repo_root.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let mut base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.push("osmogrep");
    base.push("sessions");
    base.push(format!("{}.json", hash));
    base
}
