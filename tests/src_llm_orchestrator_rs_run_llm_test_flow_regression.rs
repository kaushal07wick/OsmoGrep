
// OSMOGREP TEST
### Regression Test for `run_llm_test_flow`

#### File: src/llm/orchestrator.rs
Symbol: run_llm_test_flow
Language: Rust
Test Framework: CargoTest
Test Type: Regression
Risk: Medium
Decision: Yes
Target: Function `run_llm_test_flow`

#### INTENT
Behavior to preserve:
Errors are raised and handled predictably

Failure mode if incorrect:
Unhandled errors may surface at runtime

#### OLD CODE
```
pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    language: Language,
    framework: Option<TestFramework>,
    candidate: TestCandidate,
) {
    thread::spawn(move || {
        let started = Instant::now();

        // ---------- helpers ----------
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let cancel_and_exit = |reason: &str| {
            let _ = tx.send(AgentEvent::Log(
                LogLevel::Warn,
                format!("🤖 {}", reason),
            ));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed("Execution cancelled".into()));
        };

        // ---------- start ----------
        let _ = tx.send(AgentEvent::SpinnerStart(
            "🤖 AI generating tests…".into(),
        ));

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "🤖 Agent execution started".into(),
        ));

        if cancelled() {
            cancel_and_exit("Cancellation detected before execution");
            return;
        }

        // ---------- resolve ----------
        let resolution = resolve_test(&language, &candidate);

        if cancelled() {
            cancel_and_exit("Cancelled during test resolution");
            return;
        }

        // ---------- prompt ----------
        let prompt = build_prompt(
            &candidate,
            &resolution,
            Some(&language),
            framework.as_ref(),
        );

        if cancelled() {
            cancel_and_exit("Cancelled before LLM invocation");
            return;
        }

        // ---------- LLM ----------
        let code = match Ollama::run(prompt) {
            Ok(code) => code,
            Err(e) => {
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        if cancelled() {
            cancel_and_exit("Cancelled after LLM response");
            return;
        }

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            "🤖 AI returned generated test".into(),
        ));

        // ---------- materialize ----------
        if let Err(e) =
            materialize_test(language, &candidate, &resolution, &code)
        {
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed(e.to_string()));
            return;
        }

        let _ = tx.send(AgentEvent::GeneratedTest(code));

        // ---------- finish ----------
        let elapsed = started.elapsed().as_secs_f32();

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!("🤖 Finished in {:.2}s", elapsed),
        ));

        let _ = tx.send(AgentEvent::SpinnerStop);
        let _ = tx.send(AgentEvent::Finished);
    });
}
```

#### NEW CODE
```
pub fn run_llm_test_flow(
    tx: Sender<AgentEvent>,
    cancel_flag: Arc<AtomicBool>,
    language: Language,
    framework: Option<TestFramework>,
    candidate: TestCandidate,
) {
    thread::spawn(move || {
        let started = Instant::now();

        // ---------- helpers ----------
        let cancelled = || cancel_flag.load(Ordering::SeqCst);

        let cancel_and_exit = |reason: &str| {
            let _ = tx.send(AgentEvent::Log(
                LogLevel::Warn,
                format!("🤖 {}", reason),
            ));
            let _ = tx.send(AgentEvent::SpinnerStop);
            let _ = tx.send(AgentEvent::Failed("Execution cancelled".into()));
        };

        // ---------- start ----------
        let _ = tx.send(AgentEvent::SpinnerStart(
            "🤖 AI generating tests…".into(),
        ));

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            "🤖 Agent execution started".into(),
        ));

        if cancelled() {
            cancel_and_exit("Cancellation detected before execution");
            return;
        }

        // ---------- resolve ----------
        let resolution = resolve_test(&language, &candidate);

        if cancelled() {
            cancel_and_exit("Cancelled during test resolution");
            return;
        }

        // ---------- prompt ----------
        let prompt = build_prompt(
            &candidate,
            &resolution,
            Some(&language),
            framework.as_ref(),
        );

        if cancelled() {
            cancel_and_exit("Cancelled before LLM invocation");
            return;
        }

        // ---------- LLM ----------
        let code = match Ollama::run(prompt) {
            Ok(code) => code,
            Err(e) => {
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        if cancelled() {
            cancel_and_exit("Cancelled after LLM response");
            return;
        }

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            "🤖 AI returned generated test".into(),
        ));

        // ---------- materialize ----------
        let path = match materialize_test(language, &candidate, &resolution, &code) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(AgentEvent::SpinnerStop);
                let _ = tx.send(AgentEvent::Failed(e.to_string()));
                return;
            }
        };

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Success,
            format!("🤖 Test written to {}", path.display()),
        ));

        let _ = tx.send(AgentEvent::GeneratedTest(code));

        // ---------- finish ----------
        let elapsed = started.elapsed().as_secs_f32();

        let _ = tx.send(AgentEvent::Log(
            LogLevel::Info,
            format!("🤖 Finished in {:.2}s", elapsed),
        ));

        let _ = tx.send(AgentEvent::SpinnerStop);
        let _ = tx.send(AgentEvent::Finished);
    });
}
```

#### TEST RESOLUTION
- No existing test was found.
- Create a new regression test.

#### RUST TESTING RULES
- Prefer #[cfg(test)] mod tests in the same file
- Only create files under tests/ if clearly appropriate

#### CONSTRAINTS
- Cover core behavior
- Avoid over-testing
- Assert previously working behavior still holds

#### OUTPUT REQUIREMENTS
- Output ONLY valid test code
- No explanations
- No markdown
- No comments outside the test code
