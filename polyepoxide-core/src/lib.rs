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
//! For deterministic content addressing, ensure that:
//! - Record fields are defined in consistent order (use IndexMap)
//! - Map keys are sorted before serialization when using unordered maps
//!
//! This will be addressed in a future version.

mod bond;
mod cell;
mod key;
mod oxide;
mod schema;
mod solvent;

pub use bond::Bond;
pub use cell::Cell;
pub use key::Key;
pub use oxide::{BondMapper, BondVisitor, ByteString, Oxide};
pub use schema::{FloatType, IntType, Structure};
pub use solvent::{Solvent, SolventError};
