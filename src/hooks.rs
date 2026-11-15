use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    hooks: Option<Hooks>,
}

#[derive(Debug, Deserialize)]
struct Hooks {
    pre_edit: Option<String>,
    post_edit: Option<String>,
    pre_shell: Option<String>,
}

fn config_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("osmogrep");
    dir.push("config.toml");
    dir
}

fn load_hooks() -> Option<Hooks> {
    let raw = fs::read_to_string(config_path()).ok()?;
    let cfg: Config = toml::from_str(&raw).ok()?;
    cfg.hooks
}

pub fn run_hook(name: &str, vars: &[(&str, &str)]) -> Result<Option<String>, String> {
    run_hook_cancellable(name, vars, &crate::tools::repo_root()?, &|| false)
}

pub fn run_hook_cancellable(
    name: &str,
    vars: &[(&str, &str)],
    root: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<String>, String> {
    if cancel() {
        return Err("cancelled".into());
    }
    let hooks = match load_hooks() {
        Some(h) => h,
        None => return Ok(None),
    };

    let template = match name {
        "pre_edit" => hooks.pre_edit,
        "post_edit" => hooks.post_edit,
        "pre_shell" => hooks.pre_shell,
        _ => None,
    };

    let Some(template) = template else {
        return Ok(None);
    };

    let cmd = expand_template(&template, vars);
    let mut command = Command::new("sh");
    command.args(["-lc", &cmd]).current_dir(root);
    let out = crate::process_runner::run_command_cancellable(
        command,
        crate::process_runner::timeout_from_env("OSMOGREP_HOOK_TIMEOUT_SECS", 15),
        cancel,
    )?;
    if out.cancelled || out.timed_out {
        return Err("Hook cancelled or timed out; action stopped".into());
    }

    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&out.stdout));
    if !out.stderr.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&out.stderr));
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        Ok(Some(format!("hook={} exit={}", name, out.exit_code)))
    } else {
        Ok(Some(format!(
            "hook={} exit={} output={}",
            name,
            out.exit_code,
            trim_one_line(trimmed)
        )))
    }
}

fn expand_template(template: &str, vars: &[(&str, &str)]) -> String {
    let mut map = HashMap::new();
    for (k, v) in vars {
        map.insert(*k, *v);
    }

    let mut out = template.to_string();
    for (k, v) in map {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

fn trim_one_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.chars().count() > 180 {
        let mut x: String = line.chars().take(180).collect();
        x.push_str("...");
        x
    } else {
        line.to_string()
    }
}
