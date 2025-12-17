use crate::state::{AgentState, Phase, LogLevel};
use crate::logger::log;
use crate::detectors::diff_analyzer::analyze_diff;
use crate::git;

pub fn handle_command(state: &mut AgentState, cmd: &str) {
    state.clear_hint();
    state.clear_autocomplete();

    match cmd {
        "help" => {
            log(
                state,
                LogLevel::Info,
                "Commands: analyze | diff | diff <n> | exec | new | rollback | bls | quit | help",
            );
        }

        /* ---------- ANALYZE ---------- */

        "analyze" => {
            log(state, LogLevel::Info, "Analyzing git diff for test relevance…");

            state.diff_analysis = analyze_diff();
            state.selected_diff = None;
            state.in_diff_view = false;
            state.diff_scroll = 0;

            if state.diff_analysis.is_empty() {
                log(state, LogLevel::Success, "No relevant changes detected.");
            } else {
                log(
                    state,
                    LogLevel::Success,
                    format!(
                        "Diff analysis complete: {} file(s) analyzed.",
                        state.diff_analysis.len()
                    ),
                );
            }

            state.phase = Phase::Idle;
        }

        /* ---------- DIFF LIST ---------- */

        "diff" => {
            if state.diff_analysis.is_empty() {
                log(state, LogLevel::Warn, "No diff analysis available. Run `analyze` first.");
                return;
            }

            let messages: Vec<String> = state
                .diff_analysis
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    format!(
                        "[{}] {}{}",
                        i,
                        d.file,
                        d.symbol
                            .as_ref()
                            .map(|s| format!(" :: {}", s))
                            .unwrap_or_default()
                    )
                })
                .collect();

            for msg in messages {
                log(state, LogLevel::Info, msg);
            }
        }


        /* ---------- DIFF VIEW ---------- */

        cmd if cmd.starts_with("diff ") => {
            if state.diff_analysis.is_empty() {
                log(state, LogLevel::Warn, "No diff analysis available.");
                return;
            }

            match cmd[5..].trim().parse::<usize>() {
                Ok(idx) if idx < state.diff_analysis.len() => {
                    if state.diff_analysis[idx].delta.is_some() {
                        state.selected_diff = Some(idx);
                        state.in_diff_view = true;
                        state.diff_scroll = 0;

                        log(
                            state,
                            LogLevel::Info,
                            format!("Opened diff view for [{}]", idx),
                        );
                    } else {
                        log(
                            state,
                            LogLevel::Warn,
                            "Selected file has no symbol-level diff.",
                        );
                    }
                }
                _ => {
                    log(state, LogLevel::Warn, "Invalid diff index.");
                }
            }
        }

        "close" => {
            state.in_diff_view = false;
            state.selected_diff = None;
            state.diff_scroll = 0;

            log(state, LogLevel::Info, "Closed diff view.");
        }

        /* ---------- BRANCH OPS ---------- */

        "new" => {
            log(state, LogLevel::Info, "Creating new agent branch…");
            state.phase = Phase::CreateNewAgent;
        }

        "exec" => {
            log(state, LogLevel::Info, "Executing agent…");
            state.phase = Phase::ExecuteAgent;
        }

        "rollback" => {
            log(
                state,
                LogLevel::Warn,
                "Deleting agent branch and rolling back to base branch…",
            );
            state.phase = Phase::Rollback;
        }

        "bls" => {
            let branches = git::list_branches();
            for b in branches {
                log(state, LogLevel::Info, b);
            }
        }

        /* ---------- EXIT ---------- */

        "quit" => {
            log(state, LogLevel::Info, "Exiting OsmoGrep.");
            state.phase = Phase::Done;
        }

        "" => {}

        _ => {
            log(state, LogLevel::Warn, "Unknown command. Type `help`.");
        }
    }
}

/* ============================================================
   Autocomplete + hints
   ============================================================ */

pub fn update_command_hints(state: &mut AgentState) {
    let input = state.input.clone();
    state.clear_hint();
    state.clear_autocomplete();

    if input.is_empty() {
        state.set_hint("Type `help` to see available commands");
        return;
    }

    if "analyze".starts_with(&input) {
        state.set_autocomplete("analyze");
        state.set_hint("analyze — inspect git diff and assess test need");
    } else if "diff".starts_with(&input) {
        state.set_autocomplete("diff");
        state.set_hint("diff — list diffs, or `diff <n>` to view");
    } else if "exec".starts_with(&input) {
        state.set_autocomplete("exec");
        state.set_hint("exec — switch to agent branch");
    } else if "new".starts_with(&input) {
        state.set_autocomplete("new");
        state.set_hint("new — create a fresh agent branch");
    } else if "rollback".starts_with(&input) {
        state.set_autocomplete("rollback");
        state.set_hint("rollback — delete agent branch and return");
    } else if "bls".starts_with(&input) {
        state.set_autocomplete("bls");
        state.set_hint("bls — list local git branches");
    } else if "close".starts_with(&input) {
        state.set_autocomplete("close");
        state.set_hint("close — exit diff viewer");
    } else if "quit".starts_with(&input) {
        state.set_autocomplete("quit");
        state.set_hint("quit — exit OsmoGrep");
    } else if "help".starts_with(&input) {
        state.set_autocomplete("help");
        state.set_hint("help — list commands");
    } else {
        state.set_hint("Unknown command");
    }
}
