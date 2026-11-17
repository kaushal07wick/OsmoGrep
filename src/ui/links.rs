use std::{
    path::Path,
    process::{Command, Stdio},
};

fn valid_url(value: &str) -> Result<reqwest::Url, String> {
    if value.len() > 4096
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err("Invalid web link".into());
    }
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid web link")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Only HTTP(S) links without embedded credentials can be opened".into());
    }
    Ok(url)
}
pub fn valid(value: &str) -> bool {
    valid_url(value).is_ok()
}
fn browser(url: &str, home: Option<&Path>) -> Command {
    let personal = home
        .map(|home| home.join(".codex/open-personal-link"))
        .filter(|path| path.is_file());
    let mut command = if let Some(launcher) = personal {
        Command::new(launcher)
    } else if let Some(launcher) = std::env::var_os("OSMOGREP_BROWSER") {
        Command::new(launcher)
    } else {
        #[cfg(target_os = "macos")]
        {
            Command::new("open")
        }
        #[cfg(target_os = "windows")]
        {
            let mut command = Command::new("rundll32");
            command.arg("url.dll,FileProtocolHandler");
            command
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Command::new("xdg-open")
        }
    };
    command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
pub fn open(value: &str) -> Result<(), String> {
    let url = valid_url(value)?;
    let mut child = browser(url.as_str(), dirs::home_dir().as_deref())
        .spawn()
        .map_err(|error| format!("Could not open browser: {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn links_validate_schemes_and_keep_personal_browser_urls_as_single_arguments() {
        for link in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://user:secret@example.com",
            "https://example.com/\ncommand",
        ] {
            assert!(!super::valid(link));
        }
        let root = std::env::temp_dir().join(format!("links-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".codex")).unwrap();
        std::fs::write(root.join(".codex/open-personal-link"), "").unwrap();
        let url = "http://127.0.0.1:4173/path?x=1&y=2";
        assert!(super::valid(url));
        let command = super::browser(url, Some(&root));
        assert_eq!(
            command.get_program(),
            root.join(".codex/open-personal-link").as_os_str()
        );
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new(url)]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
