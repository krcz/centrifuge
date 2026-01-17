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
//! let mut solvent = Solvent::new();
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
mod oxide;
mod schema;
pub mod serde_helpers;
mod solvent;
mod store;
mod sync;
pub mod traverse;

pub use async_store::AsyncStore;
pub use bond::Bond;
pub use cell::Cell;
pub use cid::Cid;
pub use oxide::{compute_cid, BondMapper, BondVisitor, ByteString, Oxide};
pub use schema::{FloatType, IntType, Structure};
pub use solvent::{Solvent, SolventError};
pub use store::{MemoryStore, Store};
pub use sync::{pull, push, SyncError};

#[cfg(feature = "derive")]
pub use polyepoxide_derive::{oxide, Oxide};
