//! OpenRouter API client for the polyepoxide ecosystem.
//!
//! This crate provides integration between polyepoxide's content-addressed
//! conversation history and the OpenRouter API for LLM completions.
//!
//! # Example
//!
//! ```ignore
//! use polyepoxide_core::{Bond, Solvent};
//! use polyepoxide_llm::{ContentBlock, Message, MessageContent};
//! use silane_openrouter::{OpenRouterClient, OpenRouterRequest};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = OpenRouterClient::new("your-api-key");
//!     let mut solvent = Solvent::new();
//!
//!     // Create a user message
//!     let user_msg = Message {
//!         content: MessageContent::User(vec![
//!             ContentBlock::Text("Hello!".to_string())
//!         ]),
//!         metadata: None,
//!         previous: None,
//!     };
//!     let cell = solvent.add(user_msg);
//!
//!     // Build and send request
//!     let request = OpenRouterRequest {
//!         model: "openai/gpt-4o".to_string(),
//!         conversation_head: Bond::from_cell(Arc::clone(&cell)),
//!         params: None,
//!         tools: vec![],
//!         tool_choice: None,
//!     };
//!
//!     let response = client.complete_with_solvent(&request, &mut solvent).await.unwrap();
//! }
//! ```

mod client;
mod convert;
mod error;
mod types;

pub use client::OpenRouterClient;
pub use convert::{build_request_body, collect_messages, parse_response};
pub use error::OpenRouterError;
pub use types::{OpenRouterRequest, ToolChoice, ToolDefinition};
