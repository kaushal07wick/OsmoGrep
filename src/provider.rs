//! Wire formats stay at the model boundary; the tool loop consumes Responses-shaped items.
use super::{AgentEvent, CancelToken, ModelConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(test)]
use std::io::{BufRead, BufReader, Read};
use std::{
    collections::{BTreeMap, HashSet},
    sync::mpsc::{self, Sender},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Api {
    Responses,
    ChatCompletions,
    AnthropicMessages,
}
impl Api {
    pub fn name(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat-completions",
            Self::AnthropicMessages => "anthropic-messages",
        }
    }
    pub fn parse(s: &str) -> Result<Option<Self>, String> {
        match s {
            "auto" => Ok(None),
            "responses" => Ok(Some(Self::Responses)),
            "chat-completions" => Ok(Some(Self::ChatCompletions)),
            "anthropic-messages" => Ok(Some(Self::AnthropicMessages)),
            _ => Err("API must be auto, responses, chat-completions, or anthropic-messages".into()),
        }
    }
}
pub fn defaults(provider: &str) -> (&'static str, &'static str, Api) {
    match provider {
        "openai" => (
            "https://api.openai.com/v1",
            "OPENAI_API_KEY",
            Api::Responses,
        ),
        "anthropic" => (
            "https://api.anthropic.com/v1",
            "ANTHROPIC_API_KEY",
            Api::AnthropicMessages,
        ),
        "groq" => (
            "https://api.groq.com/openai/v1",
            "GROQ_API_KEY",
            Api::ChatCompletions,
        ),
        "mistral" => (
            "https://api.mistral.ai/v1",
            "MISTRAL_API_KEY",
            Api::ChatCompletions,
        ),
        "openrouter" => (
            "https://openrouter.ai/api/v1",
            "OPENROUTER_API_KEY",
            Api::ChatCompletions,
        ),
        "deepseek" => (
            "https://api.deepseek.com/v1",
            "DEEPSEEK_API_KEY",
            Api::ChatCompletions,
        ),
        "xai" => ("https://api.x.ai/v1", "XAI_API_KEY", Api::ChatCompletions),
        "ollama" => (
            "http://127.0.0.1:11434/v1",
            "OLLAMA_API_KEY",
            Api::ChatCompletions,
        ),
        "lmstudio" => (
            "http://127.0.0.1:1234/v1",
            "LMSTUDIO_API_KEY",
            Api::ChatCompletions,
        ),
        _ => ("", "OSMOGREP_API_KEY", Api::ChatCompletions),
    }
}
pub fn api(cfg: &ModelConfig) -> Api {
    cfg.api.unwrap_or_else(|| defaults(&cfg.provider).2)
}
pub fn authenticate(
    request: reqwest::blocking::RequestBuilder,
    cfg: &ModelConfig,
    key: &str,
) -> reqwest::blocking::RequestBuilder {
    if api(cfg) == Api::AnthropicMessages {
        request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
    } else if key.is_empty() {
        request
    } else {
        request.bearer_auth(key)
    }
}
fn endpoint(cfg: &ModelConfig) -> Result<String, String> {
    let base = cfg
        .base_url
        .as_deref()
        .unwrap_or(defaults(&cfg.provider).0)
        .trim_end_matches('/');
    let url = reqwest::Url::parse(base).map_err(|_| "Set a valid provider base_url".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Provider base_url must be HTTP(S), without credentials, query, or fragment".into(),
        );
    }
    let path = match api(cfg) {
        Api::Responses => "responses",
        Api::ChatCompletions => "chat/completions",
        Api::AnthropicMessages => "messages",
    };
    Ok(format!("{base}/{path}"))
}
fn parts(content: &Value, anthropic: bool) -> Result<Value, String> {
    if let Some(s) = content.as_str() {
        return Ok(if anthropic {
            json!([{"type":"text","text":s}])
        } else {
            json!(s)
        });
    }
    let mut out = Vec::new();
    for part in content.as_array().ok_or("Invalid message content")? {
        match part["type"].as_str() {
            Some("input_text" | "output_text" | "text") => {
                out.push(json!({"type":"text","text":part["text"]}))
            }
            Some("input_image") => {
                let url = part["image_url"].as_str().ok_or("Invalid image URL")?;
                out.push(if anthropic {
                    let (kind, data) = url
                        .strip_prefix("data:")
                        .and_then(|s| s.split_once(";base64,"))
                        .ok_or("Anthropic images require inline base64 data")?;
                    json!({"type":"image","source":{"type":"base64","media_type":kind,"data":data}})
                } else {
                    json!({"type":"image_url","image_url":{"url":url}})
                });
            }
            _ => return Err("Unsupported message content block".into()),
        }
    }
    Ok(json!(out))
}
fn append_message(messages: &mut Vec<Value>, role: &str, content: Value) {
    if let Some(last) = messages
        .last_mut()
        .filter(|m| m["role"] == role && m["content"].is_array())
    {
        if let Some(parts) = content.as_array() {
            last["content"]
                .as_array_mut()
                .unwrap()
                .extend(parts.clone());
            return;
        }
    }
    messages.push(json!({"role":role,"content":content}));
}
pub fn payload(cfg: &ModelConfig, canonical: &Value) -> Result<Value, String> {
    let configured = crate::model_options::apply(cfg, canonical)?;
    let canonical = &configured;
    let protocol = api(cfg);
    if protocol == Api::Responses {
        return Ok(canonical.clone());
    }
    let anthropic = protocol == Api::AnthropicMessages;
    let mut messages = Vec::new();
    let mut system = Vec::new();
    let mut replay = false;
    let mut replayed_calls = HashSet::new();
    let mut observations = Vec::new();
    for item in canonical["input"].as_array().ok_or("Missing model input")? {
        let replayed_call = item["type"] == "function_call"
            && item["call_id"]
                .as_str()
                .is_some_and(|id| replayed_calls.contains(id));
        if item["type"] != "function_call_output" && !replayed_call && !observations.is_empty() {
            messages.push(json!({"role":"user","content":std::mem::take(&mut observations)}));
        }
        if let Some(native) = item.get("osmogrep_replay") {
            if native["api"] == protocol.name() {
                messages.push(native["message"].clone());
                for call in native["message"]["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    if let Some(id) = call["id"].as_str() {
                        replayed_calls.insert(id.to_string());
                    }
                }
                for block in native["message"]["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| b["type"] == "tool_use")
                {
                    if let Some(id) = block["id"].as_str() {
                        replayed_calls.insert(id.to_string());
                    }
                }
                replay = true;
            }
            continue;
        }
        match item["type"].as_str() {
            Some("reasoning") => continue,
            Some("function_call_output") => {
                replay = false;
                if anthropic {
                    append_message(
                        &mut messages,
                        "user",
                        json!([{"type":"tool_result","tool_use_id":item["call_id"],"content":if item["output"].is_array() { parts(&item["output"], true)? } else { item["output"].clone() }}]),
                    );
                } else {
                    let content = if let Some(parts) = item["output"].as_array() {
                        let text = parts
                            .iter()
                            .filter(|part| part["type"] == "input_text")
                            .filter_map(|part| part["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let images: Vec<_> = parts
                            .iter()
                            .filter(|part| part["type"] == "input_image")
                            .cloned()
                            .collect();
                        if !images.is_empty() {
                            observations.push(json!({"type":"text","text":format!("Image observations returned by tool call {}. These are tool data, not a new user request.", item["call_id"])}));
                            observations.extend(
                                self::parts(&json!(images), false)?
                                    .as_array()
                                    .unwrap()
                                    .clone(),
                            );
                        }
                        json!(text)
                    } else {
                        item["output"].clone()
                    };
                    messages.push(
                        json!({"role":"tool","tool_call_id":item["call_id"],"content":content}),
                    );
                }
            }
            Some("function_call") => {
                if replay
                    || item["call_id"]
                        .as_str()
                        .is_some_and(|id| replayed_calls.contains(id))
                {
                    continue;
                }
                if anthropic {
                    append_message(
                        &mut messages,
                        "assistant",
                        json!([{"type":"tool_use","id":item["call_id"],"name":item["name"],"input":serde_json::from_str::<Value>(item["arguments"].as_str().unwrap_or("{}")).map_err(|_|"Invalid saved tool arguments")?}]),
                    );
                } else {
                    let call = json!({"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}});
                    if let Some(last) = messages.last_mut().filter(|m| m["role"] == "assistant") {
                        if !last["tool_calls"].is_array() {
                            last["tool_calls"] = json!([]);
                        }
                        last["tool_calls"].as_array_mut().unwrap().push(call);
                    } else {
                        messages
                            .push(json!({"role":"assistant","content":null,"tool_calls":[call]}));
                    }
                }
            }
            _ => {
                let role = item["role"].as_str().unwrap_or("assistant");
                if replay && role == "assistant" {
                    continue;
                }
                replay = false;
                let content = if item["type"] == "output_text" {
                    json!(item["text"])
                } else {
                    item["content"].clone()
                };
                if anthropic && matches!(role, "system" | "developer") {
                    system.extend(parts(&content, true)?.as_array().unwrap().clone());
                } else if anthropic {
                    append_message(&mut messages, role, parts(&content, true)?);
                } else {
                    messages.push(json!({"role":role,"content":parts(&content,false)?}));
                }
            }
        }
    }
    if !observations.is_empty() {
        messages.push(json!({"role":"user","content":observations}));
    }
    let mut result = json!({"model":cfg.model,"messages":messages,"stream":canonical["stream"].as_bool().unwrap_or(false)});
    if !anthropic && result["stream"] == true && cfg.provider != "mistral" {
        result["stream_options"] = json!({"include_usage":true});
    }
    if anthropic {
        if !system.is_empty() {
            result["system"] = json!(system);
        }
        result["max_tokens"] = canonical
            .get("max_output_tokens")
            .cloned()
            .unwrap_or(json!(8192));
        if let Some(budget) = canonical["thinking_budget"].as_u64() {
            if budget < 1024 || budget >= result["max_tokens"].as_u64().unwrap_or(8192) {
                return Err(
                    "Anthropic thinking_budget must be at least 1024 and below max_output_tokens"
                        .into(),
                );
            }
            result["thinking"] = json!({"type":"enabled","budget_tokens":budget});
        }
    } else if let Some(limit) = canonical.get("max_output_tokens") {
        result["max_tokens"] = limit.clone();
    }
    if !anthropic {
        if let Some(effort) = canonical.pointer("/reasoning/effort") {
            result["reasoning_effort"] = effort.clone();
        }
    }
    if let Some(tools) = canonical["tools"].as_array().filter(|a| !a.is_empty()) {
        result["tools"]=json!(tools.iter().map(|t|if anthropic {json!({"name":t["name"],"description":t["description"],"input_schema":t["parameters"]})}else{json!({"type":"function","function":{"name":t["name"],"description":t["description"],"parameters":t["parameters"]}})}).collect::<Vec<_>>());
        result["tool_choice"] = if anthropic {
            json!({"type":"auto"})
        } else {
            canonical["tool_choice"].clone()
        };
    }
    Ok(result)
}

fn normalized(cfg: &ModelConfig, raw: Value) -> Result<Value, String> {
    if let Some(error) = raw.get("error").filter(|e| !e.is_null()) {
        return Err(error["message"]
            .as_str()
            .unwrap_or("Provider returned an error")
            .into());
    }
    if api(cfg) == Api::Responses {
        if raw["status"].as_str().is_some_and(|s| s != "completed") {
            return Err(format!(
                "Model response {}: {}",
                raw["status"], raw["incomplete_details"]
            ));
        }
        validate_calls(&raw)?;
        return Ok(raw);
    }
    let anthropic = api(cfg) == Api::AnthropicMessages;
    let reason = if anthropic {
        raw["stop_reason"].as_str()
    } else {
        raw["choices"][0]["finish_reason"].as_str()
    }
    .ok_or("Model response missing stop reason")?;
    if !matches!(
        reason,
        "stop" | "tool_calls" | "end_turn" | "tool_use" | "stop_sequence"
    ) {
        return Err(format!(
            "Model stopped with {reason}; no partial tool calls were executed"
        ));
    }
    let message = if anthropic {
        let mut blocks = Vec::new();
        for block in raw["content"]
            .as_array()
            .ok_or("Missing Anthropic content")?
        {
            let fields: &[&str] = match block["type"].as_str() {
                Some("text") => &["type", "text", "citations"],
                Some("thinking") => &["type", "thinking", "signature"],
                Some("redacted_thinking") => &["type", "data"],
                Some("tool_use") => &["type", "id", "name", "input"],
                _ => return Err("Unsupported Anthropic response block".into()),
            };
            let clean = fields
                .iter()
                .filter_map(|key| {
                    block
                        .get(*key)
                        .filter(|v| !v.is_null())
                        .map(|v| (key.to_string(), v.clone()))
                })
                .collect::<serde_json::Map<String, Value>>();
            blocks.push(Value::Object(clean));
        }
        json!({"role":"assistant","content":blocks})
    } else {
        let native = &raw["choices"][0]["message"];
        let clean = [
            "role",
            "content",
            "tool_calls",
            "reasoning_content",
            "reasoning",
            "reasoning_details",
        ]
        .iter()
        .filter_map(|key| {
            native
                .get(*key)
                .filter(|v| !v.is_null())
                .map(|v| (key.to_string(), v.clone()))
        })
        .collect::<serde_json::Map<String, Value>>();
        Value::Object(clean)
    };
    let mut output = vec![
        json!({"type":"reasoning","osmogrep_replay":{"api":api(cfg).name(),"message":message}}),
    ];
    let mut text = String::new();
    if anthropic {
        for block in raw["content"]
            .as_array()
            .ok_or("Missing Anthropic content")?
        {
            if block["type"] == "text" {
                text.push_str(block["text"].as_str().unwrap_or(""));
            }
            if block["type"] == "tool_use" {
                output.push(json!({"type":"function_call","call_id":block["id"],"name":block["name"],"arguments":block["input"].to_string()}));
            }
        }
    } else {
        text = message["content"].as_str().unwrap_or("").into();
        if let Some(refusal) = raw["choices"][0]["message"]["refusal"].as_str() {
            text = refusal.into();
        }
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            output.push(json!({"type":"function_call","call_id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"]}));
        }
    }
    // Emit the preface before tools, while retaining the provider's exact assistant turn for replay.
    if !text.is_empty() {
        output.insert(1, json!({"type":"output_text","text":text}));
    }
    let mut usage = raw["usage"].clone();
    if anthropic && usage.is_object() {
        let cached = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
        let input = usage["input_tokens"]
            .as_u64()
            .unwrap_or(0)
            .saturating_add(cached)
            .saturating_add(usage["cache_creation_input_tokens"].as_u64().unwrap_or(0));
        usage["input_tokens"] = json!(input);
        usage["input_tokens_details"] = json!({"cached_tokens":cached});
        if let Some(out) = usage["output_tokens"].as_u64() {
            usage["total_tokens"] = json!(input.saturating_add(out));
        }
    }
    let value = json!({"output":output,"usage":usage});
    validate_calls(&value)?;
    Ok(value)
}

fn validate_calls(response: &Value) -> Result<(), String> {
    let mut ids = HashSet::new();
    for call in response["output"]
        .as_array()
        .ok_or("Model response missing output")?
        .iter()
        .filter(|c| c["type"] == "function_call")
    {
        let id = call["call_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Tool call missing ID")?;
        if !ids.insert(id) {
            return Err("Duplicate tool call ID".into());
        }
        if call["name"].as_str().is_none_or(str::is_empty) {
            return Err("Tool call missing name".into());
        }
        let args = call["arguments"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .filter(Value::is_object);
        if args.is_none() {
            return Err("Invalid or incomplete tool arguments; no calls were executed".into());
        }
    }
    Ok(())
}

struct StreamState {
    raw: Value,
    blocks: BTreeMap<u64, Value>,
    args: BTreeMap<u64, String>,
    finished: bool,
}
impl StreamState {
    fn new(protocol: Api) -> Self {
        Self {
            raw: if protocol == Api::ChatCompletions {
                json!({"choices":[{"message":{"role":"assistant","content":"","tool_calls":[]}}]})
            } else {
                json!({})
            },
            blocks: BTreeMap::new(),
            args: BTreeMap::new(),
            finished: false,
        }
    }
    fn event(&mut self, protocol: Api, event: Value) -> Result<Option<String>, String> {
        if event.get("error").is_some_and(|e| !e.is_null()) {
            return Err(event["error"]["message"]
                .as_str()
                .unwrap_or("Provider stream error")
                .into());
        }
        let mut delta = None;
        match protocol {
            Api::Responses => match event["type"].as_str() {
                Some("response.output_text.delta") => {
                    delta = event["delta"].as_str().map(str::to_string)
                }
                Some("response.completed") => {
                    self.raw = event["response"].clone();
                    self.finished = true;
                }
                Some("response.failed" | "response.incomplete") => {
                    return Err(format!(
                        "Model stream {}: {}",
                        event["type"], event["response"]["error"]
                    ))
                }
                _ => {}
            },
            Api::ChatCompletions => {
                if event["usage"].is_object() {
                    self.raw["usage"] = event["usage"].clone();
                } else if event["x_groq"]["usage"].is_object() {
                    self.raw["usage"] = event["x_groq"]["usage"].clone();
                }
                if let Some(choice) = event["choices"].as_array().and_then(|c| c.first()) {
                    if choice["finish_reason"].is_string() {
                        self.raw["choices"][0]["finish_reason"] = choice["finish_reason"].clone();
                    }
                    let d = &choice["delta"];
                    for field in ["content", "reasoning_content", "reasoning"] {
                        if let Some(s) = d[field].as_str() {
                            let mut full = self.raw["choices"][0]["message"][field]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            full.push_str(s);
                            self.raw["choices"][0]["message"][field] = json!(full);
                            if field == "content" {
                                delta = Some(s.into());
                            }
                        }
                    }
                    if d["reasoning_details"].is_array() {
                        let target = &mut self.raw["choices"][0]["message"]["reasoning_details"];
                        if !target.is_array() {
                            *target = json!([]);
                        }
                        target
                            .as_array_mut()
                            .unwrap()
                            .extend(d["reasoning_details"].as_array().unwrap().clone());
                    }
                    for call in d["tool_calls"].as_array().into_iter().flatten() {
                        let index = call["index"].as_u64().ok_or("Tool stream missing index")?;
                        let block=self.blocks.entry(index).or_insert(json!({"id":"","type":"function","function":{"name":"","arguments":""}}));
                        for field in ["id", "type", "extra_content"] {
                            if !call[field].is_null() {
                                block[field] = call[field].clone();
                            }
                        }
                        for field in ["name", "arguments"] {
                            if let Some(s) = call["function"][field].as_str() {
                                let mut full =
                                    block["function"][field].as_str().unwrap_or("").to_string();
                                full.push_str(s);
                                block["function"][field] = json!(full);
                            }
                        }
                    }
                }
            }
            Api::AnthropicMessages => match event["type"].as_str() {
                Some("message_start") => self.raw = event["message"].clone(),
                Some("content_block_start") => {
                    let i = event["index"].as_u64().ok_or("Missing content index")?;
                    self.blocks.insert(i, event["content_block"].clone());
                }
                Some("content_block_delta") => {
                    let i = event["index"].as_u64().ok_or("Missing content index")?;
                    let block = self
                        .blocks
                        .get_mut(&i)
                        .ok_or("Content delta without block")?;
                    let d = &event["delta"];
                    if let Some(s) = d["partial_json"].as_str() {
                        self.args.entry(i).or_default().push_str(s);
                    }
                    for field in ["text", "thinking", "signature"] {
                        if let Some(s) = d[field].as_str() {
                            let mut full = block[field].as_str().unwrap_or("").to_string();
                            full.push_str(s);
                            block[field] = json!(full);
                            if field == "text" {
                                delta = Some(s.into());
                            }
                        }
                    }
                }
                Some("message_delta") => {
                    self.raw["stop_reason"] = event["delta"]["stop_reason"].clone();
                    if let Some(u) = event["usage"].as_object() {
                        if !self.raw["usage"].is_object() {
                            self.raw["usage"] = json!({});
                        }
                        self.raw["usage"].as_object_mut().unwrap().extend(u.clone());
                    }
                }
                Some("message_stop") => self.finished = true,
                _ => {}
            },
        }
        Ok(delta)
    }
    fn finish(mut self, protocol: Api) -> Result<Value, String> {
        if !self.finished {
            return Err("Provider stream ended before its completion marker".into());
        }
        if protocol == Api::ChatCompletions {
            self.raw["choices"][0]["message"]["tool_calls"] =
                json!(self.blocks.into_values().collect::<Vec<_>>());
        } else if protocol == Api::AnthropicMessages {
            for (i, s) in self.args {
                self.blocks.get_mut(&i).ok_or("Missing tool block")?["input"] =
                    serde_json::from_str(&s).map_err(|_| "Incomplete streamed tool arguments")?;
            }
            self.raw["content"] = json!(self.blocks.into_values().collect::<Vec<_>>());
        }
        Ok(self.raw)
    }
}

#[derive(Debug)]
pub struct Failure {
    pub message: String,
    pub retryable: bool,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            retryable: false,
        }
    }
}
pub fn request(
    cfg: &ModelConfig,
    key: &str,
    canonical: &Value,
    cancel: &CancelToken,
    tx: &Sender<AgentEvent>,
) -> Result<(Value, bool), Failure> {
    let redaction_key = key.to_string();
    let url = endpoint(cfg)?;
    let body = payload(cfg, canonical)?;
    let cfg = cfg.clone();
    let key = key.to_string();
    let protocol = api(&cfg);
    let (send, recv) = mpsc::channel();
    let worker_cancel = cancel.clone();
    crate::mcp::runtime().spawn(async move {
        let fetch=async {
            let client=reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(120))
                .redirect(reqwest::redirect::Policy::none()).build().map_err(|e|Failure::from(e.to_string()))?;
            let request=client.post(url).json(&body);
            let request=if protocol==Api::AnthropicMessages {request.header("x-api-key",&key).header("anthropic-version","2023-06-01")}
                else if key.is_empty(){request}else{request.bearer_auth(&key)};
            let mut response=request.send().await.map_err(|e|Failure {message:format!("Provider connection failed: {}",e.without_url()),retryable:true})?;
            let status=response.status();
            if !status.is_success() {
                let bytes=read_body(response,64*1024).await?;
                return Err(Failure {message:super::format_api_error(&status.to_string(),&String::from_utf8_lossy(&bytes)),retryable:status.as_u16()==429 || status.is_server_error()});
            }
            if !response.headers().get("content-type").and_then(|v|v.to_str().ok()).is_some_and(|s|s.contains("text/event-stream")) {
                let bytes=read_body(response,32*1024*1024).await?;
                return normalized(&cfg,serde_json::from_slice(&bytes).map_err(|e|Failure::from(e.to_string()))?).map_err(Failure::from);
            }
            let mut state=StreamState::new(protocol);
            let mut data=String::new(); let mut line=Vec::new(); let mut total=0usize; let mut closed=false;
            'stream: while let Some(chunk)=response.chunk().await.map_err(|e|Failure::from(e.without_url().to_string()))? {
                total+=chunk.len(); if total>32*1024*1024 {return Err("Provider stream too large".to_string().into());}
                for byte in chunk {
                    if byte!=b'\n' {line.push(byte);if line.len()>1024*1024 {return Err("Provider event too large".to_string().into());}continue;}
                    let text=std::str::from_utf8(&line).map_err(|_|Failure::from("Invalid UTF-8 in provider stream".to_string()))?.trim_end_matches('\r');
                    if text.is_empty() {
                        if !data.is_empty() {
                            if data.trim()=="[DONE]" {if protocol==Api::ChatCompletions {state.finished=true;}closed=true;}
                            else {
                                let event=serde_json::from_str(&data).map_err(|_|Failure::from("Malformed provider stream event".to_string()))?;
                                if let Some(delta)=state.event(protocol,event).map_err(Failure::from)? {if !delta.is_empty() && send.send(Ok(Err(delta))).is_err(){return Err("cancelled".to_string().into());}}
                            }
                            data.clear();
                        }
                        if closed || state.finished && protocol!=Api::ChatCompletions {break 'stream;}
                    }else if let Some(text)=text.strip_prefix("data:") {
                        if !data.is_empty(){data.push('\n');}data.push_str(text.strip_prefix(' ').unwrap_or(text));
                        if data.len()>1024*1024 {return Err("Provider event too large".to_string().into());}
                    }
                    line.clear();
                }
            }
            normalized(&cfg,state.finish(protocol).map_err(Failure::from)?).map_err(Failure::from)
        };
        // Dropping the in-flight HTTP future closes the socket on cancellation.
        let result=tokio::select! {
            result=fetch=>result,
            _=async {while !worker_cancel.is_cancelled(){tokio::time::sleep(Duration::from_millis(20)).await;}}=>Err("cancelled".to_string().into()),
        };
        let _=send.send(result.map(Ok));
    });
    let mut streamed = false;
    loop {
        if cancel.is_cancelled() {
            return Err("cancelled".to_string().into());
        }
        match recv.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(Err(delta))) => {
                streamed = true;
                let _ = tx.send(AgentEvent::StreamDelta(delta));
            }
            Ok(Ok(Ok(value))) => {
                let _ = tx.send(AgentEvent::StreamDone);
                return Ok((value, streamed));
            }
            Ok(Err(mut error)) => {
                if !redaction_key.is_empty() {
                    error.message = error.message.replace(&redaction_key, "[redacted]");
                }
                error.retryable &= !streamed;
                let _ = tx.send(AgentEvent::StreamDone);
                return Err(error);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("Provider worker stopped unexpectedly".to_string().into()),
        }
    }
}

async fn read_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Failure::from(e.without_url().to_string()))?
    {
        if bytes.len() + chunk.len() > limit {
            return Err("Provider response too large".to_string().into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
