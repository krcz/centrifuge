use cid::Cid;
use std::any::Any;
use std::sync::Arc;
use std::sync::OnceLock;

use crate::oxide::{BondVisitor, Oxide};
use crate::solvent::Solvent;

/// A cell wraps an oxide value and caches its computed CID.
///
/// The CID is computed lazily on first access via `cid()`, then cached
/// for subsequent calls. This allows building large trees without computing
/// hashes until persistence or when the CID is actually needed.
pub struct Cell<T: Oxide> {
    value: T,
    cid: OnceLock<Cid>,
}

impl<T: Oxide> Cell<T> {
    /// Creates a new cell containing the given value.
    /// The CID is not computed until `cid()` is called.
    pub fn new(value: T) -> Self {
        Cell {
            value,
            cid: OnceLock::new(),
        }
    }

    /// Creates a new cell with a pre-computed CID.
    /// Use this when deserializing or when the CID is already known.
    pub fn with_cid(value: T, cid: Cid) -> Self {
        let cell = Cell {
            value,
            cid: OnceLock::new(),
        };
        let _ = cell.cid.set(cid);
        cell
    }

    /// Returns the content-addressed CID, computing it if necessary.
    pub fn cid(&self) -> Cid {
        *self.cid.get_or_init(|| self.value.compute_cid())
    }

    /// Returns a reference to the contained value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Consumes the cell and returns the contained value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T: Oxide> std::fmt::Debug for Cell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cell")
            .field("value", &self.value)
            .field("cid", &self.cid.get())
            .finish()
    }
}

/// A type-erased cell used by Solvent to store heterogeneous oxide values.
pub trait ErasedCell: Any + Send + Sync + std::fmt::Debug {
    /// Returns the CID of the underlying value.
    fn cid(&self) -> Cid;
    /// Returns the value as `Any` for type checks/downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Converts this arc into an `Any` arc to enable owned downcasting.
    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
    /// Returns a dissolved clone of this cell.
    fn dissolve_in(&self, solvent: &Solvent) -> Arc<dyn ErasedCell>;
    /// Serializes the underlying value.
    fn to_bytes(&self) -> Vec<u8>;
    /// Visits all nested bonds.
    fn visit_bonds(&self, visitor: &mut dyn BondVisitor);
}

impl<T: Oxide> ErasedCell for Cell<T> {
    fn cid(&self) -> Cid {
        Cell::cid(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Arc<dyn ErasedCell> {
        Arc::new(Cell::with_cid(self.value.dissolve_in(solvent), self.cid()))
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value.to_bytes()
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        self.value.visit_bonds(visitor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_lazy_hash() {
        let cell = Cell::new(42u64);
        // CID not computed yet (internal detail, but we can check via debug)
        let c1 = cell.cid();
        let c2 = cell.cid();
        assert_eq!(c1, c2);
    }

    #[test]
    fn cell_with_precomputed_cid() {
        let value = "test".to_string();
        let expected_cid = value.compute_cid();
        let cell = Cell::with_cid(value.clone(), expected_cid);
        assert_eq!(cell.cid(), expected_cid);
    }

    #[test]
    fn cell_value_access() {
        let cell = Cell::new("hello".to_string());
        assert_eq!(cell.value(), "hello");
    }
}
