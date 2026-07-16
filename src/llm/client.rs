use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// HTTP client for the Ollama chat completions API.
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

/// A single message in an Ollama chat conversation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Message,
}

impl OllamaClient {
    /// Create a new client pointing at the given Ollama endpoint.
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Return the configured endpoint URL.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Check if Ollama is reachable by hitting the /api/tags endpoint.
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

    /// Send a chat completion request to Ollama and return the assistant response.
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

    #[test]
    fn test_multiple_trailing_slashes() {
        let client = OllamaClient::new("http://localhost:11434///");
        assert_eq!(client.endpoint(), "http://localhost:11434");
    }

    #[tokio::test]
    async fn test_health_check_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body(r#"{"models":[]}"#)
            .create_async()
            .await;

        let client = OllamaClient::new(&server.url());
        let result = client.health_check().await.unwrap();
        assert!(result);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_health_check_server_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(500)
            .create_async()
            .await;

        let client = OllamaClient::new(&server.url());
        let result = client.health_check().await.unwrap();
        assert!(!result);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_health_check_unreachable() {
        let client = OllamaClient::new("http://127.0.0.1:1");
        let result = client.health_check().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message":{"role":"assistant","content":"Hello, world!"}}"#)
            .create_async()
            .await;

        let client = OllamaClient::new(&server.url());
        let messages = vec![Message {
            role: "user".to_string(),
            content: "say hello".to_string(),
        }];

        let result = client.chat("test-model", messages).await.unwrap();
        assert_eq!(result, "Hello, world!");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_server_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(404)
            .with_body("model not found")
            .create_async()
            .await;

        let client = OllamaClient::new(&server.url());
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let result = client.chat("nonexistent", messages).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("404"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_invalid_json_response() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;

        let client = OllamaClient::new(&server.url());
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let result = client.chat("model", messages).await;
        assert!(result.is_err());
        mock.assert_async().await;
    }
}
