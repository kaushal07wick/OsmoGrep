use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use crate::llm::ollama::Ollama;
use crate::llm::client::{LlmClient, LlmRunResult};
use crate::llm::prompt::LlmPrompt;

#[derive(Clone)]
pub enum LlmBackend {
    Ollama { model: String },
    Remote { client: LlmClient },
}

impl LlmBackend {
    pub fn ollama(model: String) -> Self {
        LlmBackend::Ollama { model }
    }

    pub fn remote(client: LlmClient) -> Self {
        LlmBackend::Remote { client }
    }

    pub fn run(
        &self,
        prompt: LlmPrompt,
        force_reload: bool,
    ) -> Result<LlmRunResult, String> {
        match self {
            LlmBackend::Ollama { model } => {
                let text = Ollama::run(prompt, model)
                    .map_err(|e| e.to_string())?;

                Ok(LlmRunResult {
                    text,
                    prompt_hash: "<ollama>".into(),
                    cached_tokens: None,
                })
            }

            LlmBackend::Remote { client } => {
                client.run(prompt, force_reload)
            }
        }
    }

    pub fn run_with_cancel(
        &self,
        prompt: LlmPrompt,
        cancel_flag: Arc<AtomicBool>,
        force_reload: bool,
    ) -> Option<LlmRunResult> 
    {
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel();
        let backend = self.clone();

        // spawn LLM call in background
        std::thread::spawn(move || {
            let result = backend.run(prompt, force_reload);
            let _ = tx.send(result);
        });

        // poll loop with cancellability
        loop {
            if cancel_flag.load(Ordering::SeqCst) {
                return None;
            }

            match rx.try_recv() {
                Ok(Ok(ok)) => return Some(ok),
                Ok(Err(_e)) => return None, // treat error as cancelled
                Err(mpsc::TryRecvError::Empty) => {
                    // keep polling
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return None;
                }
            }
        }
    }
}
