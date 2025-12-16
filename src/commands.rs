// src/commands.rs

use crate::state::{AgentState, Phase, LogLevel};
use crate::logger::log;

/* ============================================================
   Execute committed command (ENTER)
   ============================================================ */

pub fn handle_command(state: &mut AgentState, cmd: &str) {
    state.clear_hint();
    state.clear_autocomplete();

    match cmd {
        "help" => {
            log(
                state,
                LogLevel::Info,
                "Commands: exec | new | rollback | quit | help",
            );
        }

        "exec" => {
            log(state, LogLevel::Info, "Executing agent on working tree…");
            state.phase = Phase::ExecuteAgent;
        }

        "new" => {
            log(state, LogLevel::Info, "Creating new agent branch…");
            state.phase = Phase::CreateNewAgent;
        }

        "rollback" => {
            log(state, LogLevel::Warn, "Rolling back to original branch…");
            state.phase = Phase::Rollback;
        }

        "quit" => {
            log(state, LogLevel::Info, "Exiting OsmoGrep.");
            state.phase = Phase::Done;
        }

        "" => {
            // ignore empty
        }

        _ => {
            log(state, LogLevel::Warn, "Unknown command. Type `help`.");
        }
    }
}

/* ============================================================
   Live hints + autocomplete (ON EVERY KEYSTROKE)
   ============================================================ */

pub fn update_command_hints(state: &mut AgentState) {
    // ⬅️ clone FIRST, then mutate state
    let input = state.input.clone();

    state.clear_hint();
    state.clear_autocomplete();

    if input.is_empty() {
        state.set_hint("Type `help` to see available commands");
        return;
    }

    if "exec".starts_with(&input) {
        state.set_autocomplete("exec");
        state.set_hint("exec — apply working tree and run tests");
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
        state.set_hint("help — list available commands");
    } else {
        state.set_hint("Unknown command");
    }
}
