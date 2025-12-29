use polyepoxide_core::{oxide, Bond};

use crate::item::ItemId;

/// Types of events that can occur
#[derive(PartialEq, Eq)]
#[oxide]
pub enum EventKind {
    ItemCreated,
    ItemScanned,
    PhotoAdded,
    ItemPlaced,
    ItemRemoved,
}

/// A logged event
#[oxide]
pub struct Event {
    pub item_id: ItemId,
    pub kind: EventKind,
    pub timestamp: u64, // Unix timestamp millis
    pub target_id: Option<ItemId>, // Related item (for placement events)
    pub note: Option<String>,
}

/// Event log
#[oxide]
pub struct EventLog {
    pub events: Vec<Bond<Event>>,
}
