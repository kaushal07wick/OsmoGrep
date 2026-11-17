use clap::Args as ClapArgs;
use std::{error::Error, time::Duration};
#[derive(ClapArgs)]
pub struct Args {
    /// Query another provider without changing your saved model
    #[arg(long)]
    provider: Option<String>,
    /// OpenAI-compatible catalog endpoint, including /v1 where required
    #[arg(long)]
    base_url: Option<String>,
    /// Show the selected model, API and configured reasoning settings without contacting a provider
    #[arg(long, conflicts_with_all=["provider","base_url"])]
    current: bool,
    /// Print machine-readable model information
    #[arg(long)]
    json: bool,
}
pub fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let agent = crate::agent::Agent::new();
    let mut selected = agent.model_config().clone();
    if args.current {
        let configured = safe_options(&crate::model_options::get(&selected));
        let value = serde_json::json!({"provider":selected.provider,"model":selected.model,"api":crate::agent::provider::api(&selected).name(),"configured_options":configured,"limits":{
            "context_window":crate::model_options::limits(&selected).window,"output_tokens":crate::model_options::limits(&selected).output
        }});
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    if let Some(provider) = args.provider {
        if provider != selected.provider {
            selected.base_url = None;
            selected.api = None;
            selected.api_key_env = Some(crate::agent::provider::defaults(&provider).1.into());
        }
        selected.provider = provider;
    }
    if let Some(endpoint) = args.base_url {
        super::config::endpoint(&endpoint)?;
        selected.base_url = Some(endpoint);
    }
    let models = agent
        .model_catalog(selected.clone())
        .recv_timeout(Duration::from_secs(8))
        .map_err(|_| "Model discovery timed out")??;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"provider":selected.provider,"models":models})
            )?
        );
    } else {
        println!(
            "{} · {} models",
            super::clean(&selected.provider),
            models.len()
        );
        for id in models {
            println!("{} {}", if id == selected.model { "●" } else { " " }, id);
        }
        println!("Reasoning support depends on the model. Use `models --current` to inspect configured options.");
    }
    Ok(())
}
fn safe_options(options: &serde_json::Value) -> serde_json::Value {
    let mut visible = serde_json::Map::new();
    if let Some(options) = options.as_object() {
        for (key, value) in options {
            let valid = match key.as_str() {
                "reasoning" => value.as_str().is_some_and(|effort| {
                    matches!(
                        effort,
                        "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
                    )
                }),
                "tools" | "vision" => value.is_boolean(),
                "thinking_budget" | "context_window" | "max_output_tokens" => value.is_u64(),
                _ => false,
            };
            if valid {
                visible.insert(key.clone(), value.clone());
            }
        }
    }
    serde_json::Value::Object(visible)
}
#[cfg(test)]
mod tests {
    #[test]
    fn model_details_show_only_typed_public_options() {
        let options = serde_json::json!({"api_key":"SECRET","reasoning":"high","tools":true,"context_window":32000,"thinking_budget":"SECRET"});
        let public = super::safe_options(&options);
        assert_eq!(public["reasoning"], "high");
        assert_eq!(public["context_window"], 32000);
        assert!(!public.to_string().contains("SECRET"));
    }
}
