# POLYEPOXIDE-LLM

## Dependencies

- `11-POLYEPOXIDE-MODEL`

## Intent

Define portable oxide shapes for LLM conversation history, multimodal message content, tool calls, tool results, generation parameters, and usage metadata.

The component stores conversation state, not provider behavior. It should be suitable for preserving prompts and responses from many providers while keeping the conversation graph content-addressed, deduplicable, syncable, and inspectable by generic Polyepoxide tools.

## Conceptual Model

An LLM conversation is represented as a content-addressed chain of `Message` oxides. Each message optionally points to the previous message through `previous: optional<Bond<Message>>`.

The tip message is the entry point for a thread. The full history is reachable by following `previous` bonds until a message has no predecessor. There is no separate portable "conversation" root type in this component.

Branching is represented structurally: multiple messages can point to the same predecessor bond. Those continuations share the same ancestor history while remaining independent content-addressed tips.

This model records the shape and metadata of a conversation. Model execution, prompt rendering, provider API integration, and tool execution are outside this component.

## Standard Interfaces

Language implementations should expose interfaces equivalent to the following pseudocode. Names may follow host-language conventions, but the oxide shapes and graph semantics should match.

```text
type Message:
  content: MessageContent
  metadata: optional<MessageMetadata>
  previous: optional<Bond<Message>>

enum MessageContent:
  System(blocks: list<ContentBlock>)
  User(blocks: list<ContentBlock>)
  Assistant(blocks: list<ContentBlock>, tool_calls: list<ToolCall>)
  ToolResult(tool_call_id: string, result: string, is_error: bool)

enum ContentBlock:
  Text(text: string)
  Image(image: ImageData)
  Code(language: optional<string>, code: string)
  File(name: string, mime_type: optional<string>, data: ByteString)
  Thinking(text: string)

enum ImageData:
  Url(url: string, detail: optional<string>)
  Embedded(media_type: string, data: ByteString)

type ToolCall:
  id: string
  name: string
  arguments: string

type MessageMetadata:
  model: optional<string>
  timestamp_ms: optional<uint64>
  generation_params: optional<GenerationParams>
  stop_reason: optional<string>
  usage: optional<TokenUsage>

type GenerationParams:
  temperature: optional<float64>
  top_p: optional<float64>
  top_k: optional<uint32>
  max_tokens: optional<uint32>
  frequency_penalty: optional<float64>
  presence_penalty: optional<float64>
  stop: optional<list<string>>
  min_p: optional<float64>
  top_a: optional<float64>
  repetition_penalty: optional<float64>
  seed: optional<uint64>
  reasoning_effort: optional<string>
  reasoning_max_tokens: optional<uint32>

type TokenUsage:
  input_tokens: optional<uint64>
  output_tokens: optional<uint64>
  cache_read_tokens: optional<uint64>
  cache_creation_tokens: optional<uint64>
```

`Message` is an oxide. Its recursive `previous` field is represented with `Bond<Message>`, using the recursive schema mechanisms from `11-POLYEPOXIDE-MODEL`.

`MessageContent` is a tagged union over conversation roles:

- `System` stores system or developer instructions.
- `User` stores user input.
- `Assistant` stores model output and any tool calls requested by the model.
- `ToolResult` stores the result associated with a previous tool call.

`ContentBlock` allows message bodies to contain structured multimodal content. `Thinking` records model reasoning/thinking output when an application chooses to preserve it.

`ImageData::Url` references remote image content and may include a provider-style detail hint. `ImageData::Embedded` stores image bytes directly with their media type.

`ToolCall.arguments` and `ToolResult.result` are raw strings. They are usually JSON, but the portable model does not require a particular schema or parser because providers and tools may use different conventions.

`MessageMetadata` fields are optional because providers expose different subsets of model, timing, generation, stopping, and usage information. Metadata may appear on any message when useful, although assistant messages are the common case.

## Conversation Semantics

A conversation thread is identified by its tip `Message` bond or CID. Following `previous` reconstructs the history in reverse chronological order.

Two messages with equal `previous` CIDs are branches from the same conversation state. Since messages are oxides, shared ancestors are naturally content-addressed and can be deduplicated by Polyepoxide runtimes and stores.

Tool calls and tool results are connected by `ToolCall.id` and `ToolResult.tool_call_id`. This is an application-level relationship inside the message chain, not a Polyepoxide graph edge.

## Design Choices

- Conversation shape is represented as immutable oxide data rather than mutable provider conversation IDs.
- Branching is structural: shared predecessor bonds encode shared history.
- Tool arguments and results are opaque strings so provider-specific JSON or non-JSON payloads can pass through unchanged.
- Metadata is optional and extensible in practice because provider capabilities and naming vary.
- The component records reasoning/thinking content only as data supplied by an application or provider; it does not define policy for when such content should be produced, hidden, or stored.

## Out of Scope

- Model inference and streaming.
- Provider API clients.
- Prompt formatting or provider-specific transcript rendering.
- Tool registration, execution, authorization, and schema validation.
- Higher-level conversation indexing, search, summarization, or memory management.
