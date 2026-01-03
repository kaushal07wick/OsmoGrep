//! command.rs
//!
//! Command interpretation layer.
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::detectors::diff_analyzer::analyze_diff;
use crate::git;
use crate::logger::{log, log_diff_analysis};
use crate::llm::backend::LlmBackend;
use crate::llm::client::LlmClient;
use crate::state::{
    AgentState, Focus, LogLevel, Phase, SinglePanelView,
};
use crate::testgen::generator::generate_test_candidates;

pub fn handle_command(state: &mut AgentState, cmd: &str) {
    state.ui.last_activity = Instant::now();
    state.clear_hint();
    state.clear_autocomplete();

    match cmd {
        "help" => help(state),

        "inspect" => inspect(state),
        "changes" => list_changes(state),
        cmd if cmd.starts_with("changes ") => view_change(state, cmd),

        cmd if cmd.starts_with("agent run ") => agent_run(state, cmd),
        "agent status" => agent_status(state),
        "agent cancel" => agent_cancel(state),

        "clear" | "logs clear" => clear_logs(state),

        cmd if cmd.starts_with("model use ") => model_use(state, cmd),
        "model show" => model_show(state),

        "status" => status_all(state),
        "status agent" => status_agent(state),
        "status context" => status_context(state),
        "status prompt" => status_prompt(state),
        "status git" => status_git(state),
        "status system" => status_system(state),

        "artifacts test" => show_test_artifact(state),

        "branch new" => {
            log(state, LogLevel::Info, "Creating agent branch…");
            state.lifecycle.phase = Phase::CreateNewAgent;
        }

        "branch rollback" => {
            log(state, LogLevel::Warn, "Rolling back agent branch…");
            state.lifecycle.phase = Phase::Rollback;
        }

        "branch list" => {
            for b in git::list_branches() {
                log(state, LogLevel::Info, b);
            }
        }

        "close" => close_view(state),
        "quit" => {
            log(state, LogLevel::Info, "Exiting Osmogrep.");
            state.lifecycle.phase = Phase::Done;
        }

        "" => {}
        _ => log(state, LogLevel::Warn, "Unknown command. Type `help`."),
    }
}


fn help(state: &mut AgentState) {
    use LogLevel::Info;

    log(state, Info, "Commands:");
    log(state, Info, "  inspect                     — analyze git changes and build context");
    log(state, Info, "  changes                     — list analyzed changes");
    log(state, Info, "  changes <n>                  — view diff for change <n>");

    log(state, Info, "  model use ollama <model>     — use local Ollama model");
    log(state, Info, "  model use remote <provider> — configure and use remote LLM");
    log(state, Info, "  model show                  — show active model");

    log(state, Info, "  agent run <n>                — generate and run tests for change <n>");
    log(state, Info, "  agent status                 — show agent execution state");
    log(state, Info, "  agent cancel                 — cancel running agent");

    log(state, Info, "  artifacts test               — show last generated test");

    log(state, Info, "  branch new                   — create agent branch");
    log(state, Info, "  branch rollback              — rollback agent branch");
    log(state, Info, "  branch list                  — list git branches");

    log(state, Info, "  clear | logs clear            — clear logs");
    log(state, Info, "  close                        — close active view");
    log(state, Info, "  quit                         — exit osmogrep");
}


fn inspect(state: &mut AgentState) {
    log(state, LogLevel::Info, "Inspecting git changes…");

    state.context.diff_analysis = analyze_diff();
    state.context.test_candidates.clear();
    state.context.last_generated_test = None;
    state.context.generated_tests_ready = false;

    reset_diff_view(state);

    if state.context.diff_analysis.is_empty() {
        log(state, LogLevel::Success, "No relevant changes detected.");
    } else {
        log(
            state,
            LogLevel::Success,
            format!(
                "Inspection complete: {} change(s) analyzed.",
                state.context.diff_analysis.len()
            ),
        );
        log_diff_analysis(state);
    }

    state.lifecycle.phase = Phase::Idle;
}

fn list_changes(state: &mut AgentState) {
    if state.context.diff_analysis.is_empty() {
        log(state, LogLevel::Warn, "No analysis data available. Run `inspect`.");
        return;
    }

    let lines: Vec<String> = state
        .context
        .diff_analysis
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let label = match (&d.symbol, &d.delta) {
                (Some(sym), _) => format!("{} :: {}", d.file, sym),
                (None, Some(_)) => format!("{} :: <file>", d.file),
                _ => d.file.clone(),
            };
            format!("[{}] {}", i, label)
        })
        .collect();

    for line in lines {
        log(state, LogLevel::Info, line);
    }
}

fn view_change(state: &mut AgentState, cmd: &str) {
    let idx = match cmd["changes ".len()..].trim().parse::<usize>() {
        Ok(i) if i < state.context.diff_analysis.len() => i,
        _ => {
            log(state, LogLevel::Warn, "Invalid change index.");
            return;
        }
    };

    let diff = &state.context.diff_analysis[idx];
    if diff.delta.is_none() {
        log(state, LogLevel::Warn, "Selected change has no code delta.");
        return;
    }

    state.ui.selected_diff = Some(idx);
    state.ui.in_diff_view = true;
    state.ui.diff_scroll = 0;
    state.ui.diff_scroll_x = 0;

    log(state, LogLevel::Info, format!("Opened change [{}]", idx));
}

fn agent_run(state: &mut AgentState, cmd: &str) {
    let idx = match cmd["agent run ".len()..].trim().parse::<usize>() {
        Ok(i) if i < state.context.diff_analysis.len() => i,
        _ => {
            log(state, LogLevel::Warn, "Invalid change index.");
            return;
        }
    };

    let diff = state.context.diff_analysis[idx].clone();
    let candidates = generate_test_candidates(std::slice::from_ref(&diff));

    if candidates.is_empty() {
        log(state, LogLevel::Warn, "No test candidates generated.");
        return;
    }

    state.context.test_candidates = candidates;
    state.lifecycle.phase = Phase::ExecuteAgent;
}

fn agent_status(state: &mut AgentState) {
    status_agent(state);
}

fn agent_cancel(state: &mut AgentState) {
    if state.lifecycle.phase != Phase::Running {
        log(state, LogLevel::Warn, "No agent is currently running.");
        return;
    }

    state.cancel_requested.store(true, Ordering::SeqCst);
    state.ui.active_spinner = Some("Canceling agent…".into());
    log(state, LogLevel::Warn, "Agent cancellation requested.");
}


fn model_use(state: &mut AgentState, cmd: &str) {
    let args: Vec<&str> = cmd["model use ".len()..].split_whitespace().collect();

    if args.is_empty() {
        log(state, LogLevel::Warn, "Usage: model use <ollama|remote> ...");
        return;
    }

    match args[0] {
        "ollama" => model_use_ollama(state, &args),
        "remote" => model_use_remote(state, &args),
        _ => log(state, LogLevel::Warn, "Usage: model use <ollama|remote> ..."),
    }
}

fn model_use_ollama(state: &mut AgentState, args: &[&str]) {
    if args.len() != 2 {
        log(state, LogLevel::Warn, "Usage: model use ollama <model>");
        return;
    }

    let model = args[1].to_string();
    state.llm_backend = LlmBackend::ollama(model.clone());

    log(
        state,
        LogLevel::Success,
        format!("Ollama model activated: {}", model),
    );
}

fn model_use_remote(state: &mut AgentState, args: &[&str]) {
    if args.len() < 4 {
        log(
            state,
            LogLevel::Warn,
            "Usage: model use remote <openai|anthropic> <model> <key> [url]",
        );
        return;
    }

    let provider = args[1];
    let model = args[2].to_string();
    let api_key = args[3].to_string();
    let base_url = args.get(4).map(|s| s.to_string());

    let client = LlmClient::new();
    if let Err(e) = client.configure(provider, model.clone(), api_key, base_url) {
        log(state, LogLevel::Error, e);
        return;
    }

    state.llm_backend = LlmBackend::remote(client);

    log(
        state,
        LogLevel::Success,
        format!("Remote model activated: {} ({})", model, provider),
    );
}

fn model_show(state: &mut AgentState) {
    match &state.llm_backend {
        LlmBackend::Ollama { model } => {
            log(state, LogLevel::Info, format!("Active model: Ollama ({})", model));
        }
        LlmBackend::Remote { client } => {
            let cfg = client.current_config();
            log(
                state,
                LogLevel::Info,
                format!("Active model: Remote ({:?} / {})", cfg.provider, cfg.model),
            );
        }
    }
}


fn status_all(state: &mut AgentState) {
    status_agent(state);
    status_context(state);
    status_git(state);
}

fn status_agent(state: &mut AgentState) {
    log(state, LogLevel::Info, format!("Phase: {:?}", state.lifecycle.phase));
    log(
        state,
        LogLevel::Info,
        format!(
            "Cancel requested: {}",
            state.cancel_requested.load(Ordering::SeqCst)
        ),
    );
    log(
        state,
        LogLevel::Info,
        format!("Test candidates: {}", state.context.test_candidates.len()),
    );
}

fn status_context(state: &mut AgentState) {
    match &state.context_snapshot {
        Some(snapshot) => log(
            state,
            LogLevel::Info,
            format!("Context snapshot: {} file(s)", snapshot.files.len()),
        ),
        None => log(state, LogLevel::Info, "Context snapshot not built yet."),
    }
}

fn status_prompt(state: &mut AgentState) {
    match std::fs::read_to_string(".osmogrep_prompt.txt") {
        Ok(p) => {
            log(state, LogLevel::Info, "Last prompt (first 20 lines):");
            for line in p.lines().take(20) {
                log(state, LogLevel::Info, line);
            }
        }
        Err(_) => log(state, LogLevel::Warn, "No prompt artifact found."),
    }
}

fn status_git(state: &mut AgentState) {
    log(state, LogLevel::Info, format!("Branch: {}", git::current_branch()));
    log(
        state,
        LogLevel::Info,
        format!("Dirty: {}", git::working_tree_dirty()),
    );
}

fn status_system(state: &mut AgentState) {
    log(state, LogLevel::Info, format!("OS: {}", std::env::consts::OS));
    log(state, LogLevel::Info, format!("Arch: {}", std::env::consts::ARCH));
}

fn show_test_artifact(state: &mut AgentState) {
    let Some(code) = state.context.last_generated_test.clone() else {
        log(state, LogLevel::Warn, "No generated test available.");
        return;
    };

    let Some(candidate) = state.context.test_candidates.first().cloned() else {
        return;
    };

    state.ui.panel_view = Some(SinglePanelView::TestGenPreview {
        candidate,
        generated_test: Some(code),
    });

    state.ui.focus = Focus::Execution;
    state.ui.exec_scroll = 0;
}

fn close_view(state: &mut AgentState) {
    let mut closed = false;

    if state.ui.in_diff_view {
        reset_diff_view(state);
        closed = true;
    }

    if state.ui.panel_view.is_some() {
        state.ui.panel_view = None;
        state.ui.exec_scroll = 0;
        closed = true;
    }

    if closed {
        state.ui.focus = Focus::Input;
        log(state, LogLevel::Info, "Closed view.");
    } else {
        log(state, LogLevel::Warn, "No active view to close.");
    }
}

fn clear_logs(state: &mut AgentState) {
    state.logs.clear();
    state.ui.exec_scroll = 0;
    state.ui.dirty = true;
}

fn reset_diff_view(state: &mut AgentState) {
    state.ui.in_diff_view = false;
    state.ui.selected_diff = None;
    state.ui.diff_scroll = 0;
}

pub fn update_command_hints(state: &mut AgentState) {
    let input = state.ui.input.clone();
    state.clear_hint();
    state.clear_autocomplete();

    if input.is_empty() {
        state.set_hint("Type `help` to see available commands");
        return;
    }

    macro_rules! hint {
        ($cmd:expr, $desc:expr) => {
            if $cmd.starts_with(&input) {
                state.set_autocomplete($cmd);
                state.set_hint($desc);
                return;
            }
        };
    }

    hint!("inspect", "Inspect git changes");
    hint!("changes", "List detected changes");
    hint!("changes ", "View change <n>");

    hint!("model use ollama ", "Use local Ollama model");
    hint!("model use remote ", "Use remote LLM (OpenAI / Anthropic)");
    hint!("model show", "Show active model");

    hint!("agent run ", "Run agent on change <n>");
    hint!("agent status", "Agent status");
    hint!("agent cancel", "Cancel running agent");

    hint!("status", "Show all status");
    hint!("status agent", "Agent status");
    hint!("status context", "Context status");
    hint!("status prompt", "Prompt status");
    hint!("status git", "Git status");
    hint!("status system", "System info");

    hint!("artifacts test", "Show generated test");

    hint!("branch new", "Create agent branch");
    hint!("branch rollback", "Rollback agent branch");
    hint!("branch list", "List branches");

    hint!("clear", "Clear logs");
    hint!("logs clear", "Clear logs");
    hint!("close", "Close active view");
    hint!("quit", "Quit Osmogrep");
    hint!("help", "Show help");

    state.set_hint("Unknown command");
}
