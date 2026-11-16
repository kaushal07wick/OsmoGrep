use crate::agent::ModelConfig;
use serde_json::{json, Value};

pub fn get(cfg: &ModelConfig) -> Value {
    std::fs::read_to_string(crate::persistence::config_dir().join("config.toml"))
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok())
        .and_then(|c| {
            c.get("model_options")
                .and_then(|m| m.get(format!("{}/{}", cfg.provider, cfg.model)))
                .cloned()
        })
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or_else(|| json!({}))
}
pub fn limits(cfg: &ModelConfig) -> crate::compaction::Limits {
    let mut limits = crate::compaction::Limits::for_model(&cfg.model);
    let options = get(cfg);
    if let Some(n) = options["context_window"].as_u64() {
        limits.window = (n as usize).clamp(4096, 4_000_000);
    }
    if let Some(n) = options["max_output_tokens"].as_u64() {
        limits.output = (n as usize).clamp(256, limits.window / 2);
    }
    limits.output = limits.output.min(limits.window / 2);
    limits
}
pub fn apply(cfg: &ModelConfig, canonical: &Value) -> Result<Value, String> {
    apply_options(canonical, &get(cfg))
}
fn apply_options(canonical: &Value, options: &Value) -> Result<Value, String> {
    if options["vision"] == false && canonical["input"].to_string().contains("\"input_image\"") {
        return Err("This model is configured without image input. Choose a vision model or start a text-only session.".into());
    }
    let mut payload = canonical.clone();
    if payload
        .as_object_mut()
        .and_then(|m| m.remove("_osmogrep_compaction"))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        payload.as_object_mut().unwrap().remove("reasoning");
        if options["reasoning"] == "none" {
            payload["reasoning"] = json!({"effort":"none"});
        }
        return Ok(payload);
    }
    if options["tools"] == false {
        payload["tools"] = json!([]);
        payload["tool_choice"] = json!("none");
    }
    if let Some(effort) = options["reasoning"].as_str() {
        match effort {
            "auto" => {}
            "none" | "minimal" | "low" | "medium" | "high" | "xhigh" => {
                payload["reasoning"] = json!({"effort":effort})
            }
            _ => return Err("Invalid model reasoning setting".into()),
        }
    }
    if let Some(tokens) = options["thinking_budget"].as_u64() {
        payload["thinking_budget"] = json!(tokens);
    }
    Ok(payload)
}
pub fn set_reasoning(cfg: &ModelConfig, effort: &str) -> Result<(), String> {
    if !matches!(
        effort,
        "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
    ) {
        return Err("Use auto, none, minimal, low, medium, high, or xhigh; the selected model must support that effort.".into());
    }
    crate::persistence::update_config(|config| {
        if config.get("model_options").is_none() {
            config["model_options"] = toml::Value::Table(Default::default());
        }
        let key = format!("{}/{}", cfg.provider, cfg.model);
        if config["model_options"].get(&key).is_none() {
            config["model_options"][&key] = toml::Value::Table(Default::default());
        }
        config["model_options"][&key]["reasoning"] = toml::Value::String(effort.into());
        Ok(())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compaction_preserves_explicit_thinking_off_without_enabling_expensive_options() {
        let original = json!({"_osmogrep_compaction":true,"max_output_tokens":512,"input":[]});
        let payload = apply_options(&original, &json!({"reasoning":"none"})).unwrap();
        assert_eq!(payload["reasoning"]["effort"], "none");
        assert!(payload.get("_osmogrep_compaction").is_none());
        assert_eq!(payload["max_output_tokens"], 512);
        let payload = apply_options(
            &original,
            &json!({"reasoning":"high","thinking_budget":8192}),
        )
        .unwrap();
        assert!(payload.get("reasoning").is_none());
        assert!(payload.get("thinking_budget").is_none());
    }
    #[test]
    fn honors_declared_capabilities_and_rejects_unsupported_images() {
        let original =
            json!({"tools":[{"name":"read_file"}],"input":[{"role":"user","content":"hi"}]});
        let payload = apply_options(&original, &json!({"tools":false,"reasoning":"low"})).unwrap();
        assert!(payload["tools"].as_array().unwrap().is_empty());
        assert_eq!(payload["reasoning"]["effort"], "low");
        assert_eq!(original["tools"].as_array().unwrap().len(), 1);
        assert!(apply_options(
            &json!({"input":[{"content":[{"type":"input_image"}]}]}),
            &json!({"vision":false})
        )
        .is_err());
        assert!(apply_options(&original, &json!({"reasoning":"made-up"})).is_err());
    }
}
