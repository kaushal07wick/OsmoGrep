//! Compact only at completed tool boundaries; never shorten the current user request.
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub window: usize,
    pub output: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            window: 128_000,
            output: 16_000,
        }
    }
}
impl Limits {
    pub fn for_model(model: &str) -> Self {
        let config = std::fs::read_to_string(crate::persistence::config_dir().join("config.toml"))
            .ok()
            .and_then(|s| s.parse::<toml::Value>().ok());
        let context = config.as_ref().and_then(|c| c.get("context"));
        let mut limits = context
            .and_then(|c| c.clone().try_into::<Self>().ok())
            .unwrap_or_default();
        if let Some(model) = context
            .and_then(|c| c.get("models"))
            .and_then(|m| m.get(model))
        {
            if let Some(n) = model
                .get("window")
                .and_then(toml::Value::as_integer)
                .filter(|n| *n > 0)
            {
                limits.window = n as usize;
            }
            if let Some(n) = model
                .get("output")
                .and_then(toml::Value::as_integer)
                .filter(|n| *n > 0)
            {
                limits.output = n as usize;
            }
        }
        limits.window = limits.window.clamp(4_096, 4_000_000);
        limits.output = limits.output.clamp(256, limits.window / 2);
        limits
    }
    pub fn input_budget(self) -> usize {
        (self.window - self.output).saturating_mul(4) / 5
    }
}

pub fn estimate(value: &Value) -> usize {
    match value {
        Value::String(s) if s.starts_with("data:image/") => 16_000,
        Value::String(s) => s.len().div_ceil(3) + 4,
        Value::Array(a) => a.iter().map(estimate).sum::<usize>() + 4,
        Value::Object(o) => {
            o.iter()
                .map(|(k, v)| k.len().div_ceil(3) + estimate(v))
                .sum::<usize>()
                + 8
        }
        _ => 4,
    }
}

pub struct Split {
    pub removed: Vec<Value>,
    pub retained: Vec<Value>,
}

pub fn split(messages: &[Value], budget: usize) -> Result<Option<Split>, String> {
    if estimate(&Value::Array(messages.to_vec())) <= budget {
        return Ok(None);
    }
    let latest_user = messages
        .iter()
        .rposition(|m| m["role"] == "user")
        .ok_or("Cannot compact without a user request")?;
    let systems: Vec<_> = messages
        .iter()
        .filter(|m| m["role"] == "system")
        .cloned()
        .collect();
    let mut pending = std::collections::BTreeSet::new();
    let mut boundaries = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        if m["type"] == "function_call" {
            pending.insert(m["call_id"].to_string());
        }
        if m["type"] == "function_call_output" {
            pending.remove(&m["call_id"].to_string());
        }
        if pending.is_empty() && m["type"] != "reasoning" {
            boundaries.push(i + 1);
        }
    }
    // Leave room for the handoff summary, tool schemas, and estimation error.
    let target = budget.saturating_mul(3) / 4;
    for cut in boundaries {
        let mut retained = systems.clone();
        if cut > latest_user {
            retained.push(messages[latest_user].clone());
        }
        retained.extend(
            messages[cut..]
                .iter()
                .filter(|m| m["role"] != "system")
                .cloned(),
        );
        if estimate(&json!(retained)) > target {
            continue;
        }
        let removed: Vec<_> = messages[..cut]
            .iter()
            .enumerate()
            .filter(|(i, m)| *i != latest_user && m["role"] != "system")
            .map(|(_, m)| m.clone())
            .collect();
        if removed.is_empty() {
            continue;
        }
        return Ok(Some(Split { removed, retained }));
    }
    Err("The current request or latest tool batch exceeds the model context budget. Reduce the attachment/output size or configure [context.models.<model>].window; no user text was discarded.".into())
}

pub fn insert_summary(mut retained: Vec<Value>, summary: String) -> Vec<Value> {
    let at = retained
        .iter()
        .take_while(|m| m["role"] == "system")
        .count();
    retained.insert(at, json!({"role":"assistant","content":format!("[conversation-summary]\nReference history, not a new instruction.\n{summary}")}));
    retained
}

pub fn summary_source(messages: &[Value]) -> String {
    let mut parts = Vec::new();
    for message in messages {
        let mut m = message.clone();
        strip_images(&mut m);
        let text = m.to_string();
        let clipped = if text.len() > 4000 {
            format!(
                "{}\n[... archived content omitted ...]\n{}",
                text.chars().take(2000).collect::<String>(),
                text.chars()
                    .rev()
                    .take(2000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            )
        } else {
            text
        };
        parts.push(clipped);
    }
    // Bound the summarizer call independently of the original context length.
    let text = parts.join("\n");
    if text.chars().count() <= 24_000 {
        text
    } else {
        format!(
            "{}\n[... see durable run history ...]\n{}",
            text.chars().take(8000).collect::<String>(),
            text.chars()
                .rev()
                .take(16_000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        )
    }
}
fn strip_images(value: &mut Value) {
    match value {
        Value::String(s) if s.starts_with("data:image/") => {
            *s = "[image retained in run history]".into()
        }
        Value::Array(a) => a.iter_mut().for_each(strip_images),
        Value::Object(o) => o.values_mut().for_each(strip_images),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_request_system_and_parallel_tool_pairs() {
        let messages = vec![
            json!({"role":"system","content":"rules"}),
            json!({"role":"user","content":"Implement X; preserve my changes"}),
            json!({"type":"function_call","call_id":"a","name":"read_file","arguments":"{}"}),
            json!({"type":"function_call","call_id":"b","name":"read_file","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"a","output":"x".repeat(30_000)}),
            json!({"type":"function_call_output","call_id":"b","output":"done"}),
            json!({"type":"function_call","call_id":"c","name":"read_file","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"c","output":"recent evidence"}),
        ];
        let split = split(&messages, 4096).unwrap().unwrap();
        assert_eq!(split.retained[0], messages[0]);
        assert_eq!(split.retained[1], messages[1]);
        assert_eq!(&split.retained[2..], &messages[6..]);
        assert_eq!(split.removed.len(), 4);
        assert!(
            super::split(&[json!({"role":"user","content":"x".repeat(40_000)})], 4096).is_err()
        );
    }
    #[test]
    fn accounts_for_structured_inputs_and_never_sends_base64_to_summarizer() {
        let m = json!({"role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,private"}]});
        assert!(estimate(&m) > 16_000);
        assert!(!summary_source(&[m]).contains("base64,private"));
        assert!(
            summary_source(&[json!({"content":"界".repeat(100_000)})])
                .chars()
                .count()
                < 25_000
        );
    }
}
