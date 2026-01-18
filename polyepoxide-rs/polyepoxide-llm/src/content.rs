use polyepoxide_core::{oxide, ByteString};

use crate::tool::ToolCall;

/// Image data, either as a URL or embedded bytes.
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

/// A block of content within a message.
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
    /// Model's internal reasoning/thinking output.
    Thinking(String),
}

/// The content of a message, categorized by role.
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
