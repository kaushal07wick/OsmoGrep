use crate::state::{AgentState, LogLevel};
use crate::logger::log;
use crate::git::{create_agent_branch, git_create_sandbox_worktree};
use crate::llm::prompt::LlmPrompt;
use crate::detectors::language::Language;
use serde_json::Value;

use std::path::{Path, PathBuf};
use std::process::Command;

// -----------------------------------------------------------------------------
// BRANCH SETUP
// -----------------------------------------------------------------------------
fn ensure_agent_branch(state: &mut AgentState) -> bool {
    log(state, LogLevel::Info, "ensuring agent branch exists");

    let name = create_agent_branch();
    if name.trim().is_empty() {
        log(state, LogLevel::Error, "failed to create agent branch");
        return false;
    }

    log(state, LogLevel::Info, format!("agent branch: {}", name));
    true
}

// -----------------------------------------------------------------------------
// SANDBOX
// -----------------------------------------------------------------------------
fn create_sandbox(state: &mut AgentState) -> Option<PathBuf> {
    log(state, LogLevel::Info, "create sandbox");
    let p = git_create_sandbox_worktree();

    if p.exists() {
        log(state, LogLevel::Info, format!("sandbox {}", p.display()));
        return Some(p);
    }

    log(state, LogLevel::Error, "sandbox path does not exist");
    None
}

// -----------------------------------------------------------------------------
// ENV SETUP
// -----------------------------------------------------------------------------
fn ensure_env(state: &mut AgentState, root: &Path) -> bool {
    let venv = root.join(".venv");
    log(state, LogLevel::Info, "ensure venv");

    if venv.exists() {
        log(state, LogLevel::Info, "using existing venv");
        return true;
    }

    let out = Command::new("python3")
        .arg("-m")
        .arg("venv")
        .arg(&venv)
        .current_dir(root)
        .output();

    match out {
        Ok(o) if o.status.success() => {
            log(state, LogLevel::Info, "venv created");
            true
        }
        Ok(o) => {
            log(state, LogLevel::Error, String::from_utf8_lossy(&o.stderr));
            false
        }
        Err(e) => {
            log(state, LogLevel::Error, e.to_string());
            false
        }
    }
}

// -----------------------------------------------------------------------------
// DEP INSTALL
// -----------------------------------------------------------------------------
fn install_requirements(state: &mut AgentState, root: &Path) -> bool {
    let pip = root.join(".venv/bin/pip");
    let req = root.join("requirements.txt");

    if !req.exists() {
        return true;
    }

    log(state, LogLevel::Info, "install requirements.txt");
    run_pip(state, &pip, root, &["install", "-r", "requirements.txt"])
}

fn install_pyproject(state: &mut AgentState, root: &Path) -> bool {
    let pip = root.join(".venv/bin/pip");
    let py = root.join("pyproject.toml");

    if !py.exists() {
        return true;
    }

    log(state, LogLevel::Info, "install pyproject package");
    run_pip(state, &pip, root, &["install", "."])
}

// -----------------------------------------------------------------------------
// README PARSING
// -----------------------------------------------------------------------------
fn parse_readme_llm(state: &mut AgentState, text: &str) -> Option<Value> {
    let prompt = LlmPrompt {
        system: r#"
You MUST return STRICT JSON ONLY. 
NO commentary. NO explanations. NO backticks. NO markdown.

Extract commands from README. 
Return EXACTLY this shape:

{
  "install": ["cmd1", "cmd2"],
  "run": ["cmd1", "cmd2"]
}
"#.into(),
        user: format!("README:\n{}", text),
    };

    let res = state.llm_backend.run(prompt, false);
    if res.is_err() {
        log(state, LogLevel::Error, "LLM README parse failed");
        return None;
    }

    let raw = res.unwrap().text;
    log(state, LogLevel::Info, format!("RAW LLM README OUTPUT:\n{}", raw));

    let json_slice_opt = sanitize_json(&raw);
    if json_slice_opt.is_none() {
        log(state, LogLevel::Error, "sanitize_json failed");
        return None;
    }

    let slice = json_slice_opt.unwrap();
    log(state, LogLevel::Info, format!("Sanitized:\n{}", slice));

    let parsed = serde_json::from_str::<Value>(slice);
    if parsed.is_err() {
        log(state, LogLevel::Error, format!("JSON parse error: {}", parsed.err().unwrap()));
        return None;
    }

    Some(parsed.unwrap())
}

fn parse_readme(state: &mut AgentState, root: &Path) -> Option<Value> {
    let path = root.join("README.md");
    if !path.exists() {
        return None;
    }

    let text_res = std::fs::read_to_string(&path);
    if text_res.is_err() {
        return None;
    }

    let text = text_res.unwrap();
    parse_readme_llm(state, &text)
}

// -----------------------------------------------------------------------------
// USAGE EXECUTION
// -----------------------------------------------------------------------------
fn run_usage_cmds(state: &mut AgentState, root: &Path, parsed: &Value) -> Option<String> {
    let arr = parsed["run"].as_array();
    if arr.is_none() {
        return None;
    }
    let runs = arr.unwrap();

    for item in runs {
        if let Some(cmd) = item.as_str() {
            let full = format!("PYTHONPATH=\"{}\" {}", root.display(), cmd);
            log(state, LogLevel::Info, format!("run {}", full));

            let out = Command::new("bash")
                .arg("-c")
                .arg(&full)
                .current_dir(root)
                .output();

            match out {
                Ok(o) if o.status.success() => return None,
                Ok(o) => return Some(String::from_utf8_lossy(&o.stderr).to_string()),
                Err(e) => return Some(e.to_string()),
            }
        }
    }
    None
}

// -----------------------------------------------------------------------------
// ERROR FIXING STAGE
// -----------------------------------------------------------------------------
fn parse_error(state: &mut AgentState, stderr: &str) -> Option<Value> {
    let prompt = LlmPrompt {
        system: r#"
Return STRICT JSON ONLY for:

{
  "pip_package": null,
  "missing_import": null,
  "diff": ""
}

NO commentary. NO markdown. NO explanation.
"#.into(),
        user: stderr.to_string(),
    };

    let res = state.llm_backend.run(prompt, false);
    if res.is_err() {
        log(state, LogLevel::Error, "LLM error parse failed");
        return None;
    }
    let raw = res.unwrap().text;

    let cleaned_opt = sanitize_json(&raw);
    if cleaned_opt.is_none() {
        log(state, LogLevel::Error, "sanitize_json failed (error LLM)");
        return None;
    }

    let slice = cleaned_opt.unwrap();
    let parsed = serde_json::from_str::<Value>(slice);
    if parsed.is_err() {
        log(state, LogLevel::Error, "JSON parse failed (error fix)");
        return None;
    }
    Some(parsed.unwrap())
}

fn attempt_fix(state: &mut AgentState, root: &Path, stderr: &str) -> bool {
    log(state, LogLevel::Warn, "attempting fix");

    let parsed_opt = parse_error(state, stderr);
    if parsed_opt.is_none() {
        log(state, LogLevel::Error, "LLM error parse failed");
        return false;
    }
    let parsed = parsed_opt.unwrap();

    if let Some(pkg) = parsed["pip_package"].as_str() {
        if !pkg.trim().is_empty() {
            log(state, LogLevel::Info, format!("install missing pip package {}", pkg));
            let pip = root.join(".venv/bin/pip");
            return run_pip(state, &pip, root, &["install", pkg]);
        }
    }

    if let Some(imp) = parsed["missing_import"].as_str() {
        if !imp.trim().is_empty() {
            log(state, LogLevel::Info, format!("add missing import {}", imp));
            return add_import(root, imp);
        }
    }

    if let Some(diff) = parsed["diff"].as_str() {
        if !diff.trim().is_empty() {
            log(state, LogLevel::Info, "apply diff patch");
            return apply_patch(root, diff);
        }
    }

    log(state, LogLevel::Warn, "no actionable fix found");
    false
}

// -----------------------------------------------------------------------------
// PATCH & IMPORT FIXERS
// -----------------------------------------------------------------------------
fn add_import(root: &Path, imp: &str) -> bool {
    let dir_res = std::fs::read_dir(root);
    if dir_res.is_err() {
        return false;
    }

    for entry in dir_res.unwrap() {
        if entry.is_err() {
            continue;
        }
        let path = entry.unwrap().path();

        let is_py = match path.extension() {
            Some(e) => e == "py",
            None => false,
        };

        if !is_py {
            continue;
        }

        let content_res = std::fs::read_to_string(&path);
        if content_res.is_err() {
            return false;
        }

        let content = content_res.unwrap();
        if content.contains(imp) {
            return true;
        }

        let new = format!("import {}\n{}", imp, content);
        return std::fs::write(&path, new).is_ok();
    }

    false
}

fn apply_patch(root: &Path, diff: &str) -> bool {
    let tmp = root.join(".patch.diff");

    if std::fs::write(&tmp, diff).is_err() {
        return false;
    }

    let out = Command::new("patch")
        .arg("-p1")
        .arg("-i")
        .arg(&tmp)
        .current_dir(root)
        .output();

    matches!(out, Ok(o) if o.status.success())
}

// -----------------------------------------------------------------------------
// JSON SANITIZER
// -----------------------------------------------------------------------------
fn sanitize_json(raw: &str) -> Option<&str> {
    let s = raw.find('{')?;
    let e = raw.rfind('}')?;
    Some(&raw[s..=e])
}

// -----------------------------------------------------------------------------
// PIP + SHELL
// -----------------------------------------------------------------------------
fn run_pip(state: &mut AgentState, pip: &Path, root: &Path, args: &[&str]) -> bool {
    let out = Command::new(pip)
        .args(args)
        .current_dir(root)
        .output();

    match out {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            log(state, LogLevel::Error, String::from_utf8_lossy(&o.stderr));
            false
        }
        Err(e) => {
            log(state, LogLevel::Error, e.to_string());
            false
        }
    }
}

// -----------------------------------------------------------------------------
// MAIN PIPELINE
// -----------------------------------------------------------------------------
pub fn main_pipeline(state: &mut AgentState) {
    log(state, LogLevel::Info, "pipeline start");

    if !ensure_agent_branch(state) {
        log(state, LogLevel::Error, "branch fail");
        return;
    }

    let sandbox = match create_sandbox(state) {
        Some(p) => p,
        None => {
            log(state, LogLevel::Error, "sandbox fail");
            return;
        }
    };

    if !ensure_env(state, &sandbox) {
        log(state, LogLevel::Error, "env fail");
        return;
    }

    if !install_requirements(state, &sandbox) {
        log(state, LogLevel::Error, "requirements fail");
        return;
    }

    if !install_pyproject(state, &sandbox) {
        log(state, LogLevel::Error, "pyproject fail");
        return;
    }

    let parsed_opt = parse_readme(state, &sandbox);

    let parsed = match parsed_opt {
        Some(v) => v,
        None => {
            log(state, LogLevel::Info, "no readme cmds");
            log(state, LogLevel::Success, "done");
            return;
        }
    };

    let mut attempts = 0;
    loop {
        let usage_result = run_usage_cmds(state, &sandbox, &parsed);

        match usage_result {
            None => {
                log(state, LogLevel::Success, "usage ok");
                log(state, LogLevel::Success, "done");
                return;
            }
            Some(err) => {
                if attempts >= 3 {
                    log(state, LogLevel::Error, "usage retry limit reached");
                    return;
                }

                if !attempt_fix(state, &sandbox, &err) {
                    log(state, LogLevel::Error, "fix failed");
                    return;
                }

                attempts += 1;
                log(state, LogLevel::Info, "retrying usage");
            }
        }
    }
}
