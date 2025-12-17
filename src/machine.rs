use std::path::Path;

use crate::{
    git,
    logger::log,
    detectors::{language::detect_language, framework::detect_framework},
};
use crate::state::{AgentState, LogLevel, Phase};

pub fn step(state: &mut AgentState) {
    match state.phase {
        /* ---------- INIT ---------- */
        Phase::Init => {
            if !git::is_git_repo() {
                log(state, LogLevel::Error, "Not a git repository.");
                state.phase = Phase::Done;
                return;
            }

            let current = git::current_branch();
            state.current_branch = Some(current.clone());
            state.original_branch = Some(current);

            let root = Path::new(".");
            state.language = Some(detect_language(root));
            state.framework = Some(detect_framework(root));

            state.phase = Phase::DetectBase;
        }

        /* ---------- BASE BRANCH ---------- */
        Phase::DetectBase => {
            let base = git::detect_base_branch();
            state.base_branch = Some(base.clone());
            log(state, LogLevel::Success, format!("Base branch: {}", base));

            if let Some(agent) = git::find_existing_agent() {
                state.agent_branch = Some(agent.clone());
                log(state, LogLevel::Info, format!("Found agent branch {}", agent));
            }

            if git::working_tree_dirty() {
                log(state, LogLevel::Warn, "Uncommitted changes detected.");
            }

            state.phase = Phase::Idle;
        }

        /* ---------- NEW AGENT ---------- */
        Phase::CreateNewAgent => {
            let branch = git::create_agent_branch();
            state.agent_branch = Some(branch.clone());

            log(state, LogLevel::Success, format!("Created agent branch {}", branch));
            log(
                state,
                LogLevel::Info,
                "Branch created but not checked out. Use `exec` to switch.",
            );

            state.phase = Phase::Idle;
        }

        /* ---------- EXECUTE ---------- */
        Phase::ExecuteAgent => {
            if let Some(branch) = &state.agent_branch {
                git::checkout(branch);
                state.current_branch = Some(branch.clone());

                log(state, LogLevel::Success, format!("Moved to branch {}", branch));
            } else {
                log(state, LogLevel::Warn, "No agent branch to execute on.");
            }

            state.phase = Phase::Idle;
        }

        /* ---------- ROLLBACK ---------- */
        Phase::Rollback => {
            // Step 1: checkout base/original branch
            if let Some(base) = &state.base_branch {
                git::checkout(base);
                log(
                    state,
                    LogLevel::Success,
                    format!("Checked out base branch {}", base),
                );
            }

            // Step 2: delete agent branch
            if let Some(agent) = &state.agent_branch {
                git::delete_branch(agent);
                log(
                    state,
                    LogLevel::Success,
                    format!("Deleted agent branch {}", agent),
                );
            }

            // Step 3: clear agent branch state
            state.agent_branch = None;

            state.phase = Phase::Idle;
        }

        Phase::Idle | Phase::Done => {}
    }
}
