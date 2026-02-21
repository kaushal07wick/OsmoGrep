// src/commands.rs

use std::env;
use std::fs;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{io::Write, path::PathBuf};

use crate::agent::Agent;
use crate::logger::log;
use crate::persistence;
use crate::state::{
    AgentState, CommandItem, InputMode, JobKind, JobRecord, JobRequest, JobStatus, LogLevel,
    PermissionProfile, PlanItem, MAX_CONVERSATION_TOKENS,
};
use crate::test_harness::run_tests;
use crate::voice::VoiceCommand;
use serde::Deserialize;
use serde_json::Value;
use std::sync::mpsc::Sender;

pub fn handle_command(
    state: &mut AgentState,
    raw: &str,
    voice_tx: Option<&Sender<VoiceCommand>>,
    agent: Option<&mut Agent>,
    steer_tx: Option<&Sender<String>>,
) {
    state.ui.last_activity = Instant::now();
    state.clear_hint();
    state.clear_autocomplete();

    let cmd_raw = raw.trim();
    let cmd = normalize_command_prefix(cmd_raw);

    if cmd.starts_with("/model ") {
        set_model(state, &cmd, agent);
        return;
    }
    if cmd.starts_with("/test ") {
        run_test(state, &cmd);
        return;
    }
    if cmd.starts_with("/steer ") {
        set_steer(state, &cmd, steer_tx);
        return;
    }
    if cmd.starts_with("/profile ") {
        set_profile(state, &cmd);
        return;
    }
    if cmd.starts_with("/swarm ") {
        run_swarm_now(state, &cmd, agent);
        return;
    }
    if cmd.starts_with("/job ") {
        handle_job(state, &cmd);
        return;
    }
    if cmd.starts_with("/plan add ") {
        plan_add(state, &cmd);
        return;
    }
    if cmd.starts_with("/plan done ") {
        plan_done(state, &cmd);
        return;
    }
    if cmd.starts_with("/autofix ") {
        set_autofix(state, &cmd);
        return;
    }
    if cmd.starts_with("/triage") || cmd.starts_with("/traige") {
        run_triage_agent(state, &cmd);
        return;
    }
    if cmd.starts_with("/gh ") {
        handle_gh_command(state, &cmd);
        return;
    }
    if cmd.starts_with("/nv ") {
        open_nv(state, &cmd);
        return;
    }

    match cmd.as_str() {
        "/help" => help(state),
        "/clear" => clear_logs(state),
        "/voice" => voice_status(state),
        "/voice on" => voice_on(state, voice_tx),
        "/voice off" => voice_off(state, voice_tx),

        "/exit" => exit_app(state),
        "/quit" | "/q" => quit_agent(state),

        "/key" => enter_api_key_mode(state),
        "/new" => new_conversation(state),
        "/approve" => toggle_auto_approve(state),
        "/model" => show_model(state, agent),
        "/test" => run_test(state, &cmd),
        "/mcp" => show_mcp(state),
        "/providers" => show_providers(state),
        "/undo" => undo_last_change(state),
        "/diff" => show_session_diff(state),
        "/steer" => show_steer(state),
        "/compact" => compact_context(state),
        "/metrics" => show_metrics(state),
        "/profile" => show_profile(state),
        "/jobs" => show_jobs(state),
        "/autofix" => show_autofix(state),
        "/triage" | "/traige" => run_triage_agent(state, &cmd),
        "/gh" => handle_gh_command(state, &cmd),
        "/nv" => open_nv(state, &cmd),
        "/plan" => show_plan(state),
        "/plan clear" => plan_clear(state),

        "" => {}

        _ => {
            log(state, LogLevel::Warn, "Unknown command. Type /help");
        }
    }
}

fn help(state: &mut AgentState) {
    use LogLevel::Info;

    log(state, Info, "Available commands:");
    log(state, Info, "  /help        Show this help");
    log(state, Info, "  /clear       Clear logs");
    log(state, Info, "  /key         Set OpenAI API key");
    log(state, Info, "  /voice       Show voice status");
    log(state, Info, "  /voice on    Start voice input");
    log(state, Info, "  /voice off   Stop voice input");
    log(state, Info, "  /model       Show active provider/model");
    log(state, Info, "  /model <provider> <model> [base_url]");
    log(state, Info, "  /test        Run auto-detected tests");
    log(
        state,
        Info,
        "  /test <arg>  Run targeted tests (framework-specific)",
    );
    log(
        state,
        Info,
        "  /mcp         Show MCP status and configured servers",
    );
    log(state, Info, "  /providers   Show available model providers");
    log(
        state,
        Info,
        "  /undo        Revert the last agent file change",
    );
    log(
        state,
        Info,
        "  /diff        Show all file changes this session",
    );
    log(state, Info, "  /compact     Compress conversation context");
    log(state, Info, "  /metrics     Show usage and queue metrics");
    log(
        state,
        Info,
        "  /profile     Show permission profile (read-only/workspace-auto/full-access)",
    );
    log(state, Info, "  /profile <name>  Set permission profile");
    log(
        state,
        Info,
        "  /approve     Toggle dangerous tool auto-approve",
    );
    log(state, Info, "  /new         Start a fresh conversation");
    log(state, Info, "  /steer       Show current steer instruction");
    log(
        state,
        Info,
        "  /steer <txt> Set persistent steer instruction",
    );
    log(
        state,
        Info,
        "  /steer now <txt> Interrupt current run and relaunch with steer",
    );
    log(state, Info, "  /steer clear Remove steer instruction");
    log(state, Info, "  /swarm <txt> Run parallel scoped sub-agents");
    log(state, Info, "  /jobs        Show background jobs");
    log(state, Info, "  /autofix     Show auto-eval mode");
    log(state, Info, "  /autofix on|off  Toggle auto post-run tests");
    log(
        state,
        Info,
        "  /triage [args]  One-command PR/Issue triage workflow in TUI",
    );
    log(state, Info, "  /gh          Show GitHub CLI/repo status");
    log(state, Info, "  /gh prs [state] [limit]      List PRs");
    log(state, Info, "  /gh issues [state] [limit]   List issues");
    log(
        state,
        Info,
        "  /gh triage [triage flags]    Run high-volume triage (3000 + deep review + brief)",
    );
    log(
        state,
        Info,
        "  /nv [file]   Open Neovim split at repo root with tree view",
    );
    log(
        state,
        Info,
        "  /nv toggle   Toggle nvim pane in current tmux window",
    );
    log(state, Info, "  /nv help     Show nvim/tmux exit shortcuts");
    log(state, Info, "  /job test [target]   Queue test job");
    log(state, Info, "  /job swarm <txt>     Queue swarm job");
    log(state, Info, "  /job resume <id>     Requeue prior job");
    log(state, Info, "  /job cancel all      Cancel queued jobs");
    log(state, Info, "  /plan        Show plan list");
    log(state, Info, "  /plan add <txt>      Add plan item");
    log(state, Info, "  /plan done <id>      Mark plan item done");
    log(state, Info, "  /plan clear          Clear plan items");
    log(state, Info, "  /quit | /q   Stop agent execution");
    log(state, Info, "  /exit        Exit Osmogrep");
    log(state, Info, "");
    log(state, Info, "Anything else is sent to the agent.");
    log(state, Info, "!<cmd> runs a shell command directly.");
}

fn clear_logs(state: &mut AgentState) {
    state.logs.clear();
    state.ui.exec_scroll = usize::MAX;

    log(state, LogLevel::Info, "Logs cleared.");
}

fn exit_app(state: &mut AgentState) {
    log(state, LogLevel::Info, "Exiting Osmogrep.");
    state.ui.should_exit = true;
}

fn quit_agent(state: &mut AgentState) {
    log(state, LogLevel::Info, "Stopping agent execution.");
    state.ui.cancel_requested = true;
    state.stop_spinner();
    state.ui.exec_scroll = usize::MAX;
}

fn new_conversation(state: &mut AgentState) {
    state.conversation.clear();
    log(state, LogLevel::Info, "Started a new conversation.");
    let _ = persistence::save(state);
}

fn toggle_auto_approve(state: &mut AgentState) {
    state.ui.auto_approve = !state.ui.auto_approve;
    log(
        state,
        LogLevel::Info,
        format!(
            "Dangerous tool auto-approve: {}",
            if state.ui.auto_approve { "on" } else { "off" }
        ),
    );
}

fn show_model(state: &mut AgentState, agent: Option<&mut Agent>) {
    let Some(agent) = agent else {
        log(state, LogLevel::Warn, "Agent unavailable.");
        return;
    };

    let cfg = agent.model_config();
    log(
        state,
        LogLevel::Info,
        format!(
            "Model: provider={} model={} base_url={}",
            cfg.provider,
            cfg.model,
            cfg.base_url
                .clone()
                .unwrap_or_else(|| "(default)".to_string())
        ),
    );
}

fn set_model(state: &mut AgentState, cmd: &str, agent: Option<&mut Agent>) {
    let Some(agent) = agent else {
        log(state, LogLevel::Warn, "Agent unavailable.");
        return;
    };

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.len() < 3 {
        log(
            state,
            LogLevel::Warn,
            "Usage: /model <provider> <model> [base_url]",
        );
        return;
    }

    let provider = parts[1].to_string();
    let model = parts[2].to_string();
    let base_url = if parts.len() >= 4 {
        Some(parts[3].to_string())
    } else {
        None
    };

    agent.set_model_config(provider.clone(), model.clone(), base_url.clone());
    log(
        state,
        LogLevel::Success,
        format!(
            "Switched model: provider={} model={} base_url={}",
            provider,
            model,
            base_url.unwrap_or_else(|| "(default)".to_string())
        ),
    );
    let _ = persistence::save(state);
}

fn run_test(state: &mut AgentState, cmd: &str) {
    let target = cmd.strip_prefix("/test").map(str::trim).unwrap_or("");
    let target = if target.is_empty() {
        None
    } else {
        Some(target)
    };

    log(
        state,
        LogLevel::Info,
        format!(
            "Running tests{}",
            target.map(|t| format!(" ({})", t)).unwrap_or_default()
        ),
    );

    match run_tests(&state.repo_root, target) {
        Ok(run) => {
            log(
                state,
                if run.success {
                    LogLevel::Success
                } else {
                    LogLevel::Error
                },
                format!(
                    "Test run [{}] exit={} passed={} failed={} duration={}ms",
                    run.framework, run.exit_code, run.passed, run.failed, run.duration_ms
                ),
            );
            for line in run
                .output
                .lines()
                .rev()
                .take(30)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                log(state, LogLevel::Info, line.to_string());
            }
        }
        Err(e) => {
            log(state, LogLevel::Error, format!("Test run failed: {}", e));
        }
    }
}

fn show_mcp(state: &mut AgentState) {
    let enabled = crate::mcp::is_enabled();
    let servers = crate::mcp::list_servers();
    log(
        state,
        LogLevel::Info,
        format!(
            "MCP: {} (servers: {})",
            if enabled { "enabled" } else { "disabled" },
            servers.len()
        ),
    );
    if !servers.is_empty() {
        log(
            state,
            LogLevel::Info,
            format!("Servers: {}", servers.join(", ")),
        );
    }
}

fn show_providers(state: &mut AgentState) {
    log(
        state,
        LogLevel::Info,
        "Providers: openai, groq, mistral, ollama (anthropic-compatible via custom base_url)",
    );
}

fn undo_last_change(state: &mut AgentState) {
    let Some(last) = state.undo_stack.pop() else {
        log(state, LogLevel::Warn, "Nothing to undo.");
        return;
    };

    if let Err(e) = fs::write(&last.target, &last.before) {
        log(
            state,
            LogLevel::Error,
            format!("Undo failed for {}: {}", last.target, e),
        );
        return;
    }

    if let Some(pos) = state
        .session_changes
        .iter()
        .rposition(|s| s.target == last.target && s.after == last.after)
    {
        state.session_changes.remove(pos);
    }

    state.ui.diff_active = true;
    state.ui.diff_snapshot = vec![last.clone()];
    log(
        state,
        LogLevel::Success,
        format!("Undid last change on {}", last.target),
    );
    let _ = persistence::save(state);
}

fn show_session_diff(state: &mut AgentState) {
    if state.session_changes.is_empty() {
        log(state, LogLevel::Info, "No session changes to show.");
        return;
    }

    state.ui.diff_active = true;
    state.ui.diff_snapshot = state.session_changes.clone();
    log(
        state,
        LogLevel::Info,
        format!(
            "Showing {} session change(s).",
            state.ui.diff_snapshot.len()
        ),
    );
}

fn show_steer(state: &mut AgentState) {
    match state.steer.as_deref() {
        Some(s) if !s.trim().is_empty() => {
            log(state, LogLevel::Info, format!("Steer: {}", s.trim()));
        }
        _ => log(state, LogLevel::Info, "Steer: (not set)"),
    }
}

fn set_steer(state: &mut AgentState, cmd: &str, steer_tx: Option<&Sender<String>>) {
    let value = cmd.strip_prefix("/steer").map(str::trim).unwrap_or("");
    if let Some(now_text) = value.strip_prefix("now ").map(str::trim) {
        if now_text.is_empty() {
            log(state, LogLevel::Warn, "Usage: /steer now <instruction>");
            return;
        }
        state.steer = Some(now_text.to_string());
        if state.ui.agent_running {
            if let Some(tx) = steer_tx {
                if tx.send(now_text.to_string()).is_ok() {
                    log(
                        state,
                        LogLevel::Info,
                        "Steer-now injected into running agent.",
                    );
                } else {
                    state.ui.queued_agent_prompt = Some(now_text.to_string());
                    log(
                        state,
                        LogLevel::Warn,
                        "Live steer channel unavailable. Queued follow-up prompt instead.",
                    );
                }
            } else {
                state.ui.queued_agent_prompt = Some(now_text.to_string());
                log(
                    state,
                    LogLevel::Warn,
                    "Live steer channel unavailable. Queued follow-up prompt instead.",
                );
            }
        } else {
            state.ui.queued_agent_prompt = Some(now_text.to_string());
            log(
                state,
                LogLevel::Info,
                "Steer-now queued for immediate launch.",
            );
        }
        let _ = persistence::save(state);
        return;
    }

    if value.eq_ignore_ascii_case("clear") || value.eq_ignore_ascii_case("off") {
        state.steer = None;
        log(state, LogLevel::Success, "Steer instruction cleared.");
        let _ = persistence::save(state);
        return;
    }

    if value.is_empty() {
        show_steer(state);
        return;
    }

    state.steer = Some(value.to_string());
    log(state, LogLevel::Success, "Steer instruction updated.");
    let _ = persistence::save(state);
}

fn compact_context(state: &mut AgentState) {
    let before = state.conversation.token_estimate;
    state
        .conversation
        .trim_to_budget(MAX_CONVERSATION_TOKENS.saturating_mul(2) / 3);
    let after = state.conversation.token_estimate;
    log(
        state,
        LogLevel::Info,
        format!("Context compacted: {} -> {} tokens", before, after),
    );
    let _ = persistence::save(state);
}

fn show_metrics(state: &mut AgentState) {
    let total_tokens = state.usage.prompt_tokens + state.usage.completion_tokens;
    let est_cost = ((state.usage.prompt_tokens as f64) * 0.0000025)
        + ((state.usage.completion_tokens as f64) * 0.0000100);
    let queued = state
        .jobs
        .iter()
        .filter(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
        .count();
    log(
        state,
        LogLevel::Info,
        format!(
            "tokens={} cost=${:.4} context_tokens={} jobs_active={}",
            total_tokens, est_cost, state.conversation.token_estimate, queued
        ),
    );
}

fn show_profile(state: &mut AgentState) {
    log(
        state,
        LogLevel::Info,
        format!("Permission profile: {}", state.permission_profile.as_str()),
    );
}

fn set_profile(state: &mut AgentState, cmd: &str) {
    let value = cmd.strip_prefix("/profile").map(str::trim).unwrap_or("");
    let Some(profile) = PermissionProfile::parse(value) else {
        log(
            state,
            LogLevel::Warn,
            "Usage: /profile <read-only|workspace-auto|full-access>",
        );
        return;
    };
    state.permission_profile = profile;
    state.ui.auto_approve = matches!(profile, PermissionProfile::FullAccess);
    log(
        state,
        LogLevel::Success,
        format!("Permission profile set to {}", profile.as_str()),
    );
    let _ = persistence::save(state);
}

fn run_swarm_now(state: &mut AgentState, cmd: &str, agent: Option<&mut Agent>) {
    let prompt = cmd.strip_prefix("/swarm").map(str::trim).unwrap_or("");
    if prompt.is_empty() {
        log(state, LogLevel::Warn, "Usage: /swarm <task>");
        return;
    }
    let Some(agent) = agent else {
        log(state, LogLevel::Warn, "Agent unavailable.");
        return;
    };

    match agent.run_swarm(prompt) {
        Ok(outputs) => {
            log(state, LogLevel::Success, "Swarm completed.");
            for (role, text) in outputs {
                log(state, LogLevel::Info, format!("[{}]", role));
                for line in text.lines().take(20) {
                    log(state, LogLevel::Info, line.to_string());
                }
            }
        }
        Err(e) => log(state, LogLevel::Error, format!("Swarm failed: {}", e)),
    }
}

fn handle_job(state: &mut AgentState, cmd: &str) {
    let rest = cmd.strip_prefix("/job").map(str::trim).unwrap_or("");

    if rest == "cancel all" {
        let mut ids = Vec::new();
        for req in &state.job_queue {
            ids.push(req.id);
        }
        state.job_queue.clear();
        for id in ids {
            if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
                job.status = JobStatus::Cancelled;
            }
        }
        log(state, LogLevel::Info, "Cancelled queued jobs.");
        let _ = persistence::save(state);
        return;
    }

    if let Some(arg) = rest.strip_prefix("resume ") {
        let Ok(id) = arg.trim().parse::<u64>() else {
            log(state, LogLevel::Warn, "Usage: /job resume <id>");
            return;
        };
        let Some(old) = state.jobs.iter().find(|j| j.id == id).cloned() else {
            log(state, LogLevel::Warn, format!("Job #{} not found.", id));
            return;
        };
        queue_job(state, old.kind, old.input);
        return;
    }

    if let Some(arg) = rest.strip_prefix("swarm ") {
        let arg = arg.trim();
        if arg.is_empty() {
            log(state, LogLevel::Warn, "Usage: /job swarm <task>");
            return;
        }
        queue_job(state, JobKind::Swarm, arg.to_string());
        return;
    }

    if let Some(arg) = rest.strip_prefix("test") {
        queue_job(state, JobKind::Test, arg.trim().to_string());
        return;
    }

    log(
        state,
        LogLevel::Warn,
        "Usage: /job test [target] | /job swarm <task> | /job resume <id> | /job cancel all",
    );
}

fn queue_job(state: &mut AgentState, kind: JobKind, input: String) {
    let id = state.next_job_id;
    state.next_job_id += 1;
    state.jobs.push(JobRecord {
        id,
        kind: kind.clone(),
        input: input.clone(),
        status: JobStatus::Queued,
        output: None,
    });
    state.job_queue.push(JobRequest { id, kind, input });
    log(state, LogLevel::Info, format!("Queued job #{}", id));
    let _ = persistence::save(state);
}

fn show_jobs(state: &mut AgentState) {
    if state.jobs.is_empty() {
        log(state, LogLevel::Info, "No jobs yet.");
        return;
    }

    let rows: Vec<String> = state
        .jobs
        .iter()
        .rev()
        .take(15)
        .map(|job| {
            let status = match job.status {
                JobStatus::Queued => "queued",
                JobStatus::Running => "running",
                JobStatus::Done => "done",
                JobStatus::Failed => "failed",
                JobStatus::Cancelled => "cancelled",
            };
            format!(
                "#{} [{}] {} - {}",
                job.id,
                job.kind.as_str(),
                status,
                compact_line(&job.input, 80)
            )
        })
        .collect();

    for row in rows {
        log(state, LogLevel::Info, row);
    }
}

fn show_plan(state: &mut AgentState) {
    if state.plan_items.is_empty() {
        log(state, LogLevel::Info, "Plan is empty.");
        return;
    }
    let rows: Vec<String> = state
        .plan_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            format!(
                "{}. [{}] {}",
                idx + 1,
                if item.done { "x" } else { " " },
                item.text
            )
        })
        .collect();
    for row in rows {
        log(state, LogLevel::Info, row);
    }
}

fn plan_add(state: &mut AgentState, cmd: &str) {
    let text = cmd.strip_prefix("/plan add").map(str::trim).unwrap_or("");
    if text.is_empty() {
        log(state, LogLevel::Warn, "Usage: /plan add <text>");
        return;
    }
    state.plan_items.push(PlanItem {
        text: text.to_string(),
        done: false,
    });
    log(state, LogLevel::Success, "Plan item added.");
    let _ = persistence::save(state);
}

fn plan_done(state: &mut AgentState, cmd: &str) {
    let idx = cmd.strip_prefix("/plan done").map(str::trim).unwrap_or("");
    let Ok(n) = idx.parse::<usize>() else {
        log(state, LogLevel::Warn, "Usage: /plan done <id>");
        return;
    };
    if n == 0 || n > state.plan_items.len() {
        log(state, LogLevel::Warn, "Plan item out of range.");
        return;
    }
    if let Some(item) = state.plan_items.get_mut(n - 1) {
        item.done = true;
    }
    log(
        state,
        LogLevel::Success,
        format!("Plan item {} marked done.", n),
    );
    let _ = persistence::save(state);
}

fn plan_clear(state: &mut AgentState) {
    state.plan_items.clear();
    log(state, LogLevel::Info, "Plan cleared.");
    let _ = persistence::save(state);
}

fn compact_line(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut out: String = flat.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn show_autofix(state: &mut AgentState) {
    log(
        state,
        LogLevel::Info,
        format!("Auto-eval: {}", if state.auto_eval { "on" } else { "off" }),
    );
}

fn set_autofix(state: &mut AgentState, cmd: &str) {
    let value = cmd.strip_prefix("/autofix").map(str::trim).unwrap_or("");
    match value {
        "on" => state.auto_eval = true,
        "off" => state.auto_eval = false,
        _ => {
            log(state, LogLevel::Warn, "Usage: /autofix on|off");
            return;
        }
    }
    show_autofix(state);
    let _ = persistence::save(state);
}

fn run_triage_agent(state: &mut AgentState, cmd: &str) {
    let user_args = cmd
        .strip_prefix("/triage")
        .or_else(|| cmd.strip_prefix("/traige"))
        .map(str::trim)
        .unwrap_or("");

    let objective = if user_args.is_empty() {
        "Run full triage for this repo with sensible defaults.".to_string()
    } else {
        format!("Run triage with user intent/options: {user_args}")
    };

    let prompt = format!(
        "You are running an end-to-end GitHub triage workflow for the current repository.\n\
         Objective: {objective}\n\
         \n\
         Requirements:\n\
         1) Use tools (shell/read/write) and keep everything inside this repo context.\n\
         2) Verify `gh` is installed and authenticated. If not, stop and provide exact command to fix.\n\
         3) Resolve owner/repo via `gh repo view --json nameWithOwner,url,defaultBranchRef`.\n\
         4) Collect current PR and issue snapshots (open by default) using `gh pr list` and `gh issue list` with JSON fields.\n\
         5) Run `osmogrep triage` with robust defaults unless overridden by user intent:\n\
            - `--state open`\n\
            - `--limit 3000`\n\
            - `--deep-review-all`\n\
            - `--incremental`\n\
            - `--state-file .context/triage-state-<owner_repo>.json`\n\
            - include `--vision ./VISION.md` only if file exists.\n\
         6) If triage output is large, summarize top/bottom/high-risk items without losing key signals.\n\
         7) If user intent clearly asks to apply labels/comments, include `--apply-actions --comment-actions`.\n\
         \n\
         Output format (strict markdown):\n\
         - `## Triage Summary`\n\
         - `## Repo Snapshot`\n\
         - `## Top PR Candidates`\n\
         - `## Duplicate Clusters`\n\
         - `## Vision Drift / Reject Candidates`\n\
         - `## Action Plan`\n\
         - `## Commands Run`\n\
         - `## Next Steps`\n\
         \n\
         Keep content actionable and concise. Include concrete PR/Issue numbers and links where available."
    );

    state.ui.queued_agent_prompt = Some(prompt);
    log(
        state,
        LogLevel::Info,
        "Queued autonomous triage workflow. It will stream tool calls and produce a markdown report.",
    );
}

#[derive(Debug, Deserialize)]
struct GhActor {
    login: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhLabel {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPrRow {
    number: u64,
    title: Option<String>,
    url: Option<String>,
    is_draft: Option<bool>,
    review_decision: Option<String>,
    updated_at: Option<String>,
    author: Option<GhActor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhIssueRow {
    number: u64,
    title: Option<String>,
    url: Option<String>,
    updated_at: Option<String>,
    author: Option<GhActor>,
    labels: Option<Vec<GhLabel>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRepoRef {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRepoInfo {
    name_with_owner: Option<String>,
    url: Option<String>,
    default_branch_ref: Option<GhRepoRef>,
}

fn handle_gh_command(state: &mut AgentState, cmd: &str) {
    let rest = cmd.strip_prefix("/gh").map(str::trim).unwrap_or("");
    if rest.is_empty() || rest == "status" || rest == "check" {
        gh_status(state);
        return;
    }

    if let Some(arg) = rest.strip_prefix("prs") {
        gh_list_prs(state, arg.trim());
        return;
    }

    if let Some(arg) = rest.strip_prefix("issues") {
        gh_list_issues(state, arg.trim());
        return;
    }

    if let Some(arg) = rest.strip_prefix("triage") {
        gh_run_triage(state, arg.trim());
        return;
    }

    log(
        state,
        LogLevel::Warn,
        "Usage: /gh [status] | /gh prs [open|closed|merged] [limit] | /gh issues [open|closed] [limit] | /gh triage [flags] (defaults: open/3000/deep-review-all)",
    );
}

fn gh_status(state: &mut AgentState) {
    if !has_cmd("gh") {
        log(
            state,
            LogLevel::Error,
            "GitHub CLI (gh) is not installed. Install it: https://cli.github.com/",
        );
        log(
            state,
            LogLevel::Info,
            "macOS: brew install gh | Ubuntu: sudo apt-get install gh",
        );
        return;
    }

    let version = run_cmd_capture("gh", &["--version"]);
    match version {
        Ok(text) => {
            if let Some(line) = text.lines().next() {
                log(state, LogLevel::Info, format!("gh: {}", line));
            }
        }
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Failed to run gh --version: {}", e),
            );
            return;
        }
    }

    match run_cmd_capture("gh", &["auth", "status", "-h", "github.com"]) {
        Ok(_) => log(state, LogLevel::Success, "GitHub auth: ok"),
        Err(e) => {
            log(
                state,
                LogLevel::Warn,
                format!("GitHub auth not ready: {}. Run: gh auth login", e),
            );
            return;
        }
    }

    match gh_repo_info() {
        Ok(repo) => {
            let name = repo
                .name_with_owner
                .unwrap_or_else(|| "(unknown repo)".to_string());
            let url = repo.url.unwrap_or_else(|| "(unknown url)".to_string());
            let branch = repo
                .default_branch_ref
                .and_then(|r| r.name)
                .unwrap_or_else(|| "(unknown)".to_string());
            log(
                state,
                LogLevel::Info,
                format!("Repo: {} [{}] {}", name, branch, url),
            );
        }
        Err(e) => {
            log(
                state,
                LogLevel::Warn,
                format!(
                    "Could not resolve repo from current directory: {}. Run this inside a cloned GitHub repo.",
                    e
                ),
            );
        }
    }
}

fn gh_list_prs(state: &mut AgentState, arg: &str) {
    if !ensure_gh_ready(state) {
        return;
    }

    let mut pr_state = "open".to_string();
    let mut limit = 30usize;
    for tok in arg.split_whitespace() {
        match tok {
            "open" | "closed" | "merged" => pr_state = tok.to_string(),
            "all" => pr_state = "open".to_string(),
            _ => {
                if let Ok(n) = tok.parse::<usize>() {
                    limit = n.clamp(1, 500);
                }
            }
        }
    }

    let limit_s = limit.to_string();
    let args = [
        "pr",
        "list",
        "--state",
        pr_state.as_str(),
        "--limit",
        limit_s.as_str(),
        "--json",
        "number,title,url,isDraft,reviewDecision,updatedAt,author",
    ];

    let output = match run_cmd_capture("gh", &args) {
        Ok(v) => v,
        Err(e) => {
            log(state, LogLevel::Error, format!("Failed to list PRs: {}", e));
            return;
        }
    };

    let prs: Vec<GhPrRow> = match serde_json::from_str(&output) {
        Ok(v) => v,
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Failed to parse PR list JSON: {}", e),
            );
            return;
        }
    };

    log(
        state,
        LogLevel::Info,
        format!("PRs [{}] count={}", pr_state, prs.len()),
    );
    for pr in prs {
        let title = compact_line(pr.title.as_deref().unwrap_or(""), 72);
        let author = pr
            .author
            .and_then(|a| a.login)
            .unwrap_or_else(|| "unknown".to_string());
        let review = pr
            .review_decision
            .unwrap_or_else(|| "UNREVIEWED".to_string());
        let draft = if pr.is_draft.unwrap_or(false) {
            " draft"
        } else {
            ""
        };
        let updated = pr.updated_at.unwrap_or_else(|| "-".to_string());
        log(
            state,
            LogLevel::Info,
            format!(
                "#{} [{}{}] {} (@{}, {})",
                pr.number, review, draft, title, author, updated
            ),
        );
        if let Some(url) = pr.url {
            log(state, LogLevel::Info, format!("  {}", url));
        }
    }
}

fn gh_list_issues(state: &mut AgentState, arg: &str) {
    if !ensure_gh_ready(state) {
        return;
    }

    let mut issue_state = "open".to_string();
    let mut limit = 30usize;
    for tok in arg.split_whitespace() {
        match tok {
            "open" | "closed" => issue_state = tok.to_string(),
            "all" => issue_state = "open".to_string(),
            _ => {
                if let Ok(n) = tok.parse::<usize>() {
                    limit = n.clamp(1, 500);
                }
            }
        }
    }

    let limit_s = limit.to_string();
    let args = [
        "issue",
        "list",
        "--state",
        issue_state.as_str(),
        "--limit",
        limit_s.as_str(),
        "--json",
        "number,title,url,updatedAt,author,labels",
    ];

    let output = match run_cmd_capture("gh", &args) {
        Ok(v) => v,
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Failed to list issues: {}", e),
            );
            return;
        }
    };

    let issues: Vec<GhIssueRow> = match serde_json::from_str(&output) {
        Ok(v) => v,
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Failed to parse issue list JSON: {}", e),
            );
            return;
        }
    };

    log(
        state,
        LogLevel::Info,
        format!("Issues [{}] count={}", issue_state, issues.len()),
    );
    for issue in issues {
        let title = compact_line(issue.title.as_deref().unwrap_or(""), 72);
        let author = issue
            .author
            .and_then(|a| a.login)
            .unwrap_or_else(|| "unknown".to_string());
        let labels = issue
            .labels
            .unwrap_or_default()
            .into_iter()
            .filter_map(|l| l.name)
            .take(3)
            .collect::<Vec<_>>();
        let label_text = if labels.is_empty() {
            String::new()
        } else {
            format!(" labels={}", labels.join(","))
        };
        let updated = issue.updated_at.unwrap_or_else(|| "-".to_string());
        log(
            state,
            LogLevel::Info,
            format!(
                "#{} {} (@{}, {}{})",
                issue.number, title, author, updated, label_text
            ),
        );
        if let Some(url) = issue.url {
            log(state, LogLevel::Info, format!("  {}", url));
        }
    }
}

fn gh_run_triage(state: &mut AgentState, arg: &str) {
    if !ensure_gh_ready(state) {
        return;
    }

    let repo = match gh_repo_info() {
        Ok(info) => info.name_with_owner.unwrap_or_default(),
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Cannot resolve repo for triage: {}", e),
            );
            return;
        }
    };
    if repo.is_empty() {
        log(
            state,
            LogLevel::Error,
            "Cannot resolve owner/repo from current directory.",
        );
        return;
    }

    let mut pass_through: Vec<String> = arg
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|s| s.to_string())
        .collect();

    let has_repo = has_flag(&pass_through, "--repo");
    let has_json_only = has_flag(&pass_through, "--json-only");
    let has_state_file = has_flag(&pass_through, "--state-file");
    let has_incremental = has_flag(&pass_through, "--incremental");
    let has_state = has_flag(&pass_through, "--state");
    let has_limit = has_flag(&pass_through, "--limit");
    let has_deep_review_top = has_flag(&pass_through, "--deep-review-top");
    let has_deep_review_all = has_flag(&pass_through, "--deep-review-all");
    let has_out = has_flag(&pass_through, "--out");
    let has_vision = has_flag(&pass_through, "--vision");

    let repo_slug = repo.replace('/', "_");
    let context_dir = state.repo_root.join(".context");
    let _ = fs::create_dir_all(&context_dir);
    let report_json_path = context_dir.join(format!("triage-report-{}.json", repo_slug));
    let report_md_path = context_dir.join(format!("triage-brief-{}.md", repo_slug));
    let vision_default = state.repo_root.join("VISION.md");

    let mut args: Vec<String> = vec!["triage".to_string()];
    if !has_repo {
        args.push("--repo".to_string());
        args.push(repo.clone());
    }
    if !has_state {
        args.push("--state".to_string());
        args.push("open".to_string());
    }
    if !has_limit {
        args.push("--limit".to_string());
        args.push("3000".to_string());
    }
    if !has_deep_review_all && !has_deep_review_top {
        args.push("--deep-review-all".to_string());
    }
    if !has_incremental {
        args.push("--incremental".to_string());
    }
    if !has_state_file {
        args.push("--state-file".to_string());
        args.push(format!(".context/triage-state-{}.json", repo_slug));
    }
    if !has_out {
        args.push("--out".to_string());
        args.push(report_json_path.display().to_string());
    }
    if !has_vision && vision_default.exists() {
        args.push("--vision".to_string());
        args.push("./VISION.md".to_string());
    }
    args.append(&mut pass_through);
    if !has_json_only {
        args.push("--json-only".to_string());
    }

    log(
        state,
        LogLevel::Info,
        format!("Running triage for {} ...", repo),
    );

    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Unable to resolve osmogrep executable: {}", e),
            );
            return;
        }
    };

    let out = match Command::new(exe)
        .args(args.iter().map(|s| s.as_str()))
        .current_dir(&state.repo_root)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Failed to start triage: {}", e),
            );
            return;
        }
    };

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let msg = if !err.is_empty() { err } else { stdout };
        log(state, LogLevel::Error, format!("Triage failed: {}", msg));
        return;
    }

    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let report: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Triage output parse error: {}", e),
            );
            return;
        }
    };

    let scanned_prs = report
        .get("scanned_prs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let scanned_issues = report
        .get("scanned_issues")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let dupes = report
        .get("duplicate_pairs")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let actions = report
        .get("planned_actions")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let applied = report
        .get("applied_action_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    log(
        state,
        LogLevel::Success,
        format!(
            "Triage done: PRs={} Issues={} Duplicates={} PlannedActions={} AppliedActions={}",
            scanned_prs, scanned_issues, dupes, actions, applied
        ),
    );

    if let Some(ranked) = report.get("ranked_prs").and_then(Value::as_array) {
        for pr in ranked.iter().take(12) {
            let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
            let score = pr.get("score").and_then(Value::as_f64).unwrap_or(0.0);
            let decision = pr
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let title = compact_line(pr.get("title").and_then(Value::as_str).unwrap_or(""), 68);
            log(
                state,
                LogLevel::Info,
                format!("#{} [{:.1}] {} ({})", number, score, title, decision),
            );
        }
    }

    match write_triage_brief(&report, &repo, &report_md_path) {
        Ok(_) => log(
            state,
            LogLevel::Success,
            format!(
                "Wrote triage brief: {}",
                report_md_path.strip_prefix(&state.repo_root).unwrap_or(&report_md_path).display()
            ),
        ),
        Err(e) => log(
            state,
            LogLevel::Warn,
            format!("Could not write triage brief markdown: {}", e),
        ),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|t| t == flag)
}

fn write_triage_brief(report: &Value, repo: &str, path: &PathBuf) -> Result<(), String> {
    let scanned_prs = report
        .get("scanned_prs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let scanned_issues = report
        .get("scanned_issues")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let duplicates = report
        .get("duplicate_pairs")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let applied = report
        .get("applied_action_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let generated_at = report
        .get("generated_at")
        .and_then(Value::as_str)
        .unwrap_or("-");

    let mut md = String::new();
    md.push_str("# PR/Issue Triage Brief\n\n");
    md.push_str(&format!("- Repo: `{}`\n", repo));
    md.push_str(&format!("- Generated: `{}`\n", generated_at));
    md.push_str(&format!(
        "- Scanned: **{} PRs**, **{} Issues**\n",
        scanned_prs, scanned_issues
    ));
    md.push_str(&format!("- Duplicate pairs: **{}**\n", duplicates));
    md.push_str(&format!("- Applied actions: **{}**\n\n", applied));

    md.push_str("## Top PR Candidates\n\n");
    if let Some(ranked) = report.get("ranked_prs").and_then(Value::as_array) {
        if ranked.is_empty() {
            md.push_str("_No PR rankings in report._\n\n");
        } else {
            for pr in ranked.iter().take(20) {
                let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
                let score = pr.get("score").and_then(Value::as_f64).unwrap_or(0.0);
                let decision = pr
                    .get("decision")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let title = pr
                    .get("title")
                    .and_then(Value::as_str)
                    .map(|s| compact_line(s, 110))
                    .unwrap_or_else(|| "(untitled)".to_string());
                let url = pr.get("url").and_then(Value::as_str).unwrap_or("");
                if url.is_empty() {
                    md.push_str(&format!(
                        "- #{} [{:.1}] {} ({})\n",
                        number, score, title, decision
                    ));
                } else {
                    md.push_str(&format!(
                        "- [#{}]({}) [{:.1}] {} ({})\n",
                        number, url, score, title, decision
                    ));
                }
            }
            md.push('\n');
        }
    }

    md.push_str("## Duplicate Clusters (Top Pairs)\n\n");
    if let Some(pairs) = report.get("duplicate_pairs").and_then(Value::as_array) {
        if pairs.is_empty() {
            md.push_str("_No probable duplicates found._\n\n");
        } else {
            for pair in pairs.iter().take(25) {
                let l_num = pair.get("left_number").and_then(Value::as_u64).unwrap_or(0);
                let l_title = compact_line(
                    pair.get("left_title").and_then(Value::as_str).unwrap_or(""),
                    90,
                );
                let l_url = pair.get("left_url").and_then(Value::as_str).unwrap_or("");
                let r_num = pair.get("right_number").and_then(Value::as_u64).unwrap_or(0);
                let r_title = compact_line(
                    pair.get("right_title").and_then(Value::as_str).unwrap_or(""),
                    90,
                );
                let r_url = pair.get("right_url").and_then(Value::as_str).unwrap_or("");
                let sim = pair.get("similarity").and_then(Value::as_f64).unwrap_or(0.0);

                let left = if l_url.is_empty() {
                    format!("#{} {}", l_num, l_title)
                } else {
                    format!("[#{}]({}) {}", l_num, l_url, l_title)
                };
                let right = if r_url.is_empty() {
                    format!("#{} {}", r_num, r_title)
                } else {
                    format!("[#{}]({}) {}", r_num, r_url, r_title)
                };
                md.push_str(&format!("- {:.2}: {} ↔ {}\n", sim, left, right));
            }
            md.push('\n');
        }
    }

    md.push_str("## Vision Drift / Reject Candidates\n\n");
    if let Some(ranked) = report.get("ranked_prs").and_then(Value::as_array) {
        let mut any = false;
        for pr in ranked {
            let decision = pr
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            if !(decision.contains("reject") || decision.contains("hold")) {
                continue;
            }
            any = true;
            let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
            let score = pr.get("score").and_then(Value::as_f64).unwrap_or(0.0);
            let title = compact_line(pr.get("title").and_then(Value::as_str).unwrap_or(""), 95);
            let url = pr.get("url").and_then(Value::as_str).unwrap_or("");
            let rationale = pr
                .get("rationale")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .unwrap_or("See report JSON for full rationale.");
            if url.is_empty() {
                md.push_str(&format!(
                    "- #{} [{:.1}] {} ({})\n",
                    number, score, title, decision
                ));
            } else {
                md.push_str(&format!(
                    "- [#{}]({}) [{:.1}] {} ({})\n",
                    number, url, score, title, decision
                ));
            }
            md.push_str(&format!("  - {}\n", compact_line(rationale, 140)));
        }
        if !any {
            md.push_str("_No explicit reject/hold candidates from current scoring._\n");
        }
        md.push('\n');
    }

    md.push_str("## Action Plan\n\n");
    if let Some(actions) = report.get("planned_actions").and_then(Value::as_array) {
        if actions.is_empty() {
            md.push_str("_No actions planned._\n");
        } else {
            for action in actions.iter().take(40) {
                let number = action
                    .get("item_number")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let kind = action
                    .get("item_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("item");
                let action_type = action
                    .get("action_type")
                    .and_then(Value::as_str)
                    .unwrap_or("action");
                let value = action.get("value").and_then(Value::as_str).unwrap_or("-");
                let reason = action.get("reason").and_then(Value::as_str).unwrap_or("-");
                md.push_str(&format!(
                    "- {} #{}: `{}` => `{}` ({})\n",
                    kind,
                    number,
                    action_type,
                    value,
                    compact_line(reason, 120)
                ));
            }
        }
    }

    fs::write(path, md).map_err(|e| e.to_string())
}

fn ensure_gh_ready(state: &mut AgentState) -> bool {
    if !has_cmd("gh") {
        log(
            state,
            LogLevel::Error,
            "GitHub CLI (gh) is not installed. Install it: https://cli.github.com/",
        );
        return false;
    }
    if let Err(e) = run_cmd_capture("gh", &["auth", "status", "-h", "github.com"]) {
        log(
            state,
            LogLevel::Error,
            format!("GitHub CLI auth required: {}. Run: gh auth login", e),
        );
        return false;
    }
    true
}

fn gh_repo_info() -> Result<GhRepoInfo, String> {
    let raw = run_cmd_capture(
        "gh",
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner,url,defaultBranchRef",
        ],
    )?;
    serde_json::from_str::<GhRepoInfo>(&raw).map_err(|e| e.to_string())
}

fn run_cmd_capture(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if err.is_empty() {
            format!("{} exited with {}", cmd, out.status)
        } else {
            err
        };
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn open_nv(state: &mut AgentState, cmd: &str) {
    if cmd.trim() == "/nv help" {
        log(state, LogLevel::Info, "nvim exit: :q, :wq, :qa!, :qa");
        log(
            state,
            LogLevel::Info,
            "navigation: Ctrl+w h/l to move between panes",
        );
        log(
            state,
            LogLevel::Info,
            "plugin panels: press q or Esc to close (lazy, tree, etc.)",
        );
        log(state, LogLevel::Info, "tmux detach: Ctrl+b then d");
        return;
    }
    if cmd.trim() == "/nv toggle" {
        toggle_nv_pane(state);
        return;
    }

    if !has_cmd("tmux") || !has_cmd("nvim") {
        let missing = [
            if has_cmd("tmux") { None } else { Some("tmux") },
            if has_cmd("nvim") {
                None
            } else {
                Some("neovim (nvim)")
            },
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        log(
            state,
            LogLevel::Error,
            format!(
                "/nv requires missing dependency: {}. Run install.sh or install them manually.",
                missing
            ),
        );
        log(
            state,
            LogLevel::Info,
            "macOS: brew install tmux neovim | Ubuntu: sudo apt-get install -y tmux neovim",
        );
        return;
    }
    ensure_nvim_ux_bootstrap(state);

    let target = cmd.strip_prefix("/nv").map(str::trim).unwrap_or("");
    let entry = if !target.is_empty() {
        target.to_string()
    } else if let Some(active) = state.ui.active_edit_target.clone() {
        active
    } else if let Some(last) = state.session_changes.last().map(|s| s.target.clone()) {
        last
    } else {
        ".".to_string()
    };
    let repo = shell_escape(&state.repo_root.display().to_string());
    let nvim_cmd = build_nvim_command(&repo, &entry);
    let recent = recent_session_files(state, 4);

    let _ = Command::new("tmux")
        .args(["set-option", "-g", "mouse", "on"])
        .output();

    if env::var("TMUX").is_err() {
        let exe = match env::current_exe() {
            Ok(path) => shell_escape(&path.display().to_string()),
            Err(e) => {
                log(
                    state,
                    LogLevel::Error,
                    format!("Failed to resolve current executable for /nv: {}", e),
                );
                return;
            }
        };
        let session = format!(
            "osmogrep-nv-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
        let cwd = state.repo_root.display().to_string();
        let osmogrep_cmd = format!(
            "cd {} && OSMOGREP_NV_TIPS=1 OSMOGREP_MANAGED_TMUX=1 OSMOGREP_NV_SESSION={} {}",
            repo,
            shell_escape(&session),
            exe
        );

        let steps: Vec<(Vec<String>, &str)> = vec![
            (
                vec![
                    "new-session".into(),
                    "-d".into(),
                    "-s".into(),
                    session.clone(),
                    "-c".into(),
                    cwd.clone(),
                    osmogrep_cmd,
                ],
                "create tmux session",
            ),
            (
                vec![
                    "split-window".into(),
                    "-h".into(),
                    "-b".into(),
                    "-p".into(),
                    "65".into(),
                    "-t".into(),
                    format!("{}:0", session),
                    "-c".into(),
                    cwd.clone(),
                    nvim_cmd.clone(),
                ],
                "split nvim pane",
            ),
            (
                vec![
                    "set-option".into(),
                    "-t".into(),
                    session.clone(),
                    "-g".into(),
                    "mouse".into(),
                    "on".into(),
                ],
                "enable mouse",
            ),
            (
                vec![
                    "select-pane".into(),
                    "-t".into(),
                    format!("{}:0.1", session),
                ],
                "focus osmogrep pane",
            ),
        ];

        for (args, step) in steps {
            match Command::new("tmux").args(args).output() {
                Ok(out) if out.status.success() => {}
                Ok(out) => {
                    log(
                        state,
                        LogLevel::Error,
                        format!(
                            "/nv failed to {}: {}",
                            step,
                            String::from_utf8_lossy(&out.stderr).trim()
                        ),
                    );
                    return;
                }
                Err(e) => {
                    log(
                        state,
                        LogLevel::Error,
                        format!("/nv failed to {}: {}", step, e),
                    );
                    return;
                }
            }
        }

        state.ui.tmux_attach_session = Some(session.clone());
        state.ui.should_exit = true;
        log(
            state,
            LogLevel::Success,
            format!("Prepared tmux session {}. Switching now...", session),
        );
        if !recent.is_empty() {
            log(
                state,
                LogLevel::Info,
                format!("Recent edited files: {}", recent.join(", ")),
            );
        }
        return;
    }

    match Command::new("tmux")
        .args(["split-window", "-h", "-b", "-p", "65", &nvim_cmd])
        .output()
    {
        Ok(out) if out.status.success() => {
            log(
                state,
                LogLevel::Success,
                if target.is_empty() {
                    format!("Opened Neovim pane on the left at {}.", entry)
                } else {
                    format!("Opened Neovim pane with {}.", target)
                },
            );
            if !recent.is_empty() {
                log(
                    state,
                    LogLevel::Info,
                    format!("Recent edited files: {}", recent.join(", ")),
                );
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            log(
                state,
                LogLevel::Error,
                format!("Failed to open /nv pane: {}", stderr.trim()),
            );
        }
        Err(e) => {
            log(
                state,
                LogLevel::Error,
                format!("Failed to launch tmux for /nv: {}", e),
            );
        }
    }
}

fn build_nvim_command(repo: &str, entry: &str) -> String {
    let entry = shell_escape(entry);
    format!(
        "cd {repo} && nvim --headless \"+Lazy! sync\" +qa >/dev/null 2>&1 || true; nvim {entry} -c \"set termguicolors\" -c \"silent! LspStart\" -c \"silent! NvimTreeOpen\" -c \"vertical resize 32\" -c \"wincmd l\""
    )
}

fn recent_session_files(state: &AgentState, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    for snap in state.session_changes.iter().rev() {
        if !out.iter().any(|x: &String| x == &snap.target) {
            out.push(snap.target.clone());
        }
        if out.len() >= max {
            break;
        }
    }
    out
}

fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
    {
        return s.to_string();
    }
    let escaped = s.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

fn has_cmd(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ensure_nvim_ux_bootstrap(state: &mut AgentState) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let cfg_dir = home.join(".config").join("nvim");
    let init_path = cfg_dir.join("init.lua");
    let marker = "-- osmogrep-managed-nvim";

    let existing = fs::read_to_string(&init_path).ok();
    if let Some(ref text) = existing {
        if !text.contains(marker) {
            log(
                state,
                LogLevel::Info,
                format!(
                    "Detected existing nvim config at {} (leaving it unchanged).",
                    init_path.display()
                ),
            );
            return;
        }
    }

    if let Err(e) = fs::create_dir_all(&cfg_dir) {
        log(
            state,
            LogLevel::Warn,
            format!("Failed to create nvim config directory: {}", e),
        );
        return;
    }

    let managed = format!(
        r#"{marker}
vim.g.mapleader = ' '
vim.opt.termguicolors = true
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.signcolumn = 'yes'
vim.opt.cursorline = true
vim.opt.updatetime = 200
vim.opt.splitright = true
vim.opt.splitbelow = true
vim.opt.clipboard = 'unnamedplus'

local lazypath = vim.fn.stdpath('data') .. '/lazy/lazy.nvim'
vim.g.loaded_netrw = 1
vim.g.loaded_netrwPlugin = 1
local stat = vim.loop.fs_stat(lazypath)
if stat and stat.type ~= 'directory' then
  pcall(vim.fn.delete, lazypath)
  stat = nil
end
if not stat then
  vim.fn.system({{
    'git', 'clone', '--filter=blob:none',
    'https://github.com/folke/lazy.nvim.git',
    '--branch=stable', lazypath
  }})
end
vim.opt.rtp:prepend(lazypath)

require('lazy').setup({{
  {{ 'catppuccin/nvim', name = 'catppuccin', priority = 1000 }},
  {{ 'nvim-tree/nvim-web-devicons' }},
  {{
    'nvim-tree/nvim-tree.lua',
    config = function()
      require('nvim-tree').setup({{
        hijack_netrw = true,
        sync_root_with_cwd = true,
        view = {{ width = 34 }},
        renderer = {{ icons = {{ show = {{ folder = true, file = true, folder_arrow = true }} }} }},
      }})
    end
  }},
  {{
    'nvim-treesitter/nvim-treesitter',
    build = ':TSUpdate',
    config = function()
      require('nvim-treesitter.configs').setup({{
        highlight = {{ enable = true }},
        indent = {{ enable = true }},
        ensure_installed = {{ 'lua', 'vim', 'vimdoc', 'rust', 'python', 'javascript', 'typescript', 'go', 'json', 'toml', 'bash' }},
      }})
    end
  }},
  {{
    'nvim-telescope/telescope.nvim',
    dependencies = {{ 'nvim-lua/plenary.nvim' }},
  }},
  {{ 'neovim/nvim-lspconfig' }},
  {{
    'williamboman/mason.nvim',
    config = function() require('mason').setup() end
  }},
  {{
    'williamboman/mason-lspconfig.nvim',
    dependencies = {{ 'williamboman/mason.nvim', 'neovim/nvim-lspconfig' }},
    config = function()
      require('mason-lspconfig').setup({{
        ensure_installed = {{ 'lua_ls', 'rust_analyzer', 'pyright', 'ts_ls', 'gopls', 'clangd' }},
      }})
    end
  }},
}})

vim.cmd.colorscheme('catppuccin-mocha')
vim.keymap.set('n', '<leader>e', '<cmd>NvimTreeToggle<CR>', {{ silent = true }})
vim.keymap.set('n', '<C-b>', '<cmd>NvimTreeToggle<CR>', {{ silent = true }})
vim.keymap.set('n', '<leader>ff', '<cmd>Telescope find_files<CR>', {{ silent = true }})
vim.api.nvim_create_autocmd('FileType', {{
  pattern = 'lazy',
  callback = function()
    vim.keymap.set('n', 'q', '<cmd>close<CR>', {{ buffer = true, silent = true }})
    vim.keymap.set('n', '<Esc>', '<cmd>close<CR>', {{ buffer = true, silent = true }})
  end,
}})
vim.api.nvim_create_autocmd('VimEnter', {{
  callback = function()
    pcall(vim.cmd, 'NvimTreeOpen')
    pcall(vim.cmd, 'wincmd l')
  end,
}})

local lspconfig = require('lspconfig')
local servers = {{ 'lua_ls', 'rust_analyzer', 'pyright', 'ts_ls', 'gopls', 'clangd' }}
for _, server in ipairs(servers) do
  pcall(function() lspconfig[server].setup({{}}) end)
end
"#
    );

    let write_result = (|| -> Result<(), String> {
        let mut f = fs::File::create(&init_path).map_err(|e| e.to_string())?;
        f.write_all(managed.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    })();

    match write_result {
        Ok(()) => log(
            state,
            LogLevel::Info,
            format!("Prepared managed nvim UX config at {}", init_path.display()),
        ),
        Err(e) => log(
            state,
            LogLevel::Warn,
            format!("Failed to write nvim UX config: {}", e),
        ),
    }
}

fn toggle_nv_pane(state: &mut AgentState) {
    if env::var("TMUX").is_err() {
        log(
            state,
            LogLevel::Warn,
            "/nv toggle works inside tmux windows only.",
        );
        return;
    }

    let window = match tmux_current_window_id() {
        Some(w) => w,
        None => {
            log(
                state,
                LogLevel::Warn,
                "Could not resolve current tmux window.",
            );
            return;
        }
    };

    if let Some(pane_id) = tmux_find_nvim_pane(&window) {
        match Command::new("tmux")
            .args(["kill-pane", "-t", &pane_id])
            .output()
        {
            Ok(out) if out.status.success() => {
                log(state, LogLevel::Success, "Closed nvim pane.");
            }
            Ok(out) => {
                log(
                    state,
                    LogLevel::Error,
                    format!(
                        "Failed to close nvim pane: {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    ),
                );
            }
            Err(e) => log(
                state,
                LogLevel::Error,
                format!("Failed to run tmux kill-pane: {}", e),
            ),
        }
        return;
    }

    let repo = shell_escape(&state.repo_root.display().to_string());
    let entry = state
        .ui
        .active_edit_target
        .clone()
        .or_else(|| state.session_changes.last().map(|s| s.target.clone()))
        .unwrap_or_else(|| ".".to_string());
    let nvim_cmd = build_nvim_command(&repo, &entry);

    match Command::new("tmux")
        .args([
            "split-window",
            "-h",
            "-b",
            "-p",
            "65",
            "-t",
            &window,
            "-c",
            &state.repo_root.display().to_string(),
            &nvim_cmd,
        ])
        .output()
    {
        Ok(out) if out.status.success() => {
            let _ = Command::new("tmux")
                .args(["set-option", "-g", "mouse", "on"])
                .output();
            log(state, LogLevel::Success, "Opened nvim pane.");
        }
        Ok(out) => log(
            state,
            LogLevel::Error,
            format!(
                "Failed to open nvim pane: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ),
        Err(e) => log(
            state,
            LogLevel::Error,
            format!("Failed to run tmux split-window: {}", e),
        ),
    }
}

fn tmux_current_window_id() -> Option<String> {
    let out = Command::new("tmux")
        .args(["display-message", "-p", "#{session_name}:#{window_index}"])
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

fn tmux_find_nvim_pane(window: &str) -> Option<String> {
    let out = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            window,
            "-F",
            "#{pane_id} #{pane_current_command}",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let pane = parts.next()?;
        let cmd = parts.next().unwrap_or("");
        if cmd == "nvim" || cmd == "vim" {
            return Some(pane.to_string());
        }
    }
    None
}

fn enter_api_key_mode(state: &mut AgentState) {
    state.ui.input.clear();
    state.ui.input_mode = InputMode::ApiKey;
    state.ui.input_masked = true;
    state.ui.input_placeholder = Some("Enter OpenAI API key".into());

    log(
        state,
        LogLevel::Info,
        "Enter your OpenAI API key and press Enter.",
    );
}

fn voice_status(state: &mut AgentState) {
    state.voice.visible = true;
    let status = state.voice.status.clone().unwrap_or_else(|| "idle".into());

    log(
        state,
        LogLevel::Info,
        format!(
            "Voice: {} (connected: {}, url: {}, model: {})",
            if state.voice.enabled { "on" } else { "off" },
            state.voice.connected,
            state.voice.url,
            state.voice.model
        ),
    );
    log(state, LogLevel::Info, status);
}

fn voice_on(state: &mut AgentState, voice_tx: Option<&Sender<VoiceCommand>>) {
    state.voice.visible = true;
    if state.voice.enabled {
        log(state, LogLevel::Info, "Voice already enabled.");
        return;
    }

    if let Some(tx) = voice_tx {
        let _ = tx.send(VoiceCommand::Start {
            url: state.voice.url.clone(),
            model: state.voice.model.clone(),
        });
        state.voice.enabled = true;
        log(state, LogLevel::Info, "Starting voice input...");
    } else {
        log(state, LogLevel::Error, "Voice channel unavailable.");
    }
}

fn voice_off(state: &mut AgentState, voice_tx: Option<&Sender<VoiceCommand>>) {
    state.voice.visible = true;
    if !state.voice.enabled {
        log(state, LogLevel::Info, "Voice already disabled.");
        return;
    }

    if let Some(tx) = voice_tx {
        let _ = tx.send(VoiceCommand::Stop);
        state.voice.enabled = false;
        log(state, LogLevel::Info, "Stopping voice input...");
    } else {
        log(state, LogLevel::Error, "Voice channel unavailable.");
    }
}
pub fn update_command_hints(state: &mut AgentState) {
    let input = normalize_command_prefix(state.ui.input.trim());

    let prev_selected = state.ui.command_selected;

    state.ui.command_items.clear();

    if !input.starts_with('/') {
        state.ui.command_selected = 0;
        return;
    }

    let all: &[CommandItem] = &[
        CommandItem {
            cmd: "/help",
            desc: "Show available commands",
        },
        CommandItem {
            cmd: "/clear",
            desc: "Clear logs",
        },
        CommandItem {
            cmd: "/key",
            desc: "Set OpenAI API key",
        },
        CommandItem {
            cmd: "/voice",
            desc: "Show voice status",
        },
        CommandItem {
            cmd: "/voice on",
            desc: "Start voice input",
        },
        CommandItem {
            cmd: "/voice off",
            desc: "Stop voice input",
        },
        CommandItem {
            cmd: "/model",
            desc: "Show active provider/model",
        },
        CommandItem {
            cmd: "/test",
            desc: "Run auto-detected tests",
        },
        CommandItem {
            cmd: "/mcp",
            desc: "Show MCP status and servers",
        },
        CommandItem {
            cmd: "/providers",
            desc: "Show available model providers",
        },
        CommandItem {
            cmd: "/undo",
            desc: "Revert the last agent file change",
        },
        CommandItem {
            cmd: "/diff",
            desc: "Show all file changes this session",
        },
        CommandItem {
            cmd: "/compact",
            desc: "Compress conversation context",
        },
        CommandItem {
            cmd: "/metrics",
            desc: "Show usage and queue metrics",
        },
        CommandItem {
            cmd: "/profile",
            desc: "Show or set permission profile",
        },
        CommandItem {
            cmd: "/approve",
            desc: "Toggle dangerous tool auto-approve",
        },
        CommandItem {
            cmd: "/new",
            desc: "Start a fresh conversation",
        },
        CommandItem {
            cmd: "/steer",
            desc: "Set or show persistent steer instruction",
        },
        CommandItem {
            cmd: "/swarm",
            desc: "Run parallel scoped sub-agents",
        },
        CommandItem {
            cmd: "/jobs",
            desc: "Show background jobs",
        },
        CommandItem {
            cmd: "/autofix",
            desc: "Toggle auto post-run eval tests",
        },
        CommandItem {
            cmd: "/triage",
            desc: "Run autonomous GitHub PR/Issue triage workflow",
        },
        CommandItem {
            cmd: "/gh",
            desc: "GitHub repo status + PR/issue triage commands",
        },
        CommandItem {
            cmd: "/nv",
            desc: "Open Neovim split (auto tmux bootstrap)",
        },
        CommandItem {
            cmd: "/job",
            desc: "Queue background job",
        },
        CommandItem {
            cmd: "/plan",
            desc: "Show/update plan items",
        },
        CommandItem {
            cmd: "/quit",
            desc: "Stop agent execution",
        },
        CommandItem {
            cmd: "/q",
            desc: "Stop agent execution",
        },
        CommandItem {
            cmd: "/exit",
            desc: "Exit Osmogrep",
        },
    ];

    for item in all {
        if input == "/" || item.cmd.starts_with(&input) {
            state.ui.command_items.push(*item);
        }
    }

    if state.ui.command_items.is_empty() {
        for item in all {
            if item.cmd.contains(&input)
                || item
                    .desc
                    .to_ascii_lowercase()
                    .contains(&input[1..].to_ascii_lowercase())
            {
                state.ui.command_items.push(*item);
            }
        }
    }

    if state.ui.command_items.is_empty() {
        state.ui.command_selected = 0;
    } else {
        state.ui.command_selected = prev_selected.min(state.ui.command_items.len() - 1);
    }
}

fn normalize_command_prefix(input: &str) -> String {
    if let Some(rest) = input.strip_prefix('／') {
        format!("/{}", rest)
    } else if let Some(rest) = input.strip_prefix('÷') {
        format!("/{}", rest)
    } else {
        input.to_string()
    }
}
