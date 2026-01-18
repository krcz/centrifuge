use polyepoxide_core::{oxide, Bond};

use crate::content::MessageContent;
use crate::metadata::MessageMetadata;

/// A message in a conversation history.
///
/// Messages form a singly linked list via `previous`, enabling tree structures
/// where multiple messages can branch from a common ancestor.
#[oxide]
pub struct Message {
    pub content: MessageContent,
    pub metadata: Option<MessageMetadata>,
    /// Link to the previous message in the conversation.
    pub previous: Option<Bond<Message>>,
}
