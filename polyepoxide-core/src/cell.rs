use std::sync::OnceLock;

use crate::key::Key;
use crate::oxide::Oxide;

/// A cell wraps an oxide value and caches its computed hash.
///
/// The hash is computed lazily on first access via `key()`, then cached
/// for subsequent calls. This allows building large trees without computing
/// hashes until persistence or when the key is actually needed.
pub struct Cell<T: Oxide> {
    value: T,
    key: OnceLock<Key>,
}

impl<T: Oxide> Cell<T> {
    /// Creates a new cell containing the given value.
    /// The hash is not computed until `key()` is called.
    pub fn new(value: T) -> Self {
        Cell {
            value,
            key: OnceLock::new(),
        }
    }

    /// Creates a new cell with a pre-computed key.
    /// Use this when deserializing or when the key is already known.
    pub fn with_key(value: T, key: Key) -> Self {
        let cell = Cell {
            value,
            key: OnceLock::new(),
        };
        let _ = cell.key.set(key);
        cell
    }

    /// Returns the content-addressed key, computing it if necessary.
    pub fn key(&self) -> Key {
        *self.key.get_or_init(|| self.value.compute_key())
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
            .field("key", &self.key.get())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_lazy_hash() {
        let cell = Cell::new(42u64);
        // Key not computed yet (internal detail, but we can check via debug)
        let k1 = cell.key();
        let k2 = cell.key();
        assert_eq!(k1, k2);
    }

    #[test]
    fn cell_with_precomputed_key() {
        let value = "test".to_string();
        let expected_key = value.compute_key();
        let cell = Cell::with_key(value.clone(), expected_key);
        assert_eq!(cell.key(), expected_key);
    }

    #[test]
    fn cell_value_access() {
        let cell = Cell::new("hello".to_string());
        assert_eq!(cell.value(), "hello");
    }
}
