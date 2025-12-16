// src/machine.rs
use crate::{git, logger::log};
use crate::state::{AgentState, LogLevel, Phase};

pub fn step(state: &mut AgentState) {
    match state.phase {
        Phase::Init => {
            if !git::is_git_repo() {
                log(state, LogLevel::Error, "Not a git repository.");
                state.phase = Phase::Done;
                return;
            }
            state.original_branch = Some(git::current_branch());
            state.phase = Phase::DetectBase;
        }

        Phase::DetectBase => {
            let base = git::detect_base_branch();
            state.base_branch = Some(base.clone());
            log(state, LogLevel::Success, format!("Base branch: {}", base));

            if let Some(agent) = git::find_existing_agent() {
                state.agent_branch = Some(agent.clone());
                log(state, LogLevel::Info, format!("Reusing {}", agent));
            }

            if git::working_tree_dirty() {
                log(state, LogLevel::Warn, "Uncommitted changes detected.");
            }

            state.phase = Phase::Idle;
        }

        Phase::CreateNewAgent => {
            let branch = git::create_agent_branch();
            state.agent_branch = Some(branch.clone());
            log(state, LogLevel::Success, format!("Created {}", branch));
            state.phase = Phase::Idle;
        }

        Phase::ExecuteAgent => {
            if let Some(branch) = state.agent_branch.clone() {
                git::checkout(&branch);
                let diff = git::diff();
                if diff.is_empty() {
                    log(state, LogLevel::Info, "No working tree changes to apply.");
                } else if let Err(e) = git::apply_diff(&diff) {
                    log(state, LogLevel::Error, e);
                } else {
                    log(state, LogLevel::Success, "Working tree applied.");
                }
            }
            state.phase = Phase::Idle;
        }

        Phase::Rollback => {
            if let Some(orig) = state.original_branch.clone() {
                git::checkout(&orig);
                log(state, LogLevel::Success, "Returned to original branch.");
            }
            state.phase = Phase::Idle;
        }

        Phase::Idle | Phase::Done => {}
    }
}
