//! Polyepoxide is a Merkle DAG-based synchronization database.
//!
//! Core concepts:
//! - **Oxide**: A value that can be stored in the DAG (content-addressable, serializable)
//! - **Key**: A cryptographic hash uniquely identifying an oxide
//! - **Cell**: Wraps an oxide with cached hash computation
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
//! println!("Key: {}", cell.key());
//!
//! // Create bonds to reference other values
//! let bond = solvent.bond(42u64);
//! assert!(bond.is_resolved());
//! ```
//!
//! # Canonicalization Note
//!
//! **WARNING**: Canonical CBOR serialization is not yet fully implemented.
//!
//! **Map key ordering**: The design document specifies that `Map` entries should be
//! sorted by key for deterministic hashing (RFC 8949 §4.2.1: sort by serialized length
//! first, then lexicographically). Currently, serde/ciborium does NOT sort map keys
//! automatically. This means:
//! - `HashMap<K, V>` serialization order is non-deterministic
//! - Two logically equal maps may produce different CBOR bytes and different content hashes
//!
//! **Workarounds**:
//! - Pre-sort map entries before serialization
//! - Use `BTreeMap` for naturally sorted keys (lexicographic on Debug representation)
//!
//! A proper fix requires either custom serialization or post-processing CBOR bytes.

mod async_store;
mod bond;
mod cell;
mod key;
mod oxide;
mod schema;
pub mod serde_helpers;
mod solvent;
mod store;
mod sync;

pub use async_store::AsyncStore;
pub use bond::Bond;
pub use cell::Cell;
pub use key::Key;
pub use oxide::{BondMapper, BondVisitor, ByteString, Oxide};
pub use schema::{FloatType, IntType, Structure};
pub use solvent::{Solvent, SolventError};
pub use store::{MemoryStore, Store};
pub use sync::{pull, push, SyncError};

#[cfg(feature = "derive")]
pub use polyepoxide_derive::{oxide, Oxide};
