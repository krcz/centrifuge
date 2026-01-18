use polyepoxide_core::{oxide, Bond};
use polyepoxide_llm::{GenerationParams, Message};

/// Definition of a tool that can be used by the model.
#[oxide]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema as a JSON string.
    pub parameters: String,
}

/// Strategy for tool selection.
#[oxide]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Specific { name: String },
}

/// A request to the OpenRouter API.
#[oxide]
pub struct OpenRouterRequest {
    pub model: String,
    /// Last message in the conversation (history via previous bonds).
    pub conversation_head: Bond<Message>,
    /// Generation parameters.
    pub params: Option<GenerationParams>,
    /// Available tools.
    pub tools: Vec<ToolDefinition>,
    /// Tool choice strategy.
    pub tool_choice: Option<ToolChoice>,
}
