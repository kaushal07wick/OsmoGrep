mod agent;
mod commands;
mod context;
mod harness;
mod hooks;
mod logger;
mod mcp;
mod persistence;
mod process_runner;
mod shell_guard;
mod state;
mod test_harness;
mod tool_budget;
mod tool_guard;
mod tools;
mod triage;
mod ui;
mod verify_stop;
mod verification;
mod voice;
mod worktree;

use std::{
    error::Error,
    fs, io,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{
    event, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};

use clap::{Args, Parser, Subcommand};

use crate::{
    agent::{Agent, AgentEvent, CancelToken, RunControl},
    context::ContextEvent,
    logger::{
        flush_streaming_log, log, log_agent_output, log_status, log_tool_call, log_tool_result,
        log_user_input, update_streaming_log,
    },
    state::{
        AgentState, DiffSnapshot, InputMode, JobKind, JobStatus, LogLevel, PermissionProfile,
        MAX_CONVERSATION_TOKENS,
    },
    ui::{main_ui::handle_event, tui::draw_ui},
};

enum JobEvent {
    Finished {
        id: u64,
        ok: bool,
        output: String,
        kind: JobKind,
    },
}

#[derive(Parser)]
#[command(
    name = "osmogrep",
    version,
    about = "A lightweight Rust-based TUI Agent, for debugging, code reviews, and runtime bug catching."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Run the coding agent headlessly and print events to stdout
    Run(RunArgs),
    /// Analyze GitHub PRs/issues for duplicates, ranking, and scope drift
    Triage(triage::TriageArgs),
}

#[derive(Args, Debug)]
struct RunArgs {
    /// Repository root for tool execution
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,

    /// Prompt text to send to the agent
    #[arg(long, conflicts_with = "prompt_file")]
    prompt: Option<String>,

    /// File containing the prompt to send to the agent
    #[arg(long)]
    prompt_file: Option<PathBuf>,

    /// Emit newline-delimited JSON events for non-TUI callers
    #[arg(long, default_value_t = false)]
    json_events: bool,

    /// Permission profile: read-only, workspace-auto, or full-access
    #[arg(long, default_value = "workspace-auto")]
    permission_profile: String,

    /// Approve dangerous workspace actions without an interactive prompt
    #[arg(long, default_value_t = false)]
    auto_approve: bool,
}

fn env_truthy(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(val) => {
            let v = val.to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => default,
    }
}

fn agent_iteration_limit() -> usize {
    std::env::var("OSMOGREP_MAX_ITERATIONS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(90)
}

fn run_shell(state: &mut AgentState, cmd: &str) {
    log(state, LogLevel::Info, &format!("SHELL : $ {}", cmd));

    if let Err(e) = crate::shell_guard::check_shell_command(cmd) {
        log(state, LogLevel::Error, e);
        return;
    }

    let timeout = crate::process_runner::timeout_from_env("OSMOGREP_SHELL_TIMEOUT_SECS", 120);
    match crate::process_runner::run_shell_command(cmd, Some(&state.repo_root), timeout) {
        Ok(out) => {
            let mut combined = String::new();
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                combined.push_str(line);
                combined.push('\n');
                log(state, LogLevel::Info, line);
            }
            for line in String::from_utf8_lossy(&out.stderr).lines() {
                combined.push_str(line);
                combined.push('\n');
                log(state, LogLevel::Error, line);
            }
            if let Some(ev) =
                crate::verification::record_command(&state.repo_root, cmd, out.exit_code, &combined)
            {
                log(
                    state,
                    if ev.status == "passed" {
                        LogLevel::Success
                    } else {
                        LogLevel::Error
                    },
                    format!(
                        "Verification evidence [{}:{}:{}] {}",
                        ev.kind, ev.scope, ev.status, ev.canonical_command
                    ),
                );
            }
            if out.timed_out {
                log(
                    state,
                    LogLevel::Error,
                    format!("Shell command timed out after {}ms", out.duration_ms),
                );
            }
        }
        Err(e) => {
            log(state, LogLevel::Error, e.to_string());
        }
    }
}

fn warn_if_verification_needed(state: &mut AgentState) {
    if state.session_changes.is_empty() {
        return;
    }

    let status = crate::verification::latest_status(&state.repo_root);
    if !status.needs_verification {
        return;
    }

    let level = if status.status == "failed" {
        LogLevel::Error
    } else {
        LogLevel::Warn
    };
    log(
        state,
        level,
        format!(
            "Verification status is {}. Run /verify or /test before claiming the work is complete.",
            status.status
        ),
    );

    if !status.verifiable_changed_paths.is_empty() {
        let mut paths = status
            .verifiable_changed_paths
            .iter()
            .take(6)
            .map(|path| format!("`{path}`"))
            .collect::<Vec<_>>();
        let remaining = status.verifiable_changed_paths.len().saturating_sub(6);
        if remaining > 0 {
            paths.push(format!("+{remaining} more"));
        }
        log(
            state,
            LogLevel::Warn,
            format!("Unverified paths: {}", paths.join(", ")),
        );
    }
}

fn queue_auto_review_if_needed(state: &mut AgentState) {
    let changes = unreviewed_changes(&state.session_changes, state.reviewed_change_count);
    if changes.is_empty() {
        state.reviewed_change_count = state.reviewed_change_count.min(state.session_changes.len());
        return;
    }

    let changes = changes.to_vec();
    let Ok(input) = serde_json::to_string(&changes) else {
        log(
            state,
            LogLevel::Error,
            "Auto-review could not serialize session changes.",
        );
        return;
    };

    let id = state.next_job_id;
    state.next_job_id += 1;
    state.jobs.push(crate::state::JobRecord {
        id,
        kind: JobKind::Review,
        input: format!("{} change(s)", changes.len()),
        status: JobStatus::Queued,
        output: None,
    });
    state.job_queue.push(crate::state::JobRequest {
        id,
        kind: JobKind::Review,
        input,
    });
    state.reviewed_change_count = state.session_changes.len();
    let _ = persistence::save(state);
    log(
        state,
        LogLevel::Info,
        format!("Auto-review queued as job #{}", id),
    );
}

fn unreviewed_changes(changes: &[DiffSnapshot], reviewed_change_count: usize) -> &[DiffSnapshot] {
    let reviewed = reviewed_change_count.min(changes.len());
    &changes[reviewed..]
}

fn start_agent_run(
    state: &mut AgentState,
    agent: &Agent,
    text: &str,
    agent_rx: &mut Option<mpsc::Receiver<AgentEvent>>,
    agent_cancel: &mut Option<CancelToken>,
    agent_steer_tx: &mut Option<mpsc::Sender<String>>,
) {
    if !agent.is_configured() {
        log(state, LogLevel::Warn, "OPENAI_API_KEY not set. Use /key");
        return;
    }
    if text.trim().is_empty() {
        return;
    }

    let (tx, rx) = mpsc::channel();
    let repo_root = state.repo_root.clone();
    let prior_messages = state.conversation.messages.clone();
    let auto_approve = state.ui.auto_approve;
    let steer = state.steer.clone();
    let permission_profile = state.permission_profile;

    let RunControl { cancel, steer_tx } = agent.spawn(
        repo_root,
        text.to_string(),
        prior_messages,
        steer,
        permission_profile,
        auto_approve,
        tx,
    );
    *agent_rx = Some(rx);
    *agent_cancel = Some(cancel);
    *agent_steer_tx = Some(steer_tx);
    state.ui.queued_agent_prompt = None;
    state.ui.last_activity = Instant::now();
    state.ui.hint = None;
    state.ui.autocomplete = None;
    state.ui.input_mode = InputMode::AgentText;
    state.ui.input_masked = false;
    state.ui.input_placeholder = None;
    state.ui.history_index = None;
    state.ui.command_items.clear();
    state.ui.command_selected = 0;
    state.ui.follow_tail = true;
    state.ui.exec_scroll = usize::MAX;
    state.ui.spinner_started_at = Some(Instant::now());
    state.ui.agent_running = true;
    state.ui.run_phase = "starting".to_string();
    state.ui.run_detail = Some("building request".to_string());
    state.ui.run_iteration = 0;
    state.ui.run_iteration_limit = agent_iteration_limit();
    state.ui.current_tool = None;
    state.ui.current_tool_detail = None;
    state.ui.last_tool_status = None;
    state.ui.streaming_active = false;
    state.ui.streaming_buffer.clear();
    state.ui.streaming_lines_logged = 0;
    state.ui.active_edit_target = None;
    state.usage.prompt_tokens += (text.len() / 4).max(1);
    let _ = persistence::save(state);
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if let Some(CliCommand::Run(args)) = cli.command {
        let code = run_headless(args)?;
        if code != 0 {
            std::process::exit(code);
        }
        return Ok(());
    }
    if let Some(CliCommand::Triage(args)) = cli.command {
        triage::run(args)?;
        return Ok(());
    }

    run_tui()
}

fn run_headless(args: RunArgs) -> Result<i32, Box<dyn Error>> {
    let prompt = match (args.prompt, args.prompt_file) {
        (Some(prompt), None) => prompt,
        (None, Some(path)) => fs::read_to_string(path)?,
        (None, None) => {
            return Err("provide --prompt or --prompt-file".into());
        }
        (Some(_), Some(_)) => {
            return Err("use only one of --prompt or --prompt-file".into());
        }
    };

    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("prompt is empty".into());
    }

    let repo_root = fs::canonicalize(&args.repo_root).unwrap_or(args.repo_root);
    let permission_profile = PermissionProfile::parse(&args.permission_profile)
        .ok_or("permission profile must be read-only, workspace-auto, or full-access")?;
    let agent = Agent::new();
    if !agent.is_configured() {
        return Err("OPENAI_API_KEY is not set".into());
    }

    let (tx, rx) = mpsc::channel();
    let _control = agent.spawn(
        repo_root,
        prompt,
        Vec::new(),
        None,
        permission_profile,
        args.auto_approve,
        tx,
    );

    let mut exit_code = 0;
    loop {
        let evt = match rx.recv() {
            Ok(evt) => evt,
            Err(_) => {
                exit_code = exit_code.max(1);
                break;
            }
        };

        let done = matches!(evt, AgentEvent::Done);
        let failed = matches!(evt, AgentEvent::Error(_) | AgentEvent::Cancelled);
        emit_headless_event(evt, args.json_events, args.auto_approve);
        if failed {
            exit_code = exit_code.max(1);
        }
        if done || failed {
            break;
        }
    }

    Ok(exit_code)
}

fn emit_headless_event(evt: AgentEvent, json_events: bool, auto_approve: bool) {
    if json_events {
        emit_headless_json(evt, auto_approve);
    } else {
        emit_headless_text(evt, auto_approve);
    }
    let _ = io::stdout().flush();
}

fn emit_headless_json(evt: AgentEvent, auto_approve: bool) {
    let value = match evt {
        AgentEvent::ToolCall { name, args } => {
            serde_json::json!({ "type": "tool_call", "name": name, "args": args })
        }
        AgentEvent::ToolResult { summary } => {
            serde_json::json!({ "type": "tool_result", "summary": summary })
        }
        AgentEvent::ToolDiff {
            tool,
            target,
            before,
            after,
        } => serde_json::json!({
            "type": "tool_diff",
            "tool": tool,
            "target": target,
            "before": before,
            "after": after
        }),
        AgentEvent::PreviewDiff {
            tool,
            target,
            before,
            after,
        } => serde_json::json!({
            "type": "preview_diff",
            "tool": tool,
            "target": target,
            "before": before,
            "after": after
        }),
        AgentEvent::OutputText(text) => {
            serde_json::json!({ "type": "output_text", "text": text })
        }
        AgentEvent::StreamDelta(text) => {
            serde_json::json!({ "type": "stream_delta", "text": text })
        }
        AgentEvent::StreamDone => serde_json::json!({ "type": "stream_done" }),
        AgentEvent::RunStatus {
            phase,
            detail,
            iteration,
            max_iterations,
        } => serde_json::json!({
            "type": "run_status",
            "phase": phase,
            "detail": detail,
            "iteration": iteration,
            "max_iterations": max_iterations
        }),
        AgentEvent::PermissionRequest {
            tool_name,
            args_summary,
            reply_tx,
        } => {
            let approved = auto_approve;
            let _ = reply_tx.send(approved);
            serde_json::json!({
                "type": "permission_request",
                "tool_name": tool_name,
                "args_summary": args_summary,
                "approved": approved
            })
        }
        AgentEvent::ConversationUpdate(_) => {
            serde_json::json!({ "type": "conversation_update" })
        }
        AgentEvent::Cancelled => serde_json::json!({ "type": "cancelled" }),
        AgentEvent::Error(message) => {
            serde_json::json!({ "type": "error", "message": message })
        }
        AgentEvent::Done => serde_json::json!({ "type": "done" }),
    };
    println!("{}", value);
}

fn emit_headless_text(evt: AgentEvent, auto_approve: bool) {
    match evt {
        AgentEvent::ToolCall { name, args } => {
            println!("[tool] {} {}", name, compact_json(&args));
        }
        AgentEvent::ToolResult { summary } => println!("[result] {}", summary),
        AgentEvent::ToolDiff { tool, target, .. } => {
            println!("[diff] {} {}", tool, target);
        }
        AgentEvent::PreviewDiff { tool, target, .. } => {
            println!("[preview] {} {}", tool, target);
        }
        AgentEvent::OutputText(text) => println!("{}", text),
        AgentEvent::StreamDelta(text) => print!("{}", text),
        AgentEvent::StreamDone => println!(),
        AgentEvent::RunStatus {
            phase,
            detail,
            iteration,
            max_iterations,
        } => {
            println!("[status] {phase} {iteration}/{max_iterations} {detail}");
        }
        AgentEvent::PermissionRequest {
            tool_name,
            args_summary,
            reply_tx,
        } => {
            let approved = auto_approve;
            let _ = reply_tx.send(approved);
            println!(
                "[permission] {} {} {}",
                if approved { "approved" } else { "denied" },
                tool_name,
                args_summary
            );
        }
        AgentEvent::ConversationUpdate(_) => {}
        AgentEvent::Cancelled => println!("[cancelled]"),
        AgentEvent::Error(message) => eprintln!("[error] {}", message),
        AgentEvent::Done => println!("[done]"),
    }
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn run_tui() -> Result<(), Box<dyn Error>> {
    setup_terminal()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = init_state();
    persistence::load(&mut state);
    let mut agent = Agent::new();
    if env_truthy("OSMOGREP_NV_TIPS", false) {
        log(
            &mut state,
            LogLevel::Info,
            "nvim tips: :q quit, :wq save+quit, :qa! quit all without saving",
        );
        log(
            &mut state,
            LogLevel::Info,
            "pane tips: Ctrl+w h/l switch panes, tmux detach: Ctrl+b then d",
        );
    }

    let (voice_cmd_tx, voice_cmd_rx) = mpsc::channel();
    let (voice_evt_tx, voice_evt_rx) = mpsc::channel();
    let _voice_handle = voice::spawn_voice_worker(voice_cmd_rx, voice_evt_tx.clone());
    let proxy_listen = std::env::var("VLLM_REALTIME_PROXY_LISTEN").ok();
    if let Some(listen_addr) = proxy_listen.clone() {
        let _proxy_handle = voice::spawn_voice_proxy_worker(
            listen_addr.clone(),
            state.voice.url.clone(),
            state.voice.model.clone(),
            voice_evt_tx.clone(),
        );
        state.voice.visible = true;
        state.voice.enabled = true;
    } else if env_truthy("VLLM_REALTIME_AUTOCONNECT", false) {
        let _ = voice_cmd_tx.send(voice::VoiceCommand::Start {
            url: state.voice.url.clone(),
            model: state.voice.model.clone(),
        });
        state.voice.visible = true;
        state.voice.enabled = true;
        log_status(&mut state, "Connecting voice input...");
    }

    let mut agent_rx: Option<mpsc::Receiver<AgentEvent>> = None;
    let mut agent_cancel: Option<CancelToken> = None;
    let mut agent_steer_tx: Option<mpsc::Sender<String>> = None;
    let (job_tx, job_rx) = mpsc::channel::<JobEvent>();
    let mut running_jobs = 0usize;
    let mut context_rx: Option<mpsc::Receiver<ContextEvent>> = None;
    let voice_silence_ms: u64 = std::env::var("VLLM_REALTIME_SILENCE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1200);

    /* ---------- SPAWN CONTEXT INDEXER ---------- */

    {
        let (tx, rx) = mpsc::channel();
        let root = state.repo_root.clone();

        context::spawn_indexer(root, tx);
        context_rx = Some(rx);
    }

    /* ---------- MAIN LOOP ---------- */

    loop {
        let (input_rect, _, exec_rect) = draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(120))? {
            let ev = event::read()?;
            handle_event(&mut state, ev, input_rect, Rect::default(), exec_rect);
        }

        if state.ui.should_exit {
            let _ = persistence::save(&state);
            break;
        }

        loop {
            match job_rx.try_recv() {
                Ok(JobEvent::Finished {
                    id,
                    ok,
                    output,
                    kind,
                }) => {
                    running_jobs = running_jobs.saturating_sub(1);
                    if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
                        job.status = if ok {
                            JobStatus::Done
                        } else {
                            JobStatus::Failed
                        };
                        job.output = Some(output.clone());
                    }
                    log(
                        &mut state,
                        if ok {
                            LogLevel::Success
                        } else {
                            LogLevel::Error
                        },
                        format!(
                            "Job #{} [{}] {}",
                            id,
                            kind.as_str(),
                            if ok { "completed" } else { "failed" }
                        ),
                    );
                    for line in output.lines().take(24) {
                        log(&mut state, LogLevel::Info, line.to_string());
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        while running_jobs < 2 && !state.job_queue.is_empty() {
            let req = state.job_queue.remove(0);
            if let Some(job) = state.jobs.iter_mut().find(|j| j.id == req.id) {
                job.status = JobStatus::Running;
            }

            let tx = job_tx.clone();
            let model_cfg = agent.model_config().clone();
            let api_key = agent.api_key();
            let repo_root = state.repo_root.clone();
            running_jobs += 1;

            std::thread::spawn(move || {
                let (ok, output, kind) = match req.kind {
                    JobKind::Swarm => match api_key {
                        Some(k) => match agent::run_swarm_job(model_cfg, k, req.input.clone()) {
                            Ok(s) => (true, s, JobKind::Swarm),
                            Err(e) => (false, e, JobKind::Swarm),
                        },
                        None => (false, "OPENAI_API_KEY not set".to_string(), JobKind::Swarm),
                    },
                    JobKind::Review => match api_key {
                        Some(k) => match serde_json::from_str::<Vec<DiffSnapshot>>(&req.input) {
                            Ok(changes) => match agent::run_review_job(model_cfg, k, changes) {
                                Ok(s) => (true, s, JobKind::Review),
                                Err(e) => (false, e, JobKind::Review),
                            },
                            Err(e) => (false, e.to_string(), JobKind::Review),
                        },
                        None => (false, "OPENAI_API_KEY not set".to_string(), JobKind::Review),
                    },
                    JobKind::Test => {
                        let target = if req.input.trim().is_empty() {
                            None
                        } else {
                            Some(req.input.as_str())
                        };
                        match test_harness::run_tests(&repo_root, target) {
                            Ok(run) => (
                                run.success,
                                format!(
                                    "framework={} exit={} passed={} failed={}\n{}",
                                    run.framework,
                                    run.exit_code,
                                    run.passed,
                                    run.failed,
                                    run.output
                                ),
                                JobKind::Test,
                            ),
                            Err(e) => (false, e, JobKind::Test),
                        }
                    }
                };
                let _ = tx.send(JobEvent::Finished {
                    id: req.id,
                    ok,
                    output,
                    kind,
                });
            });
        }

        if let Some(mut rx) = context_rx.take() {
            let mut done = false;

            loop {
                match rx.try_recv() {
                    Ok(evt) => match evt {
                        ContextEvent::Started => {
                            state.ui.indexing = true;
                            state.ui.indexed = false;
                            state.ui.spinner_started_at = Some(Instant::now());
                        }

                        ContextEvent::Finished => {
                            state.ui.indexing = false;
                            state.ui.indexed = true;
                            state.ui.spinner_started_at = None;
                            done = true;
                        }

                        ContextEvent::Error(_e) => {
                            state.ui.indexing = false;
                            state.ui.spinner_started_at = None;
                            done = true;
                        }
                    },

                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        state.ui.indexing = false;
                        state.ui.spinner_started_at = None;
                        done = true;
                        break;
                    }
                }
            }
            if !done {
                context_rx = Some(rx);
            }
        }

        loop {
            match voice_evt_rx.try_recv() {
                Ok(evt) => match evt {
                    voice::VoiceEvent::Connected => {
                        state.voice.visible = true;
                        state.voice.connected = true;
                        state.voice.status = Some("connected".into());
                        state.voice.buffer.clear();
                        state.voice.last_activity = Some(Instant::now());
                        state.voice.last_inserted = None;
                        log_status(&mut state, "Voice connected.");
                    }
                    voice::VoiceEvent::Disconnected => {
                        state.voice.connected = false;
                        state.voice.enabled = false;
                        state.voice.status = Some("disconnected".into());
                        state.voice.partial = None;
                        state.voice.last_final = None;
                        state.voice.buffer.clear();
                        state.voice.last_activity = None;
                        state.voice.last_inserted = None;
                    }
                    voice::VoiceEvent::Partial(delta) => {
                        state.voice.partial = Some(delta);
                        state.voice.last_final = None;
                        if let Some(delta) = state.voice.partial.as_deref() {
                            state.voice.buffer.push_str(delta);
                        }
                        state.voice.last_activity = Some(Instant::now());
                        if state.ui.input.is_empty()
                            || state.voice.last_inserted.as_deref() == Some(state.ui.input.as_str())
                        {
                            state.ui.input = state.voice.buffer.clone();
                            state.voice.last_inserted = Some(state.ui.input.clone());
                        }
                    }
                    voice::VoiceEvent::Final(text) => {
                        let final_text = if text.trim().is_empty() {
                            state.voice.buffer.clone()
                        } else {
                            text
                        };
                        state.ui.input_mode = InputMode::AgentText;
                        state.ui.input_masked = false;
                        state.ui.input_placeholder = None;
                        state.ui.history_index = None;
                        state.clear_hint();
                        state.clear_autocomplete();
                        if !final_text.trim().is_empty() {
                            if state.ui.input.is_empty()
                                || state.voice.last_inserted.as_deref()
                                    == Some(state.ui.input.as_str())
                            {
                                state.ui.input = final_text.clone();
                            } else {
                                if !state.ui.input.ends_with(' ') {
                                    state.ui.input.push(' ');
                                }
                                state.ui.input.push_str(&final_text);
                            }
                            state.voice.last_inserted = Some(state.ui.input.clone());
                        }
                        state.voice.partial = None;
                        state.voice.last_final = Some(final_text);
                        state.voice.buffer.clear();
                        state.voice.last_activity = Some(Instant::now());
                    }
                    voice::VoiceEvent::Error(msg) => {
                        state.voice.visible = true;
                        state.voice.enabled = false;
                        state.voice.connected = false;
                        state.voice.status = Some(format!("error: {msg}"));
                        state.voice.buffer.clear();
                        state.voice.last_activity = None;
                        state.voice.last_inserted = None;
                        log(&mut state, LogLevel::Error, msg);
                    }
                    voice::VoiceEvent::Status(msg) => {
                        let status = msg.clone();
                        state.voice.status = Some(msg);
                        log_status(&mut state, status);
                    }
                },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        if state.voice.connected {
            if let Some(last) = state.voice.last_activity {
                if !state.voice.buffer.is_empty()
                    && last.elapsed() >= Duration::from_millis(voice_silence_ms)
                {
                    let final_text = state.voice.buffer.clone();
                    if !final_text.trim().is_empty() {
                        state.ui.input_mode = InputMode::AgentText;
                        state.ui.input_masked = false;
                        state.ui.input_placeholder = None;
                        state.ui.history_index = None;
                        state.clear_hint();
                        state.clear_autocomplete();
                        if state.ui.input.is_empty()
                            || state.voice.last_inserted.as_deref() == Some(state.ui.input.as_str())
                        {
                            state.ui.input = final_text.clone();
                        } else {
                            if !state.ui.input.ends_with(' ') {
                                state.ui.input.push(' ');
                            }
                            state.ui.input.push_str(&final_text);
                        }
                        state.voice.last_final = Some(final_text);
                        state.voice.last_inserted = Some(state.ui.input.clone());
                    }
                    state.voice.buffer.clear();
                    state.voice.partial = None;
                    state.voice.last_activity = Some(Instant::now());
                }
            }
        }

        if state.ui.cancel_requested {
            if let Some(token) = agent_cancel.as_ref() {
                token.cancel();
                log_status(&mut state, "Cancellation requested.");
            }
            state.ui.cancel_requested = false;
        }

        if let Some(rx) = agent_rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(evt) => match evt {
                        AgentEvent::ToolCall { name, args } => {
                            let cmd = match args {
                                serde_json::Value::Object(ref map) => map
                                    .values()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                _ => String::new(),
                            };

                            log_tool_call(&mut state, &name, cmd);
                            state.ui.run_phase = "tool".to_string();
                            state.ui.current_tool = Some(name.clone());
                            state.ui.current_tool_detail = Some(compact_json(&args));
                            state.ui.active_edit_target = tool_target_path(&name, &args);
                        }

                        AgentEvent::ToolResult { summary } => {
                            state.ui.last_tool_status = Some(summary.clone());
                            log_tool_result(&mut state, summary);
                        }

                        AgentEvent::ToolDiff {
                            tool,
                            target,
                            before,
                            after,
                        } => {
                            let snap = DiffSnapshot {
                                tool,
                                target,
                                before,
                                after,
                            };

                            state.session_changes.push(snap.clone());
                            state.undo_stack.push(snap.clone());
                            state.ui.diff_active = true;
                            state.ui.diff_snapshot = vec![snap];
                            state.ui.active_edit_target =
                                state.ui.diff_snapshot.first().map(|d| d.target.clone());
                            let _ = persistence::save(&state);
                        }

                        AgentEvent::PreviewDiff {
                            tool,
                            target,
                            before,
                            after,
                        } => {
                            state.ui.diff_active = true;
                            state.ui.diff_snapshot = vec![DiffSnapshot {
                                tool: format!("preview:{tool}"),
                                target,
                                before,
                                after,
                            }];
                        }

                        AgentEvent::OutputText(text) => {
                            state.usage.completion_tokens += (text.len() / 4).max(1);
                            if !state.ui.streaming_active {
                                log_agent_output(&mut state, &text);
                            }
                            let _ = persistence::save(&state);
                        }

                        AgentEvent::StreamDelta(delta) => {
                            state.ui.streaming_active = true;
                            state.ui.follow_tail = true;
                            state.ui.streaming_buffer.push_str(&delta);
                            update_streaming_log(&mut state);
                        }

                        AgentEvent::StreamDone => {
                            if state.ui.streaming_active {
                                flush_streaming_log(&mut state);
                            }
                            state.ui.streaming_active = false;
                        }

                        AgentEvent::RunStatus {
                            phase,
                            detail,
                            iteration,
                            max_iterations,
                        } => {
                            state.ui.run_phase = phase;
                            state.ui.run_detail = Some(detail);
                            state.ui.run_iteration = iteration;
                            state.ui.run_iteration_limit = max_iterations;
                        }

                        AgentEvent::PermissionRequest {
                            tool_name,
                            args_summary,
                            reply_tx,
                        } => {
                            if state.ui.auto_approve {
                                let _ = reply_tx.send(true);
                                log_status(
                                    &mut state,
                                    format!("Auto-approved {} ({})", tool_name, args_summary),
                                );
                            } else {
                                state.ui.pending_permission =
                                    Some(crate::state::PendingPermission {
                                        tool_name,
                                        args_summary,
                                        reply_tx,
                                    });
                            }
                        }

                        AgentEvent::ConversationUpdate(messages) => {
                            state.conversation.set_messages(messages);
                            state.conversation.trim_to_budget(MAX_CONVERSATION_TOKENS);
                            let _ = persistence::save(&state);
                        }

                        AgentEvent::Cancelled => {
                            state.ui.streaming_active = false;
                            flush_streaming_log(&mut state);
                            log(&mut state, LogLevel::Warn, "Agent cancelled.");
                            state.ui.spinner_started_at = None;
                            state.ui.agent_running = false;
                            state.ui.run_phase = "cancelled".to_string();
                            state.ui.current_tool = None;
                            state.ui.current_tool_detail = None;
                            state.ui.pending_permission = None;
                            state.ui.active_edit_target = None;
                            agent_cancel = None;
                            agent_steer_tx = None;
                            agent_rx = None;
                            break;
                        }

                        AgentEvent::Error(e) => {
                            log(&mut state, LogLevel::Error, e);
                            state.ui.streaming_active = false;
                            flush_streaming_log(&mut state);
                            state.ui.spinner_started_at = None;
                            state.ui.agent_running = false;
                            state.ui.run_phase = "error".to_string();
                            state.ui.current_tool = None;
                            state.ui.current_tool_detail = None;
                            state.ui.pending_permission = None;
                            state.ui.active_edit_target = None;
                            agent_cancel = None;
                            agent_steer_tx = None;
                            agent_rx = None;
                            break;
                        }

                        AgentEvent::Done => {
                            state.ui.spinner_started_at = None;
                            state.ui.agent_running = false;
                            state.ui.run_phase = "idle".to_string();
                            state.ui.current_tool = None;
                            state.ui.current_tool_detail = None;
                            state.ui.pending_permission = None;
                            state.ui.active_edit_target = None;
                            warn_if_verification_needed(&mut state);
                            queue_auto_review_if_needed(&mut state);
                            if state.auto_eval && !state.session_changes.is_empty() {
                                let id = state.next_job_id;
                                state.next_job_id += 1;
                                state.jobs.push(crate::state::JobRecord {
                                    id,
                                    kind: JobKind::Test,
                                    input: String::new(),
                                    status: JobStatus::Queued,
                                    output: None,
                                });
                                state.job_queue.push(crate::state::JobRequest {
                                    id,
                                    kind: JobKind::Test,
                                    input: String::new(),
                                });
                                log(
                                    &mut state,
                                    LogLevel::Info,
                                    format!("Auto-eval queued as job #{}", id),
                                );
                            }
                            agent_cancel = None;
                            agent_steer_tx = None;
                            agent_rx = None;
                            break;
                        }
                    },

                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        state.ui.streaming_active = false;
                        flush_streaming_log(&mut state);
                        state.ui.spinner_started_at = None;
                        state.ui.agent_running = false;
                        state.ui.run_phase = "disconnected".to_string();
                        state.ui.current_tool = None;
                        state.ui.current_tool_detail = None;
                        state.ui.pending_permission = None;
                        state.ui.active_edit_target = None;
                        agent_cancel = None;
                        agent_steer_tx = None;
                        agent_rx = None;
                        break;
                    }
                }
            }
        }

        if state.ui.execution_pending {
            state.ui.execution_pending = false;

            let mode = state.ui.input_mode;
            let raw = state.commit_input();
            let text = raw.trim();

            state.ui.hint = None;
            state.ui.autocomplete = None;

            match mode {
                InputMode::ApiKey => {
                    log(&mut state, LogLevel::Info, "[api key entered]");
                }

                _ if !text.is_empty() => {
                    log_user_input(&mut state, text);
                }

                _ => {}
            }

            match mode {
                InputMode::Command => {
                    commands::handle_command(
                        &mut state,
                        text,
                        Some(&voice_cmd_tx),
                        Some(&mut agent),
                        agent_steer_tx.as_ref(),
                    );
                }

                InputMode::Shell => {
                    if !text.is_empty() {
                        run_shell(&mut state, text);
                    }
                    continue;
                }

                InputMode::AgentText => {
                    if !text.is_empty() {
                        if agent_rx.is_some() {
                            if let Some(tx) = agent_steer_tx.as_ref() {
                                let _ = tx.send(text.to_string());
                                log_status(&mut state, "Steer sent to running agent.");
                            } else {
                                state.ui.queued_agent_prompt = Some(text.to_string());
                                log_status(&mut state, "Queued follow-up prompt.");
                            }
                        } else {
                            start_agent_run(
                                &mut state,
                                &agent,
                                text,
                                &mut agent_rx,
                                &mut agent_cancel,
                                &mut agent_steer_tx,
                            );
                        }
                    }
                }

                InputMode::ApiKey => {
                    if !text.is_empty() {
                        agent.set_api_key(text.to_string());
                        log(&mut state, LogLevel::Success, "API key saved.");

                        state.ui.input_mode = InputMode::AgentText;
                        state.ui.input_masked = false;
                        state.ui.input_placeholder = None;
                    }
                }
            }
        }

        if agent_rx.is_none() {
            if let Some(next_prompt) = state.ui.queued_agent_prompt.take() {
                log_user_input(&mut state, &next_prompt);
                start_agent_run(
                    &mut state,
                    &agent,
                    &next_prompt,
                    &mut agent_rx,
                    &mut agent_cancel,
                    &mut agent_steer_tx,
                );
            }
        }

        if !state.ui.execution_pending && agent_rx.is_none() {
            commands::update_command_hints(&mut state);
        }
    }

    let attach_session = state.ui.tmux_attach_session.clone();
    let managed_tmux = env_truthy("OSMOGREP_MANAGED_TMUX", false);
    let managed_session = std::env::var("OSMOGREP_NV_SESSION").ok();
    teardown_terminal(&mut terminal)?;

    if managed_tmux {
        let target = managed_session.or_else(current_tmux_session_name);
        if let Some(session) = target {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &session])
                .status();
        }
        return Ok(());
    }

    if let Some(session) = attach_session {
        let _ = Command::new("tmux")
            .args(["attach-session", "-t", &session])
            .status();
    }
    Ok(())
}

fn init_state() -> AgentState {
    let voice_url = std::env::var("VLLM_REALTIME_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8000/v1/realtime".into());
    let voice_model = std::env::var("VLLM_REALTIME_MODEL")
        .unwrap_or_else(|_| "mistralai/Voxtral-Mini-4B-Realtime-2602".into());

    AgentState {
        ui: crate::state::UiState {
            input: String::new(),
            input_mode: InputMode::AgentText,
            input_masked: false,
            input_placeholder: None,
            execution_pending: false,
            should_exit: false,
            history: Vec::new(),
            history_index: None,
            hint: None,
            autocomplete: None,
            command_items: Vec::new(),
            command_selected: 0,
            last_activity: Instant::now(),
            exec_scroll: usize::MAX,
            follow_tail: true,
            active_spinner: None,
            spinner_started_at: None,
            agent_running: false,
            run_phase: "idle".to_string(),
            run_detail: None,
            run_iteration: 0,
            run_iteration_limit: 0,
            current_tool: None,
            current_tool_detail: None,
            last_tool_status: None,
            cancel_requested: false,
            auto_approve: false,
            pending_permission: None,
            streaming_buffer: String::new(),
            streaming_active: false,
            streaming_lines_logged: 0,
            tmux_attach_session: None,
            active_edit_target: None,
            queued_agent_prompt: None,
            diff_active: false,
            diff_snapshot: Vec::new(),

            indexing: false,
            indexed: false,
        },

        logs: crate::state::LogBuffer::new(),
        session_changes: Vec::new(),
        reviewed_change_count: 0,
        undo_stack: Vec::new(),
        usage: crate::state::UsageStats::default(),
        steer: None,
        auto_eval: true,
        permission_profile: PermissionProfile::WorkspaceAuto,
        jobs: Vec::new(),
        job_queue: Vec::new(),
        next_job_id: 1,
        plan_items: Vec::new(),
        started_at: Instant::now(),
        repo_root: std::env::current_dir().unwrap(),
        voice: crate::state::VoiceState {
            visible: false,
            enabled: false,
            connected: false,
            status: None,
            partial: None,
            last_final: None,
            buffer: String::new(),
            last_activity: None,
            last_inserted: None,
            url: voice_url,
            model: voice_model,
        },
        conversation: crate::state::ConversationHistory::new(),
    }
}
fn setup_terminal() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    Ok(())
}

fn teardown_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn tool_target_path(name: &str, args: &serde_json::Value) -> Option<String> {
    let is_file_tool = matches!(
        name,
        "write_file"
            | "edit_file"
            | "read_file"
            | "patch"
            | "notebook_edit"
            | "find_definition"
            | "find_references"
    );
    if !is_file_tool {
        return None;
    }
    args.get("path")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| {
            args.get("file")
                .and_then(serde_json::Value::as_str)
                .map(|s| s.to_string())
        })
}

fn current_tmux_session_name() -> Option<String> {
    let out = Command::new("tmux")
        .args(["display-message", "-p", "#{session_name}"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreviewed_changes_returns_only_new_diffs() {
        let changes = vec![diff("a"), diff("b"), diff("c")];

        let pending = unreviewed_changes(&changes, 1);

        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].target, "b");
        assert_eq!(pending[1].target, "c");
    }

    #[test]
    fn unreviewed_changes_clamps_stale_cursor() {
        let changes = vec![diff("a")];

        let pending = unreviewed_changes(&changes, 10);

        assert!(pending.is_empty());
    }

    fn diff(target: &str) -> DiffSnapshot {
        DiffSnapshot {
            tool: "edit_file".to_string(),
            target: target.to_string(),
            before: "before".to_string(),
            after: "after".to_string(),
        }
    }
}
