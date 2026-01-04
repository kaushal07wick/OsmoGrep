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

    pub fn run(&self, prompt: LlmPrompt) -> Result<LlmRunResult, String> {
        match self {
            LlmBackend::Ollama { model } => {
                // Ollama has no cache metadata → fabricate minimal result
                let text = Ollama::run(prompt, model)
                    .map_err(|e| e.to_string())?;

                Ok(LlmRunResult {
                    text,
                    prompt_hash: "<ollama>".into(),
                    cached_tokens: None,
                })
            }

            LlmBackend::Remote { client } => {
                client.run(prompt)
            }
        }
    }
}
