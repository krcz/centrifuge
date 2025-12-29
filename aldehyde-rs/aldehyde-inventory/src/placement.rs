use polyepoxide_core::{oxide, Bond};

use crate::item::ItemId;

/// Placement of an item within the hierarchy
#[oxide]
pub struct Placement {
    pub item_id: ItemId,
    pub location_id: Option<ItemId>, // Parent container, None = top-level
}

/// Complete placement map for all items
#[oxide]
pub struct PlacementMap {
    pub placements: Vec<Bond<Placement>>,
}
