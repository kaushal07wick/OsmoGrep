//! command.rs
//!
//! Command interpretation layer.
//!
//! Responsibilities:
//! - Parse and validate user commands
//! - Translate commands into explicit state mutations
//! - Emit informational logs

use std::time::Instant;
use std::sync::atomic::Ordering;

use crate::context::types::IndexStatus;
use crate::detectors::diff_analyzer::analyze_diff;
use crate::git;
use crate::logger::{log, log_diff_analysis};
use crate::testgen::generator::generate_test_candidates;
use crate::state::{
    AgentState,
    Focus,
    LogLevel,
    Phase,
    SinglePanelView,
};


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
        cmd if cmd.starts_with("model set ") => set_model(state, cmd),
        "model show" => show_model(state),
        "status" => status_all(state),
        "status model" => status_model(state),
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

        _ => {
            log(state, LogLevel::Warn, "Unknown command. Type `help`.");
        }
    }
}


fn help(state: &mut AgentState) {
    use LogLevel::Info;

    log(state, Info, "Commands:");
    log(state, Info, "  inspect                   — analyze git changes");
    log(state, Info, "  changes                   — list analyzed diffs");
    log(state, Info, "  changes <n>                — view diff details");
    log(state, Info, "  model set <name>           — set Ollama model");
    log(state, Info, "  model show                — show active model");

    log(state, Info, "  agent run <n>              — execute agent on diff");
    log(state, Info, "  agent status               — show agent state");
    log(state, Info, "  agent cancel               — cancel running agent");

    log(state, Info, "  status                     — full system overview");
    log(state, Info, "  status model               — LLM configuration");
    log(state, Info, "  status agent               — agent lifecycle");
    log(state, Info, "  status context             — context engine");
    log(state, Info, "  status prompt              — last generated prompt");
    log(state, Info, "  status git                 — git repository state");
    log(state, Info, "  status system              — system info");
    log(state, Info, "  clear                    — clear execution logs");
    log(state, Info, "  logs clear               — clear execution logs");
    log(state, Info, "  artifacts test             — view generated test");
    log(state, Info, "  branch new                 — create agent branch");
    log(state, Info, "  branch rollback            — rollback agent branch");
    log(state, Info, "  branch list                — list branches");
    log(state, Info, "  close                      — close active view");
    log(state, Info, "  quit                       — exit osmogrep");
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

    // Collect first (immutable borrow only)
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

    // Log after borrow is released
    for line in lines {
        log(state, LogLevel::Info, line);
    }
}

fn clear_logs(state: &mut AgentState) {
    state.logs.clear();
    state.ui.exec_scroll = 0;
    state.ui.auto_scroll = true;
    state.ui.dirty = true;
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

    // ---- extract index status without holding borrow ----
    let index_status = match &state.context_index {
        Some(h) => h.status.read().unwrap().clone(),
        None => {
            log(state, LogLevel::Warn, "Repository index not ready.");
            return;
        }
    };

    match index_status {
        IndexStatus::Ready => {}
        IndexStatus::Indexing => {
            log(state, LogLevel::Warn, "Repository index still building.");
            return;
        }
        IndexStatus::Failed(err) => {
            log(state, LogLevel::Error, err);
            return;
        }
    }

    // ---- extract diff before mutation/logging ----
    let diff = state.context.diff_analysis[idx].clone();
    let candidates = generate_test_candidates(std::slice::from_ref(&diff));

    if candidates.is_empty() {
        log(state, LogLevel::Warn, "No test candidates generated.");
        return;
    }

    state.context.test_candidates = candidates;
    state.lifecycle.phase = Phase::ExecuteAgent;
}


fn set_model(state: &mut AgentState, cmd: &str) {
    let model = cmd["model set ".len()..].trim();

    if model.is_empty() {
        log(state, LogLevel::Warn, "Usage: model set <ollama-model-name>");
        return;
    }

    state.ollama_model = model.to_string();

    log(
        state,
        LogLevel::Success,
        format!("Ollama model set to `{}`", model),
    );
}

fn show_model(state: &mut AgentState) {
    let model = if state.ollama_model.is_empty() {
        "<default>"
    } else {
        &state.ollama_model
    };

    log(
        state,
        LogLevel::Info,
        format!("Active model: {}", model),
    );
}

fn status_all(state: &mut AgentState) {
    status_agent(state);
    status_model(state);
    status_context(state);
    status_git(state);
}

fn status_model(state: &mut AgentState) {
    let model = if state.ollama_model.is_empty() {
        "<default>"
    } else {
        &state.ollama_model
    };

    log(
        state,
        LogLevel::Info,
        format!("Model: {}", model),
    );
}

fn status_agent(state: &mut AgentState) {
    log(state, LogLevel::Info, format!("Phase: {:?}", state.lifecycle.phase));
    log(
        state,
        LogLevel::Info,
        format!("Cancel requested: {}", state.cancel_requested.load(Ordering::SeqCst)),
    );
    log(
        state,
        LogLevel::Info,
        format!("Test candidates: {}", state.context.test_candidates.len()),
    );
}

fn status_context(state: &mut AgentState) {
    match &state.context_index {
        Some(i) => {
            log(
                state,
                LogLevel::Info,
                format!("Index status: {:?}", i.status.read().unwrap()),
            );
        }
        None => log(state, LogLevel::Warn, "Context index not initialized."),
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

fn show_test_artifact(state: &mut AgentState) {
    let code = match &state.context.last_generated_test {
        Some(c) => c.clone(),
        None => {
            log(state, LogLevel::Warn, "No generated test available.");
            return;
        }
    };

    let candidate = match state.context.test_candidates.first().cloned() {
        Some(c) => c,
        None => return,
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

    // ----- core workflow -----
    hint!("inspect", "inspect — analyze git changes");
    hint!("changes", "changes — list analyzed diffs");
    hint!("changes ", "changes <n> — view diff details");

    // ----- agent -----
    hint!("agent run ", "agent run <n> — execute agent on diff");
    hint!("agent status", "agent status — show agent lifecycle state");
    hint!("agent cancel", "agent cancel — cancel running agent");

    // ----- model -----
    hint!("model set ", "model set <name> — set Ollama model");
    hint!("model show", "model show — display active model");

    // ----- status / inspection -----
    hint!("status", "status — full system overview");
    hint!("status model", "status model — model configuration");
    hint!("status agent", "status agent — agent lifecycle");
    hint!("status context", "status context — context engine state");
    hint!("status prompt", "status prompt — last generated prompt");
    hint!("status git", "status git — git repository status");
    hint!("status system", "status system — system info");
    hint!("clear", "clear — clear execution logs");
    hint!("logs clear", "logs clear — clear execution logs");

    // ----- artifacts -----
    hint!("artifacts test", "artifacts test — view generated test");

    // ----- branch management -----
    hint!("branch new", "branch new — create agent branch");
    hint!("branch rollback", "branch rollback — rollback agent branch");
    hint!("branch list", "branch list — list git branches");

    // ----- ui / lifecycle -----
    hint!("close", "close — close active view");
    hint!("quit", "quit — exit osmogrep");
    hint!("help", "help — show all commands");

    state.set_hint("Unknown command");
}


fn reset_diff_view(state: &mut AgentState) {
    state.ui.in_diff_view = false;
    state.ui.selected_diff = None;
    state.ui.diff_scroll = 0;
}
