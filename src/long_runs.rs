//! Explicit, budgeted supervision of the existing coding loop.
use crate::{
    agent::{Agent, AgentEvent, CancelToken},
    persistence,
    state::PermissionProfile,
};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Args, Debug)]
pub struct TaskArgs {
    #[arg(long, default_value = ".", global = true)]
    pub repo_root: PathBuf,
    #[command(subcommand)]
    pub command: TaskCommand,
}
#[derive(Subcommand, Debug)]
pub enum TaskCommand {
    Start {
        #[arg(long)]
        objective: String,
        #[arg(long = "criterion", required = true)]
        criteria: Vec<String>,
        #[arg(long, default_value_t = 28_800)]
        max_seconds: u64,
        #[arg(long, default_value_t = 2_000_000)]
        max_tokens: u64,
        #[arg(long, default_value_t = 1000)]
        max_requests: u64,
        #[arg(long, default_value = "workspace-auto")]
        permission_profile: String,
        #[arg(long)]
        auto_approve: bool,
        #[arg(long)]
        json_events: bool,
    },
    Resume {
        id: String,
        /// Confirm that uncertain effects from the previous process were inspected.
        #[arg(long)]
        reviewed: bool,
        #[arg(long, default_value = "workspace-auto")]
        permission_profile: String,
        #[arg(long)]
        auto_approve: bool,
        #[arg(long)]
        json_events: bool,
    },
    Status {
        id: String,
    },
    List,
    Stop {
        id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub root: PathBuf,
    pub objective: String,
    pub criteria: Vec<String>,
    pub status: String,
    pub reason: String,
    pub progress: Vec<String>,
    pub evidence: Vec<String>,
    pub max_ms: u64,
    pub max_tokens: u64,
    pub max_requests: u64,
    pub used_ms: u64,
    pub charged_tokens: u64,
    pub requests: u64,
    pub session: String,
    pub run_id: Option<String>,
    pub updated: i64,
    completion_requested: bool,
    revision: u64,
}
pub struct Task {
    data: Mutex<Record>,
    path: PathBuf,
    _owner: fs::File,
    started: Instant,
    base_ms: u64,
    last_final: Mutex<(u64, usize)>,
}
impl Drop for Task {
    fn drop(&mut self) {
        let _ = self._owner.unlock();
    }
}
fn path(root: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Use a task UUID")?;
    Ok(
        crate::file_guard::internal_dir(root, ".context/osmogrep-tasks")?
            .join(format!("{id}.json")),
    )
}
fn read(root: &Path, id: &str) -> Result<Record, String> {
    let path = path(root, id)?;
    if fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Refusing a linked task record".into());
    }
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
        return Err("Task record exceeds 1 MiB".into());
    }
    let record: Record = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if record.id != id || record.root != root.canonicalize().map_err(|e| e.to_string())? {
        return Err("Task does not belong to this workspace".into());
    }
    validate(&record)?;
    Ok(record)
}
fn validate(r: &Record) -> Result<(), String> {
    if r.objective.trim().is_empty()
        || r.objective.len() > 32 * 1024
        || r.criteria.is_empty()
        || r.criteria.len() > 32
        || r.criteria
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 2048)
    {
        return Err("Supply an objective of at most 32 KiB and 1–32 nonempty criteria of at most 2 KiB each".into());
    }
    if !(1..=7 * 24 * 3600 * 1000).contains(&r.max_ms)
        || r.max_tokens == 0
        || r.max_tokens > 1_000_000_000
        || !(1..=100_000).contains(&r.max_requests)
    {
        return Err("Budgets must be positive: at most 7 days, 1 billion tokens and 100,000 provider requests".into());
    }
    Ok(())
}
impl Task {
    fn own(root: &Path, mut record: Record) -> Result<Arc<Self>, String> {
        validate(&record)?;
        let path = path(root, &record.id)?;
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(false).write(true).read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let owner = options
            .open(path.with_extension("lock"))
            .map_err(|e| e.to_string())?;
        owner
            .try_lock()
            .map_err(|_| "This task already has a running supervisor")?;
        if path.exists() {
            record = read(root, &record.id)?;
        }
        if record.status == "completed" {
            return Err("This task is already completed; start a new objective instead".into());
        }
        record.status = "running".into();
        record.reason.clear();
        record.completion_requested = false;
        let base_ms = record.used_ms;
        let task = Arc::new(Self {
            data: Mutex::new(record),
            path,
            _owner: owner,
            started: Instant::now(),
            base_ms,
            last_final: Mutex::new((0, 0)),
        });
        if task.path.with_extension("stop").exists() {
            fs::remove_file(task.path.with_extension("stop")).map_err(|e| e.to_string())?;
        }
        task.update(|_| Ok(()))?;
        Ok(task)
    }
    fn update<T>(&self, f: impl FnOnce(&mut Record) -> Result<T, String>) -> Result<T, String> {
        let mut r = self.data.lock().map_err(|_| "Task state unavailable")?;
        let value = f(&mut r)?;
        r.used_ms = self
            .base_ms
            .saturating_add(self.started.elapsed().as_millis().min(u64::MAX as u128) as u64);
        r.updated = chrono::Utc::now().timestamp_millis();
        persistence::atomic_write(
            &self.path,
            &serde_json::to_vec(&*r).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("Cannot persist task state: {e}"))?;
        Ok(value)
    }
    pub fn snapshot(&self) -> Result<Record, String> {
        self.data
            .lock()
            .map(|r| r.clone())
            .map_err(|_| "Task state unavailable".into())
    }
    pub fn max_requests(&self) -> usize {
        self.data.lock().map_or(1, |r| r.max_requests as usize)
    }
    pub fn attach(&self, run: &str) -> Result<(), String> {
        self.update(|r| {
            r.run_id = Some(run.into());
            Ok(())
        })
    }
    pub fn instructions(&self) -> Result<String, String> {
        let r = self.snapshot()?;
        Ok(format!("[durable-task]\nObjective: {}\nAcceptance criteria:\n{}\nRecorded progress: {}\nRemaining budgets: {} provider requests, ~{} charged tokens, {} seconds.\nWork through the objective using ordinary coding tools. Use task_progress checkpoint after meaningful milestones. A short answer alone does not complete this task. Call task_progress complete with one concrete evidence entry per criterion only when all are met, then provide a concise final summary. If an external blocker needs the user, call task_progress blocked and explain it; do not invent success. Tests are appropriate only when the task requires them. This supervised mode uses one coding agent; delegation and nested AI workflow tools are unavailable because they do not yet share this budget. If the objective requires those tools, report the limitation instead of bypassing it. Budget exhaustion stops safely; it is not completion.\n[/durable-task]",r.objective,r.criteria.iter().enumerate().map(|(i,s)|format!("{}. {s}",i+1)).collect::<Vec<_>>().join("\n"),r.progress.iter().rev().take(8).rev().map(|s|s.chars().take(512).collect::<String>()).collect::<Vec<_>>().join(" | "),r.max_requests.saturating_sub(r.requests),r.max_tokens.saturating_sub(r.charged_tokens),r.max_ms.saturating_sub(r.used_ms)/1000))
    }
    pub fn tick(&self) -> Result<(), String> {
        let r = self.snapshot()?;
        if self.path.with_extension("stop").exists() {
            return Err("Stop requested by the user".into());
        }
        if r.status != "running" {
            return Err(r.reason);
        }
        if self
            .base_ms
            .saturating_add(self.started.elapsed().as_millis() as u64)
            >= r.max_ms
        {
            return Err("Wall-time budget exhausted".into());
        }
        if chrono::Utc::now().timestamp_millis().abs_diff(r.updated) >= 1000 {
            self.update(|_| Ok(()))?;
        }
        Ok(())
    }
    pub fn reserve(&self, payload: &Value) -> Result<u64, String> {
        self.tick()?;
        let input = crate::compaction::estimate(payload) as u64;
        let output = payload["max_output_tokens"].as_u64().unwrap_or(16384);
        let reserve = input.saturating_add(output);
        self.update(|r| {
            if r.requests >= r.max_requests {
                return Err("Provider-request budget exhausted".into());
            }
            if r.charged_tokens.saturating_add(reserve) > r.max_tokens {
                return Err("Token budget cannot cover the next request reservation".into());
            }
            r.requests += 1;
            r.charged_tokens = r.charged_tokens.saturating_add(reserve);
            Ok(reserve)
        })
    }
    pub fn account(&self, reservation: u64, response: &Value) -> Result<(), String> {
        self.update(|r| {
            let usage = &response["usage"];
            let actual = usage["input_tokens"]
                .as_u64()
                .zip(usage["output_tokens"].as_u64())
                .map(|(a, b)| a.saturating_add(b))
                .unwrap_or(reservation);
            r.charged_tokens = r
                .charged_tokens
                .saturating_sub(reservation)
                .saturating_add(actual);
            Ok(())
        })
    }
    pub fn stop(&self, reason: &str) -> Result<(), String> {
        self.update(|r| {
            if r.status != "completed" {
                r.status = "paused".into();
                r.reason = reason.into();
            }
            Ok(())
        })
    }
    pub fn stall(&self, reason: &str) -> Result<(), String> {
        self.stop(reason)
    }
    /// A claimed completion is accepted only with durable criterion evidence.
    pub fn final_answer(&self) -> Result<bool, String> {
        self.tick()?;
        let r = self.snapshot()?;
        if r.completion_requested {
            return Ok(true);
        }
        let mut last = self
            .last_final
            .lock()
            .map_err(|_| "Task progress unavailable")?;
        if last.0 == r.revision {
            last.1 += 1;
        } else {
            *last = (r.revision, 1);
        }
        if last.1 >= 3 {
            return Err("Stalled: three final answers without a new durable milestone or completion evidence".into());
        }
        Ok(false)
    }
    pub fn complete(&self) -> Result<(), String> {
        self.tick()?;
        self.update(|r| {
            if !r.completion_requested {
                return Err("Completion evidence was not recorded".into());
            }
            r.status = "completed".into();
            r.reason = "Model reported every criterion satisfied; see recorded evidence".into();
            Ok(())
        })
    }
    fn action(&self, args: Value) -> Result<Value, String> {
        let action = args["action"].as_str().ok_or("Missing task action")?;
        let note = args["summary"].as_str().unwrap_or("").trim();
        if note.is_empty() || note.len() > 4096 {
            return Err("Task progress needs a nonempty summary of at most 4096 bytes".into());
        }
        self.tick()?;
        self.update(|r|{
            match action {
                "checkpoint"=>{if r.progress.last().is_none_or(|s|s!=note){r.progress.push(note.into());r.revision+=1;if r.progress.len()>50{r.progress.remove(0);}}},
                "complete"=>{
                    let evidence=args["evidence"].as_array().ok_or("Supply one evidence entry per acceptance criterion")?;
                    if evidence.len()!=r.criteria.len()||evidence.iter().any(|s|s.as_str().is_none_or(|s|s.trim().is_empty()||s.len()>4096)){return Err("Each acceptance criterion requires its own nonempty evidence entry of at most 4096 bytes".into());}
                    r.evidence=evidence.iter().map(|s|s.as_str().unwrap().to_string()).collect();r.completion_requested=true;r.revision+=1;
                },
                "blocked"=>{r.status="paused".into();r.reason=format!("External blocker: {note}");},
                _=>return Err("Use checkpoint, complete, or blocked".into()),
            }
            Ok(json!({"recorded":action,"summary":note,"completion":"Evidence is a model claim, not independent verification"}))
        })
    }
}
pub struct Progress(pub Arc<Task>);
impl crate::tools::Tool for Progress {
    fn name(&self) -> &'static str {
        "task_progress"
    }
    fn safety(&self) -> crate::tools::ToolSafety {
        crate::tools::ToolSafety::Safe
    }
    fn schema(&self) -> Value {
        json!({"type":"function","name":"task_progress","description":"Record durable progress for the current user-authorized long task. Checkpoint meaningful milestones, complete only after every criterion has concrete evidence, or report an external blocker honestly. Does not edit project files, expand scope or approve tools.","parameters":{"type":"object","properties":{"action":{"type":"string","enum":["checkpoint","complete","blocked"]},"summary":{"type":"string"},"evidence":{"type":"array","items":{"type":"string"}}},"required":["action","summary"],"additionalProperties":false}})
    }
    fn call(&self, args: Value) -> crate::tools::ToolResult {
        self.0.action(args)
    }
}

pub fn run(args: TaskArgs, team: bool) -> Result<(), String> {
    let root = args.repo_root.canonicalize().map_err(|e| e.to_string())?;
    let (record, reviewed, profile, approve, json_events) = match args.command {
        TaskCommand::Start {
            objective,
            criteria,
            max_seconds,
            max_tokens,
            max_requests,
            permission_profile,
            auto_approve,
            json_events,
        } => (
            Record {
                id: uuid::Uuid::new_v4().to_string(),
                root: root.clone(),
                objective,
                criteria,
                status: "new".into(),
                reason: String::new(),
                progress: vec![],
                evidence: vec![],
                max_ms: max_seconds
                    .checked_mul(1000)
                    .ok_or("Wall-time budget too large")?,
                max_tokens,
                max_requests,
                used_ms: 0,
                charged_tokens: 0,
                requests: 0,
                session: String::new(),
                run_id: None,
                updated: 0,
                completion_requested: false,
                revision: 0,
            },
            true,
            permission_profile,
            auto_approve,
            json_events,
        ),
        TaskCommand::Resume {
            id,
            reviewed,
            permission_profile,
            auto_approve,
            json_events,
        } => (
            read(&root, &id)?,
            reviewed,
            permission_profile,
            auto_approve,
            json_events,
        ),
        TaskCommand::Status { id } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&read(&root, &id)?).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
        TaskCommand::Stop { id } => {
            let record = read(&root, &id)?;
            if record.status == "completed" {
                return Err("Task is already completed".into());
            }
            persistence::atomic_write(&path(&root, &id)?.with_extension("stop"), b"user stop\n")
                .map_err(|e| e.to_string())?;
            println!("Stop requested for {id}");
            return Ok(());
        }
        TaskCommand::List => {
            let dir = crate::file_guard::internal_dir(&root, ".context/osmogrep-tasks")?;
            for entry in fs::read_dir(dir)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
            {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Some(id) = p.file_stem().and_then(|s| s.to_str()) {
                        match read(&root, id) {
                            Ok(r) => println!(
                                "{}  {:<10}  {}",
                                r.id,
                                r.status,
                                r.objective.lines().next().unwrap_or("")
                            ),
                            Err(e) => eprintln!("{id}: {e}"),
                        }
                    }
                }
            }
            return Ok(());
        }
    };
    let uncertain = if let Some(run) = record.run_id.as_deref() {
        record.status == "running"
            || persistence::recovery_checkpoint(&root, run)?["phase"] == "tool_in_flight"
    } else {
        false
    };
    if uncertain && !reviewed {
        return Err("Inspect the workspace for uncertain tool effects, then use task resume <UUID> --reviewed. Unfinished tools are never automatically replayed.".into());
    }
    let profile = PermissionProfile::parse(&profile).ok_or("Invalid permission profile")?;
    let task = Task::own(&root, record)?;
    let mut state = crate::init_state();
    state.repo_root = root.clone();
    state.session.update_latest = false;
    state.permission_profile = profile;
    state.ui.auto_approve = approve;
    state.preferences.team_messages = team;
    if let Some(run) = task.snapshot()?.run_id {
        persistence::recover(&mut state, &run)?;
    }
    task.update(|r| {
        r.session = state.session.id.clone();
        Ok(())
    })?;
    persistence::save(&state)?;
    let agent = Agent::new();
    crate::team::poll(&mut state, agent.model_config());
    if team {
        if let Some(error) = state.ui.team.error.clone() {
            task.stop(&error)?;
            return Err(error);
        }
    }
    let objective = task.snapshot()?.objective;
    crate::logger::log_user_input(&mut state, &objective);
    let mut emitter = crate::HeadlessEmitter::new(json_events, approve);
    if json_events {
        emitter.emit_json_value(json!({"type":"task_started","task_id":task.snapshot()?.id,"session_id":state.session.id}));
    } else {
        println!("Task {} · {}", task.snapshot()?.id, objective);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let control = agent.spawn_task(
        root,
        objective,
        state.conversation.messages.clone(),
        profile,
        approve,
        task.clone(),
        tx,
    );
    struct CancelOnDrop(CancelToken);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
    let _cancel_on_exit = CancelOnDrop(control.cancel.clone());
    let mut stopped = None;
    let mut last_tick = Instant::now();
    loop {
        if last_tick.elapsed() >= Duration::from_millis(250) {
            last_tick = Instant::now();
            if stopped.is_none() {
                if let Err(reason) = task.tick() {
                    control.cancel.cancel();
                    let _ = task.stop(&reason);
                    stopped = Some(reason);
                }
            }
            crate::team::poll(&mut state, agent.model_config());
        }
        let event = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => event,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => {
                stopped.get_or_insert("Agent event channel closed".into());
                break;
            }
        };
        crate::team::observe(&mut state, &event);
        crate::logger::record_runtime_event(&mut state, &event);
        match &event {
            AgentEvent::ConversationUpdate(messages) => {
                state.conversation.set_messages(messages.clone());
                persistence::save(&state)?;
            }
            AgentEvent::Error(reason) => {
                stopped.get_or_insert(reason.clone());
            }
            AgentEvent::Cancelled => {
                stopped.get_or_insert("Cancelled".into());
            }
            _ => {}
        }
        let done = matches!(event, AgentEvent::Done);
        emitter.emit_event(event);
        if done {
            break;
        }
    }
    if let Some(reason) = stopped {
        task.stop(&reason)?;
    }
    let result = task.snapshot()?;
    if result.status != "completed" {
        task.stop(if result.reason.is_empty() {
            "Agent ended without completion evidence"
        } else {
            &result.reason
        })?;
    }
    let result = task.snapshot()?;
    crate::logger::log_status(
        &mut state,
        format!("Task {} · {} · {}", result.id, result.status, result.reason),
    );
    persistence::save(&state)?;
    if json_events {
        emitter.emit_json_value(json!({"type":"task_finished","task":result}));
    } else {
        println!("Task {} · {} · {}", result.id, result.status, result.reason);
    }
    if result.status == "completed" {
        Ok(())
    } else {
        Err(result.reason)
    }
}

pub fn inject_context(input: &mut Value, instructions: &str) -> Result<(), String> {
    let messages = input
        .as_array_mut()
        .ok_or("Task conversation must be a message array")?;
    if messages.is_empty() {
        return Err("Task conversation needs its system instructions".into());
    }
    let mut index = 0;
    messages.retain(|m| {
        let keep = index == 0
            || m["role"] != "system"
            || !m["content"]
                .as_str()
                .is_some_and(|s| s.starts_with("[durable-task]\n"));
        index += 1;
        keep
    });
    messages.insert(1, json!({"role":"system","content":instructions}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(root: &Path) -> Record {
        Record {
            id: uuid::Uuid::new_v4().to_string(),
            root: root.canonicalize().unwrap(),
            objective: "Implement a small change".into(),
            criteria: vec![
                "Requested behavior works".into(),
                "Existing behavior preserved".into(),
            ],
            status: "new".into(),
            reason: String::new(),
            progress: vec![],
            evidence: vec![],
            max_ms: 60_000,
            max_tokens: 100_000,
            max_requests: 2,
            used_ms: 0,
            charged_tokens: 0,
            requests: 0,
            session: String::new(),
            run_id: None,
            updated: 0,
            completion_requested: false,
            revision: 0,
        }
    }
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("long-task-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn reservations_survive_restart_and_exclusive_owner_prevents_double_run() {
        let root = root();
        let r = record(&root);
        let id = r.id.clone();
        let task = Task::own(&root, r).unwrap();
        let first = task
            .reserve(&json!({"max_output_tokens":64,"input":[]}))
            .unwrap();
        assert!(first >= 64);
        task.account(
            first,
            &json!({"usage":{"input_tokens":10,"output_tokens":5}}),
        )
        .unwrap();
        assert!(Task::own(&root, read(&root, &id).unwrap()).is_err());
        task.stop("User paused").unwrap();
        drop(task);
        let task = Task::own(&root, read(&root, &id).unwrap()).unwrap();
        assert_eq!(task.snapshot().unwrap().charged_tokens, 15);
        task.reserve(&json!({"max_output_tokens":64})).unwrap();
        assert!(task.reserve(&json!({"max_output_tokens":64})).is_err());
        assert_eq!(task.snapshot().unwrap().requests, 2);
        drop(task);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn criteria_evidence_is_required_and_completion_is_not_a_short_answer() {
        let root = root();
        let task = Task::own(&root, record(&root)).unwrap();
        assert!(!task.final_answer().unwrap());
        assert!(task
            .action(json!({"action":"complete","summary":"done","evidence":["one"]}))
            .is_err());
        task.action(json!({"action":"checkpoint","summary":"Implemented the requested change"}))
            .unwrap();
        task.action(json!({"action":"complete","summary":"Done","evidence":["Behavior checked in fixture","Original tests preserved"]})).unwrap();
        assert!(task.final_answer().unwrap());
        assert_eq!(task.snapshot().unwrap().status, "running");
        task.complete().unwrap();
        assert_eq!(task.snapshot().unwrap().status, "completed");
        let r = task.snapshot().unwrap();
        drop(task);
        assert!(Task::own(&root, r).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stalled_answers_deadline_and_user_stop_do_not_turn_into_success() {
        let root = root();
        let task = Task::own(&root, record(&root)).unwrap();
        assert!(!task.final_answer().unwrap());
        assert!(!task.final_answer().unwrap());
        assert!(task.final_answer().unwrap_err().contains("Stalled"));
        persistence::atomic_write(&task.path.with_extension("stop"), b"stop").unwrap();
        assert!(task.tick().unwrap_err().contains("Stop requested"));
        drop(task);
        let mut r = record(&root);
        r.max_ms = 1;
        let task = Task::own(&root, r).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        assert!(task.reserve(&json!({})).unwrap_err().contains("Wall-time"));
        assert_eq!(task.snapshot().unwrap().requests, 0);
        drop(task);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn refreshed_task_context_preserves_later_directory_instructions() {
        let mut input = json!([{"role":"system","content":"System rules and directory instructions mentioning [durable-task] tags"},{"role":"system","content":"[durable-task]\nold\n[/durable-task]"},{"role":"user","content":"Original user requirement"}]);
        inject_context(&mut input, "[durable-task]\nnew\n[/durable-task]").unwrap();
        assert!(input[0]["content"]
            .as_str()
            .unwrap()
            .contains("directory instructions"));
        assert_eq!(input.as_array().unwrap().len(), 3);
        assert!(input[1]["content"].as_str().unwrap().contains("new"));
        assert_eq!(input[2]["content"], "Original user requirement");
    }
}
