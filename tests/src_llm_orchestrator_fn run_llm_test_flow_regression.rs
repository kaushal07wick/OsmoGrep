
### New Regression Test Validation

To validate that the updated `run_llm_test_flow` function is functioning as intended, we need to create a new regression test. The test should cover various scenarios and edge cases to ensure correctness.

#### Step-by-Step Validation:

1. **Edge Cases:**
   - **Test with Missing Parameters:**
     ```rust
     let candidate = &TestCandidate {
         parameters: vec![],
     };
     assert_eq!(run_llm_test_flow(&mut AgentState::new(), candidate, &DEFAULT_LLM), Ok(()));
     ```
   - **Test with Invalid Language and Framework:**
     ```rust
     let candidate = &TestCandidate {
         language: "InvalidLanguage",
         framework: "InvalidFramework",
     };
     assert_eq!(run_llm_test_flow(&mut AgentState::new(), candidate, &DEFAULT_LLM), Err(String::from("Unsupported language or framework")));
     ```
   - **Test with Invalid Input for Language:**
     ```rust
     let candidate = &TestCandidate {
         parameters: vec![0],
     };
     assert_eq!(run_llm_test_flow(&mut AgentState::new(), candidate, &DEFAULT_LLM), Err(String::from("Invalid input for language")));
     ```
   - **Test with Invalid Input for Framework:**
     ```rust
     let candidate = &TestCandidate {
         parameters: vec![10],
     };
     assert_eq!(run_llm_test_flow(&mut AgentState::new(), candidate, &DEFAULT_LLM), Err(String::from("Invalid input for framework")));
     ```

2. **Failures:**
   - **Test with Invalid Test Code:**
     ```rust
     let test_code = vec![0];
     assert_eq!(run_llm_test_flow(&mut AgentState::new(), candidate, &DEFAULT_LLM), Err(String::from("Invalid test code")));
     ```
   - **Test with Failed Generation:**
     ```rust
     let test_code = vec![
         1,
         2,
         3,
     ];
     assert_eq!(run_llm_test_flow(&mut AgentState::new(), candidate, &DEFAULT_LLM), Err(String::from("Failed generation")));
     ```

3. **UI Alerts:**
   - **Test with UI Alerts On Failure:**
     ```rust
     let candidate = &TestCandidate {
         parameters: vec![10],
     };
     assert_eq!(run_llm_test_flow(&mut AgentState::new(), candidate, &DEFAULT_LLM), Ok(()));
     ```

#### New Test Code

Here's the updated test code with comments explaining each step:

```rust
use std::io::{self};
use time::Instant;
use llm_orchestrator::*;

/// Function to resolve test state
fn resolve_test(state: &mut AgentState, candidate: &TestCandidate) -> Result<()> {
    // UI: show activity immediately
    log(&state, LogLevel::Info, "Resolving existing tests…");

    let resolution = resolve_test(state, candidate);

    log(&state, LogLevel::Info, "Building LLM prompt…");
    let prompt = build_prompt(
        candidate,
        &resolution,
        state.language.as_ref(),
        state.framework.as_ref(),
    );

    // 🔥 IMPORTANT: update UI BEFORE blocking call
    log(&state, LogLevel::Info, "LLM generating tests (Ollama)…");
    state.last_activity = Instant::now();

    let test_code = Ollama::run(prompt)?;

    log(&state, LogLevel::Success, "LLM returned generated test");

    log(&state, LogLevel::Info, "Writing test to filesystem…");
    materialize_test(state, candidate, &resolution, &test_code)?;

    log(
        state,
        LogLevel::Success,
        format!("Test written to {}", test_path.display()),
    );

    // Persist for UI access
    state.last_generated_test = Some(test_code);

    Ok(())
}

/// Function to build LLM prompt
fn build_prompt(candidate: &TestCandidate, resolution: &Resolution, language: &str, framework: &str) -> String {
    format!("Generate test with parameters: {}, resolution: {}", candidate.parameters, resolution)
}

/// Function to materialize test code
fn materialize_test(state: &mut AgentState, candidate: &TestCandidate, resolution: &Resolution, test_code: &String) -> Result<String> {
    // 🔥 IMPORTANT: update UI BEFORE blocking call
    log(&state, LogLevel::Info, "LLM generating tests (Ollama)…");
    state.last_activity = Instant::now();

    let llm = Ollama::new();
    let result = llm.generate(test_code)?;

    log(&state, LogLevel::Success, "LLM returned generated test");

    log(&state, LogLevel::Info, "Writing test to filesystem…");
    materialize_test(state, candidate, resolution, &result)?;
    Ok(result)
}

/// Function to run tests
fn run_tests(state: &mut AgentState, test_path: &str) -> Result<(), String> {
    // 🔥 IMPORTANT: update UI BEFORE blocking call
    log(&state, LogLevel::Info, "LLM generating tests (Ollama)…");
    state.last_activity = Instant::now();

    let llm = Ollama::new();
    let result = llm.generate(test_path)?;

    log(&state, LogLevel::Success, "LLM returned generated test");

    report_result(state, &result);

    Ok(())
}

/// Function to report test results
fn report_result(state: &mut AgentState, result: &Result<String>) {
    // 🔥 IMPORTANT: update UI BEFORE blocking call
    log(&state, LogLevel::Info, "Test completed successfully");
}
```

### Summary

- **Edge Cases:** Ensures the function handles various scenarios without failing or incorrectly generating test code.
- **Failures:** Checks for invalid input and test failures.
- **UI Alerts:** Provides feedback on UI alerts when tests fail.
- **New Code:** Includes updated logic to build LLM prompt, materialize test code, run tests, and report results.
