//! LLM conversation history as a content-addressed DAG.
//!
//! This crate provides types for representing conversation history with LLMs,
//! stored as content-addressed structures using polyepoxide-core.
//!
//! Conversations are modeled as a singly linked list of messages, where each
//! message references its predecessor via a `Bond<Message>`. This enables:
//! - Efficient storage with deduplication
//! - Tree-structured conversations (branching)
//! - Content-addressable message references

mod content;
mod message;
mod metadata;
mod tool;

pub use content::{ContentBlock, ImageData, MessageContent};
pub use message::Message;
pub use metadata::{GenerationParams, MessageMetadata, TokenUsage};
pub use tool::ToolCall;

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::{Bond, Oxide, Solvent};
    use std::sync::Arc;

    #[test]
    fn single_message_roundtrip() {
        let msg = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Hello!".to_string())]),
            metadata: None,
            previous: None,
        };

        let bytes = msg.to_bytes();
        let recovered: Message = Message::from_bytes(&bytes).unwrap();

        match &recovered.content {
            MessageContent::User(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Text(text) => assert_eq!(text, "Hello!"),
                    _ => panic!("Expected Text block"),
                }
            }
            _ => panic!("Expected User message"),
        }
        assert!(recovered.previous.is_none());
    }

    #[test]
    fn conversation_chain() {
        let mut solvent = Solvent::new();

        let msg1 = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Hi".to_string())]),
            metadata: None,
            previous: None,
        };
        let cell1 = solvent.add(msg1);

        let msg2 = Message {
            content: MessageContent::Assistant {
                blocks: vec![ContentBlock::Text("Hello!".to_string())],
                tool_calls: vec![],
            },
            metadata: Some(MessageMetadata {
                model: Some("gpt-4".to_string()),
                timestamp_ms: Some(1700000000000),
                generation_params: None,
                stop_reason: Some("end_turn".to_string()),
                usage: None,
            }),
            previous: Some(Bond::from_cell(Arc::clone(&cell1))),
        };
        let cell2 = solvent.add(msg2);

        // Verify chain
        let msg = cell2.value();
        assert!(msg.previous.is_some());

        let prev = msg.previous.as_ref().unwrap();
        assert!(prev.is_resolved());
        match &prev.value().unwrap().content {
            MessageContent::User(blocks) => match &blocks[0] {
                ContentBlock::Text(text) => assert_eq!(text, "Hi"),
                _ => panic!("Expected Text"),
            },
            _ => panic!("Expected User"),
        }
    }

    #[test]
    fn branching_conversation() {
        let mut solvent = Solvent::new();

        // Common ancestor
        let base = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Question?".to_string())]),
            metadata: None,
            previous: None,
        };
        let base_cell = solvent.add(base);

        // Two different responses branching from the same message
        let branch_a = Message {
            content: MessageContent::Assistant {
                blocks: vec![ContentBlock::Text("Answer A".to_string())],
                tool_calls: vec![],
            },
            metadata: None,
            previous: Some(Bond::from_cell(Arc::clone(&base_cell))),
        };
        let branch_b = Message {
            content: MessageContent::Assistant {
                blocks: vec![ContentBlock::Text("Answer B".to_string())],
                tool_calls: vec![],
            },
            metadata: None,
            previous: Some(Bond::from_cell(Arc::clone(&base_cell))),
        };

        let cell_a = solvent.add(branch_a);
        let cell_b = solvent.add(branch_b);

        // Both branches point to the same predecessor
        assert_eq!(
            cell_a.value().previous.as_ref().unwrap().cid(),
            cell_b.value().previous.as_ref().unwrap().cid()
        );

        // 3 messages total (base + 2 branches)
        assert_eq!(solvent.len(), 3);
    }

    #[test]
    fn tool_call_roundtrip() {
        let msg = Message {
            content: MessageContent::Assistant {
                blocks: vec![ContentBlock::Text("Let me check that.".to_string())],
                tool_calls: vec![ToolCall {
                    id: "call_123".to_string(),
                    name: "get_weather".to_string(),
                    arguments: r#"{"location": "Paris"}"#.to_string(),
                }],
            },
            metadata: None,
            previous: None,
        };

        let bytes = msg.to_bytes();
        let recovered: Message = Message::from_bytes(&bytes).unwrap();

        match &recovered.content {
            MessageContent::Assistant { blocks, tool_calls } => {
                assert_eq!(blocks.len(), 1);
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].id, "call_123");
                assert_eq!(tool_calls[0].name, "get_weather");
                assert_eq!(tool_calls[0].arguments, r#"{"location": "Paris"}"#);
            }
            _ => panic!("Expected Assistant message"),
        }
    }

    #[test]
    fn tool_result_roundtrip() {
        let msg = Message {
            content: MessageContent::ToolResult {
                tool_call_id: "call_123".to_string(),
                result: r#"{"temperature": 22, "unit": "celsius"}"#.to_string(),
                is_error: false,
            },
            metadata: None,
            previous: None,
        };

        let bytes = msg.to_bytes();
        let recovered: Message = Message::from_bytes(&bytes).unwrap();

        match &recovered.content {
            MessageContent::ToolResult {
                tool_call_id,
                result,
                is_error,
            } => {
                assert_eq!(tool_call_id, "call_123");
                assert_eq!(result, r#"{"temperature": 22, "unit": "celsius"}"#);
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult message"),
        }
    }

    #[test]
    fn rich_content_blocks() {
        use polyepoxide_core::ByteString;

        let msg = Message {
            content: MessageContent::User(vec![
                ContentBlock::Text("Here's an image:".to_string()),
                ContentBlock::Image(ImageData::Url {
                    url: "https://example.com/image.png".to_string(),
                    detail: Some("high".to_string()),
                }),
                ContentBlock::Code {
                    language: Some("python".to_string()),
                    code: "print('hello')".to_string(),
                },
                ContentBlock::File {
                    name: "data.csv".to_string(),
                    mime_type: Some("text/csv".to_string()),
                    data: ByteString::new(b"a,b,c\n1,2,3".to_vec()),
                },
            ]),
            metadata: None,
            previous: None,
        };

        let bytes = msg.to_bytes();
        let recovered: Message = Message::from_bytes(&bytes).unwrap();

        match &recovered.content {
            MessageContent::User(blocks) => {
                assert_eq!(blocks.len(), 4);

                match &blocks[0] {
                    ContentBlock::Text(t) => assert_eq!(t, "Here's an image:"),
                    _ => panic!("Expected Text"),
                }

                match &blocks[1] {
                    ContentBlock::Image(ImageData::Url { url, detail }) => {
                        assert_eq!(url, "https://example.com/image.png");
                        assert_eq!(detail.as_deref(), Some("high"));
                    }
                    _ => panic!("Expected Image Url"),
                }

                match &blocks[2] {
                    ContentBlock::Code { language, code } => {
                        assert_eq!(language.as_deref(), Some("python"));
                        assert_eq!(code, "print('hello')");
                    }
                    _ => panic!("Expected Code"),
                }

                match &blocks[3] {
                    ContentBlock::File {
                        name,
                        mime_type,
                        data,
                    } => {
                        assert_eq!(name, "data.csv");
                        assert_eq!(mime_type.as_deref(), Some("text/csv"));
                        assert_eq!(data.as_bytes(), b"a,b,c\n1,2,3");
                    }
                    _ => panic!("Expected File"),
                }
            }
            _ => panic!("Expected User message"),
        }
    }

    #[test]
    fn metadata_roundtrip() {
        let msg = Message {
            content: MessageContent::Assistant {
                blocks: vec![ContentBlock::Text("Response".to_string())],
                tool_calls: vec![],
            },
            metadata: Some(MessageMetadata {
                model: Some("claude-3-opus".to_string()),
                timestamp_ms: Some(1700000000000),
                generation_params: Some(GenerationParams {
                    temperature: Some(0.7),
                    top_p: Some(0.9),
                    top_k: Some(40),
                    max_tokens: Some(1024),
                    frequency_penalty: None,
                    presence_penalty: None,
                    stop: Some(vec!["END".to_string()]),
                    min_p: None,
                    top_a: None,
                    repetition_penalty: None,
                    seed: None,
                    reasoning_effort: None,
                    reasoning_max_tokens: None,
                }),
                stop_reason: Some("end_turn".to_string()),
                usage: Some(TokenUsage {
                    input_tokens: Some(100),
                    output_tokens: Some(50),
                    cache_read_tokens: Some(80),
                    cache_creation_tokens: Some(20),
                }),
            }),
            previous: None,
        };

        let bytes = msg.to_bytes();
        let recovered: Message = Message::from_bytes(&bytes).unwrap();

        let meta = recovered.metadata.as_ref().unwrap();
        assert_eq!(meta.model.as_deref(), Some("claude-3-opus"));
        assert_eq!(meta.timestamp_ms, Some(1700000000000));

        let params = meta.generation_params.as_ref().unwrap();
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.top_k, Some(40));
        assert_eq!(params.stop.as_ref().unwrap(), &vec!["END".to_string()]);

        let usage = meta.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.cache_read_tokens, Some(80));
    }
}
