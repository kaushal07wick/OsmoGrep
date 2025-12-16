// src/machine.rs
use std::path::Path;

use crate::{
    git,
    logger::log,
    detectors::{
        language::detect_language,
        framework::detect_framework,
    },
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

            // capture original branch
            state.original_branch = Some(git::current_branch());

            // === STEP 1: STATIC DETECTION ===
            let root = Path::new(".");

            let lang = detect_language(root);
            let fw = detect_framework(root);

            state.language = Some(lang.clone());
            state.framework = Some(fw.clone());

            log(
                state,
                LogLevel::Info,
                format!("Detected language: {:?}, framework: {:?}", lang, fw),
            );

            state.phase = Phase::DetectBase;
        }

        /* ---------- BASE BRANCH ---------- */
        Phase::DetectBase => {
            let base = git::detect_base_branch();
            state.base_branch = Some(base.clone());
            log(state, LogLevel::Success, format!("Base branch: {}", base));

            // reuse existing agent branch if present
            if let Some(agent) = git::find_existing_agent() {
                state.agent_branch = Some(agent.clone());
                log(state, LogLevel::Info, format!("Reusing {}", agent));
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
            log(state, LogLevel::Success, format!("Created {}", branch));
            state.phase = Phase::Idle;
        }

        /* ---------- EXECUTE (NO TESTS YET) ---------- */
        Phase::ExecuteAgent => {
            if let Some(branch) = state.agent_branch.clone() {
                git::checkout(&branch);
                log(state, LogLevel::Info, format!("Checked out {}", branch));

                // NOTE:
                // Diff application stays here for now,
                // but STEP 1 does NOT depend on this.
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

        /* ---------- ROLLBACK ---------- */
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
