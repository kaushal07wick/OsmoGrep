use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct Config {
    mcp: Option<McpConfig>,
}

#[derive(Debug, Deserialize)]
struct McpConfig {
    enabled: Option<bool>,
    default_server: Option<String>,
    servers: Option<HashMap<String, McpServerConfig>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub cmd: String,
    pub timeout_ms: Option<u64>,
}

fn config_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("osmogrep");
    dir.push("config.toml");
    dir
}

fn load_config() -> Option<McpConfig> {
    let raw = fs::read_to_string(config_path()).ok()?;
    let cfg: Config = toml::from_str(&raw).ok()?;
    cfg.mcp
}

pub fn is_enabled() -> bool {
    load_config().and_then(|c| c.enabled).unwrap_or(false)
}

pub fn list_servers() -> Vec<String> {
    load_config()
        .and_then(|c| c.servers)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn call(server: Option<&str>, method: &str, args: &Value) -> Result<Value, String> {
    call_with_timeout(server, method, args, Duration::from_secs(120))
}

pub fn call_with_timeout(
    server: Option<&str>,
    method: &str,
    args: &Value,
    limit: Duration,
) -> Result<Value, String> {
    let cfg = load_config().ok_or("MCP config not found")?;
    if !cfg.enabled.unwrap_or(false) {
        return Err("MCP is disabled. Set [mcp].enabled = true".to_string());
    }

    let servers = cfg.servers.ok_or("No MCP servers configured")?;
    let target = match server {
        Some(s) => s.to_string(),
        None => cfg
            .default_server
            .ok_or("No server passed and no [mcp].default_server configured")?,
    };

    let scfg = servers
        .get(&target)
        .ok_or_else(|| format!("Unknown MCP server: {}", target))?
        .clone();

    let timeout =
        Duration::from_millis(scfg.timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000)).min(limit);
    if timeout.is_zero() {
        return Err("MCP execution deadline exceeded".into());
    }

    // Scaffold protocol: run configured command with method/args available as env vars.
    // This keeps integration simple while allowing external MCP bridges.
    let mut cmd = Command::new("sh");
    cmd.arg("-lc")
        .arg(&scfg.cmd)
        .env("OSMOGREP_MCP_SERVER", &target)
        .env("OSMOGREP_MCP_METHOD", method)
        .env(
            "OSMOGREP_MCP_ARGS",
            serde_json::to_string(args).unwrap_or_else(|_| "{}".into()),
        );

    let out = run_bridge(cmd, timeout)?;
    Ok(serde_json::json!({
        "server": target,
        "method": method,
        "exit_code": out.exit_code,
        "stdout": String::from_utf8_lossy(&out.stdout),
        "stderr": truncate(&String::from_utf8_lossy(&out.stderr), 4000),
    }))
}

fn run_bridge(
    cmd: Command,
    timeout: Duration,
) -> Result<crate::process_runner::ProcessRun, String> {
    let out = crate::process_runner::run_command(cmd, timeout)?;
    if out.timed_out || out.cancelled {
        return Err("MCP bridge timed out or was cancelled".into());
    }
    if out.stdout.len() > 4 * 1024 * 1024 {
        return Err("MCP bridge response exceeds 4 MiB".into());
    }
    Ok(out)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let head: String = s.chars().take(n).collect();
        format!("{}\n...truncated...", head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_captures_machine_readable_stdout() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf '{\"session\":\"vm-test\"}'"]);
        let result = run_bridge(cmd, Duration::from_secs(2)).unwrap();
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["session"], "vm-test");
    }
}
