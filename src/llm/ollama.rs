use std::io::{self, Write};
use std::process::{Command, Stdio};

use crate::llm::prompt::LlmPrompt;

pub struct Ollama;

impl Ollama {
    pub fn run(prompt: LlmPrompt) -> io::Result<String> {
        let mut child = Command::new("python3")
            .arg("src/llm_py/ollama.py")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Build input once
        let input = format!("{}\n\n{}", prompt.system, prompt.user);

        // Write prompt safely
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(input.as_bytes())?;
            stdin.flush()?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Failed to open stdin for Ollama process",
            ));
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string())
    }
}
