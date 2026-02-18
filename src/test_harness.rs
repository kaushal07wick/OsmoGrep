use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use regex::Regex;
use serde::Serialize;

const OUTPUT_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct TestRun {
    pub framework: String,
    pub command: String,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub success: bool,
    pub passed: usize,
    pub failed: usize,
    pub output: String,
}

pub fn run_tests(repo_root: &Path, target: Option<&str>) -> Result<TestRun, String> {
    let (framework, command) = detect_framework_and_command(repo_root, target)?;

    let started = Instant::now();
    let out = Command::new("sh")
        .arg("-lc")
        .arg(&command)
        .current_dir(repo_root)
        .output()
        .map_err(|e| e.to_string())?;
    let duration_ms = started.elapsed().as_millis();

    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&out.stdout));
    if !out.stderr.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&out.stderr));
    }

    let output = truncate_output(&text);
    let exit_code = out.status.code().unwrap_or(-1);
    let (passed, failed) = parse_counts(&framework, &text);

    Ok(TestRun {
        framework,
        command,
        exit_code,
        duration_ms,
        success: out.status.success(),
        passed,
        failed,
        output,
    })
}

fn detect_framework_and_command(repo_root: &Path, target: Option<&str>) -> Result<(String, String), String> {
    let target = target.unwrap_or("").trim();

    let cargo = repo_root.join("Cargo.toml").exists();
    let go = repo_root.join("go.mod").exists();
    let py = repo_root.join("pyproject.toml").exists()
        || repo_root.join("pytest.ini").exists()
        || repo_root.join("tests").exists();
    let pkg = repo_root.join("package.json");
    let js = if pkg.exists() {
        fs::read_to_string(pkg)
            .ok()
            .map(|s| s.contains("jest") || s.contains("vitest") || s.contains("\"test\""))
            .unwrap_or(false)
    } else {
        false
    };

    let (framework, mut cmd) = if cargo {
        ("cargo", "cargo test --color never".to_string())
    } else if py {
        ("pytest", "pytest -q".to_string())
    } else if js {
        ("jest", "npm test -- --runInBand".to_string())
    } else if go {
        ("go", "go test ./...".to_string())
    } else {
        return Err("No supported test framework detected (cargo/pytest/jest/go).".to_string());
    };

    if !target.is_empty() {
        cmd.push(' ');
        cmd.push_str(target);
    }

    Ok((framework.to_string(), cmd))
}

fn parse_counts(framework: &str, output: &str) -> (usize, usize) {
    match framework {
        "cargo" => parse_cargo_counts(output),
        "pytest" => parse_pytest_counts(output),
        "jest" => parse_jest_counts(output),
        "go" => parse_go_counts(output),
        _ => (0, 0),
    }
}

fn parse_cargo_counts(output: &str) -> (usize, usize) {
    let re = Regex::new(r"test result:\s+\w+\.\s+(\d+) passed;\s+(\d+) failed;").unwrap();
    if let Some(c) = re.captures_iter(output).last() {
        let p = c.get(1).and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(0);
        let f = c.get(2).and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(0);
        return (p, f);
    }
    (0, 0)
}

fn parse_pytest_counts(output: &str) -> (usize, usize) {
    let pass = Regex::new(r"(\d+)\s+passed").unwrap();
    let fail = Regex::new(r"(\d+)\s+failed").unwrap();

    let p = pass
        .captures_iter(output)
        .last()
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(0);
    let f = fail
        .captures_iter(output)
        .last()
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(0);

    (p, f)
}

fn parse_jest_counts(output: &str) -> (usize, usize) {
    let pass = Regex::new(r"(\d+)\s+passed").unwrap();
    let fail = Regex::new(r"(\d+)\s+failed").unwrap();

    let p = pass
        .captures_iter(output)
        .last()
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(0);
    let f = fail
        .captures_iter(output)
        .last()
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(0);

    (p, f)
}

fn parse_go_counts(output: &str) -> (usize, usize) {
    let failed = output.lines().filter(|l| l.starts_with("--- FAIL:")).count();
    let passed = output.lines().filter(|l| l.starts_with("ok\t")).count();
    (passed, failed)
}

fn truncate_output(s: &str) -> String {
    if s.chars().count() <= OUTPUT_LIMIT {
        return s.to_string();
    }

    let tail: String = s
        .chars()
        .rev()
        .take(OUTPUT_LIMIT)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    format!("...truncated...\n{}", tail)
}
