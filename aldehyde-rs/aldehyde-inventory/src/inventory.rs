use aldehyde_core::Photo;
use polyepoxide_core::{oxide, Bond};

use crate::event::EventLog;
use crate::item::{Item, ItemId};
use crate::placement::PlacementMap;

/// Association between an item and its photos
#[oxide]
pub struct ItemPhotos {
    pub item_id: ItemId,
    pub photos: Vec<Bond<Photo>>,
}

/// All photo attachments
#[oxide]
pub struct PhotoRegistry {
    pub attachments: Vec<Bond<ItemPhotos>>,
}

/// Root of the inventory system
#[oxide]
pub struct Inventory {
    pub items: Vec<Bond<Item>>,
    pub placements: Bond<PlacementMap>,
    pub photos: Bond<PhotoRegistry>,
    pub events: Bond<EventLog>,
}
