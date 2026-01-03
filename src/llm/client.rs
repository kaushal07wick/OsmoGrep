use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::prompt::LlmPrompt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Provider {
    OpenAI,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Clone)]
pub struct LlmClient {
    cfg: Arc<Mutex<ProviderConfig>>,
}

impl LlmClient {
    pub fn new() -> Self {
        let cfg = load_config().unwrap_or_else(default_config);
        Self { cfg: Arc::new(Mutex::new(cfg)) }
    }

    pub fn configure(
        &self,
        provider_name: &str,
        model: String,
        api_key: String,
        base_url: Option<String>,
    ) -> Result<(), String> {
        if api_key.trim().is_empty() {
            return Err("API key cannot be empty".into());
        }

        let provider = match provider_name {
            "openai" => Provider::OpenAI,
            "anthropic" => Provider::Anthropic,
            _ => return Err("Unknown provider".into()),
        };

        let mut guard = self.cfg.lock().map_err(|_| "Config lock poisoned")?;
        *guard = ProviderConfig {
            provider,
            model,
            api_key,
            base_url,
        };

        save_config(&guard).map_err(|e| e.to_string())
    }
    
    pub fn current_config(&self) -> ProviderConfig {
        self.cfg
            .lock()
            .expect("llm config lock poisoned")
            .clone()
    }

    pub fn run(&self, prompt: LlmPrompt) -> Result<String, String> {
        let cfg = {
            let guard = self.cfg.lock().map_err(|_| "Config lock poisoned")?;
            guard.clone()
        };

        let (url, headers, body) = build_request(&cfg, &prompt);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;

        let mut req = client.post(url).json(&body);
        for (k, v) in headers {
            req = req.header(k, v);
        }

        let resp = req.send().map_err(|e| e.to_string())?;
        let status = resp.status();
        let json: Value = resp.json().map_err(|e| e.to_string())?;

        if !status.is_success() {
            return Err(format!("LLM error {}: {}", status, json));
        }

        extract_text(&cfg.provider, &json)
    }
}

fn build_request(
    cfg: &ProviderConfig,
    prompt: &LlmPrompt,
) -> (String, Vec<(&'static str, String)>, Value) {
    match cfg.provider {
        Provider::OpenAI => {
            let url = cfg.base_url.clone()
                .unwrap_or_else(|| "https://api.openai.com/v1/responses".into());

            let body = serde_json::json!({
                "model": cfg.model,
                "instructions": prompt.system,
                "input": prompt.user,
            });

            (
                url,
                vec![
                    ("Authorization", format!("Bearer {}", cfg.api_key)),
                    ("Content-Type", "application/json".into()),
                ],
                body,
            )
        }

        Provider::Anthropic => {
            let url = cfg.base_url.clone()
                .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".into());

            let body = serde_json::json!({
                "model": cfg.model,
                "max_tokens": 1024,
                "system": prompt.system,
                "messages": [{ "role": "user", "content": prompt.user }]
            });

            (
                url,
                vec![
                    ("x-api-key", cfg.api_key.clone()),
                    ("anthropic-version", "2023-06-01".into()),
                    ("Content-Type", "application/json".into()),
                ],
                body,
            )
        }
    }
}

fn extract_text(provider: &Provider, v: &Value) -> Result<String, String> {
    match provider {
        Provider::OpenAI => {
            v.get("output")
                .and_then(|o| o.as_array())
                .and_then(|arr| {
                    arr.iter().find_map(|item| {
                        item.get("content")?
                            .as_array()?
                            .iter()
                            .find_map(|c| c.get("text")?.as_str())
                    })
                })
                .map(str::to_owned)
                .ok_or("OpenAI response parse failure".into())
        }

        Provider::Anthropic => {
            v.pointer("/content/0/text")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or("Anthropic response parse failure".into())
        }
    }
}

fn default_config() -> ProviderConfig {
    ProviderConfig {
        provider: Provider::OpenAI,
        model: "gpt-5.2".to_string(),
        api_key: String::new(),
        base_url: None,
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("osmogrep/llm.json")
}

fn load_config() -> Option<ProviderConfig> {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn save_config(cfg: &ProviderConfig) -> std::io::Result<()> {
    let path = config_path();
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(path, serde_json::to_string_pretty(cfg).unwrap())
}
