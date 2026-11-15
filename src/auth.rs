//! Standard device authorization for user-registered OAuth clients; no borrowed vendor client IDs.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize, Clone)]
struct Config {
    client_id: String,
    device_url: String,
    token_url: String,
    #[serde(default)]
    scopes: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct Token {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: u64,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn config(provider: &str) -> Result<Config, String> {
    let text = std::fs::read_to_string(crate::persistence::config_dir().join("config.toml"))
        .map_err(|e| e.to_string())?;
    let value: toml::Value = text.parse::<toml::Value>().map_err(|e| e.to_string())?;
    let cfg:Config=value.get("auth").and_then(|a|a.get(provider)).ok_or("Configure [auth.<provider>] with your registered client_id, device_url, token_url, and scopes first")?.clone().try_into().map_err(|e:toml::de::Error|e.to_string())?;
    endpoint(&cfg.device_url)?;
    endpoint(&cfg.token_url)?;
    Ok(cfg)
}
fn endpoint(url: &str) -> Result<(), String> {
    let url = url::Url::parse(url).map_err(|_| "Invalid OAuth endpoint")?;
    if (url.scheme() != "https"
        && !(url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("OAuth endpoints must use HTTPS without embedded credentials (loopback HTTP is allowed)".into());
    }
    Ok(())
}
fn post(url: &str, form: &BTreeMap<&str, String>) -> Result<Value, String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?
        .post(url)
        .form(form)
        .send()
        .map_err(|_| "OAuth endpoint could not be reached")?;
    let status = response.status();
    use std::io::Read;
    let mut bytes = Vec::new();
    response
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "OAuth response could not be read")?;
    if bytes.len() > 128 * 1024 {
        return Err("OAuth response exceeds 128 KiB".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "OAuth endpoint returned invalid JSON")?;
    if !status.is_success() && !value["error"].is_string() {
        return Err(format!("OAuth endpoint returned HTTP {status}"));
    }
    Ok(value)
}
fn token(value: &Value, previous: Option<String>) -> Result<Token, String> {
    let access = value["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("OAuth response has no access token")?;
    Ok(Token {
        access_token: access.into(),
        refresh_token: value["refresh_token"]
            .as_str()
            .map(str::to_string)
            .or(previous),
        expires_at: now().saturating_add(value["expires_in"].as_u64().unwrap_or(3600)),
    })
}
pub fn login(provider: &str) -> Result<(), String> {
    let token = authorize(config(provider)?, std::thread::sleep)?;
    crate::credentials::set(
        &format!("oauth/{provider}"),
        &serde_json::to_string(&token).map_err(|e| e.to_string())?,
    )?;
    println!("Signed in to {provider}. Credentials saved in the OS credential store.");
    Ok(())
}
fn authorize(cfg: Config, wait: impl Fn(Duration)) -> Result<Token, String> {
    let mut form = BTreeMap::from([
        ("client_id", cfg.client_id.clone()),
        ("scope", cfg.scopes.join(" ")),
    ]);
    let device = post(&cfg.device_url, &form)?;
    let code = device["device_code"]
        .as_str()
        .ok_or("OAuth server did not return a device code")?;
    let uri = device["verification_uri"]
        .as_str()
        .ok_or("OAuth server did not return a verification URI")?;
    endpoint(uri)?;
    let user_code = device["user_code"]
        .as_str()
        .ok_or("OAuth server did not return a user code")?;
    println!(
        "Open {uri}\nEnter code: {user_code}\nWaiting for authorization. Ctrl+C cancels login."
    );
    form = BTreeMap::from([
        ("client_id", cfg.client_id),
        ("device_code", code.into()),
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code".into(),
        ),
    ]);
    let mut interval = device["interval"].as_u64().unwrap_or(5).clamp(1, 30);
    let deadline = now().saturating_add(device["expires_in"].as_u64().unwrap_or(600).min(1800));
    while now() < deadline {
        wait(Duration::from_secs(interval));
        let value = post(&cfg.token_url, &form)?;
        match value["error"].as_str() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                interval = (interval + 5).min(60);
                continue;
            }
            Some("access_denied") => return Err("Authorization was denied".into()),
            Some(_) => {
                return Err("Authorization expired or was rejected; start login again".into())
            }
            None => return token(&value, None),
        }
    }
    Err("Device authorization expired".into())
}
pub fn has_token(provider: &str) -> bool {
    crate::credentials::get(&format!("oauth/{provider}"))
        .ok()
        .flatten()
        .is_some()
}
pub fn access_token(provider: &str) -> Result<Option<String>, String> {
    let Some(raw) = crate::credentials::get(&format!("oauth/{provider}"))? else {
        return Ok(None);
    };
    let mut saved: Token = serde_json::from_str(&raw)
        .map_err(|_| "Stored OAuth credential is invalid; sign in again")?;
    if saved.expires_at <= now() + 60 {
        // Serialize refresh across processes so rotating refresh tokens cannot race.
        let path = crate::persistence::config_dir().join(format!(
            "oauth-{}.lock",
            crate::file_guard::digest(provider.as_bytes())
        ));
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "Another OsmoGrep process is refreshing this login; retry shortly")?;
        saved = serde_json::from_str(
            &crate::credentials::get(&format!("oauth/{provider}"))?.ok_or("Login was removed")?,
        )
        .map_err(|_| "Stored OAuth credential is invalid")?;
        if saved.expires_at <= now() + 60 {
            let cfg = config(provider)?;
            let refresh = saved
                .refresh_token
                .ok_or("OAuth login expired; sign in again")?;
            let value = post(
                &cfg.token_url,
                &BTreeMap::from([
                    ("client_id", cfg.client_id),
                    ("grant_type", "refresh_token".into()),
                    ("refresh_token", refresh.clone()),
                ]),
            )?;
            saved = token(&value, Some(refresh))?;
            crate::credentials::set(
                &format!("oauth/{provider}"),
                &serde_json::to_string(&saved).map_err(|e| e.to_string())?,
            )?;
        }
    }
    Ok(Some(saved.access_token))
}
pub fn logout(provider: &str) -> Result<(), String> {
    crate::credentials::delete(&format!("oauth/{provider}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_authorization_polls_pending_then_refreshes_through_http() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let verification = format!("{base}/verify");
        let server = std::thread::spawn(move || {
            for step in 0..4 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                let header = String::from_utf8(header).unwrap();
                let size = header
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|v| v.parse::<usize>().ok())
                    })
                    .unwrap();
                let mut body = vec![0; size];
                socket.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                assert!(body.contains("client_id=fixture"));
                if step > 0 {
                    assert!(body.contains(if step == 3 {
                        "grant_type=refresh_token"
                    } else {
                        "device_code=device-secret"
                    }));
                }
                let value=match step {
                    0=>serde_json::json!({"device_code":"device-secret","user_code":"TEST-CODE","verification_uri":verification,"expires_in":60,"interval":1}),
                    1=>serde_json::json!({"error":"authorization_pending"}),
                    2=>serde_json::json!({"access_token":"access-one","refresh_token":"refresh-one","expires_in":120}),
                    _=>serde_json::json!({"access_token":"access-two","refresh_token":"refresh-two","expires_in":3600}),
                }.to_string();
                write!(socket,"HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",if step==1 {"400 Bad Request"}else{"200 OK"},value.len(),value).unwrap();
            }
        });
        let saved = authorize(
            Config {
                client_id: "fixture".into(),
                device_url: format!("{base}/device"),
                token_url: format!("{base}/token"),
                scopes: vec!["offline_access".into()],
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(saved.access_token, "access-one");
        let refreshed = token(
            &post(
                &format!("{base}/token"),
                &BTreeMap::from([
                    ("client_id", "fixture".into()),
                    ("grant_type", "refresh_token".into()),
                    ("refresh_token", saved.refresh_token.clone().unwrap()),
                ]),
            )
            .unwrap(),
            saved.refresh_token,
        )
        .unwrap();
        assert_eq!(refreshed.access_token, "access-two");
        assert_eq!(refreshed.refresh_token.as_deref(), Some("refresh-two"));
        server.join().unwrap();
    }
    #[test]
    fn validates_transport_and_preserves_rotating_refresh_tokens() {
        assert!(endpoint("https://auth.example/token").is_ok());
        assert!(endpoint("http://example.com/token").is_err());
        assert!(endpoint("https://secret@host/token").is_err());
        let token = token(
            &serde_json::json!({"access_token":"new","expires_in":120}),
            Some("keep".into()),
        )
        .unwrap();
        assert_eq!(token.refresh_token.as_deref(), Some("keep"));
        assert!(token.expires_at > now());
    }
}
