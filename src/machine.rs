//! machine.rs
//!
//! Agent lifecycle state machine.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::{
    detectors::{framework::detect_framework, language::detect_language},
    executor::run::run_single_test,
    git,
    llm::orchestrator::run_llm_test_flow,
    logger::log,
    testgen::summarizer::summarize,
};

use crate::state::{AgentEvent, AgentState, LogLevel, Phase};

/* ============================================================
   Public API
   ============================================================ */

/// Advance the agent by one lifecycle step.
/// MUST be called from the main loop.
pub fn step(state: &mut AgentState) {
    match state.lifecycle.phase {
        Phase::Init => init_repo(state),
        Phase::DetectBase => detect_base_branch(state),
        Phase::CreateNewAgent => create_agent_branch(state),

        Phase::Idle => attach_summaries(state),

        Phase::ExecuteAgent => execute_agent(state),
        Phase::Running => handle_running(state),
        Phase::Rollback => rollback_agent(state),

        Phase::Done => {}
    }
}

/* ============================================================
   Phase Implementations
   ============================================================ */

fn init_repo(state: &mut AgentState) {
    if !git::is_git_repo() {
        log(state, LogLevel::Error, "Not a git repository.");
        transition(state, Phase::Done);
        return;
    }

    let current = git::current_branch();
    state.lifecycle.current_branch = Some(current.clone());
    state.lifecycle.original_branch = Some(current);

    let repo_root = detect_repo_root();

    // reuse existing detectors
    let language = detect_language(&repo_root);
    let framework = detect_framework(&repo_root);

    state.lifecycle.language = Some(language);
    state.lifecycle.framework = Some(framework);

    // spawn context indexer ONCE, in Init
    if state.context_index.is_none() {
        state.context_index = Some(
            crate::context::spawn_repo_indexer(repo_root.clone())
        );

        log(
            state,
            LogLevel::Info,
            "Indexing repository context",
        );
    }

    transition(state, Phase::DetectBase);
}


fn detect_base_branch(state: &mut AgentState) {
    let base = git::detect_base_branch();
    state.lifecycle.base_branch = Some(base.clone());

    log(state, LogLevel::Success, format!("Base branch detected: {}", base));

    if let Some(agent) = git::find_existing_agent() {
        state.lifecycle.agent_branch = Some(agent.clone());
        log(state, LogLevel::Info, format!("Found existing agent branch {}", agent));
    }

    if git::working_tree_dirty() {
        log(state, LogLevel::Warn, "Working tree contains uncommitted changes.");
    }

    transition(state, Phase::Idle);
}

fn create_agent_branch(state: &mut AgentState) {
    let branch = ensure_agent_branch(state);
    log(state, LogLevel::Success, format!("Created agent branch {}", branch));
    transition(state, Phase::Idle);
}

/* ============================================================
   Execution
   ============================================================ */

fn execute_agent(state: &mut AgentState) {
    if state.cancel_requested.load(Ordering::SeqCst) {
        transition(state, Phase::Rollback);
        return;
    }

    state.cancel_requested.store(false, Ordering::SeqCst);

    let branch = ensure_agent_branch(state);
    checkout_branch(state, &branch);

    let candidate = match state.context.test_candidates.first().cloned() {
        Some(c) => c,
        None => {
            log(state, LogLevel::Warn, "No test candidate available.");
            transition(state, Phase::Idle);
            return;
        }
    };

    let language = match state.lifecycle.language {
        Some(l) => l,
        None => {
            log(state, LogLevel::Error, "Language not detected.");
            transition(state, Phase::Idle);
            return;
        }
    };

    let framework = state.lifecycle.framework.clone();

    log(state, LogLevel::Info, "🤖 Agent started");

    run_llm_test_flow(
        state.agent_tx.clone(),
        state.cancel_requested.clone(),
        language,
        framework,
        candidate,
    );

    transition(state, Phase::Running);
}

fn handle_running(state: &mut AgentState) {
    if state.cancel_requested.load(Ordering::SeqCst) {
        log(state, LogLevel::Warn, "Agent cancelled.");
        return_to_base_branch(state);
        transition(state, Phase::Idle);
        return;
    }
    if state.context.generated_tests_ready {
    state.context.generated_tests_ready = false;

    // Orchestrator already ran tests and emitted results
    return_to_base_branch(state);
    transition(state, Phase::Idle);
    }
}


/* ============================================================
   Rollback
   ============================================================ */

fn rollback_agent(state: &mut AgentState) {
    if let Some(base) = state.lifecycle.base_branch.clone() {
        checkout_branch(state, &base);
    }

    if let Some(agent) = state.lifecycle.agent_branch.take() {
        git::delete_branch(&agent);
        log(state, LogLevel::Success, format!("Deleted agent branch {}", agent));
    }

    reset_views(state);
    transition(state, Phase::Idle);
}

/* ============================================================
   Helpers
   ============================================================ */

fn ensure_agent_branch(state: &mut AgentState) -> String {
    if let Some(branch) = &state.lifecycle.agent_branch {
        return branch.clone();
    }

    let branch = git::create_agent_branch();
    state.lifecycle.agent_branch = Some(branch.clone());
    branch
}

fn checkout_branch(state: &mut AgentState, branch: &str) {
    git::checkout(branch);
    state.lifecycle.current_branch = Some(branch.to_string());
}

fn reset_views(state: &mut AgentState) {
    state.ui.in_diff_view = false;
    state.ui.selected_diff = None;
    state.ui.diff_scroll = 0;
}

fn detect_repo_root() -> PathBuf {
    Path::new(".").to_path_buf()
}

fn transition(state: &mut AgentState, next: Phase) {
    state.lifecycle.phase = next;
}

fn return_to_base_branch(state: &mut AgentState) {
    if let Some(base) = state.lifecycle.base_branch.clone() {
        git::checkout(&base);
        state.lifecycle.current_branch = Some(base);
    }
}

/* ============================================================
   Semantic enrichment
   ============================================================ */

fn attach_summaries(state: &mut AgentState) {
    for diff in &mut state.context.diff_analysis {
        if diff.summary.is_none() {
            diff.summary = Some(summarize(diff));
        }
    }
}
