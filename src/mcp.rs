use rmcp::{
    service::RunningService,
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
        TokioChildProcess,
    },
    RoleClient, ServiceExt,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
mod local;
pub use local::register_local;

type Connections = HashMap<String, RunningService<RoleClient, ()>>;
static CONNECTIONS: OnceLock<Mutex<Connections>> = OnceLock::new();
pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Cannot initialize async I/O runtime")
    })
}
fn connections() -> &'static Mutex<Connections> {
    CONNECTIONS.get_or_init(Default::default)
}

pub fn call_cancellable(
    server: Option<&str>,
    method: &str,
    args: &Value,
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let cfg = effective_config().ok_or("MCP config not found")?;
    if !cfg.enabled.unwrap_or(false) {
        return Err("MCP is disabled".into());
    }
    let server = server
        .or(cfg.default_server.as_deref())
        .ok_or("Choose an MCP server")?;
    let scfg = cfg
        .servers
        .as_ref()
        .and_then(|s| s.get(server))
        .ok_or("Unknown MCP server")?;
    let timeout = Duration::from_millis(scfg.timeout_ms.unwrap_or(30_000).clamp(1000, 120_000));
    let args = local::scoped_arguments(server, method, args)?;
    if scfg.transport.as_deref() == Some("bridge")
        || (scfg.transport.is_none() && scfg.command.is_none() && scfg.url.is_none())
    {
        return bridge_call(server, scfg, method, &args, timeout, cancel);
    }
    native_call(server, scfg, method, &args, timeout, cancel)
}

fn native_call(
    server: &str,
    cfg: &McpServerConfig,
    method: &str,
    args: &Value,
    timeout: Duration,
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let root = crate::tools::repo_root()?;
    let token = cfg
        .bearer_env
        .as_ref()
        .map(|name| {
            std::env::var(name)
                .map_err(|_| format!("MCP credential environment variable {name} is unset"))
        })
        .transpose()?;
    let key = format!(
        "{}:{server}:{}:{}",
        root.display(),
        serde_json::to_string(cfg).map_err(|e| e.to_string())?,
        crate::file_guard::digest(token.as_deref().unwrap_or("").as_bytes())
    );
    let result:Result<Value,String>=runtime().block_on(async {
        let operation=async {
            let cached=connections().lock().map_err(|_|"MCP registry unavailable")?.get(&key).map(|s|s.peer().clone());
            let peer=if let Some(peer)=cached {peer}else{
                let service=if let Some(url)=&cfg.url {
                    let parsed=url::Url::parse(url).map_err(|e|e.to_string())?;
                    if parsed.scheme()!="https" && !(parsed.scheme()=="http" && matches!(parsed.host_str(),Some("127.0.0.1"|"localhost"|"::1"))) {return Err("Remote MCP requires HTTPS (HTTP allowed only for loopback)".into());}
                    if !parsed.username().is_empty() || parsed.password().is_some() {return Err("MCP URL must not embed credentials".into());}
                    let mut config=StreamableHttpClientTransportConfig::with_uri(url.clone());
                    if let Some(token)=&token {config=config.auth_header(token.clone());}
                    ().serve(StreamableHttpClientTransport::from_config(config)).await.map_err(|e|e.to_string())?
                }else{
                    let mut command=if let Some(program)=&cfg.command {let mut c=Command::new(program);c.args(&cfg.args);c}else{let mut c=Command::new("sh");c.args(["-lc",cfg.cmd.as_deref().ok_or("Stdio MCP requires command or cmd")?]);c};
                    command.current_dir(&root);
                    let command=crate::sandbox::wrap(command)?;
                    let transport=TokioChildProcess::new(tokio::process::Command::from(command)).map_err(|e|e.to_string())?;
                    ().serve(transport).await.map_err(|e|e.to_string())?
                };
                let peer=service.peer().clone();
                connections().lock().map_err(|_|"MCP registry unavailable")?.insert(key.clone(),service);peer
            };
            let value=match method {
                "tools/list"=>serde_json::to_value(peer.list_all_tools().await.map_err(|e|e.to_string())?),
                "resources/list"=>serde_json::to_value(peer.list_all_resources().await.map_err(|e|e.to_string())?),
                "prompts/list"=>serde_json::to_value(peer.list_all_prompts().await.map_err(|e|e.to_string())?),
                "tools/call"=>serde_json::to_value(peer.call_tool(serde_json::from_value(args.clone()).map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?),
                "resources/read"=>serde_json::to_value(peer.read_resource(serde_json::from_value(args.clone()).map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?),
                "prompts/get"=>serde_json::to_value(peer.get_prompt(serde_json::from_value(args.clone()).map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?),
                _=>return Err("Use tools/list, tools/call, resources/list, resources/read, prompts/list, or prompts/get".into()),
            }.map_err(|e|e.to_string())?;
            if value.to_string().len()>4*1024*1024 {return Err("MCP result exceeds 4 MiB; request a smaller resource".into());}
            Ok(value)
        };
        tokio::select! {
            result=tokio::time::timeout(timeout,operation)=>result.map_err(|_|"MCP request timed out".to_string())?,
            _=async {while !cancel() {tokio::time::sleep(Duration::from_millis(20)).await;}}=>Err("cancelled".into()),
        }
    });
    if result.is_err() {
        if let Some(service) = connections()
            .lock()
            .map_err(|_| "MCP registry unavailable")?
            .remove(&key)
        {
            service.cancellation_token().cancel();
        }
    }
    result.map_err(|e| {
        if let Some(token) = token.filter(|s| !s.is_empty()) {
            e.replace(&token, "[redacted]")
        } else {
            e
        }
    })
}

pub struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        let services = connections()
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default();
        for (_, service) in services {
            service.cancellation_token().cancel();
        }
    }
}

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub cmd: Option<String>,
    pub timeout_ms: Option<u64>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    pub transport: Option<String>,
    pub bearer_env: Option<String>,
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
    effective_config().and_then(|c| c.enabled).unwrap_or(false)
}
fn effective_config() -> Option<McpConfig> {
    local::merge(load_config())
}

pub fn list_servers() -> Vec<String> {
    effective_config()
        .and_then(|c| c.servers)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn call_with_timeout(
    server: Option<&str>,
    method: &str,
    args: &Value,
    limit: Duration,
) -> Result<Value, String> {
    let cfg = effective_config().ok_or("MCP config not found")?;
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
    let args = local::scoped_arguments(&target, method, args)?;

    let timeout =
        Duration::from_millis(scfg.timeout_ms.unwrap_or(30_000).clamp(1_000, 120_000)).min(limit);
    if timeout.is_zero() {
        return Err("MCP execution deadline exceeded".into());
    }

    let transport = scfg.transport.as_deref().unwrap_or(if scfg.url.is_some() {
        "http"
    } else if scfg.command.is_some() {
        "stdio"
    } else {
        "bridge"
    });
    if transport != "bridge" {
        return native_call(&target, &scfg, method, &args, timeout, &|| false);
    }
    bridge_call(&target, &scfg, method, &args, timeout, &|| false)
}

pub(crate) fn call_tool(
    server: &str,
    name: &str,
    args: &Value,
    timeout: Duration,
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let cfg = effective_config().ok_or("MCP config not found")?;
    if !cfg.enabled.unwrap_or(false) {
        return Err("MCP is disabled".into());
    }
    let scfg = cfg
        .servers
        .as_ref()
        .and_then(|servers| servers.get(server))
        .ok_or("Unknown MCP server")?;
    let timeout = timeout.min(Duration::from_millis(
        scfg.timeout_ms.unwrap_or(30_000).clamp(1000, 120_000),
    ));
    if timeout.is_zero() || cancel() {
        return Err("MCP operation cancelled or deadline exceeded".into());
    }
    let bridge = scfg.transport.as_deref() == Some("bridge")
        || (scfg.transport.is_none() && scfg.command.is_none() && scfg.url.is_none());
    if bridge {
        bridge_call(
            server,
            scfg,
            name,
            &local::scoped_arguments(server, name, args)?,
            timeout,
            cancel,
        )
    } else {
        let args = serde_json::json!({"name":name,"arguments":args});
        native_call(
            server,
            scfg,
            "tools/call",
            &local::scoped_arguments(server, "tools/call", &args)?,
            timeout,
            cancel,
        )
    }
}
fn bridge_call(
    target: &str,
    scfg: &McpServerConfig,
    method: &str,
    args: &Value,
    timeout: Duration,
    cancel: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let mut cmd = Command::new("sh");
    cmd.args(["-lc", scfg.cmd.as_deref().ok_or("Bridge requires cmd")?])
        .env("OSMOGREP_MCP_SERVER", target)
        .env("OSMOGREP_MCP_METHOD", method)
        .env(
            "OSMOGREP_MCP_ARGS",
            serde_json::to_string(args).map_err(|e| e.to_string())?,
        );
    let out = run_bridge_cancellable(cmd, timeout, cancel)?;
    Ok(
        serde_json::json!({"server":target,"method":method,"exit_code":out.exit_code,"stdout":String::from_utf8_lossy(&out.stdout),"stderr":truncate(&String::from_utf8_lossy(&out.stderr),4000)}),
    )
}

#[cfg(test)]
fn run_bridge(
    cmd: Command,
    timeout: Duration,
) -> Result<crate::process_runner::ProcessRun, String> {
    run_bridge_cancellable(cmd, timeout, &|| false)
}
fn run_bridge_cancellable(
    cmd: Command,
    timeout: Duration,
    cancel: &dyn Fn() -> bool,
) -> Result<crate::process_runner::ProcessRun, String> {
    let out = crate::process_runner::run_command_cancellable(cmd, timeout, cancel)?;
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
    fn native_http_handshake_and_tool_discovery() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.starts_with("POST /mcp "));
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        if key.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let request: Value = serde_json::from_slice(&bytes).unwrap();
                if request["method"] == "notifications/initialized" {
                    socket.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    continue;
                }
                let result = if request["method"] == "initialize" {
                    serde_json::json!({"protocolVersion":request["params"]["protocolVersion"],"capabilities":{"tools":{}},"serverInfo":{"name":"http-fixture","version":"1"}})
                } else {
                    assert_eq!(request["method"], "tools/list");
                    serde_json::json!({"tools":[{"name":"remote_echo","inputSchema":{"type":"object","properties":{}}}]})
                };
                let body = serde_json::to_vec(
                    &serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
                )
                .unwrap();
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                socket.write_all(&body).unwrap();
            }
        });
        let cfg = McpServerConfig {
            cmd: None,
            command: None,
            args: Vec::new(),
            url: Some(format!("http://{address}/mcp")),
            transport: Some("http".into()),
            bearer_env: None,
            timeout_ms: Some(3000),
        };
        let value = native_call(
            "http-fixture",
            &cfg,
            "tools/list",
            &serde_json::json!({}),
            Duration::from_secs(3),
            &|| false,
        )
        .unwrap();
        assert_eq!(value[0]["name"], "remote_echo");
        worker.join().unwrap();
    }

    #[test]
    fn native_stdio_discovers_tools_reads_resources_and_reuses_the_connection() {
        let script = r#"import sys,json
for line in sys.stdin:
 r=json.loads(line)
 if 'id' not in r: continue
 method=r['method']; p=r.get('params',{})
 if method=='initialize': result={'protocolVersion':p['protocolVersion'],'capabilities':{'tools':{},'resources':{}},'serverInfo':{'name':'fixture','version':'1'}}
 elif method=='tools/list': result={'tools':[{'name':'echo','description':'Echo','inputSchema':{'type':'object','properties':{}}}]}
 elif method=='tools/call': result={'content':p['arguments'].get('content',[{'type':'text','text':p['arguments']['text']}])}
 elif method=='resources/read': result={'contents':[{'uri':p['uri'],'text':'resource contents'}]}
 else: result={}
 print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#;
        let cfg = McpServerConfig {
            cmd: None,
            timeout_ms: None,
            command: Some(if cfg!(windows) { "python" } else { "python3" }.into()),
            args: vec!["-u".into(), "-c".into(), script.into()],
            url: None,
            transport: Some("stdio".into()),
            bearer_env: None,
        };
        let tools = native_call(
            "fixture",
            &cfg,
            "tools/list",
            &serde_json::json!({}),
            Duration::from_secs(10),
            &|| false,
        )
        .unwrap();
        assert_eq!(tools[0]["name"], "echo");
        let result = native_call(
            "fixture",
            &cfg,
            "tools/call",
            &serde_json::json!({"name":"echo","arguments":{"text":"round trip"}}),
            Duration::from_secs(10),
            &|| false,
        )
        .unwrap();
        assert_eq!(result["content"][0]["text"], "round trip");
        use base64::Engine;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageOutputFormat::Png)
            .unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
        let screenshot = native_call(
            "fixture", &cfg, "tools/call",
            &serde_json::json!({"name":"echo","arguments":{"text":"screen","content":[{"type":"image","mimeType":"image/png","data":encoded}]}}),
            Duration::from_secs(10), &||false,
        ).unwrap();
        let observation = crate::tool_budget::model_output(
            &crate::tools::repo_root().unwrap(),
            "mcp_call",
            &screenshot,
        )
        .unwrap();
        assert_eq!(
            observation[1]["image_url"],
            format!("data:image/png;base64,{encoded}")
        );
        assert!(!observation[0]["text"].as_str().unwrap().contains(&encoded));
        let resource = native_call(
            "fixture",
            &cfg,
            "resources/read",
            &serde_json::json!({"uri":"fixture://example"}),
            Duration::from_secs(10),
            &|| false,
        )
        .unwrap();
        assert_eq!(resource["contents"][0]["text"], "resource contents");
        assert!(native_call(
            "fixture",
            &cfg,
            "tools/list",
            &serde_json::json!({}),
            Duration::from_secs(10),
            &|| true
        )
        .is_err());
    }

    #[test]
    fn bridge_captures_machine_readable_stdout() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf '{\"session\":\"vm-test\"}'"]);
        let result = run_bridge(cmd, Duration::from_secs(2)).unwrap();
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["session"], "vm-test");
    }
}
