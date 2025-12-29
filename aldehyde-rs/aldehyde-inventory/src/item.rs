use polyepoxide_core::oxide;

/// Stable identifier for items across versions (UUID format)
pub type ItemId = String;

/// Core item in the inventory
#[oxide]
pub struct Item {
    pub id: ItemId,
    pub name: String,
    pub description: Option<String>,
}
