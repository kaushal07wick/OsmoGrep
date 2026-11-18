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
    #[serde(skip)]
    verification_origin: Option<String>,
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
pub fn account_endpoint(path: &str) -> Result<String, String> {
    let origin =
        std::env::var("OSMOGREP_ACCOUNT_URL").unwrap_or_else(|_| "https://app.osmogrep.com".into());
    endpoint(&origin)?;
    let mut url = url::Url::parse(&origin).map_err(|_| "Invalid account origin")?;
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(
            "OSMOGREP_ACCOUNT_URL must be an HTTPS origin, without a path, query or fragment"
                .into(),
        );
    }
    url.set_path(path);
    Ok(url.into())
}

fn config(provider: &str) -> Result<Config, String> {
    if provider == "osmogrep" {
        let device_url = account_endpoint("/api/cli/auth/device")?;
        return Ok(Config {
            verification_origin: Some(
                url::Url::parse(&device_url)
                    .map_err(|_| "Invalid account origin")?
                    .origin()
                    .ascii_serialization(),
            ),
            client_id: "osmogrep-cli".into(),
            device_url,
            token_url: account_endpoint("/api/cli/auth/token")?,
            scopes: vec!["account:read".into()],
        });
    }

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
    if status.is_success() && bytes.is_empty() {
        return Ok(serde_json::json!({}));
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
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 16 * 1024
                && !s.chars().any(char::is_whitespace)
                && !s.chars().any(char::is_control)
        })
        .ok_or("OAuth response has no valid access token")?;
    let expires_in = match value.get("expires_in") {
        Some(value) => value
            .as_u64()
            .filter(|seconds| *seconds > 0)
            .ok_or("OAuth response has an invalid expiry")?,
        None => 3600,
    };
    if value.get("token_type").is_some_and(|kind| {
        !kind
            .as_str()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("bearer"))
    }) {
        return Err("OAuth response has an unsupported token type".into());
    }
    Ok(Token {
        access_token: access.into(),
        refresh_token: value["refresh_token"]
            .as_str()
            .map(str::to_string)
            .or(previous),
        expires_at: now().saturating_add(expires_in),
    })
}
fn credential_lock(provider: &str) -> Result<std::fs::File, String> {
    let directory = crate::persistence::config_dir();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    crate::storage::lock_file(&directory.join(format!(
        "oauth-{}.lock",
        crate::file_guard::digest(provider.as_bytes())
    )))
}

pub fn login(provider: &str) -> Result<(), String> {
    login_browser(provider, true)
}
pub fn login_browser(provider: &str, open_browser: bool) -> Result<(), String> {
    login_with(provider, &crate::agent::CancelToken::new(), |uri, code| {
        println!("Confirm code: {code}\nOpen {uri}\nWaiting for browser approval. Ctrl+C cancels.");
        if open_browser {
            if let Err(error) = crate::ui::links::open(uri) {
                eprintln!("{error}. Open the link above manually.");
            }
        }
        Ok(())
    })?;
    println!("Signed in to {provider}. Credentials saved in the OS credential store.");
    Ok(())
}
pub fn login_with(
    provider: &str,
    cancel: &crate::agent::CancelToken,
    announce: impl Fn(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    let saved = authorize(
        config(provider)?,
        |duration| wait(duration, cancel),
        announce,
    )?;
    let result = (|| {
        if cancel.is_cancelled() {
            return Err("Login cancelled".to_string());
        }
        let _lock = credential_lock(provider)?;
        if cancel.is_cancelled() {
            return Err("Login cancelled".to_string());
        }
        crate::credentials::set(
            &format!("oauth/{provider}"),
            &serde_json::to_string(&saved).map_err(|e| e.to_string())?,
        )
    })();
    if let Err(error) = result {
        if provider == "osmogrep"
            && revoke_account_at(&account_endpoint("/api/cli/auth/revoke")?, &saved).is_err()
        {
            return Err(format!(
                "{error}. The new connection could not be revoked; remove it from your account."
            ));
        }
        return Err(error);
    }
    Ok(())
}
fn revoke_account_at(url: &str, saved: &Token) -> Result<(), String> {
    let refresh = saved
        .refresh_token
        .as_ref()
        .filter(|token| !token.is_empty())
        .ok_or("Account credential has no revocable refresh token")?;
    let value = post(
        url,
        &BTreeMap::from([
            ("client_id", "osmogrep-cli".into()),
            ("token", refresh.clone()),
            ("token_type_hint", "refresh_token".into()),
        ]),
    )?;
    if value["error"].is_string() {
        return Err("Account server rejected revocation".into());
    }
    Ok(())
}

fn wait(duration: Duration, cancel: &crate::agent::CancelToken) -> Result<(), String> {
    let deadline = std::time::Instant::now() + duration;
    loop {
        if cancel.is_cancelled() {
            return Err("Login cancelled".into());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}
fn verification_link(
    device: &Value,
    expected_origin: Option<&str>,
) -> Result<(String, String), String> {
    let uri = device["verification_uri"]
        .as_str()
        .ok_or("OAuth server did not return a verification URI")?;
    endpoint(uri)?;
    let parsed = url::Url::parse(uri).map_err(|_| "Invalid verification URI")?;
    if expected_origin.is_some_and(|origin| parsed.origin().ascii_serialization() != origin) {
        return Err("Verification link does not belong to the account server".into());
    }
    let complete = device["verification_uri_complete"].as_str().unwrap_or(uri);
    endpoint(complete)?;
    let complete_url = url::Url::parse(complete).map_err(|_| "Invalid verification link")?;
    if expected_origin.is_some()
        && [parsed.clone(), complete_url.clone()].iter().any(|url| {
            url.fragment().is_some() || url.query_pairs().any(|(key, _)| key != "user_code")
        })
    {
        return Err("Account verification links may contain only the public user code".into());
    }

    if complete_url.origin() != parsed.origin() || !crate::ui::links::valid(complete) {
        return Err("Invalid verification link origin or characters".into());
    }
    let code = device["user_code"]
        .as_str()
        .filter(|value| {
            !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
        })
        .ok_or("OAuth server did not return a valid user code")?;
    Ok((complete.into(), code.into()))
}
fn authorize(
    cfg: Config,
    wait: impl Fn(Duration) -> Result<(), String>,
    announce: impl Fn(&str, &str) -> Result<(), String>,
) -> Result<Token, String> {
    let mut form = BTreeMap::from([
        ("client_id", cfg.client_id.clone()),
        ("scope", cfg.scopes.join(" ")),
    ]);
    let device = post(&cfg.device_url, &form)?;
    let code = device["device_code"]
        .as_str()
        .ok_or("OAuth server did not return a device code")?;
    let (uri, user_code) = verification_link(&device, cfg.verification_origin.as_deref())?;
    announce(&uri, &user_code)?;
    form = BTreeMap::from([
        ("client_id", cfg.client_id),
        ("device_code", code.into()),
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code".into(),
        ),
    ]);
    let mut interval = device["interval"].as_u64().unwrap_or(5).clamp(1, 300);
    let deadline = now().saturating_add(device["expires_in"].as_u64().unwrap_or(600).min(1800));
    while now() < deadline {
        wait(Duration::from_secs(interval))?;
        let value = post(&cfg.token_url, &form)?;
        match value["error"].as_str() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                interval = (interval + 5).min(300);
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
        let _lock = credential_lock(provider)?;
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
    let _lock = credential_lock(provider)?;
    let revoke = if provider == "osmogrep" {
        (|| {
            let Some(raw) = crate::credentials::get("oauth/osmogrep")? else {
                return Ok(());
            };
            let saved: Token =
                serde_json::from_str(&raw).map_err(|_| "Stored account credential is invalid")?;
            revoke_account_at(&account_endpoint("/api/cli/auth/revoke")?, &saved)
        })()
    } else {
        Ok(())
    };
    crate::credentials::delete(&format!("oauth/{provider}"))?;
    revoke.map_err(|_| "Signed out locally; server revocation could not be confirmed".into())
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
                verification_origin: None,
            },
            |_| Ok(()),
            |_, _| Ok(()),
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
    fn browser_handoff_rejects_cross_origin_links_and_terminal_control_codes() {
        let device = serde_json::json!({"verification_uri":"https://app.osmogrep.com/cli/authorize","verification_uri_complete":"https://app.osmogrep.com/cli/authorize?user_code=ABCD-EFGH","user_code":"ABCD-EFGH","device_code":"private-device-secret"});
        let (link, code) = verification_link(&device, Some("https://app.osmogrep.com")).unwrap();
        assert!(link.contains("user_code=ABCD-EFGH"));
        assert_eq!(code, "ABCD-EFGH");
        assert!(!link.contains("private-device-secret"));
        assert!(verification_link(&device, Some("https://other.example")).is_err());
        let mut unsafe_link = device.clone();
        unsafe_link["verification_uri_complete"] =
            serde_json::json!("https://other.example/approve");
        assert!(verification_link(&unsafe_link, None).is_err());
        unsafe_link = device.clone();
        unsafe_link["verification_uri_complete"] = serde_json::json!(
            "https://app.osmogrep.com/cli/authorize?device_code=private-device-secret"
        );
        assert!(verification_link(&unsafe_link, Some("https://app.osmogrep.com")).is_err());
        unsafe_link = device.clone();
        unsafe_link["user_code"] = serde_json::json!("ABCD\u{1b}[2J");
        assert!(verification_link(&unsafe_link, None).is_err());
        let cancel = crate::agent::CancelToken::new();
        cancel.cancel();
        let started = std::time::Instant::now();
        assert!(wait(Duration::from_secs(30), &cancel).is_err());
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    fn oauth_fixture(replies: Vec<Value>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let verification = format!("{base}/verify");
        let thread = std::thread::spawn(move || {
            for mut value in replies {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                    assert!(header.len() <= 8192);
                }
                let header = String::from_utf8(header).unwrap();
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|length| length.parse::<usize>().ok())
                    })
                    .unwrap();
                assert!(length <= 8192);
                let mut body = vec![0; length];
                socket.read_exact(&mut body).unwrap();
                if value.get("device_code").is_some() {
                    value["verification_uri"] = serde_json::json!(verification);
                }
                let status = if value.get("error").is_some() {
                    "400 Bad Request"
                } else {
                    "200 OK"
                };
                let body = value.to_string();
                write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (base, thread)
    }
    fn fixture_config(base: &str) -> Config {
        Config {
            client_id: "fixture".into(),
            device_url: format!("{base}/device"),
            token_url: format!("{base}/token"),
            scopes: vec!["account:read".into()],
            verification_origin: Some(base.into()),
        }
    }
    #[test]
    fn device_flow_handles_slow_down_denial_expiry_and_cancellation_without_saving_secrets() {
        let device = serde_json::json!({"device_code":"private-code","user_code":"ABCD-EFGH","interval":2,"expires_in":60});
        let (base, server) = oauth_fixture(vec![
            device.clone(),
            serde_json::json!({"error":"slow_down"}),
            serde_json::json!({"error":"authorization_pending"}),
            serde_json::json!({"access_token":"fixture-access","token_type":"Bearer","expires_in":60}),
        ]);
        let delays = std::cell::RefCell::new(vec![]);
        authorize(
            fixture_config(&base),
            |duration| {
                delays.borrow_mut().push(duration.as_secs());
                Ok(())
            },
            |uri, code| {
                assert_eq!(code, "ABCD-EFGH");
                assert!(!uri.contains("private-code"));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*delays.borrow(), vec![2, 7, 7]);
        server.join().unwrap();
        for error in ["access_denied", "expired_token"] {
            let (base, server) =
                oauth_fixture(vec![device.clone(), serde_json::json!({"error":error})]);
            assert!(authorize(fixture_config(&base), |_| Ok(()), |_, _| Ok(())).is_err());
            server.join().unwrap();
        }
        let mut expired = device.clone();
        expired["expires_in"] = serde_json::json!(0);
        let (base, server) = oauth_fixture(vec![expired]);
        assert!(authorize(
            fixture_config(&base),
            |_| panic!("expired login must not poll"),
            |_, _| Ok(())
        )
        .is_err());
        server.join().unwrap();
        let (base, server) = oauth_fixture(vec![device]);
        assert!(authorize(
            fixture_config(&base),
            |_| panic!("cancelled login must not poll"),
            |_, _| Err("cancelled".into())
        )
        .is_err());
        server.join().unwrap();
        for value in [
            serde_json::json!({"access_token":"token","expires_in":0}),
            serde_json::json!({"access_token":"token","expires_in":-1}),
            serde_json::json!({"access_token":"token","token_type":"MAC"}),
            serde_json::json!({"access_token":"token\nunsafe"}),
        ] {
            assert!(token(&value, None).is_err());
        }
    }

    #[test]
    fn account_revocation_uses_the_refresh_token_and_surfaces_remote_errors() {
        let saved = Token {
            access_token: "fixture-access".into(),
            refresh_token: Some("fixture-refresh".into()),
            expires_at: now() + 60,
        };
        let (base, server) = oauth_fixture(vec![serde_json::json!({})]);
        revoke_account_at(&format!("{base}/revoke"), &saved).unwrap();
        server.join().unwrap();
        let (base, server) = oauth_fixture(vec![serde_json::json!({"error":"server_error"})]);
        assert!(revoke_account_at(&format!("{base}/revoke"), &saved).is_err());
        server.join().unwrap();
        assert!(revoke_account_at(
            "http://127.0.0.1:1/never",
            &Token {
                refresh_token: None,
                ..saved
            }
        )
        .is_err());
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
