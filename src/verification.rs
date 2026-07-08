use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const OUTPUT_SUMMARY_LIMIT: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationEvidence {
    pub id: String,
    pub created_at: String,
    pub command: String,
    pub canonical_command: String,
    pub kind: String,
    pub scope: String,
    pub status: String,
    pub exit_code: i32,
    pub root: String,
    pub output_summary: String,
}

pub fn record_command(
    repo_root: &Path,
    command: &str,
    exit_code: i32,
    output: &str,
) -> Option<VerificationEvidence> {
    let evidence = classify_command(repo_root, command, exit_code, output)?;
    append(repo_root, &evidence).ok()?;
    Some(evidence)
}

pub fn classify_command(
    repo_root: &Path,
    command: &str,
    exit_code: i32,
    output: &str,
) -> Option<VerificationEvidence> {
    let (canonical_command, kind, args) = detect_verification_command(command)?;
    let scope = if args.iter().any(|arg| looks_like_target(arg)) {
        "targeted"
    } else {
        "full"
    };

    Some(VerificationEvidence {
        id: Uuid::new_v4().to_string(),
        created_at: Utc::now().to_rfc3339(),
        command: command.trim().to_string(),
        canonical_command,
        kind,
        scope: scope.to_string(),
        status: if exit_code == 0 { "passed" } else { "failed" }.to_string(),
        exit_code,
        root: repo_root.display().to_string(),
        output_summary: summarize_output(output),
    })
}

fn append(repo_root: &Path, evidence: &VerificationEvidence) -> Result<(), String> {
    let path = ledger_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let value = serde_json::to_value(evidence).map_err(|e| e.to_string())?;
    writeln!(file, "{}", value).map_err(|e| e.to_string())
}

fn ledger_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".context")
        .join("osmogrep-verification.jsonl")
}

fn detect_verification_command(command: &str) -> Option<(String, String, Vec<String>)> {
    for segment in command_segments(command) {
        let tokens = strip_prefix_tokens(tokenize_segment(&segment));
        if tokens.is_empty() {
            continue;
        }
        if let Some(found) = detect_tokens(&tokens) {
            return Some(found);
        }
    }
    None
}

fn command_segments(command: &str) -> Vec<String> {
    command
        .replace("&&", "\n")
        .replace("||", "\n")
        .replace(';', "\n")
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn tokenize_segment(segment: &str) -> Vec<String> {
    segment
        .split_whitespace()
        .map(|s| {
            s.trim_matches('"')
                .trim_matches('\'')
                .trim_matches('`')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn strip_prefix_tokens(tokens: Vec<String>) -> Vec<String> {
    let mut idx = 0usize;
    if tokens.get(idx).map(String::as_str) == Some("env") {
        idx += 1;
    }
    while tokens
        .get(idx)
        .map(|token| token.contains('=') && !token.starts_with('-'))
        .unwrap_or(false)
    {
        idx += 1;
    }
    while matches!(
        tokens.get(idx).map(String::as_str),
        Some("command" | "time" | "noglob")
    ) {
        idx += 1;
    }
    tokens.into_iter().skip(idx).collect()
}

fn detect_tokens(tokens: &[String]) -> Option<(String, String, Vec<String>)> {
    let first = tokens.first()?.as_str();
    match first {
        "cargo" => detect_cargo(tokens),
        "go" if tokens.get(1).map(String::as_str) == Some("test") => Some((
            "go test".to_string(),
            "test".to_string(),
            tokens[2..].to_vec(),
        )),
        "pytest" => Some((
            "pytest".to_string(),
            "test".to_string(),
            tokens[1..].to_vec(),
        )),
        "python" | "python3"
            if tokens.get(1).map(String::as_str) == Some("-m")
                && tokens.get(2).map(String::as_str) == Some("pytest") =>
        {
            Some((
                "pytest".to_string(),
                "test".to_string(),
                tokens[3..].to_vec(),
            ))
        }
        "uv" | "poetry" | "pipenv"
            if tokens.get(1).map(String::as_str) == Some("run")
                && tokens.get(2).map(String::as_str) == Some("pytest") =>
        {
            Some((
                "pytest".to_string(),
                "test".to_string(),
                tokens[3..].to_vec(),
            ))
        }
        "npm" | "pnpm" | "yarn" | "bun" => detect_package_manager(tokens),
        "make" => detect_make(tokens),
        _ => None,
    }
}

fn detect_cargo(tokens: &[String]) -> Option<(String, String, Vec<String>)> {
    match tokens.get(1).map(String::as_str)? {
        "test" => Some((
            "cargo test".to_string(),
            "test".to_string(),
            tokens[2..].to_vec(),
        )),
        "check" => Some((
            "cargo check".to_string(),
            "check".to_string(),
            tokens[2..].to_vec(),
        )),
        "build" => Some((
            "cargo build".to_string(),
            "build".to_string(),
            tokens[2..].to_vec(),
        )),
        "clippy" => Some((
            "cargo clippy".to_string(),
            "lint".to_string(),
            tokens[2..].to_vec(),
        )),
        "fmt" => Some((
            "cargo fmt".to_string(),
            "format".to_string(),
            tokens[2..].to_vec(),
        )),
        "nextest" if tokens.get(2).map(String::as_str) == Some("run") => Some((
            "cargo nextest run".to_string(),
            "test".to_string(),
            tokens[3..].to_vec(),
        )),
        _ => None,
    }
}

fn detect_package_manager(tokens: &[String]) -> Option<(String, String, Vec<String>)> {
    let pm = tokens.first()?.as_str();
    let sub = tokens.get(1)?.as_str();
    if sub == "test" {
        return Some((
            format!("{pm} test"),
            "test".to_string(),
            tokens[2..].to_vec(),
        ));
    }
    if sub != "run" {
        return None;
    }

    let script = tokens.get(2)?;
    let kind = script_kind(script)?;
    Some((format!("{pm} run {script}"), kind, tokens[3..].to_vec()))
}

fn detect_make(tokens: &[String]) -> Option<(String, String, Vec<String>)> {
    let target = tokens.get(1)?;
    let kind = script_kind(target)?;
    Some((format!("make {target}"), kind, tokens[2..].to_vec()))
}

fn script_kind(script: &str) -> Option<String> {
    let lower = script.to_ascii_lowercase();
    if lower.contains("lint") || lower.contains("eslint") || lower.contains("ruff") {
        return Some("lint".to_string());
    }
    if lower.contains("typecheck") || lower.contains("tsc") || lower.contains("mypy") {
        return Some("typecheck".to_string());
    }
    if lower.contains("build") {
        return Some("build".to_string());
    }
    if lower.contains("fmt") || lower.contains("format") {
        return Some("format".to_string());
    }
    if lower.contains("check") && !lower.contains("test") {
        return Some("check".to_string());
    }
    if lower.contains("test") {
        return Some("test".to_string());
    }
    None
}

fn looks_like_target(arg: &str) -> bool {
    let arg = arg.trim();
    if arg.is_empty() || arg.starts_with('-') || arg.contains('=') {
        return false;
    }
    arg.contains('/')
        || arg.contains('\\')
        || arg.contains("::")
        || arg.ends_with(".rs")
        || arg.ends_with(".py")
        || arg.ends_with(".js")
        || arg.ends_with(".jsx")
        || arg.ends_with(".ts")
        || arg.ends_with(".tsx")
        || arg.ends_with(".go")
        || arg.starts_with("test_")
        || arg.starts_with("tests")
        || arg.starts_with("spec")
        || arg.starts_with("__tests__")
}

fn summarize_output(output: &str) -> String {
    let text = output.trim();
    if text.chars().count() <= OUTPUT_SUMMARY_LIMIT {
        return text.to_string();
    }

    let head_len = OUTPUT_SUMMARY_LIMIT / 3;
    let tail_len = OUTPUT_SUMMARY_LIMIT - head_len;
    let head: String = text.chars().take(head_len).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(tail_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!(
        "{head}\n... [{} chars omitted] ...\n{tail}",
        text.chars().count().saturating_sub(OUTPUT_SUMMARY_LIMIT)
    )
}

pub fn to_json(evidence: &Option<VerificationEvidence>) -> serde_json::Value {
    evidence
        .as_ref()
        .map(|e| serde_json::to_value(e).unwrap_or_else(|_| json!(null)))
        .unwrap_or_else(|| json!(null))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_cargo_test_as_full_verification() {
        let ev = classify_command(Path::new("."), "cargo test --color never", 0, "ok").unwrap();
        assert_eq!(ev.canonical_command, "cargo test");
        assert_eq!(ev.kind, "test");
        assert_eq!(ev.scope, "full");
        assert_eq!(ev.status, "passed");
    }

    #[test]
    fn classifies_targeted_pytest() {
        let ev = classify_command(
            Path::new("."),
            "python -m pytest tests/test_api.py",
            1,
            "no",
        )
        .unwrap();
        assert_eq!(ev.canonical_command, "pytest");
        assert_eq!(ev.scope, "targeted");
        assert_eq!(ev.status, "failed");
    }

    #[test]
    fn ignores_non_verification_commands() {
        assert!(classify_command(Path::new("."), "ls -la", 0, "").is_none());
    }
}
