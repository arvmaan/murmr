use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// A single message in a chat conversation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Unified LLM client that supports both Ollama and OpenAI-compatible APIs.
#[derive(Debug, Clone)]
pub struct LlmClient {
    endpoint: String,
    api_key: Option<String>,
    protocol: Protocol,
    client: reqwest::Client,
}

/// Which API protocol to use.
#[derive(Debug, Clone, PartialEq)]
pub enum Protocol {
    /// Ollama native API (POST /api/chat)
    Ollama,
    /// OpenAI-compatible API (POST /v1/chat/completions)
    /// Works with: OpenAI, Groq, Together, OpenRouter, vLLM, etc.
    OpenAI,
    /// Anthropic Messages API (POST /v1/messages)
    Anthropic,
    /// AWS Bedrock (uses AWS SDK with SigV4 signing)
    /// Model IDs like: us.anthropic.claude-sonnet-4-20250514-v1:0
    Bedrock,
}

impl Protocol {
    /// Detect protocol from endpoint URL.
    pub fn detect(endpoint: &str) -> Self {
        let lower = endpoint.to_lowercase();
        if lower.contains("localhost:11434") || lower.contains("127.0.0.1:11434") {
            Self::Ollama
        } else if lower.contains("anthropic.com") {
            Self::Anthropic
        } else if lower.contains("/v1") {
            Self::OpenAI
        } else {
            Self::Ollama
        }
    }

    /// Parse from config string.
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ollama" => Self::Ollama,
            "openai" | "openai-compatible" | "groq" | "together" | "openrouter" => Self::OpenAI,
            "anthropic" | "claude" => Self::Anthropic,
            "bedrock" | "aws" | "aws-bedrock" => Self::Bedrock,
            _ => Self::Ollama,
        }
    }
}

// --- Ollama request/response ---
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: Message,
}

// --- OpenAI request/response ---
#[derive(Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: Message,
}

// --- Anthropic request/response ---
#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

impl LlmClient {
    /// Create a new LLM client.
    ///
    /// Protocol is auto-detected from the endpoint if not specified:
    /// - localhost:11434 → Ollama
    /// - anything with /v1 → OpenAI-compatible
    /// - has api_key → OpenAI-compatible
    pub fn new(endpoint: &str, api_key: Option<&str>, protocol: Option<&str>) -> Self {
        let protocol = match protocol {
            Some(p) => Protocol::from_str(p),
            None => {
                let detected = Protocol::detect(endpoint);
                if detected != Protocol::Ollama {
                    detected
                } else if api_key.is_some() {
                    Protocol::OpenAI
                } else {
                    Protocol::Ollama
                }
            }
        };

        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.map(|s| s.to_string()),
            protocol,
            client: reqwest::Client::new(),
        }
    }

    /// Convenience constructor for local Ollama.
    pub fn ollama(endpoint: &str) -> Self {
        Self::new(endpoint, None, Some("ollama"))
    }

    /// Return the configured endpoint URL.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Return the protocol in use.
    pub fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    /// Check if the LLM service is reachable.
    pub async fn health_check(&self) -> Result<bool> {
        if self.protocol == Protocol::Bedrock {
            // Bedrock uses AWS SDK — we can't easily health-check without making
            // a real inference call. Just verify credentials are loadable.
            #[cfg(feature = "bedrock")]
            {
                let _config =
                    aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                return Ok(true);
            }
            #[cfg(not(feature = "bedrock"))]
            {
                anyhow::bail!("bedrock support not compiled in (enable the 'bedrock' feature)");
            }
        }

        let url = match self.protocol {
            Protocol::Ollama => format!("{}/api/tags", self.endpoint),
            Protocol::OpenAI => format!("{}/v1/models", self.endpoint),
            Protocol::Anthropic => format!("{}/v1/messages", self.endpoint),
            Protocol::Bedrock => unreachable!(),
        };

        let mut req = match self.protocol {
            Protocol::Anthropic => {
                // Anthropic doesn't have a list endpoint; just check connectivity
                self.client.post(&url).timeout(std::time::Duration::from_secs(5))
                    .header("x-api-key", self.api_key.as_deref().unwrap_or(""))
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .body(r#"{"model":"claude-haiku-4-5-20251001","max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#)
            }
            _ => self
                .client
                .get(&url)
                .timeout(std::time::Duration::from_secs(5)),
        };

        if let Some(ref key) = self.api_key {
            if self.protocol != Protocol::Anthropic {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
        }

        let resp = req
            .send()
            .await
            .context("failed to connect to LLM service")?;
        Ok(resp.status().is_success())
    }

    /// Send a chat completion request and return the assistant response.
    pub async fn chat(&self, model: &str, messages: Vec<Message>) -> Result<String> {
        match self.protocol {
            Protocol::Ollama => self.chat_ollama(model, messages).await,
            Protocol::OpenAI => self.chat_openai(model, messages).await,
            Protocol::Anthropic => self.chat_anthropic(model, messages).await,
            Protocol::Bedrock => self.chat_bedrock(model, messages).await,
        }
    }

    async fn chat_ollama(&self, model: &str, messages: Vec<Message>) -> Result<String> {
        let request = OllamaChatRequest {
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

        let chat_resp: OllamaChatResponse = resp
            .json()
            .await
            .context("failed to parse Ollama response")?;

        Ok(chat_resp.message.content)
    }

    async fn chat_openai(&self, model: &str, messages: Vec<Message>) -> Result<String> {
        let request = OpenAIChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
        };

        let url = if self.endpoint.ends_with("/v1") {
            format!("{}/chat/completions", self.endpoint)
        } else {
            format!("{}/v1/chat/completions", self.endpoint)
        };

        let mut req = self.client.post(&url).json(&request);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.context("failed to send request to LLM")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API returned {}: {}", status, body);
        }

        let chat_resp: OpenAIChatResponse =
            resp.json().await.context("failed to parse LLM response")?;

        chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow::anyhow!("LLM returned empty choices array"))
    }

    async fn chat_anthropic(&self, model: &str, messages: Vec<Message>) -> Result<String> {
        // Anthropic separates system from user/assistant messages
        let mut system_prompt: Option<String> = None;
        let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();

        for msg in messages {
            if msg.role == "system" {
                system_prompt = Some(msg.content);
            } else {
                anthropic_messages.push(AnthropicMessage {
                    role: msg.role,
                    content: msg.content,
                });
            }
        }

        let request = AnthropicRequest {
            model: model.to_string(),
            max_tokens: 4096,
            system: system_prompt,
            messages: anthropic_messages,
        };

        let url = if self.endpoint.ends_with("/v1") {
            format!("{}/messages", self.endpoint)
        } else {
            format!("{}/v1/messages", self.endpoint)
        };

        let mut req = self
            .client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .json(&request);

        if let Some(ref key) = self.api_key {
            req = req.header("x-api-key", key);
        }

        let resp = req
            .send()
            .await
            .context("failed to send request to Anthropic")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API returned {}: {}", status, body);
        }

        let anthropic_resp: AnthropicResponse = resp
            .json()
            .await
            .context("failed to parse Anthropic response")?;

        anthropic_resp
            .content
            .into_iter()
            .next()
            .map(|c| c.text)
            .ok_or_else(|| anyhow::anyhow!("Anthropic returned empty content array"))
    }

    #[cfg(feature = "bedrock")]
    async fn chat_bedrock(&self, model: &str, messages: Vec<Message>) -> Result<String> {
        use aws_sdk_bedrockruntime::types::{
            ContentBlock, ConversationRole, Message as BedrockMessage, SystemContentBlock,
        };
        use aws_sdk_bedrockruntime::Client as BedrockClient;

        let region = self
            .endpoint
            .strip_prefix("bedrock:")
            .unwrap_or("us-east-1");

        // Disable the EC2 instance-metadata (IMDS) credential provider. On a
        // non-EC2 machine (and especially a GUI app that lacks the shell's AWS_*
        // env vars) the default provider chain can fall through to IMDS and
        // hang on metadata lookups. We only ever use file/SSO/env credentials.
        std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .load()
            .await;

        let client = BedrockClient::new(&config);

        let mut system_prompts: Vec<SystemContentBlock> = Vec::new();
        let mut bedrock_messages: Vec<BedrockMessage> = Vec::new();

        for msg in messages {
            match msg.role.as_str() {
                "system" => {
                    system_prompts.push(SystemContentBlock::Text(msg.content));
                }
                "user" => {
                    bedrock_messages.push(
                        BedrockMessage::builder()
                            .role(ConversationRole::User)
                            .content(ContentBlock::Text(msg.content))
                            .build()
                            .map_err(|e| anyhow::anyhow!("failed to build message: {}", e))?,
                    );
                }
                "assistant" => {
                    bedrock_messages.push(
                        BedrockMessage::builder()
                            .role(ConversationRole::Assistant)
                            .content(ContentBlock::Text(msg.content))
                            .build()
                            .map_err(|e| anyhow::anyhow!("failed to build message: {}", e))?,
                    );
                }
                _ => {}
            }
        }

        let mut req = client
            .converse()
            .model_id(model)
            .set_messages(Some(bedrock_messages));

        if !system_prompts.is_empty() {
            req = req.set_system(Some(system_prompts));
        }

        let response = req
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Bedrock request failed: {}", e))?;

        let output = response
            .output()
            .ok_or_else(|| anyhow::anyhow!("Bedrock returned no output"))?;

        match output {
            aws_sdk_bedrockruntime::types::ConverseOutput::Message(msg) => {
                let text = msg
                    .content()
                    .iter()
                    .filter_map(|block| {
                        if let ContentBlock::Text(t) = block {
                            Some(t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                Ok(text)
            }
            _ => anyhow::bail!("unexpected Bedrock output type"),
        }
    }

    #[cfg(not(feature = "bedrock"))]
    async fn chat_bedrock(&self, _model: &str, _messages: Vec<Message>) -> Result<String> {
        anyhow::bail!(
            "Bedrock support not compiled in. Rebuild with: cargo build --features bedrock"
        )
    }
}

/// Backwards-compatible type alias.
pub type OllamaClient = LlmClient;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_construction() {
        let client = LlmClient::new("http://localhost:11434", None, None);
        assert_eq!(client.endpoint(), "http://localhost:11434");
        assert_eq!(*client.protocol(), Protocol::Ollama);
    }

    #[test]
    fn test_trailing_slash_stripped() {
        let client = LlmClient::new("http://localhost:11434/", None, None);
        assert_eq!(client.endpoint(), "http://localhost:11434");
    }

    #[test]
    fn test_multiple_trailing_slashes() {
        let client = LlmClient::new("http://localhost:11434///", None, None);
        assert_eq!(client.endpoint(), "http://localhost:11434");
    }

    #[test]
    fn test_protocol_detection_ollama() {
        let client = LlmClient::new("http://localhost:11434", None, None);
        assert_eq!(*client.protocol(), Protocol::Ollama);
    }

    #[test]
    fn test_protocol_detection_openai_by_key() {
        let client = LlmClient::new("https://api.openai.com", Some("sk-123"), None);
        assert_eq!(*client.protocol(), Protocol::OpenAI);
    }

    #[test]
    fn test_protocol_detection_openai_by_url() {
        let client = LlmClient::new("https://api.groq.com/openai/v1", None, None);
        assert_eq!(*client.protocol(), Protocol::OpenAI);
    }

    #[test]
    fn test_protocol_explicit_override() {
        let client = LlmClient::new("http://localhost:8080", None, Some("openai"));
        assert_eq!(*client.protocol(), Protocol::OpenAI);
    }

    #[test]
    fn test_protocol_from_str() {
        assert_eq!(Protocol::from_str("ollama"), Protocol::Ollama);
        assert_eq!(Protocol::from_str("openai"), Protocol::OpenAI);
        assert_eq!(Protocol::from_str("groq"), Protocol::OpenAI);
        assert_eq!(Protocol::from_str("together"), Protocol::OpenAI);
        assert_eq!(Protocol::from_str("openrouter"), Protocol::OpenAI);
        assert_eq!(Protocol::from_str("anthropic"), Protocol::Anthropic);
        assert_eq!(Protocol::from_str("claude"), Protocol::Anthropic);
        assert_eq!(Protocol::from_str("unknown"), Protocol::Ollama);
    }

    #[test]
    fn test_protocol_detection_anthropic() {
        let client = LlmClient::new("https://api.anthropic.com", Some("sk-ant-123"), None);
        assert_eq!(*client.protocol(), Protocol::Anthropic);
    }

    #[test]
    fn test_protocol_explicit_anthropic() {
        let client = LlmClient::new("http://localhost:8080", Some("key"), Some("anthropic"));
        assert_eq!(*client.protocol(), Protocol::Anthropic);
    }

    #[tokio::test]
    async fn test_health_check_ollama_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body(r#"{"models":[]}"#)
            .create_async()
            .await;

        let client = LlmClient::ollama(&server.url());
        let result = client.health_check().await.unwrap();
        assert!(result);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_health_check_openai_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let client = LlmClient::new(&server.url(), Some("sk-test"), Some("openai"));
        let result = client.health_check().await.unwrap();
        assert!(result);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_health_check_unreachable() {
        let client = LlmClient::new("http://127.0.0.1:1", None, None);
        let result = client.health_check().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_ollama_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message":{"role":"assistant","content":"Hello, world!"}}"#)
            .create_async()
            .await;

        let client = LlmClient::ollama(&server.url());
        let messages = vec![Message {
            role: "user".to_string(),
            content: "say hello".to_string(),
        }];

        let result = client.chat("test-model", messages).await.unwrap();
        assert_eq!(result, "Hello, world!");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_openai_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"choices":[{"message":{"role":"assistant","content":"Hello from OpenAI!"}}]}"#,
            )
            .create_async()
            .await;

        let client = LlmClient::new(&server.url(), Some("sk-test"), Some("openai"));
        let messages = vec![Message {
            role: "user".to_string(),
            content: "say hello".to_string(),
        }];

        let result = client.chat("gpt-4o", messages).await.unwrap();
        assert_eq!(result, "Hello from OpenAI!");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_openai_empty_choices() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(r#"{"choices":[]}"#)
            .create_async()
            .await;

        let client = LlmClient::new(&server.url(), Some("sk-test"), Some("openai"));
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let result = client.chat("model", messages).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty choices"));
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

        let client = LlmClient::ollama(&server.url());
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

        let client = LlmClient::ollama(&server.url());
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let result = client.chat("model", messages).await;
        assert!(result.is_err());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_anthropic_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"content":[{"type":"text","text":"Hello from Claude!"}]}"#)
            .create_async()
            .await;

        let client = LlmClient::new(&server.url(), Some("sk-ant-test"), Some("anthropic"));
        let messages = vec![
            Message {
                role: "system".to_string(),
                content: "You are helpful.".to_string(),
            },
            Message {
                role: "user".to_string(),
                content: "say hello".to_string(),
            },
        ];

        let result = client
            .chat("claude-haiku-4-5-20251001", messages)
            .await
            .unwrap();
        assert_eq!(result, "Hello from Claude!");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_anthropic_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .with_status(401)
            .with_body(r#"{"error":{"message":"invalid api key"}}"#)
            .create_async()
            .await;

        let client = LlmClient::new(&server.url(), Some("bad-key"), Some("anthropic"));
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let result = client.chat("claude-haiku-4-5-20251001", messages).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("401"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_anthropic_empty_content() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(r#"{"content":[]}"#)
            .create_async()
            .await;

        let client = LlmClient::new(&server.url(), Some("key"), Some("anthropic"));
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];

        let result = client.chat("model", messages).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty content"));
        mock.assert_async().await;
    }
}
