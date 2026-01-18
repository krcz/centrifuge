use polyepoxide_core::{Cell, Solvent};
use polyepoxide_llm::Message;
use std::sync::Arc;
use tracing::{debug, instrument};

use crate::convert::{build_request_body, parse_response};
use crate::error::OpenRouterError;
use crate::types::OpenRouterRequest;

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Client for the OpenRouter API.
pub struct OpenRouterClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenRouterClient {
    /// Creates a new client with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Creates a new client with a custom base URL.
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    /// Executes a completion request.
    ///
    /// Returns an assistant Message with `previous` pointing to the conversation head.
    #[instrument(skip(self, request), fields(model = %request.model))]
    pub async fn complete(&self, request: &OpenRouterRequest) -> Result<Message, OpenRouterError> {
        let body = build_request_body(request)?;

        debug!("Sending request to OpenRouter");

        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let response_body: serde_json::Value = response.json().await?;

        if !status.is_success() {
            let message = response_body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            return Err(OpenRouterError::Api {
                status: status.as_u16(),
                message,
            });
        }

        debug!("Received successful response");

        parse_response(&response_body, request.conversation_head.clone())
    }

    /// Executes a completion request and stores the result in a Solvent.
    ///
    /// Returns the cell containing the assistant message.
    #[instrument(skip(self, request, solvent), fields(model = %request.model))]
    pub async fn complete_with_solvent(
        &self,
        request: &OpenRouterRequest,
        solvent: &mut Solvent,
    ) -> Result<Arc<Cell<Message>>, OpenRouterError> {
        let message = self.complete(request).await?;
        Ok(solvent.add(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::Bond;
    use polyepoxide_llm::{ContentBlock, MessageContent};

    #[test]
    fn test_client_creation() {
        let client = OpenRouterClient::new("test-key");
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url() {
        let client = OpenRouterClient::with_base_url("test-key", "https://custom.api.com");
        assert_eq!(client.base_url, "https://custom.api.com");
    }

    #[tokio::test]
    #[ignore = "requires OPENROUTER_API_KEY env var"]
    async fn test_live_api() {
        let api_key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY not set");
        let client = OpenRouterClient::new(api_key);

        let mut solvent = Solvent::new();
        let user_msg = Message {
            content: MessageContent::User(vec![ContentBlock::Text(
                "Say 'hello' and nothing else.".to_string(),
            )]),
            metadata: None,
            previous: None,
        };
        let cell = solvent.add(user_msg);

        let request = OpenRouterRequest {
            model: "openai/gpt-4o-mini".to_string(),
            conversation_head: Bond::from_cell(Arc::clone(&cell)),
            params: None,
            tools: vec![],
            tool_choice: None,
        };

        let result = client.complete_with_solvent(&request, &mut solvent).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        match &response.value().content {
            MessageContent::Assistant { blocks, .. } => {
                assert!(!blocks.is_empty());
            }
            _ => panic!("Expected assistant message"),
        }
    }
}
