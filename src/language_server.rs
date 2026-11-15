//! Persistent, read-only LSP requests against user-configured language servers.
use crate::tools::{Tool, ToolResult, ToolSafety};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::oneshot,
};

#[derive(Clone, Deserialize, Serialize)]
struct Config {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
    language_id: String,
    timeout_ms: Option<u64>,
}
struct Client {
    child: tokio::sync::Mutex<Child>,
    input: Arc<tokio::sync::Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>,
    diagnostics: Arc<Mutex<HashMap<String, Value>>>,
    closed: Arc<AtomicBool>,
    next: AtomicU64,
    documents: Arc<tokio::sync::Mutex<HashMap<String, (String, i64)>>>,
}
static CLIENTS: OnceLock<Mutex<HashMap<String, Arc<Client>>>> = OnceLock::new();
fn clients() -> &'static Mutex<HashMap<String, Arc<Client>>> {
    CLIENTS.get_or_init(Default::default)
}
async fn write(input: &tokio::sync::Mutex<ChildStdin>, message: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("LSP message exceeds 8 MiB".into());
    }
    let mut input = input.lock().await;
    input
        .write_all(format!("Content-Length: {}\r\n\r\n", bytes.len()).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    input.write_all(&bytes).await.map_err(|e| e.to_string())?;
    input.flush().await.map_err(|e| e.to_string())
}
async fn frame(reader: &mut BufReader<ChildStdout>) -> Result<Value, String> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 8192 {
            return Err("LSP header exceeds 8 KiB".into());
        }
        header.push(reader.read_u8().await.map_err(|e| e.to_string())?);
    }
    let text = std::str::from_utf8(&header).map_err(|_| "Invalid LSP header")?;
    let size = text
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
                .map(|(_, n)| n.trim().parse::<usize>())
        })
        .ok_or("LSP message has no Content-Length")?
        .map_err(|_| "Invalid LSP message length")?;
    if size > 8 * 1024 * 1024 {
        return Err("LSP body exceeds 8 MiB".into());
    }
    let mut bytes = vec![0; size];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
impl Client {
    async fn start(root: &Path, cfg: &Config) -> Result<Arc<Self>, String> {
        let mut command = std::process::Command::new(&cfg.command);
        command.args(&cfg.args).current_dir(root);
        let command = crate::sandbox::wrap(command)?;
        let mut child = tokio::process::Command::from(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Cannot start language server {}: {e}", cfg.command))?;
        let input = Arc::new(tokio::sync::Mutex::new(
            child.stdin.take().ok_or("No LSP stdin")?,
        ));
        let output = child.stdout.take().ok_or("No LSP stdout")?;
        let client = Arc::new(Self {
            child: tokio::sync::Mutex::new(child),
            input: input.clone(),
            pending: Default::default(),
            diagnostics: Default::default(),
            closed: Arc::new(AtomicBool::new(false)),
            next: AtomicU64::new(1),
            documents: Default::default(),
        });
        let documents = client.documents.clone();
        let (pending, diagnostics, closed) = (
            client.pending.clone(),
            client.diagnostics.clone(),
            client.closed.clone(),
        );
        tokio::spawn(async move {
            let mut reader = BufReader::new(output);
            while let Ok(message) = frame(&mut reader).await {
                if message.get("method").is_some() {
                    if message["method"] == "textDocument/publishDiagnostics" {
                        if let Some(uri) = message["params"]["uri"].as_str() {
                            let reported = message["params"]["version"].as_i64();
                            let current =
                                documents.lock().await.get(uri).map(|(_, version)| *version);
                            if reported
                                .zip(current)
                                .is_some_and(|(reported, current)| reported < current)
                            {
                                continue;
                            }
                            if let Ok(mut d) = diagnostics.lock() {
                                if d.len() >= 256 {
                                    d.clear();
                                }
                                d.insert(uri.into(), json!({"diagnostics":message["params"]["diagnostics"],"version":reported,"version_confirmed":reported.is_some()}));
                            }
                        }
                    }
                    if let Some(id) = message.get("id") {
                        let response = match message["method"].as_str() {
                            Some("workspace/configuration") => {
                                json!({"jsonrpc":"2.0","id":id,"result":message["params"]["items"].as_array().map(|a|vec![Value::Null;a.len()]).unwrap_or_default()})
                            }
                            Some(
                                "window/workDoneProgress/create"
                                | "client/registerCapability"
                                | "client/unregisterCapability",
                            ) => json!({"jsonrpc":"2.0","id":id,"result":null}),
                            _ => {
                                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Server-initiated edits, commands, and sampling are not allowed"}})
                            }
                        };
                        if write(&input, &response).await.is_err() {
                            break;
                        }
                    }
                } else if let Some(id) = message["id"].as_u64() {
                    if let Some(reply) = pending.lock().ok().and_then(|mut p| p.remove(&id)) {
                        let _ = reply.send(if message.get("error").is_some() {
                            Err(message["error"].to_string())
                        } else {
                            Ok(message["result"].clone())
                        });
                    }
                }
            }
            closed.store(true, Ordering::SeqCst);
            if let Ok(mut p) = pending.lock() {
                for (_, reply) in p.drain() {
                    let _ = reply.send(Err("Language server disconnected".into()));
                }
            }
        });
        let uri = url::Url::from_directory_path(root)
            .map_err(|_| "Cannot encode workspace URI")?
            .to_string();
        client.request("initialize",json!({"processId":std::process::id(),"rootUri":uri,"workspaceFolders":[{"uri":uri,"name":root.file_name().unwrap_or_default().to_string_lossy()}],"capabilities":{"textDocument":{"publishDiagnostics":{"relatedInformation":true}}},"clientInfo":{"name":"osmogrep","version":env!("CARGO_PKG_VERSION")}})).await?;
        client.notify("initialized", json!({})).await?;
        Ok(client)
    }
    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        write(
            &self.input,
            &json!({"jsonrpc":"2.0","method":method,"params":params}),
        )
        .await
    }
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| "LSP request map unavailable")?
            .insert(id, tx);
        if let Err(error) = write(
            &self.input,
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .await
        {
            self.pending
                .lock()
                .map_err(|_| "LSP request map unavailable")?
                .remove(&id);
            return Err(error);
        }
        rx.await
            .map_err(|_| "Language server request was cancelled".to_string())?
    }
}
pub struct LanguageServer;
impl Tool for LanguageServer {
    fn name(&self) -> &'static str {
        "language_server"
    }
    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }
    fn schema(&self) -> Value {
        json!({"type":"function","name":"language_server","description":"Query a user-configured language server for definitions, references, hover information, symbols, or diagnostics. Positions use zero-based LSP line/UTF-16 character indices. Configure [lsp.servers.<name>] with command, args, language_id, and extensions. No server-initiated edits or commands are executed.","parameters":{"type":"object","properties":{"path":{"type":"string"},"server":{"type":"string"},"action":{"type":"string","enum":["definition","references","hover","symbols","diagnostics"]},"line":{"type":"integer","minimum":0},"character":{"type":"integer","minimum":0}},"required":["path","action"],"additionalProperties":false}})
    }
    fn call(&self, args: Value) -> ToolResult {
        self.call_cancellable(args, &|| false)
    }
    fn call_cancellable(&self, args: Value, cancel: &dyn Fn() -> bool) -> ToolResult {
        let root = crate::tools::repo_root()?;
        let path = crate::file_guard::workspace_path(
            &root,
            Path::new(args["path"].as_str().ok_or("Missing LSP path")?),
            false,
        )?;
        let config = std::fs::read_to_string(crate::persistence::config_dir().join("config.toml"))
            .map_err(|_| "Configure [lsp.servers.<name>] first")?
            .parse::<toml::Value>()
            .map_err(|e| e.to_string())?;
        let servers: HashMap<String, Config> = config
            .get("lsp")
            .and_then(|l| l.get("servers"))
            .ok_or("No language servers configured")?
            .clone()
            .try_into()
            .map_err(|e: toml::de::Error| e.to_string())?;
        let (name, cfg) = if let Some(name) = args["server"].as_str() {
            (name, servers.get(name).ok_or("Unknown language server")?)
        } else {
            servers
                .iter()
                .find(|(_, cfg)| {
                    cfg.extensions
                        .iter()
                        .any(|e| Some(e.as_str()) == path.extension().and_then(|e| e.to_str()))
                })
                .map(|(n, c)| (n.as_str(), c))
                .ok_or("No language server is configured for this file extension")?
        };
        let key = format!(
            "{}:{name}:{}",
            root.display(),
            crate::file_guard::digest(
                serde_json::to_string(cfg)
                    .map_err(|e| e.to_string())?
                    .as_bytes()
            )
        );
        let timeout = Duration::from_millis(cfg.timeout_ms.unwrap_or(30_000).clamp(1000, 120_000));
        let result=crate::mcp::runtime().block_on(async {
            let operation=async {
                let cached=clients().lock().map_err(|_|"LSP registry unavailable")?.get(&key).filter(|c|!c.closed.load(Ordering::SeqCst)).cloned();
                let client=if let Some(client)=cached{client}else{let client=Client::start(&root,cfg).await?;clients().lock().map_err(|_|"LSP registry unavailable")?.insert(key.clone(),client.clone());client};
                if fs_size(&path)?>4*1024*1024{return Err("LSP document exceeds 4 MiB".into());}let text=std::fs::read_to_string(&path).map_err(|e|e.to_string())?;
                let uri=url::Url::from_file_path(&path).map_err(|_|"Cannot encode document URI")?.to_string();let hash=crate::file_guard::digest(text.as_bytes());
                {let mut documents=client.documents.lock().await;match documents.get(&uri){
                    None=>{client.diagnostics.lock().map_err(|_|"Diagnostic cache unavailable")?.remove(&uri);client.notify("textDocument/didOpen",json!({"textDocument":{"uri":uri,"languageId":cfg.language_id,"version":1,"text":text}})).await?;documents.insert(uri.clone(),(hash,1));},
                    Some((previous,version)) if *previous!=hash=>{let version=version+1;client.diagnostics.lock().map_err(|_|"Diagnostic cache unavailable")?.remove(&uri);client.notify("textDocument/didChange",json!({"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":text}]})).await?;documents.insert(uri.clone(),(hash,version));},_=>{}
                }}
                let mut params=json!({"textDocument":{"uri":uri},"position":{"line":args["line"].as_u64().unwrap_or(0),"character":args["character"].as_u64().unwrap_or(0)}});
                let method=match args["action"].as_str(){Some("definition")=>"textDocument/definition",Some("references")=>{params["context"]=json!({"includeDeclaration":true});"textDocument/references"},Some("hover")=>"textDocument/hover",Some("symbols")=>"textDocument/documentSymbol",Some("diagnostics")=>{
                    let deadline=std::time::Instant::now()+Duration::from_secs(2);loop {
                        if let Some(value)=client.diagnostics.lock().map_err(|_|"Diagnostic cache unavailable")?.get(&uri).cloned(){return Ok(json!({"received":true,"uri":uri,"diagnostics":value["diagnostics"],"version":value["version"],"version_confirmed":value["version_confirmed"]}));}
                        if std::time::Instant::now()>=deadline{return Ok(json!({"received":false,"uri":uri,"note":"Server has not published diagnostics yet; this is not a clean diagnostic result"}));}tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                },_=>return Err("Unknown LSP action".into())};
                client.request(method,params).await
            };
            tokio::select! {result=tokio::time::timeout(timeout,operation)=>result.map_err(|_|"Language server request timed out".to_string())?,_=async{while !cancel(){tokio::time::sleep(Duration::from_millis(20)).await;}}=>Err("cancelled".into())}
        });
        if result.is_err() {
            if let Some(client) = clients()
                .lock()
                .map_err(|_| "LSP registry unavailable")?
                .remove(&key)
            {
                crate::mcp::runtime().spawn(async move {
                    let _ = client.child.lock().await.kill().await;
                });
            }
        }
        result
    }
}
fn fs_size(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}
pub struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        let all = clients()
            .lock()
            .map(|mut c| std::mem::take(&mut *c))
            .unwrap_or_default();
        for (_, client) in all {
            crate::mcp::runtime().spawn(async move {
                let _ = client.child.lock().await.kill().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lsp_handshake_diagnostics_and_server_edit_rejection() {
        let script = r#"import sys,json
def read():
 size=0
 while True:
  line=sys.stdin.buffer.readline()
  if not line: raise EOFError()
  if line.lower().startswith(b'content-length:'): size=int(line.split(b':')[1])
  if line==b'\r\n': break
 return json.loads(sys.stdin.buffer.read(size))
def send(value):
 data=json.dumps(value).encode();sys.stdout.buffer.write(('Content-Length: %d\r\n\r\n'%len(data)).encode()+data);sys.stdout.buffer.flush()
while True:
 try: r=read()
 except EOFError: break
 m=r.get('method')
 if m=='initialize': send({'jsonrpc':'2.0','id':r['id'],'result':{'capabilities':{}}})
 elif m=='textDocument/didOpen': send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{'uri':r['params']['textDocument']['uri'],'diagnostics':[{'message':'fixture diagnostic','severity':2}]}})
 elif m=='textDocument/hover':
  send({'jsonrpc':'2.0','id':'unsafe-edit','method':'workspace/applyEdit','params':{'edit':{}}})
  reply=read()
  assert reply['id']=='unsafe-edit' and reply['error']['code']==-32601
  send({'jsonrpc':'2.0','id':r['id'],'result':{'contents':'hover result; unsafe edit rejected'}})
"#;
        let root = std::env::temp_dir().join(format!("lsp-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let cfg = Config {
            command: if cfg!(windows) { "python" } else { "python3" }.into(),
            args: vec!["-u".into(), "-c".into(), script.into()],
            extensions: vec!["py".into()],
            language_id: "python".into(),
            timeout_ms: None,
        };
        crate::mcp::runtime().block_on(async {
            let client=tokio::time::timeout(Duration::from_secs(3),Client::start(&root,&cfg)).await.unwrap().unwrap();
            client.notify("textDocument/didOpen",json!({"textDocument":{"uri":"file:///fixture.py","languageId":"python","version":1,"text":"x = 1"}})).await.unwrap();
            let hover=tokio::time::timeout(Duration::from_secs(3),client.request("textDocument/hover",json!({"textDocument":{"uri":"file:///fixture.py"},"position":{"line":0,"character":0}}))).await.unwrap().unwrap();
            assert!(hover["contents"].as_str().unwrap().contains("unsafe edit rejected"));
            assert_eq!(client.diagnostics.lock().unwrap()["file:///fixture.py"]["diagnostics"][0]["message"],"fixture diagnostic");
            client.child.lock().await.kill().await.unwrap();
        });
        std::fs::remove_dir_all(root).unwrap();
    }
}
