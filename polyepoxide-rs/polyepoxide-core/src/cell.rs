use cid::Cid;
use std::sync::OnceLock;

use crate::oxide::Oxide;

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
