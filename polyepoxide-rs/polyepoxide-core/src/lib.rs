//! Polyepoxide is a Merkle DAG-based synchronization database.
//!
//! Core concepts:
//! - **Oxide**: A value that can be stored in the DAG (content-addressable, serializable)
//! - **Cid**: A content identifier uniquely identifying an oxide (via IPLD CID standard)
//! - **Cell**: Wraps an oxide with cached CID computation
//! - **Bond**: A typed reference to another oxide (resolved or unresolved)
//! - **Solvent**: Manages oxides in memory and coordinates loading from stores
//!
//! # Example
//!
//! ```
//! use polyepoxide_core::{Solvent, Bond};
//!
//! let solvent = Solvent::new();
//!
//! // Add values to the solvent
//! let cell = solvent.add("hello world".to_string());
//! println!("CID: {}", cell.cid());
//!
//! // Create bonds to reference other values
//! let bond = solvent.bond(42u64);
//! assert!(bond.is_resolved());
//! ```
//!
//! # Serialization
//!
//! Polyepoxide uses DAG-CBOR (via `serde_ipld_dagcbor`) for deterministic serialization.
//! This ensures:
//! - Canonical map key ordering (RFC 8949 §4.2.1)
//! - CBOR tag 42 for CID links
//! - Consistent content addressing across implementations

mod async_store;
mod bond;
mod cell;
mod common;
mod cursor;
mod dyn_bond;
mod oxide;
mod reflexive;
mod schema;
pub mod serde_helpers;
mod solvent;
mod store;
mod sync;
pub mod traverse;

pub use async_store::{AsyncStore, IdentityAsyncStoreOverlay, identity_overlay_async};
pub use bond::{Bond, ErasedBond};
pub use cell::{Cell, ErasedCell};
pub use cid::Cid;
pub use common::Catalogue;
pub use cursor::{Cursor, CursorError};
pub use dyn_bond::{DynBond, DynBondError};
pub use oxide::{BondVisitor, ByteString, Oxide, compute_cid};
pub use reflexive::{
    Ligation, MULTIHASH_IDENTITY, POLYEPOXIDE_REFLEXIVE_CODEC, data_to_reflexive_cid,
    is_identity_cid, is_reflexive_cid, ligase_cid, ligation_cid, make_identity_cid,
    reflexive_to_data_cid, resolve_ligation, resolve_ligation_bond, resolve_reflexive_with_store,
    slot_cid, with_codec,
};
pub use schema::{FloatType, IntType, Structure};
pub use solvent::{Solvent, SolventError};
pub use store::{
    IdentityStoreOverlay, MemoryStore, Store, identity_digest_from_key, identity_overlay,
    key_from_cid,
};
pub use sync::{SyncError, pull, push};

#[cfg(feature = "derive")]
pub use polyepoxide_derive::{Oxide, oxide};
