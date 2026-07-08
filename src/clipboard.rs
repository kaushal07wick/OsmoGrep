use std::{
    env,
    fs::OpenOptions,
    io::{self, Write},
    process::{Command, Stdio},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};

const OSC52_MAX_RAW_BYTES: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyBackend {
    #[cfg(target_os = "macos")]
    MacosPbcopy,
    #[cfg(target_os = "windows")]
    WindowsClip,
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    WlCopy,
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    Xclip,
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    Xsel,
    Osc52,
}

impl CopyBackend {
    pub fn label(self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Self::MacosPbcopy => "pbcopy",
            #[cfg(target_os = "windows")]
            Self::WindowsClip => "clip",
            #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
            Self::WlCopy => "wl-copy",
            #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
            Self::Xclip => "xclip",
            #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
            Self::Xsel => "xsel",
            Self::Osc52 => "OSC52",
        }
    }
}

pub fn copy_text_to_clipboard(text: &str) -> Result<CopyBackend, String> {
    if text.is_empty() {
        return Err("Nothing to copy.".to_string());
    }

    if env::var_os("OSMOGREP_CLIPBOARD_OSC52").is_some() || running_over_ssh() {
        copy_with_osc52(text)?;
        return Ok(CopyBackend::Osc52);
    }

    for &backend in platform_backends() {
        if copy_with_backend(backend, text).is_ok() {
            return Ok(backend);
        }
    }

    copy_with_osc52(text)?;
    Ok(CopyBackend::Osc52)
}

fn running_over_ssh() -> bool {
    env::var_os("SSH_TTY").is_some() || env::var_os("SSH_CONNECTION").is_some()
}

fn platform_backends() -> &'static [CopyBackend] {
    #[cfg(target_os = "macos")]
    {
        &[CopyBackend::MacosPbcopy]
    }

    #[cfg(target_os = "windows")]
    {
        &[CopyBackend::WindowsClip]
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        &[CopyBackend::WlCopy, CopyBackend::Xclip, CopyBackend::Xsel]
    }
}

fn copy_with_backend(backend: CopyBackend, text: &str) -> Result<(), String> {
    match backend {
        #[cfg(target_os = "macos")]
        CopyBackend::MacosPbcopy => copy_with_command("pbcopy", &[], text),
        #[cfg(target_os = "windows")]
        CopyBackend::WindowsClip => copy_with_command("clip", &[], text),
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        CopyBackend::WlCopy => copy_with_command("wl-copy", &[], text),
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        CopyBackend::Xclip => copy_with_command("xclip", &["-selection", "clipboard"], text),
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        CopyBackend::Xsel => copy_with_command("xsel", &["--clipboard", "--input"], text),
        CopyBackend::Osc52 => copy_with_osc52(text),
    }
}

fn copy_with_command(program: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{program} unavailable: {err}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|err| format!("{program} write failed: {err}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("{program} failed: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!("{program} exited with {}", output.status))
        } else {
            Err(format!("{program} exited with {}: {stderr}", output.status))
        }
    }
}

fn copy_with_osc52(text: &str) -> Result<(), String> {
    let sequence = osc52_sequence(text)?;

    #[cfg(unix)]
    {
        if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty") {
            return write_osc52_to(&mut tty, &sequence);
        }
    }

    let mut stdout = io::stdout().lock();
    write_osc52_to(&mut stdout, &sequence)
}

fn write_osc52_to(writer: &mut impl Write, sequence: &str) -> Result<(), String> {
    writer
        .write_all(sequence.as_bytes())
        .map_err(|err| format!("OSC52 write failed: {err}"))?;
    writer
        .flush()
        .map_err(|err| format!("OSC52 flush failed: {err}"))
}

fn osc52_sequence(text: &str) -> Result<String, String> {
    let raw_bytes = text.len();
    if raw_bytes > OSC52_MAX_RAW_BYTES {
        return Err(format!(
            "OSC52 payload too large ({raw_bytes} bytes; max {OSC52_MAX_RAW_BYTES})"
        ));
    }

    Ok(format!(
        "\x1b]52;c;{}\x07",
        STANDARD.encode(text.as_bytes())
    ))
}

#[cfg(test)]
mod tests {
    use super::{osc52_sequence, CopyBackend, OSC52_MAX_RAW_BYTES};

    #[test]
    fn osc52_sequence_encodes_text() {
        assert_eq!(osc52_sequence("hello").unwrap(), "\x1b]52;c;aGVsbG8=\x07");
    }

    #[test]
    fn osc52_sequence_rejects_large_payloads() {
        let text = "x".repeat(OSC52_MAX_RAW_BYTES + 1);
        let err = osc52_sequence(&text).unwrap_err();

        assert!(err.contains("OSC52 payload too large"));
    }

    #[test]
    fn backend_labels_are_stable() {
        #[cfg(target_os = "macos")]
        assert_eq!(CopyBackend::MacosPbcopy.label(), "pbcopy");
        #[cfg(target_os = "windows")]
        assert_eq!(CopyBackend::WindowsClip.label(), "clip");
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            assert_eq!(CopyBackend::WlCopy.label(), "wl-copy");
            assert_eq!(CopyBackend::Xclip.label(), "xclip");
            assert_eq!(CopyBackend::Xsel.label(), "xsel");
        }
        assert_eq!(CopyBackend::Osc52.label(), "OSC52");
    }
}
