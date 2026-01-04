use std::hint;
use std::sync::atomic::Ordering;
use std::time::Instant;
use std::sync::mpsc;
use crate::detectors::diff_analyzer::analyze_diff;
use crate::git;
use crate::logger::{log, log_diff_analysis};
use crate::llm::backend::LlmBackend;
use crate::llm::client::{LlmClient, Provider};
use crate::state::{
    AgentState, Focus, LogLevel, Phase, SinglePanelView, InputMode,
};
use crate::testgen::generator::generate_test_candidates;
use crate::testgen::runner::TestOutcome;
use crate::testgen::test_suite::{run_full_test_suite, write_test_suite_report};


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

        cmd if cmd.starts_with("model use ") => model_use(state, cmd),
        "model show" => model_show(state),

        "run-tests" | "test-suite" => {
            let repo_root = match std::env::current_dir() {
                Ok(p) => p,
                Err(e) => {
                    log(state, LogLevel::Error, format!("Failed to get repo root: {e}"));
                    return;
                }
            };

            match run_full_test_suite(state, repo_root) {
                Ok(_) => {
                    log(state, LogLevel::Info, "Test suite started".to_string());
                }
                Err(e) => {
                    log(state, LogLevel::Error, format!("Failed to start test suite: {e}"));
                }
            }
        },

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
        _ => log(state, LogLevel::Warn, "Unknown command. Type `help`."),
    }
}

fn help(state: &mut AgentState) {
    use LogLevel::Info;

    log(state, Info, "Commands:");
    log(state, Info, "  inspect                     — analyze git changes and build context");
    log(state, Info, "  changes                     — list analyzed changes");
    log(state, Info, "  changes <n>                  — view diff for change <n>");

    log(state, Info, "  model use ollama <model>     — use local Ollama model");
    log(state, Info, "  model use openai <model>     — configure OpenAI model (prompts for key)");
    log(state, Info, "  model use anthropic <model>  — configure Anthropic model (prompts for key)");
    log(state, Info, "  model show                  — show active model");

    log(state, Info, "  agent run <n>                — generate and run tests for change <n>");
    log(state, Info, "  agent status                 — show agent execution state");
    log(state, Info, "  agent cancel                 — cancel running agent");
    log(state, Info, "  run-tests | test-suite        — run full test suite");
    log(state, Info, "  artifacts test               — show last generated test");

    log(state, Info, "  branch new                   — create agent branch");
    log(state, Info, "  branch rollback              — rollback agent branch");
    log(state, Info, "  branch list                  — list git branches");

    log(state, Info, "  clear | logs clear            — clear logs");
    log(state, Info, "  close                        — close active view");
    log(state, Info, "  quit                         — exit osmogrep");
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

    let entries: Vec<String> = state
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

    for line in entries {
        log(state, LogLevel::Info, line);
    }
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

    let diff = state.context.diff_analysis[idx].clone();
    let candidates = generate_test_candidates(std::slice::from_ref(&diff));

    if candidates.is_empty() {
        log(state, LogLevel::Warn, "No test candidates generated.");
        return;
    }

    state.context.test_candidates = candidates;
    state.lifecycle.phase = Phase::ExecuteAgent;
}

fn agent_status(state: &mut AgentState) {
    log(state, LogLevel::Info, format!("Phase: {:?}", state.lifecycle.phase));
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

fn model_use(state: &mut AgentState, cmd: &str) {
    let args: Vec<&str> = cmd.split_whitespace().collect();

    match args.as_slice() {
        ["model", "use", "ollama", model] => {
            state.llm_backend = LlmBackend::ollama(model.to_string());

            state.ui.input.clear();
            state.ui.input_mode = InputMode::Command;
            state.ui.input_masked = false;
            state.ui.input_placeholder = None;

            log(
                state,
                LogLevel::Success,
                format!("Using Ollama model: {}", model),
            );
        }

        ["model", "use", "openai", model] => {
            enter_api_key_mode(state, Provider::OpenAI, model);
            log(
                state,
                LogLevel::Info,
                format!("Enter OpenAI API key for model {}", model),
            );
        }

        ["model", "use", "anthropic", model] => {
            enter_api_key_mode(state, Provider::Anthropic, model);
            log(
                state,
                LogLevel::Info,
                format!("Enter Anthropic API key for model {}", model),
            );
        }

        _ => {
            log(
                state,
                LogLevel::Warn,
                "Usage: model use ollama <model> | model use <openai|anthropic> <model>",
            );
        }
    }
}

fn enter_api_key_mode(
    state: &mut AgentState,
    provider: Provider,
    model: &str,
) {
    state.ui.input.clear();
    state.ui.input_mode = InputMode::ApiKey {
        provider,
        model: model.to_string(),
    };
    state.ui.input_masked = true;
    state.ui.input_placeholder = Some("Add API key".into());
    state.ui.focus = Focus::Input;
}

fn model_show(state: &mut AgentState) {
    match &state.llm_backend {
        LlmBackend::Ollama { model } => {
            log(state, LogLevel::Info, format!("Active model: Ollama ({})", model));
        }
        LlmBackend::Remote { client } => {
            let cfg = client.current_config();
            log(
                state,
                LogLevel::Info,
                format!("Active model: Remote ({:?} / {})", cfg.provider, cfg.model),
            );
        }
    }
}

fn show_test_artifact(state: &mut AgentState) {
    let Some(code) = state.context.last_generated_test.clone() else {
        log(state, LogLevel::Warn, "No generated test available.");
        return;
    };

    let Some(candidate) = state.context.test_candidates.first().cloned() else {
        return;
    };

    state.ui.panel_view = Some(SinglePanelView::TestGenPreview {
        candidate,
        generated_test: Some(code),
    });

    state.ui.focus = Focus::Execution;
    state.ui.exec_scroll = 0;
    state.ui.panel_scroll_x = 0
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
        state.ui.panel_scroll_x = 0;
        closed = true;
    }

    if closed {
        state.ui.focus = Focus::Input;
        log(state, LogLevel::Info, "Closed view.");
    } else {
        log(state, LogLevel::Warn, "No active view to close.");
    }
}

fn clear_logs(state: &mut AgentState) {
    state.logs.clear();
    state.ui.exec_scroll = 0;
    state.ui.panel_scroll_x = 0;
    state.ui.dirty = true;
}

fn reset_diff_view(state: &mut AgentState) {
    state.ui.in_diff_view = false;
    state.ui.selected_diff = None;
    state.ui.diff_scroll = 0;
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

    hint!("inspect", "Analyze git changes and build context");
    hint!("changes", "List analyzed changes");
    hint!("changes ", "View diff for change <n>");
    hint!("model use ollama ", "Use local Ollama model");
    hint!("model use openai ", "Configure OpenAI model (will prompt for API key)");
    hint!(
        "model use anthropic ",
        "Configure Anthropic model (will prompt for API key)"
    );
    hint!("model show", "Show active model");
    hint!("agent run ", "Generate and run tests for change <n>");
    hint!("agent status", "Show agent execution state");
    hint!("agent cancel", "Cancel running agent");
    hint!("artifacts test", "Show last generated test");
    hint!("run-tests", "Run full test suite");
    hint!("test-suite", "Run full test suite");
    hint!("branch new", "Create agent branch");
    hint!("branch rollback", "Rollback agent branch");
    hint!("branch list", "List git branches");
    hint!("clear", "Clear logs");
    hint!("logs clear", "Clear logs");
    hint!("close", "Close active view");
    hint!("quit", "Exit Osmogrep");
    hint!("help", "Show help");

    state.set_hint("Unknown command");
}
