use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct OllamaClient {
    endpoint: String,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Message,
}

impl OllamaClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Check if Ollama is reachable.
    pub async fn health_check(&self) -> Result<bool> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.endpoint))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("failed to connect to Ollama")?;
        Ok(resp.status().is_success())
    }

    /// Send a chat completion request to Ollama.
    pub async fn chat(&self, model: &str, messages: Vec<Message>) -> Result<String> {
        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.endpoint))
            .json(&request)
            .send()
            .await
            .context("failed to send request to Ollama")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned {}: {}", status, body);
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .context("failed to parse Ollama response")?;

        Ok(chat_resp.message.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_construction() {
        let client = OllamaClient::new("http://localhost:11434");
        assert_eq!(client.endpoint(), "http://localhost:11434");
    }

    #[test]
    fn test_trailing_slash_stripped() {
        let client = OllamaClient::new("http://localhost:11434/");
        assert_eq!(client.endpoint(), "http://localhost:11434");
    }
}
