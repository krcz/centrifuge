//! Aldehyde Inventory - Home inventory system built on Polyepoxide

pub mod event;
pub mod inventory;
pub mod item;
pub mod placement;

pub use event::{Event, EventKind, EventLog};
pub use inventory::{Inventory, ItemPhotos, PhotoRegistry};
pub use item::{Item, ItemId};
pub use placement::{Placement, PlacementMap};

// Re-export photo types from core
pub use aldehyde_core::{ExifData, ExifTag, ExifValue, Photo};
