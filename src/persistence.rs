use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct Session {
    pub id: String,
    pub update_latest: bool,
    lock: RefCell<Option<fs::File>>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            update_latest: true,
            lock: RefCell::new(None),
        }
    }
}
impl Session {
    fn acquire(&self, path: &Path) -> Result<(), String> {
        if self.lock.borrow().is_some() {
            return Ok(());
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))
            .map_err(|e| e.to_string())?;
        file.try_lock().map_err(|_| {
            format!(
                "Session {} is already open. Fork it to work independently.",
                self.id
            )
        })?;
        *self.lock.borrow_mut() = Some(file);
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(file) = self.lock.get_mut().take() {
            // Release immediately even if a concurrent fork has not exec'd yet.
            let _ = file.unlock();
        }
    }
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::state::{
    AgentState, DiffSnapshot, JobRecord, PermissionProfile, PlanItem, UiAccent, UiDensity, UiTheme,
};

#[derive(Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    queued_prompts: Vec<String>,
    #[serde(default)]
    repo_root: String,
    #[serde(default)]
    logs: Vec<(String, String)>,
    #[serde(default)]
    preferences: crate::preferences::Preferences,
    #[serde(default)]
    conversation: Vec<Value>,
    #[serde(default)]
    steer: Option<String>,
    #[serde(default = "default_auto_eval")]
    auto_eval: bool,
    #[serde(default)]
    automation_opt_in: bool,
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

pub fn load_settings(state: &mut AgentState) -> Result<(), String> {
    let path = match fs::read_to_string(latest_file(&state.repo_root)) {
        Ok(id) => session_path(id.trim())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => legacy_file(&state.repo_root),
        Err(error) => return Err(error.to_string()),
    };
    load_settings_from(state, &path)
}
fn load_settings_from(state: &mut AgentState, path: &Path) -> Result<(), String> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let saved: PersistedState = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if !saved.repo_root.is_empty() && Path::new(&saved.repo_root) != state.repo_root {
        return Err("Saved settings belong to a different workspace".into());
    }
    state.preferences = saved.preferences;
    state.preferences.team_messages = false;
    state.preferences.normalize();
    state.permission_profile = saved.permission_profile;
    state.theme = saved.theme;
    state.accent = saved.accent;
    state.density = saved.density;
    Ok(())
}

fn apply_saved(state: &mut AgentState, saved: PersistedState) {
    state.ui.queued_agent_prompts = saved.queued_prompts.into();
    state.ui.queue_paused = !state.ui.queued_agent_prompts.is_empty();
    state.logs.clear();
    for (level, text) in saved.logs {
        let level = match level.as_str() {
            "error" => crate::state::LogLevel::Error,
            "warn" => crate::state::LogLevel::Warn,
            "success" => crate::state::LogLevel::Success,
            _ => crate::state::LogLevel::Info,
        };
        state.logs.push(level, text);
    }

    if !saved.conversation.is_empty() {
        state.conversation.set_messages(saved.conversation);
    }
    state.preferences = saved.preferences;
    state.preferences.normalize();
    state.steer = saved.steer;
    state.auto_eval = saved.auto_eval && saved.automation_opt_in;
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
    let path = session_path(&state.session.id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    state.session.acquire(&path)?;

    let payload = PersistedState {
        queued_prompts: state.ui.queued_agent_prompts.iter().cloned().collect(),
        repo_root: state.repo_root.to_string_lossy().to_string(),
        logs: state
            .logs
            .iter()
            .map(|l| {
                (
                    match l.level {
                        crate::state::LogLevel::Error => "error",
                        crate::state::LogLevel::Warn => "warn",
                        crate::state::LogLevel::Success => "success",
                        _ => "info",
                    }
                    .to_string(),
                    l.text.clone(),
                )
            })
            .collect(),
        preferences: state.preferences.clone(),
        conversation: state.conversation.messages.clone(),
        steer: state.steer.clone(),
        auto_eval: state.auto_eval,
        automation_opt_in: state.auto_eval,
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
    atomic_write(&path, text.as_bytes()).map_err(|e| e.to_string())?;
    if state.session.update_latest {
        atomic_write(&latest_file(&state.repo_root), state.session.id.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn save_or_log(state: &mut AgentState) {
    if let Err(error) = save(state) {
        state.logs.push(
            crate::state::LogLevel::Error,
            format!("Session could not be saved: {error}"),
        );
    }
}

fn session_path(id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid session ID".into());
    }
    Ok(session_dir().join(format!("{id}.json")))
}

pub fn preview_session(id: &str) -> Result<Vec<String>, String> {
    let path = session_path(id)?;
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err(
            "Session snapshot exceeds the 32 MiB preview limit; fork or resume to inspect it"
                .into(),
        );
    }
    let saved: PersistedState = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(saved
        .logs
        .into_iter()
        .rev()
        .take(200)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .flat_map(|(_, text)| {
            text.lines()
                .map(|s| {
                    s.chars()
                        .filter(|c| !c.is_control())
                        .take(500)
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        })
        .collect())
}

pub fn load_id(state: &mut AgentState, id: &str, fork: bool) -> Result<(), String> {
    if !fork && id == state.session.id {
        return Ok(());
    }
    let path = session_path(id)?;
    let saved: PersistedState =
        serde_json::from_slice(&fs::read(&path).map_err(|e| format!("Cannot read session: {e}"))?)
            .map_err(|e| e.to_string())?;
    if !saved.repo_root.is_empty() && Path::new(&saved.repo_root) != state.repo_root {
        return Err(format!(
            "Session belongs to {}; open that workspace to resume it",
            saved.repo_root
        ));
    }
    let mut session = if fork {
        Session::default()
    } else {
        Session {
            id: id.into(),
            update_latest: state.session.update_latest,
            lock: RefCell::new(None),
        }
    };
    session.update_latest = state.session.update_latest;
    if !fork {
        session.acquire(&path)?;
    }
    state.session = session;
    apply_saved(state, saved);
    state.ui.diff_active = false;
    state.ui.transcript_selection = None;
    state.ui.follow_tail = true;
    state.ui.exec_scroll = usize::MAX;
    save(state)
}

pub fn new_session(state: &mut AgentState) -> Result<(), String> {
    save(state)?;
    let update_latest = state.session.update_latest;
    state.session = Session::default();
    state.session.update_latest = update_latest;
    state.session_name = None;
    state.conversation.clear();
    state.logs.clear();
    state.undo_stack.clear();
    state.session_changes.clear();
    state.plan_items.clear();
    state.jobs.clear();
    state.reviewed_change_count = 0;
    state.ui.queued_agent_prompts.clear();
    state.ui.diff_active = false;
    state.ui.diff_snapshot.clear();
    state.usage.prompt_tokens = 0;
    state.usage.completion_tokens = 0;
    save(state)
}

pub fn search_sessions(query: &str) -> Result<Vec<SessionSummary>, String> {
    let query = query.to_lowercase();
    Ok(list_sessions()?
        .into_iter()
        .filter(|s| {
            query.is_empty()
                || fs::read_to_string(session_dir().join(format!("{}.json", s.id)))
                    .map(|text| text.to_lowercase().contains(&query))
                    .unwrap_or(false)
        })
        .collect())
}

pub fn recovery_checkpoint(root: &Path, run_id: &str) -> Result<Value, String> {
    uuid::Uuid::parse_str(run_id).map_err(|_| "Use the run UUID from .context/osmogrep-runs")?;
    let dir = root.join(".context/osmogrep-runs");
    let path = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(&format!("-{run_id}.checkpoint.json")))
        })
        .ok_or("No recovery checkpoint for that run")?;
    let saved: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(saved)
}

pub fn recover(state: &mut AgentState, run_id: &str) -> Result<(), String> {
    let saved = recovery_checkpoint(&state.repo_root, run_id)?;
    let mut messages = saved["input"]
        .as_array()
        .cloned()
        .ok_or("Invalid recovery checkpoint")?;
    let uncertain = saved["phase"] == "tool_in_flight";
    if uncertain {
        messages.pop();
    }
    messages.push(serde_json::json!({"role":"assistant","content":if uncertain {
        "[Run interrupted during a tool call. Its effects are uncertain. Inspect the workspace before retrying a mutation. Previous permission approvals are not reusable. Wait for the user's next request.]"
    } else { "[Recovered run history. Wait for the user's next request; do not automatically resume previous work.]" }}));
    new_session(state)?;
    state.conversation.set_messages(messages);
    state.ui.queue_paused = true;
    state.logs.push(crate::state::LogLevel::Warn,if uncertain {"Recovered into a new session. A tool may have run; inspect its effects before continuing."}else{"Recovered into a new session. Send a request when ready."});
    save(state)
}

pub struct SessionSummary {
    pub id: String,
    pub repo_root: String,
    pub title: String,
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
            repo_root: saved.repo_root,
            title: saved
                .conversation
                .iter()
                .rev()
                .find(|m| m["role"] == "user")
                .and_then(|m| m["content"].as_str())
                .unwrap_or("New conversation")
                .chars()
                .take(100)
                .collect(),
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
    false
}

fn legacy_file(repo_root: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(repo_root.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    session_dir().join(format!("{}.json", hash))
}

fn latest_file(repo_root: &Path) -> PathBuf {
    legacy_file(repo_root).with_extension("latest")
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("osmogrep")
}

pub fn update_config(
    update: impl FnOnce(&mut toml::Value) -> Result<(), String>,
) -> Result<(), String> {
    let path = config_dir().join("config.toml");
    let mut config = match fs::read_to_string(&path) {
        Ok(text) => text.parse::<toml::Value>().map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            toml::Value::Table(Default::default())
        }
        Err(e) => return Err(e.to_string()),
    };
    update(&mut config)?;
    fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    atomic_write(
        &path,
        toml::to_string(&config)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())
}

pub fn session_dir() -> PathBuf {
    config_dir().join("sessions")
}

#[cfg(test)]
mod tests {
    #[test]
    fn sessions_hold_exclusive_locks_and_release_on_drop() {
        let dir = std::env::temp_dir().join(format!("session-lock-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("thread.json");
        let first = super::Session::default();
        first.acquire(&path).unwrap();
        first.acquire(&path).unwrap();
        let second = super::Session::default();
        assert!(second.acquire(&path).is_err());
        drop(first);
        second.acquire(&path).unwrap();
        drop(second);
        std::fs::remove_dir_all(dir).unwrap();
        assert!(super::session_path("../outside").is_err());
    }
    #[test]
    fn inherited_automation_defaults_do_not_become_opt_in() {
        let legacy: super::PersistedState = serde_json::from_str(r#"{"auto_eval":true}"#).unwrap();
        assert!(!(legacy.auto_eval && legacy.automation_opt_in));
        let explicit: super::PersistedState =
            serde_json::from_str(r#"{"auto_eval":true,"automation_opt_in":true}"#).unwrap();
        assert!(explicit.auto_eval && explicit.automation_opt_in);
    }
    use super::list_sessions_in;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn fresh_start_carries_ui_settings_without_resuming_the_previous_task() {
        let root = std::env::temp_dir().join(format!("osmogrep-fresh-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("old.json");
        let saved = serde_json::json!({"repo_root":root,"theme":"Light","preferences":{"reduce_motion":true,"team_messages":true},"auto_eval":true,"automation_opt_in":true,"session_name":"Old task","conversation":[{"role":"user","content":"OLD_REQUEST"}],"logs":[["info","OLD_OUTPUT"]],"queued_prompts":["OLD_QUEUED"],"plan_mode":true});
        // Use the serializer's enum spelling instead of coupling the fixture to it.
        let mut saved = saved;
        saved["theme"] = serde_json::to_value(crate::state::UiTheme::Light).unwrap();
        saved["permission_profile"] =
            serde_json::to_value(crate::state::PermissionProfile::ReadOnly).unwrap();
        fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let original = fs::read(&path).unwrap();
        for _ in 0..2 {
            let mut state = crate::init_state();
            state.repo_root = root.clone();
            let id = state.session.id.clone();
            let messages = state.conversation.messages.clone();
            super::load_settings_from(&mut state, &path).unwrap();
            assert_eq!(state.session.id, id);
            assert_eq!(state.conversation.messages, messages);
            assert!(state.ui.queued_agent_prompts.is_empty());
            assert!(state.session_name.is_none());
            assert!(state
                .logs
                .iter()
                .all(|line| !line.text.contains("OLD_OUTPUT")));
            assert!(!state.auto_eval && !state.plan_mode && !state.preferences.team_messages);
            assert_eq!(
                state.permission_profile,
                crate::state::PermissionProfile::ReadOnly
            );
            assert!(!state.ui.auto_approve);
            assert!(state.preferences.reduce_motion);
            assert_eq!(state.theme, crate::state::UiTheme::Light);
        }
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn atomic_preferences_round_trip_and_failed_replace_preserve_existing_data() {
        let dir = std::env::temp_dir().join(format!("osmogrep-preferences-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let mut p = crate::preferences::Preferences::default();
        p.reduce_motion = true;
        p.key_bindings.insert("commands".into(), "ctrl+p".into());
        super::atomic_write(&path, &serde_json::to_vec(&p).unwrap()).unwrap();
        let restored: crate::preferences::Preferences =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(restored.reduce_motion);
        assert_eq!(restored.key_bindings.get("commands").unwrap(), "ctrl+p");
        assert!(super::atomic_write(&dir, b"not a file").is_err());
        assert!(path.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

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
