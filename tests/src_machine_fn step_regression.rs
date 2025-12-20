
```
// src/machine.rs

/// Steps a machine to execute an agent, possibly including rollback and execution.
fn step(state: &mut AgentState) {
    match state.phase {
        // ---------- INIT ----------
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

        // ---------- BASE BRANCH ----------
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

        // ---------- NEW AGENT ----------
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

        // ---------- RUN AGENT ----------
        Phase::ExecuteAgent => {
            // 1. Ensure agent branch exists
            let agent_branch = match &state.agent_branch {
                Some(b) => b.clone(),
                None => {
                    let b = git::create_agent_branch();
                    state.agent_branch = Some(b.clone());
                    log(
                        state,
                        LogLevel::Success,
                        format!("Created agent branch {}", b),
                    );
                    b
                }
            };

            // 2. Checkout agent branch
            git::checkout(&agent_branch);
            state.current_branch = Some(agent_branch.clone());

            log(
                state,
                LogLevel::Success,
                format!("Checked out agent branch {}", agent_branch),
            );

            // 3. Get the single test candidate
            let candidate = match state.test_candidates.first().cloned() {
                Some(c) => c,
                None => {
                    log(
                        state,
                        LogLevel::Warn,
                        "No test candidate available. Run `testgen` first.",
                    );
                    state.phase = Phase::Idle;
                    return;
                }
            };

            if let Err(e) = run_llm_test_flow(state, &candidate) {
                log(
                    state,
                    LogLevel::Error,
                    format!("Agent failed: {}", e),
                );
            } else {
                log(state, LogLevel::Success, "Agent run completed.");
            }

            state.focus = crate::state::Focus::Execution;
            state.phase = Phase::Idle;
        }

        // ---------- ROLLBACK ----------
        Phase::Rollback => {
            // Step 1: checkout base branch
            if let Some(base) = state.base_branch.clone() {
                git::checkout(&base);
                state.current_branch = Some(base.clone());
                log(
                    state,
                    LogLevel::Success,
                    format!("Checked out base branch {}", base),
                );
            }

            // Step 2: delete agent branch
            if let Some(agent) = state.agent_branch.take() {
                git::delete_branch(&agent);
                log(
                    state,
                    LogLevel::Success,
                    format!("Deleted agent branch {}", agent),
                );
            }

            // Step 3: reset UI state
            state.in_diff_view = false;
            state.selected_diff = None;
            state.diff_scroll = 0;

            state.phase = Phase::Idle;
        }

        _ => {
            log(state, LogLevel::Error, "Unhandled phase in step function.");
            state.phase = Phase::Done;
        }
    }
}

// Testgen rules
#[cfg(test)]
mod testgen {
    // ...
}
```
