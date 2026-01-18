use base64::Engine;
use polyepoxide_core::Bond;
use polyepoxide_llm::{
    ContentBlock, GenerationParams, ImageData, Message, MessageContent, MessageMetadata,
    TokenUsage, ToolCall,
};
use serde_json::{json, Value};

use crate::error::OpenRouterError;
use crate::types::{OpenRouterRequest, ToolChoice, ToolDefinition};

/// Collects messages from the conversation chain, oldest first.
pub fn collect_messages(head: &Bond<Message>) -> Result<Vec<&Message>, OpenRouterError> {
    let mut messages = Vec::new();
    let mut current = Some(head);

    while let Some(bond) = current {
        let msg = bond.value().ok_or_else(|| OpenRouterError::UnresolvedBond(bond.cid()))?;
        messages.push(msg);
        current = msg.previous.as_ref();
    }

    messages.reverse();
    Ok(messages)
}

/// Converts a ContentBlock to OpenRouter JSON format.
fn content_block_to_json(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text(text) => json!({
            "type": "text",
            "text": text
        }),
        ContentBlock::Image(ImageData::Url { url, detail }) => {
            let mut image_url = json!({ "url": url });
            if let Some(d) = detail {
                image_url["detail"] = json!(d);
            }
            json!({
                "type": "image_url",
                "image_url": image_url
            })
        }
        ContentBlock::Image(ImageData::Embedded { media_type, data }) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(data.as_bytes());
            json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:{};base64,{}", media_type, b64)
                }
            })
        }
        ContentBlock::Code { language, code } => {
            let lang = language.as_deref().unwrap_or("");
            json!({
                "type": "text",
                "text": format!("```{}\n{}\n```", lang, code)
            })
        }
        ContentBlock::File {
            name,
            mime_type,
            data,
        } => {
            // Represent file as text if possible
            if let Ok(text) = std::str::from_utf8(data.as_bytes()) {
                let mime = mime_type.as_deref().unwrap_or("text/plain");
                json!({
                    "type": "text",
                    "text": format!("[File: {} ({})]:\n{}", name, mime, text)
                })
            } else {
                let b64 = base64::engine::general_purpose::STANDARD.encode(data.as_bytes());
                let mime = mime_type.as_deref().unwrap_or("application/octet-stream");
                json!({
                    "type": "text",
                    "text": format!("[Binary file: {} ({})]: base64:{}", name, mime, b64)
                })
            }
        }
        ContentBlock::Thinking(text) => json!({
            "type": "text",
            "text": format!("<thinking>\n{}\n</thinking>", text)
        }),
    }
}

/// Converts a ToolCall to OpenRouter JSON format.
fn tool_call_to_json(call: &ToolCall) -> Value {
    json!({
        "id": call.id,
        "type": "function",
        "function": {
            "name": call.name,
            "arguments": call.arguments
        }
    })
}

/// Converts a Message to OpenRouter JSON format.
fn message_to_json(msg: &Message) -> Value {
    match &msg.content {
        MessageContent::System(blocks) => {
            let content: Vec<Value> = blocks.iter().map(content_block_to_json).collect();
            json!({
                "role": "system",
                "content": content
            })
        }
        MessageContent::User(blocks) => {
            let content: Vec<Value> = blocks.iter().map(content_block_to_json).collect();
            json!({
                "role": "user",
                "content": content
            })
        }
        MessageContent::Assistant { blocks, tool_calls } => {
            let content: Vec<Value> = blocks.iter().map(content_block_to_json).collect();
            let mut msg_json = json!({
                "role": "assistant",
                "content": content
            });
            if !tool_calls.is_empty() {
                msg_json["tool_calls"] =
                    Value::Array(tool_calls.iter().map(tool_call_to_json).collect());
            }
            msg_json
        }
        MessageContent::ToolResult {
            tool_call_id,
            result,
            is_error,
        } => {
            let mut msg_json = json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": result
            });
            if *is_error {
                msg_json["is_error"] = json!(true);
            }
            msg_json
        }
    }
}

/// Converts a ToolDefinition to OpenRouter JSON format.
fn tool_definition_to_json(tool: &ToolDefinition) -> Value {
    let parameters: Value = serde_json::from_str(&tool.parameters).unwrap_or(json!({}));
    let mut function = json!({
        "name": tool.name,
        "parameters": parameters
    });
    if let Some(desc) = &tool.description {
        function["description"] = json!(desc);
    }
    json!({
        "type": "function",
        "function": function
    })
}

/// Converts ToolChoice to OpenRouter JSON format.
fn tool_choice_to_json(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Specific { name } => json!({
            "type": "function",
            "function": { "name": name }
        }),
    }
}

/// Builds the full OpenRouter API request body.
pub fn build_request_body(request: &OpenRouterRequest) -> Result<Value, OpenRouterError> {
    let messages = collect_messages(&request.conversation_head)?;
    let messages_json: Vec<Value> = messages.iter().map(|m| message_to_json(m)).collect();

    let mut body = json!({
        "model": request.model,
        "messages": messages_json
    });

    if let Some(params) = &request.params {
        apply_generation_params(&mut body, params);
    }

    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.iter().map(tool_definition_to_json).collect());
    }

    if let Some(choice) = &request.tool_choice {
        body["tool_choice"] = tool_choice_to_json(choice);
    }

    Ok(body)
}

/// Applies GenerationParams to the request body.
fn apply_generation_params(body: &mut Value, params: &GenerationParams) {
    if let Some(temp) = params.temperature {
        body["temperature"] = json!(temp);
    }
    if let Some(top_p) = params.top_p {
        body["top_p"] = json!(top_p);
    }
    if let Some(top_k) = params.top_k {
        body["top_k"] = json!(top_k);
    }
    if let Some(max_tokens) = params.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(freq_pen) = params.frequency_penalty {
        body["frequency_penalty"] = json!(freq_pen);
    }
    if let Some(pres_pen) = params.presence_penalty {
        body["presence_penalty"] = json!(pres_pen);
    }
    if let Some(stop) = &params.stop {
        body["stop"] = json!(stop);
    }
    if let Some(min_p) = params.min_p {
        body["min_p"] = json!(min_p);
    }
    if let Some(top_a) = params.top_a {
        body["top_a"] = json!(top_a);
    }
    if let Some(rep_pen) = params.repetition_penalty {
        body["repetition_penalty"] = json!(rep_pen);
    }
    if let Some(seed) = params.seed {
        body["seed"] = json!(seed);
    }

    // Reasoning/thinking parameters
    if params.reasoning_effort.is_some() || params.reasoning_max_tokens.is_some() {
        let mut reasoning = json!({});
        if let Some(effort) = &params.reasoning_effort {
            reasoning["effort"] = json!(effort);
        }
        if let Some(max_tokens) = params.reasoning_max_tokens {
            reasoning["max_tokens"] = json!(max_tokens);
        }
        body["reasoning"] = reasoning;
    }
}

/// Parses an OpenRouter API response into a Message.
pub fn parse_response(
    response: &Value,
    conversation_head: Bond<Message>,
) -> Result<Message, OpenRouterError> {
    let choice = response
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| OpenRouterError::Api {
            status: 0,
            message: "No choices in response".to_string(),
        })?;

    let msg = choice.get("message").ok_or_else(|| OpenRouterError::Api {
        status: 0,
        message: "No message in choice".to_string(),
    })?;

    let mut blocks = Vec::new();

    // Check for reasoning content
    if let Some(reasoning) = response.get("reasoning").and_then(|r| r.as_str()) {
        if !reasoning.is_empty() {
            blocks.push(ContentBlock::Thinking(reasoning.to_string()));
        }
    }

    // Parse content
    if let Some(content) = msg.get("content") {
        if let Some(text) = content.as_str() {
            if !text.is_empty() {
                blocks.push(ContentBlock::Text(text.to_string()));
            }
        } else if let Some(arr) = content.as_array() {
            for item in arr {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    blocks.push(ContentBlock::Text(text.to_string()));
                }
            }
        }
    }

    // Parse tool calls
    let mut tool_calls = Vec::new();
    if let Some(calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
        for call in calls {
            let id = call
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let function = call.get("function").ok_or_else(|| OpenRouterError::Api {
                status: 0,
                message: "No function in tool_call".to_string(),
            })?;
            let name = function
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = function
                .get("arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}")
                .to_string();

            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }
    }

    // Build metadata
    let model = response
        .get("model")
        .and_then(|m| m.as_str())
        .map(String::from);

    let stop_reason = choice
        .get("finish_reason")
        .and_then(|r| r.as_str())
        .map(String::from);

    let usage = response.get("usage").map(|u| TokenUsage {
        input_tokens: u.get("prompt_tokens").and_then(|t| t.as_u64()),
        output_tokens: u.get("completion_tokens").and_then(|t| t.as_u64()),
        cache_read_tokens: u
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|t| t.as_u64()),
        cache_creation_tokens: None,
    });

    let metadata = Some(MessageMetadata {
        model,
        timestamp_ms: None,
        generation_params: None,
        stop_reason,
        usage,
    });

    Ok(Message {
        content: MessageContent::Assistant { blocks, tool_calls },
        metadata,
        previous: Some(conversation_head),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::Solvent;
    use std::sync::Arc;

    #[test]
    fn test_collect_messages_single() {
        let mut solvent = Solvent::new();
        let msg = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Hello".to_string())]),
            metadata: None,
            previous: None,
        };
        let cell = solvent.add(msg);
        let bond = Bond::from_cell(Arc::clone(&cell));

        let collected = collect_messages(&bond).unwrap();
        assert_eq!(collected.len(), 1);
    }

    #[test]
    fn test_collect_messages_chain() {
        let mut solvent = Solvent::new();

        let msg1 = Message {
            content: MessageContent::User(vec![ContentBlock::Text("First".to_string())]),
            metadata: None,
            previous: None,
        };
        let cell1 = solvent.add(msg1);

        let msg2 = Message {
            content: MessageContent::Assistant {
                blocks: vec![ContentBlock::Text("Second".to_string())],
                tool_calls: vec![],
            },
            metadata: None,
            previous: Some(Bond::from_cell(Arc::clone(&cell1))),
        };
        let cell2 = solvent.add(msg2);

        let msg3 = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Third".to_string())]),
            metadata: None,
            previous: Some(Bond::from_cell(Arc::clone(&cell2))),
        };
        let cell3 = solvent.add(msg3);

        let bond = Bond::from_cell(Arc::clone(&cell3));
        let collected = collect_messages(&bond).unwrap();

        assert_eq!(collected.len(), 3);
        // Oldest first
        match &collected[0].content {
            MessageContent::User(blocks) => match &blocks[0] {
                ContentBlock::Text(t) => assert_eq!(t, "First"),
                _ => panic!("Expected text"),
            },
            _ => panic!("Expected user message"),
        }
    }

    #[test]
    fn test_message_to_json_user() {
        let msg = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Hello".to_string())]),
            metadata: None,
            previous: None,
        };
        let json = message_to_json(&msg);

        assert_eq!(json["role"], "user");
        assert!(json["content"].is_array());
    }

    #[test]
    fn test_message_to_json_assistant_with_tools() {
        let msg = Message {
            content: MessageContent::Assistant {
                blocks: vec![ContentBlock::Text("Let me check".to_string())],
                tool_calls: vec![ToolCall {
                    id: "call_123".to_string(),
                    name: "get_weather".to_string(),
                    arguments: r#"{"city": "Paris"}"#.to_string(),
                }],
            },
            metadata: None,
            previous: None,
        };
        let json = message_to_json(&msg);

        assert_eq!(json["role"], "assistant");
        assert!(json["tool_calls"].is_array());
        assert_eq!(json["tool_calls"][0]["id"], "call_123");
    }

    #[test]
    fn test_parse_response_simple() {
        let mut solvent = Solvent::new();
        let msg = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Hello".to_string())]),
            metadata: None,
            previous: None,
        };
        let cell = solvent.add(msg);
        let bond = Bond::from_cell(Arc::clone(&cell));

        let response = json!({
            "model": "gpt-4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hi there!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5
            }
        });

        let parsed = parse_response(&response, bond).unwrap();

        match &parsed.content {
            MessageContent::Assistant { blocks, tool_calls } => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Text(t) => assert_eq!(t, "Hi there!"),
                    _ => panic!("Expected text"),
                }
                assert!(tool_calls.is_empty());
            }
            _ => panic!("Expected assistant message"),
        }

        let meta = parsed.metadata.as_ref().unwrap();
        assert_eq!(meta.model.as_deref(), Some("gpt-4"));
        assert_eq!(meta.stop_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_parse_response_with_reasoning() {
        let mut solvent = Solvent::new();
        let msg = Message {
            content: MessageContent::User(vec![ContentBlock::Text("Hello".to_string())]),
            metadata: None,
            previous: None,
        };
        let cell = solvent.add(msg);
        let bond = Bond::from_cell(Arc::clone(&cell));

        let response = json!({
            "model": "deepseek-reasoner",
            "reasoning": "Let me think about this...",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "The answer is 42"
                },
                "finish_reason": "stop"
            }]
        });

        let parsed = parse_response(&response, bond).unwrap();

        match &parsed.content {
            MessageContent::Assistant { blocks, .. } => {
                assert_eq!(blocks.len(), 2);
                match &blocks[0] {
                    ContentBlock::Thinking(t) => assert_eq!(t, "Let me think about this..."),
                    _ => panic!("Expected thinking block first"),
                }
                match &blocks[1] {
                    ContentBlock::Text(t) => assert_eq!(t, "The answer is 42"),
                    _ => panic!("Expected text block second"),
                }
            }
            _ => panic!("Expected assistant message"),
        }
    }
}
