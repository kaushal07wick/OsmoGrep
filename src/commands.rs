//! command.rs
//!
//! Command interpretation layer.
//!
//! Responsibilities:
//! - Parse and validate user commands
//! - Translate commands into explicit state mutations
//! - Emit informational logs
//!
//! Non-responsibilities:
//! - Agent orchestration
//! - Git side effects beyond intent signaling
//! - UI rendering logic

use std::time::Instant;

use crate::{
    detectors::diff_analyzer::analyze_diff,
    git,
    logger::{log, log_diff_analysis},
    testgen::{generator::generate_test_candidates, resolve::resolve_test},
};
use crate::state::{AgentState, Focus, LogLevel, Phase, SinglePanelView};

/* ============================================================
   Command Handling
   ============================================================ */

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
        "agent cancel" => agent_cancel(state), // ✅ FIX

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


/* ============================================================
   Command Implementations
   ============================================================ */

fn help(state: &mut AgentState) {
    use LogLevel::Info;

    log(state, Info, "Commands:");
    log(state, Info, "  inspect                  — analyze git changes");
    log(state, Info, "  changes                  — list analyzed diffs");
    log(state, Info, "  changes <n>               — view diff details");
    log(state, Info, "  agent run <n>             — execute agent on diff");
    log(state, Info, "  agent status              — show agent context");
    log(state, Info, "  agent cancel              — cancel running agent");
    log(state, Info, "  artifacts test            — view generated test");
    log(state, Info, "  branch new                — create agent branch");
    log(state, Info, "  branch rollback           — rollback agent branch");
    log(state, Info, "  branch list               — list branches");
    log(state, Info, "  close                     — close active view");
    log(state, Info, "  quit                      — exit osmogrep");
}


fn inspect(state: &mut AgentState) {
    log(state, LogLevel::Info, "Inspecting git changes…");

    state.context.diff_analysis = analyze_diff();
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

    let entries: Vec<String> = state
        .context
        .diff_analysis
        .iter()
        .enumerate()
        .map(|(i, d)| {
            match (&d.symbol, &d.delta) {
                (Some(sym), _) => format!("[{}] {} :: {}", i, d.file, sym),
                (None, Some(_)) => format!("[{}] {} :: <file>", i, d.file),
                _ => format!("[{}] {}", i, d.file),
            }
        })
        .collect();

    for line in entries {
        log(state, LogLevel::Info, line);
    }
}


fn view_change(state: &mut AgentState, cmd: &str) {
    if state.context.diff_analysis.is_empty() {
        log(state, LogLevel::Warn, "No analysis data available.");
        return;
    }

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

    let target = match &diff.symbol {
        Some(sym) => format!("{} :: {}", diff.file, sym),
        None => diff.file.clone(),
    };

    log(state, LogLevel::Info, format!("Opened change [{}] {}", idx, target));
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
    let candidates = generate_test_candidates(
        std::slice::from_ref(&state.context.diff_analysis[idx]),
        |c| resolve_test(
        state.lifecycle.language.as_ref().unwrap(),
        c,
    ),

    );


    if candidates.is_empty() {
        log(state, LogLevel::Warn, "No viable test candidates produced.");
        return;
    }

    state.context.test_candidates = vec![candidates[0].clone()];

    log(
        state,
        LogLevel::Success,
        format!("Agent scheduled for change [{}]", idx),
    );

    state.lifecycle.phase = Phase::ExecuteAgent;
}

fn agent_status(state: &mut AgentState) {
    log(
        state,
        LogLevel::Info,
        format!("Agent phase: {:?}", state.lifecycle.phase),
    );
    log(
        state,
        LogLevel::Info,
        format!(
            "Test candidates: {}",
            state.context.test_candidates.len()
        ),
    );
}

fn agent_cancel(state: &mut AgentState) {
    use std::sync::atomic::Ordering;

    if state.lifecycle.phase != Phase::Running {
        log(state, LogLevel::Warn, "No agent is currently running.");
        return;
    }

    state.cancel_requested.store(true, Ordering::SeqCst);
    state.ui.active_spinner = Some("🛑 Canceling agent…".into());

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
        None => {
            log(state, LogLevel::Warn, "No test candidate context available.");
            return;
        }
    };

    state.ui.panel_view = Some(SinglePanelView::TestGenPreview {
        candidate,
        generated_test: Some(code),
    });

    state.ui.focus = Focus::Execution;
    state.ui.exec_scroll = 0;

    log(state, LogLevel::Info, "Opened test artifact.");
}

fn close_view(state: &mut AgentState) {
    if state.ui.panel_view.is_some() {
        state.ui.panel_view = None;
        state.ui.exec_scroll = 0;
        state.ui.focus = Focus::Input;
        log(state, LogLevel::Info, "Closed panel.");
        return;
    }

    reset_diff_view(state);
    log(state, LogLevel::Info, "Closed view.");
}

/* ============================================================
   Autocomplete + Hints
   ============================================================ */

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

    hint!("inspect", "inspect — analyze git changes");
    hint!("changes", "changes — list or view analyzed diffs");
    hint!("agent run ", "agent run <n> — execute agent on diff");
    hint!("agent status", "agent status — show agent context");
    hint!("agent cancel", "agent cancel - cancel running agent");
    hint!("artifacts test", "artifacts test — view generated test");
    hint!("branch new", "branch new — create agent branch");
    hint!("branch rollback", "branch rollback — rollback agent branch");
    hint!("branch list", "branch list — list branches");
    hint!("close", "close — close active view");
    hint!("quit", "quit — exit osmogrep");
    hint!("help", "help — show commands");

    state.set_hint("Unknown command");
}

/* ============================================================
   Small Helpers
   ============================================================ */

fn reset_diff_view(state: &mut AgentState) {
    state.ui.in_diff_view = false;
    state.ui.selected_diff = None;
    state.ui.diff_scroll = 0;
}
