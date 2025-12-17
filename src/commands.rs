use crate::state::{AgentState, Phase, LogLevel};
use crate::logger::log;
use crate::detectors::diff_analyzer::analyze_diff;

pub fn handle_command(state: &mut AgentState, cmd: &str) {
    state.clear_hint();
    state.clear_autocomplete();

    match cmd {
        "help" => {
            log(
                state,
                LogLevel::Info,
                "Commands: analyze | exec | new | rollback | quit | help",
            );
        }

        "analyze" => {
            log(state, LogLevel::Info, "Analyzing git diff for test relevance…");

            state.diff_analysis = analyze_diff();

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

        "new" => {
            log(state, LogLevel::Info, "Creating new agent branch…");
            state.phase = Phase::CreateNewAgent;
        }

        "exec" => {
            log(state, LogLevel::Info, "Executing agent…");
            state.phase = Phase::ExecuteAgent;
        }

        "rollback" => {
            log(state, LogLevel::Warn, "Rolling back to original branch…");
            state.phase = Phase::Rollback;
        }

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
    } else if "exec".starts_with(&input) {
        state.set_autocomplete("exec");
        state.set_hint("exec — switch to agent branch");
    } else if "new".starts_with(&input) {
        state.set_autocomplete("new");
        state.set_hint("new — create a fresh agent branch");
    } else if "rollback".starts_with(&input) {
        state.set_autocomplete("rollback");
        state.set_hint("rollback — return to original branch");
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
