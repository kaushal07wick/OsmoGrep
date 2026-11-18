//! First-party account identity and server-recorded metrics; model credentials stay separate.
use crate::{agent::CancelToken, auth};
use serde::{Deserialize, Serialize};
use std::{io::Read, sync::mpsc, time::Duration};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    pub name: Option<String>,
    pub username: Option<String>,
}
impl User {
    pub fn label(&self) -> &str {
        self.name
            .as_deref()
            .or(self.username.as_deref())
            .unwrap_or("GitHub account")
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub kind: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Stats {
    pub reviews: Option<u64>,
    pub running: Option<u64>,
    pub queued: Option<u64>,
    pub verified: Option<u64>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Usage {
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub model_calls: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Profile {
    pub user: User,
    pub workspace: Option<Workspace>,
    #[serde(default)]
    pub stats: Stats,
    #[serde(default)]
    pub usage: Usage,
}
fn parse(bytes: &[u8]) -> Result<Profile, String> {
    let value: Profile =
        serde_json::from_slice(bytes).map_err(|_| "Account server returned an invalid profile")?;
    let mut labels = vec![value.user.id.as_str()];
    labels.extend(value.user.name.as_deref());
    labels.extend(value.user.username.as_deref());
    if let Some(workspace) = &value.workspace {
        labels.extend([workspace.id.as_str(), workspace.name.as_str()]);
        if !matches!(workspace.kind.as_str(), "personal" | "team") {
            return Err("Account server returned an invalid workspace kind".into());
        }
    }
    if labels.iter().any(|label| {
        label.trim().is_empty() || label.len() > 256 || label.chars().any(char::is_control)
    }) {
        return Err("Account server returned invalid identity labels".into());
    }
    for date in [&value.usage.period_start, &value.usage.period_end]
        .into_iter()
        .flatten()
    {
        chrono::DateTime::parse_from_rfc3339(date)
            .map_err(|_| "Account usage has an invalid period")?;
    }
    if let (Some(start), Some(end)) = (&value.usage.period_start, &value.usage.period_end) {
        if chrono::DateTime::parse_from_rfc3339(start).unwrap()
            >= chrono::DateTime::parse_from_rfc3339(end).unwrap()
        {
            return Err("Account usage period must end after it starts".into());
        }
    }
    Ok(value)
}
fn fetch(url: &str, token: &str) -> Result<Profile, String> {
    let response = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Could not prepare the account connection")?
        .get(url)
        .bearer_auth(token)
        .send()
        .map_err(|_| "Account server could not be reached")?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err("Account access expired or was revoked. Sign in again.".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "Account server returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Account response could not be read")?;
    if bytes.len() > 64 * 1024 {
        return Err("Account response exceeds 64 KiB".into());
    }
    parse(&bytes)
}
pub fn load() -> Result<Option<Profile>, String> {
    let Some(token) = auth::access_token("osmogrep")? else {
        return Ok(None);
    };
    fetch(&auth::account_endpoint("/api/cli/account")?, &token).map(Some)
}
pub fn print(json: bool) -> Result<(), String> {
    let profile = load()?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"signed_in":profile.is_some(),"account":profile})
            )
            .map_err(|e| e.to_string())?
        );
    } else if let Some(profile) = profile {
        println!(
            "{}{}",
            profile.user.label(),
            profile
                .user
                .username
                .as_ref()
                .map(|name| format!(" · @{name}"))
                .unwrap_or_default()
        );
        if let Some(workspace) = profile.workspace {
            println!("Workspace: {} ({})", workspace.name, workspace.kind);
        }
        println!(
            "Hosted account · reviews: {} · verified fixes: {}",
            count(profile.stats.reviews),
            count(profile.stats.verified)
        );
        println!(
            "Hosted model calls: {} · input: {} · output: {}",
            count(profile.usage.model_calls),
            count(profile.usage.input_tokens),
            count(profile.usage.output_tokens)
        );
        println!("Local CLI usage remains separate: osmogrep usage");
    } else {
        println!("Not signed in. Run osmogrep login, or open Settings → Account.");
    }
    Ok(())
}
pub fn count(value: Option<u64>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unavailable".into())
}

#[derive(Clone, Copy)]
pub enum Operation {
    Refresh,
    Login,
    Logout,
}
pub enum Event {
    Waiting {
        url: String,
        code: String,
        browser_error: Option<String>,
    },
    Done(Result<Option<Profile>, String>),
}
pub struct Job {
    pub events: mpsc::Receiver<Event>,
    cancel: CancelToken,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub fn start(operation: Operation) -> Job {
    let (tx, events) = mpsc::channel();
    let cancel = CancelToken::new();
    let worker_cancel = cancel.clone();
    std::thread::spawn(move || {
        let result = (|| {
            match operation {
                Operation::Refresh => {}
                Operation::Login => auth::login_with("osmogrep", &worker_cancel, |url, code| {
                    if worker_cancel.is_cancelled() {
                        return Err("Login cancelled".into());
                    }
                    let browser_error = crate::ui::links::open(url).err();
                    tx.send(Event::Waiting {
                        url: url.into(),
                        code: code.into(),
                        browser_error,
                    })
                    .map_err(|_| "Login cancelled".to_string())
                })?,
                Operation::Logout => {
                    auth::logout("osmogrep")?;
                    return Ok(None);
                }
            }
            if worker_cancel.is_cancelled() {
                return Err("Account action cancelled".into());
            }
            load()
        })();
        let _ = tx.send(Event::Done(result));
    });
    Job { events, cancel }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_summary_keeps_missing_metrics_unavailable_and_rejects_invalid_identity_or_counts() {
        let profile = parse(br#"{"user":{"id":"123","name":"User","username":"octocat"},"stats":{"reviews":0},"provider_key":"must not escape"}"#).unwrap();
        assert_eq!(count(profile.stats.reviews), "0");
        let unnamed = parse(br#"{"user":{"id":"123","name":null,"username":"octocat"}}"#).unwrap();
        assert_eq!(unnamed.user.label(), "octocat");
        let no_public_name =
            parse(br#"{"user":{"id":"123","name":null,"username":null}}"#).unwrap();
        assert_eq!(no_public_name.user.label(), "GitHub account");
        assert_eq!(count(profile.usage.model_calls), "unavailable");
        assert!(!serde_json::to_string(&profile)
            .unwrap()
            .contains("must not escape"));
        for bytes in [
            br#"{"user":{"id":"","name":"User"}}"#.as_slice(),
            br#"{"user":{"id":"123","name":"User\u001b[2J"}}"#,
            br#"{"user":{"id":"123","name":"User"},"stats":{"reviews":-1}}"#,
            br#"{"user":{"id":"123","name":"User"},"usage":{"input_tokens":"unknown"}}"#,
            br#"{"user":{"id":"123","name":"User"},"usage":{"period_start":"2026-12-01T00:00:00Z","period_end":"2026-11-01T00:00:00Z"}}"#,
        ] { assert!(parse(bytes).is_err()); }
    }
    #[test]
    fn closing_an_account_job_cancels_its_worker() {
        let (_tx, events) = mpsc::channel();
        let cancel = CancelToken::new();
        let job = Job {
            events,
            cancel: cancel.clone(),
        };
        assert!(!cancel.is_cancelled());
        drop(job);
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn account_http_sends_a_bearer_token_and_never_follows_redirects() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api/cli/account", server.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            for redirect in [false, true] {
                let (mut socket, _) = server.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                assert!(String::from_utf8(header)
                    .unwrap()
                    .to_lowercase()
                    .contains("authorization: bearer fixture-account-only"));
                let body = r#"{"user":{"id":"123","name":"User"}}"#;
                write!(socket,"HTTP/1.1 {}\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/never\r\nConnection: close\r\n\r\n{}",if redirect {"302 Found"}else{"200 OK"},body.len(),body).unwrap();
            }
        });
        assert_eq!(fetch(&url, "fixture-account-only").unwrap().user.id, "123");
        assert!(fetch(&url, "fixture-account-only")
            .unwrap_err()
            .contains("302"));
        thread.join().unwrap();
    }
}

#[cfg(test)]
#[path = "account/live_tests.rs"]
mod live_tests;
