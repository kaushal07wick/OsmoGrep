use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::path::PathBuf;

use crate::llm::prompt::LlmPrompt;

pub struct Ollama;

impl Ollama {
    pub fn run(prompt: LlmPrompt, model: &str) -> io::Result<String> {
        // Absolute path to ollama.py
        let script: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("llm_py")
            .join("ollama.py");

        let mut child = Command::new("python3")
            .arg(script)
            // 👇 single, explicit bridge
            .env("OSMOGREP_OLLAMA_MODEL", model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let input = format!("{}\n\n{}", prompt.system, prompt.user);

        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "Failed to open stdin")
            })?;
            stdin.write_all(input.as_bytes())?;
        } // stdin dropped → EOF

        let output = child.wait_with_output()?;

        if !output.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
