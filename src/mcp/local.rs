//! Device connections exist only in this process and their originating workspace.
use super::{McpConfig, McpServerConfig};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

struct LocalServer {
    config: McpServerConfig,
    phone: Option<String>,
}
type Registry = HashMap<(PathBuf, String), LocalServer>;
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
fn root() -> Option<PathBuf> {
    crate::tools::context_root()
        .or_else(|| std::env::current_dir().ok())
        .and_then(|p| p.canonicalize().ok())
}

pub fn register_local(
    root: &Path,
    name: &str,
    command: PathBuf,
    args: Vec<String>,
    phone: Option<String>,
) -> Result<(), String> {
    registry()
        .lock()
        .map_err(|_| "Local MCP registry unavailable")?
        .insert(
            (root.canonicalize().map_err(|e| e.to_string())?, name.into()),
            LocalServer {
                config: McpServerConfig {
                    command: Some(command.to_string_lossy().into_owned()),
                    args,
                    cmd: None,
                    timeout_ms: Some(120000),
                    url: None,
                    transport: Some("stdio".into()),
                    bearer_env: None,
                },
                phone,
            },
        );
    Ok(())
}

pub(super) fn merge(config: Option<McpConfig>) -> Option<McpConfig> {
    let Some(root) = root() else { return config };
    let Ok(servers) = registry().lock() else {
        return config;
    };
    let local: Vec<_> = servers
        .iter()
        .filter(|((path, _), _)| path == &root)
        .collect();
    if local.is_empty() {
        return config;
    }
    let mut config = config.unwrap_or(McpConfig {
        enabled: Some(false),
        default_server: None,
        servers: None,
    });
    if !config.enabled.unwrap_or(false) {
        config.servers = None;
        config.default_server = None;
    }
    let configured = config.servers.get_or_insert_with(Default::default);
    for ((_, name), server) in local {
        configured.insert(name.clone(), server.config.clone());
    }
    config.enabled = Some(true);
    if config.default_server.is_none() && configured.len() == 1 {
        config.default_server = configured.keys().next().cloned();
    }
    Some(config)
}

pub(super) fn scoped_arguments(server: &str, method: &str, args: &Value) -> Result<Value, String> {
    let mut args = args.clone();
    if method != "tools/call" || args["name"] == "phone_devices" {
        return Ok(args);
    }
    let Some(root) = root() else { return Ok(args) };
    let registry = registry()
        .lock()
        .map_err(|_| "Local MCP registry unavailable")?;
    let Some(phone) = registry
        .get(&(root, server.into()))
        .and_then(|s| s.phone.as_deref())
    else {
        return Ok(args);
    };
    if args["arguments"]["phone"]
        .as_str()
        .is_some_and(|p| p != phone)
    {
        return Err(format!("This viewer is bound to phone '{phone}'. Open the requested target explicitly before controlling it."));
    }
    if args.get("arguments").is_none() {
        args["arguments"] = serde_json::json!({});
    }
    args["arguments"]
        .as_object_mut()
        .ok_or("MCP tool arguments must be an object")?
        .insert("phone".into(), Value::String(phone.into()));
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_viewers_do_not_enable_unrelated_disabled_servers_and_bind_phone_actions() {
        let root = std::env::temp_dir().join(format!("mcp-local-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        crate::tools::in_root(&root, || {
            register_local(
                &root,
                "test-phone",
                PathBuf::from("/tmp/phone"),
                vec!["mcp".into()],
                Some("selected".into()),
            )
            .unwrap();
            let configured = McpConfig {
                enabled: Some(false),
                default_server: Some("unrelated".into()),
                servers: Some(HashMap::from([(
                    "unrelated".into(),
                    McpServerConfig {
                        command: Some("must-not-run".into()),
                        args: vec![],
                        cmd: None,
                        timeout_ms: None,
                        url: None,
                        transport: Some("stdio".into()),
                        bearer_env: None,
                    },
                )])),
            };
            let config = merge(Some(configured)).unwrap();
            assert_eq!(config.servers.unwrap().len(), 1);
            assert_eq!(config.default_server.as_deref(), Some("test-phone"));
            let args = scoped_arguments(
                "test-phone",
                "tools/call",
                &serde_json::json!({"name":"phone_observe","arguments":{}}),
            )
            .unwrap();
            assert_eq!(args["arguments"]["phone"], "selected");
            assert!(scoped_arguments(
                "test-phone",
                "tools/call",
                &serde_json::json!({"name":"phone_act","arguments":{"phone":"other"}})
            )
            .is_err());
            let discover = scoped_arguments(
                "test-phone",
                "tools/call",
                &serde_json::json!({"name":"phone_devices"}),
            )
            .unwrap();
            assert!(discover.get("arguments").is_none());
        });
        let other = root.join("other-workspace");
        std::fs::create_dir_all(&other).unwrap();
        crate::tools::in_root(&other, || assert!(merge(None).is_none()));
    }
}
