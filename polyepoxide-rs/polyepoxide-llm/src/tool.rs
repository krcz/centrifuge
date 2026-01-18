use polyepoxide_core::oxide;

/// A tool call made by an assistant.
#[oxide]
pub struct ToolCall {
    /// Unique identifier for this tool call (used to match with ToolResult).
    pub id: String,
    /// Name of the tool being called.
    pub name: String,
    /// Arguments as raw JSON string.
    pub arguments: String,
}
