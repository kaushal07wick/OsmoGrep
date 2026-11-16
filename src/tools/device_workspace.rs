use super::{Tool, ToolResult, ToolSafety};
use serde_json::{json, Value};

pub struct DeviceWorkspace;
impl Tool for DeviceWorkspace {
    fn name(&self) -> &'static str {
        "device_workspace"
    }
    fn schema(&self) -> Value {
        json!({"type":"function","name":self.name(),
            "description":"Prepare and open a dedicated virtual Mac or iPhone simulator for the requested app development task. Automatically install the optional companion, download a missing runtime, create or reuse the named device, and boot it. Shows a native viewer beside chat in Ghostty; headless callers get a background command session to poll with run_shell. Connects the device's MCP server for screenshots, app interaction and lifecycle controls. Setup can take time and guest account/privacy steps may require the user. Use only when running or inspecting the requested app needs a device; questions and ordinary source edits do not require launching one.",
            "parameters":{"type":"object","properties":{
                "platform":{"type":"string","enum":["mac","iphone"]},
                "name":{"type":"string","description":"Device name; defaults to default. Reuse the same name throughout the task."}
            },"required":["platform"],"additionalProperties":false}})
    }
    fn safety(&self) -> ToolSafety {
        ToolSafety::Dangerous
    }
    fn call(&self, args: Value) -> ToolResult {
        self.call_cancellable(args, &|| false)
    }
    fn call_cancellable(&self, args: Value, cancel: &dyn Fn() -> bool) -> ToolResult {
        let device = match args["platform"].as_str() {
            Some("mac") => crate::workspace::Device::Mac,
            Some("iphone") => crate::workspace::Device::Phone,
            _ => return Err("platform must be mac or iphone".into()),
        };
        let name = args["name"].as_str().unwrap_or("default");
        crate::workspace::open_for_agent(&crate::tools::repo_root()?, device, name, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[ignore = "requires the isolated fake companion fixture"]
    fn automatic_preparation_reuses_devices_and_registers_agent_controls() {
        let root = std::env::var_os("OSMOGREP_DEVICE_TEST_ROOT")
            .map(std::path::PathBuf::from)
            .expect("Set the isolated fixture root");
        assert!(root.join("fake-companion-fixture").is_file());
        assert!(!crate::workspace::ghostty::available());
        let _cleanup = crate::mcp::Cleanup;
        let registry = crate::tools::ToolRegistry::with_root(root.clone());
        let scope = crate::tools::ToolScope::for_prompt("Continue working on this project");
        assert!(!registry
            .scoped_schema(&scope)
            .iter()
            .any(|tool| tool["name"] == "mcp_call"));
        crate::tools::in_root(&root, || {
            for (platform, server, expected) in [
                ("mac", "terminal-mac", "screenshot"),
                ("iphone", "terminal-phone", "phone_observe"),
            ] {
                for phase in ["created", "reused"] {
                    let result = registry
                        .call_cancellable(
                            "device_workspace",
                            json!({"platform":platform,"name":"app-preview"}),
                            &|| false,
                        )
                        .unwrap();
                    assert_eq!(result["status"], "starting");
                    assert_eq!(result["viewer"], false);
                    assert_eq!(result["server"], server);
                    assert!(registry
                        .scoped_schema(&scope)
                        .iter()
                        .any(|tool| tool["name"] == "mcp_call"));
                    assert!(registry
                        .scoped_schema(&crate::tools::ToolScope::for_prompt("hello"))
                        .is_empty());
                    let progress = crate::command_sessions::interact(
                        &root,
                        &json!({"session_id":result["session_id"],"yield_ms":2000}),
                        &|| false,
                    )
                    .unwrap();
                    assert_eq!(progress["exit_code"], 0, "{progress}");
                    assert!(
                        progress["stdout"].as_str().unwrap().contains(phase),
                        "{progress}"
                    );
                }
                let tools =
                    crate::mcp::call_cancellable(Some(server), "tools/list", &json!({}), &|| false)
                        .unwrap();
                assert!(tools
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["name"] == expected));
                println!("{platform}: automatic create/reuse, pollable setup session and workspace-scoped MCP controls passed");
            }
        });
    }
    #[test]
    fn preparation_rejects_bad_targets_and_cancels_before_host_or_download_work() {
        assert_eq!(DeviceWorkspace.safety(), ToolSafety::Dangerous);
        assert!(DeviceWorkspace
            .call(json!({"platform":"linux"}))
            .unwrap_err()
            .contains("platform"));
        assert!(DeviceWorkspace
            .call(json!({"platform":"mac","name":"a;touch marker"}))
            .is_err());
        assert!(DeviceWorkspace
            .call_cancellable(json!({"platform":"iphone"}), &|| true)
            .unwrap_err()
            .contains("cancelled"));
    }
}
