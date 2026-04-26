# POLYEPOXIDE-RS-LLM

## Dependencies

- `12-POLYEPOXIDE-LLM`
- `31-POLYEPOXIDE-RS-MODEL`

## Intent

Describe how the portable LLM data model from `12-POLYEPOXIDE-LLM` is represented in Rust through the `polyepoxide-llm` crate.

This spec records the Rust type shapes, derive behavior, schema-relevant encoding choices, and current behavioral coverage. It is not a provider API client spec and does not define model execution, prompt rendering, or tool execution.

## Crates and Modules

- `polyepoxide_llm`: crate exports and behavioral tests.
- `polyepoxide_llm::message`: `Message`.
- `polyepoxide_llm::content`: `ImageData`, `ContentBlock`, `MessageContent`.
- `polyepoxide_llm::tool`: `ToolCall`.
- `polyepoxide_llm::metadata`: `GenerationParams`, `TokenUsage`, `MessageMetadata`.

The crate depends on `polyepoxide-core` for `#[oxide]`, `Oxide`, `Bond<T>`, and `ByteString`.

## Rust Type Encoding

All public data model types use `#[oxide]`. The macro generates `Debug`, `Clone`, `Serialize`, `Deserialize`, and `Oxide` implementations, plus schema generation, bond traversal, and solvent dissolution behavior as described in `31-POLYEPOXIDE-RS-MODEL`.

### `Message`

```rust
#[oxide]
pub struct Message {
    pub content: MessageContent,
    pub metadata: Option<MessageMetadata>,
    pub previous: Option<Bond<Message>>,
}
```

`Message` is the oxide node for a conversation entry. `previous: Option<Bond<Message>>` makes the type self-referential. The derived schema uses `Slot(0)` for the recursive reference and wraps the schema root in a `Ligase`.

`metadata` and `previous` are named `Option` fields. The `#[oxide]` macro applies record-field option encoding: `None` is omitted and `Some(value)` is encoded as the direct inner value.

### `MessageContent`

```rust
#[oxide]
pub enum MessageContent {
    System(Vec<ContentBlock>),
    User(Vec<ContentBlock>),
    Assistant {
        blocks: Vec<ContentBlock>,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        tool_call_id: String,
        result: String,
        is_error: bool,
    },
}
```

`MessageContent` is encoded as `Structure::Tagged`. Variant order is schema-relevant and follows the Rust declaration order.

### `ContentBlock`

```rust
#[oxide]
pub enum ContentBlock {
    Text(String),
    Image(ImageData),
    Code {
        language: Option<String>,
        code: String,
    },
    File {
        name: String,
        mime_type: Option<String>,
        data: ByteString,
    },
    Thinking(String),
}
```

`ContentBlock` is encoded as `Structure::Tagged`. `File.data` uses `ByteString` rather than `Vec<u8>` so the generated schema is `Structure::ByteString` instead of a sequence of integers.

### `ImageData`

```rust
#[oxide]
pub enum ImageData {
    Url {
        url: String,
        detail: Option<String>,
    },
    Embedded {
        media_type: String,
        data: ByteString,
    },
}
```

`ImageData` is encoded as `Structure::Tagged`. Embedded image bytes use `ByteString` for the same schema reason as file bytes.

### `ToolCall`

```rust
#[oxide]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}
```

`arguments` is a raw string. It is usually JSON in provider integrations, but the Rust data model does not parse or validate it.

### `MessageMetadata`

```rust
#[oxide]
pub struct MessageMetadata {
    pub model: Option<String>,
    pub timestamp_ms: Option<u64>,
    pub generation_params: Option<GenerationParams>,
    pub stop_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}
```

All fields are optional because providers expose different metadata. As named `Option` fields, they use record-field option encoding.

### `GenerationParams`

```rust
#[oxide]
pub struct GenerationParams {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub max_tokens: Option<u32>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub stop: Option<Vec<String>>,
    pub min_p: Option<f64>,
    pub top_a: Option<f64>,
    pub repetition_penalty: Option<f64>,
    pub seed: Option<u64>,
    pub reasoning_effort: Option<String>,
    pub reasoning_max_tokens: Option<u32>,
}
```

These fields are pass-through generation settings. The crate stores them as provider-neutral metadata and does not validate provider-specific ranges or accepted string values.

### `TokenUsage`

```rust
#[oxide]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
}
```

Token counts use `u64` to avoid coupling persisted data to small provider limits.

## Conversation Behavior In Rust

Conversation history uses `Bond<Message>` exactly as described by the portable LLM spec. A tip `Message` is enough to identify a thread. Branches are represented by two or more messages whose `previous` bonds have the same CID.

When messages are inserted into a `Solvent`, shared predecessors deduplicate through normal Polyepoxide CID behavior. This crate does not provide a separate conversation runtime or mutable conversation identifier.

Tool calls and tool results are linked by `ToolCall.id` and `MessageContent::ToolResult.tool_call_id`. That relationship is ordinary string data, not a `Bond`.

## Rust Design Choices

- `#[oxide]` keeps LLM values close to ordinary Rust structs and enums while deriving Polyepoxide schema, serialization, traversal, and dissolution behavior.
- `ByteString` is used for embedded file and image bytes so schema generation preserves byte-oriented encoding.
- `ToolCall.arguments` and `ToolResult.result` are raw `String` values so provider-specific payloads pass through unchanged.
- `Thinking` is the Rust variant name for model reasoning/thinking content.
- The crate models durable conversation data only; execution concerns stay outside the crate.

## Existing Behavioral Coverage

Current crate tests cover:

- single message round trip
- conversation chains
- branching histories
- assistant tool calls
- tool results
- rich content blocks
- message metadata round trip

## Out of Scope

- Provider API clients.
- Model inference and streaming.
- Prompt formatting or provider-specific transcript rendering.
- Tool registration, execution, authorization, and schema validation.
- Store synchronization and document import/export beyond the behavior inherited from the core oxide model.
