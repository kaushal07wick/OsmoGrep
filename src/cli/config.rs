use crate::agent::{provider::Api, ModelConfig};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use std::{error::Error, path::Path};

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Show model and connection settings; credentials are never printed
    Show,
    /// Print the global configuration path
    Path,
    /// Read one supported setting
    Get {
        #[arg(value_enum)]
        key: Key,
    },
    /// Change a setting for future processes; existing sessions retain their model
    Set {
        #[arg(value_enum)]
        key: Key,
        value: String,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Key {
    Provider,
    Model,
    BaseUrl,
    Api,
    Reasoning,
}

pub(crate) fn read(path: &Path) -> Result<toml::Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse().map_err(|_| {
            "Configuration is invalid TOML; the original file has been preserved.".into()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(toml::Value::Table(Default::default()))
        }
        Err(error) => Err(error.to_string()),
    }
}
fn model(document: &toml::Value) -> Result<ModelConfig, String> {
    document
        .get("model")
        .cloned()
        .map(|value| {
            value
                .try_into()
                .map_err(|_| "The [model] configuration is invalid".into())
        })
        .unwrap_or_else(|| Ok(ModelConfig::default()))
}
pub(crate) fn endpoint(value: &str) -> Result<(), String> {
    let url =
        reqwest::Url::parse(value).map_err(|_| "Use an http:// or https:// model endpoint")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Use an HTTP(S) endpoint without embedded credentials, query parameters or fragments"
                .into(),
        );
    }
    Ok(())
}
fn set(document: &mut toml::Value, key: Key, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return Err("Setting must be nonempty text without control characters".into());
    }
    let mut selected = model(document)?;
    match key {
        Key::Provider => {
            if !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            {
                return Err("Provider names use letters, digits, '-' or '_'".into());
            }
            if selected.provider != value {
                if let Some(legacy) = document.get("api_key").cloned() {
                    if !legacy.is_str() {
                        return Err(
                            "Legacy api_key must be a string; configuration was preserved".into(),
                        );
                    }
                    let table = document
                        .as_table_mut()
                        .ok_or("Configuration must be a table")?;
                    let keys = table
                        .entry("api_keys")
                        .or_insert_with(|| toml::Value::Table(Default::default()))
                        .as_table_mut()
                        .ok_or("api_keys must be a table")?;
                    keys.entry(selected.provider.clone()).or_insert(legacy);
                    table.remove("api_key");
                }
                selected.base_url = None;
                selected.api = None;
                selected.api_key_env = Some(crate::agent::provider::defaults(value).1.into());
            }
            selected.provider = value.into();
        }
        Key::Model => {
            if value.len() > 160 || value.chars().any(char::is_whitespace) {
                return Err("Use the exact model ID without whitespace (at most 160 bytes)".into());
            }
            selected.model = value.into();
        }
        Key::BaseUrl => {
            if value == "default" {
                selected.base_url = None;
            } else {
                endpoint(value)?;
                selected.base_url = Some(value.into());
            }
        }
        Key::Api => selected.api = Api::parse(value)?,
        Key::Reasoning => {
            if !matches!(
                value,
                "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
            ) {
                return Err("Use auto, none, minimal, low, medium, high or xhigh; support depends on the model".into());
            }
            let key = format!("{}/{}", selected.provider, selected.model);
            let table = document
                .as_table_mut()
                .ok_or("Configuration must be a table")?;
            let options = table
                .entry("model_options")
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()
                .ok_or("model_options must be a table")?;
            options
                .entry(key)
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()
                .ok_or("Model options must be a table")?
                .insert("reasoning".into(), toml::Value::String(value.into()));
            return Ok(());
        }
    }
    document
        .as_table_mut()
        .ok_or("Configuration must be a table")?
        .insert(
            "model".into(),
            toml::Value::try_from(selected).map_err(|error| error.to_string())?,
        );
    Ok(())
}
fn safe_view(document: &toml::Value) -> Result<serde_json::Value, String> {
    let mut selected = model(document)?;
    if selected
        .base_url
        .as_deref()
        .is_some_and(|value| endpoint(value).is_err())
    {
        selected.base_url = Some("[redacted: invalid or credential-bearing endpoint]".into());
    }
    let reasoning = document
        .get("model_options")
        .and_then(|options| options.get(format!("{}/{}", selected.provider, selected.model)))
        .and_then(|options| options.get("reasoning"))
        .and_then(toml::Value::as_str)
        .unwrap_or("auto");
    let reasoning = if matches!(
        reasoning,
        "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
    ) {
        reasoning
    } else {
        "[invalid setting]"
    };
    Ok(
        serde_json::json!({"provider":selected.provider,"model":selected.model,"base-url":selected.base_url,"api":selected.api.map(Api::name).unwrap_or("auto"),"reasoning":reasoning}),
    )
}
pub fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let path = crate::persistence::config_dir().join("config.toml");
    if matches!(args.command, Some(Command::Path)) {
        println!("{}", path.display());
        return Ok(());
    }
    if let Some(Command::Set { key, value }) = args.command {
        crate::storage::update_toml(&path, |document| set(document, key, &value))?;
        println!("Saved. New OsmoGrep processes will use this setting; environment overrides still take precedence.");
        return Ok(());
    }
    let view = safe_view(&read(&path)?)?;
    match args.command {
        Some(Command::Get { key }) => {
            let name = match key {
                Key::Provider => "provider",
                Key::Model => "model",
                Key::BaseUrl => "base-url",
                Key::Api => "api",
                Key::Reasoning => "reasoning",
            };
            println!("{}", super::clean(view[name].as_str().unwrap_or("default")));
        }
        _ => println!("{}", serde_json::to_string_pretty(&view)?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_preserve_unowned_fields_validate_connections_and_hide_credentials() {
        let mut document: toml::Value = "api_key='NEVER_PRINT_ME'\n[mcp]\nenabled=true\n[model]\nprovider='openai'\nmodel='test'\nbase_url='https://user:password@host/v1?key=SECRET'".parse().unwrap();
        let view = safe_view(&document).unwrap().to_string();
        for secret in ["NEVER_PRINT_ME", "password", "SECRET"] {
            assert!(!view.contains(secret));
        }
        set(&mut document, Key::Provider, "ollama").unwrap();
        assert!(document["mcp"]["enabled"].as_bool().unwrap());
        assert!(model(&document).unwrap().base_url.is_none());
        assert!(set(&mut document, Key::BaseUrl, "https://user:secret@host/v1").is_err());
        assert!(set(&mut document, Key::Model, "bad model").is_err());
        set(&mut document, Key::BaseUrl, "http://127.0.0.1:11434/v1").unwrap();
        set(&mut document, Key::Model, "qwen3.5:4b").unwrap();
        set(&mut document, Key::Reasoning, "high").unwrap();
        assert_eq!(safe_view(&document).unwrap()["reasoning"], "high");
        assert!(document.get("api_key").is_none());
        assert_eq!(
            document["api_keys"]["openai"].as_str(),
            Some("NEVER_PRINT_ME")
        );
    }
}
