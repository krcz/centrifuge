use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::RwLock;

use crate::Key;

/// A simple key-value store for oxide bytes.
///
/// Stores operate on raw bytes — serialization/deserialization is handled
/// by higher layers (Solvent). Stores have no knowledge of oxide types,
/// schemas, or sync configuration.
///
/// All methods take `&self` to support stores with internal locking (e.g., RocksDB).
pub trait Store {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Retrieves the bytes associated with a key, or None if not present.
    fn get(&self, key: &Key) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Stores bytes at the given key.
    fn put(&self, key: &Key, value: &[u8]) -> Result<(), Self::Error>;

    /// Checks whether a key exists in the store.
    fn has(&self, key: &Key) -> Result<bool, Self::Error>;
}

/// An in-memory store backed by a HashMap.
///
/// Useful for testing and as a reference implementation.
#[derive(Debug, Default)]
pub struct MemoryStore {
    data: RwLock<HashMap<Key, Vec<u8>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemoryStore {
    type Error = Infallible;

    fn get(&self, key: &Key) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.data.read().unwrap().get(key).cloned())
    }

    fn put(&self, key: &Key, value: &[u8]) -> Result<(), Self::Error> {
        self.data.write().unwrap().insert(*key, value.to_vec());
        Ok(())
    }

    fn has(&self, key: &Key) -> Result<bool, Self::Error> {
        Ok(self.data.read().unwrap().contains_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_put_get() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"test");
        let value = b"hello world";

        store.put(&key, value).unwrap();
        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn memory_store_get_missing() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"nonexistent");

        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, None);
    }

    #[test]
    fn memory_store_has() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"test");

        assert!(!store.has(&key).unwrap());

        store.put(&key, b"value").unwrap();

        assert!(store.has(&key).unwrap());
    }

    #[test]
    fn memory_store_overwrite() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"test");

        store.put(&key, b"first").unwrap();
        store.put(&key, b"second").unwrap();

        let retrieved = store.get(&key).unwrap();
        assert_eq!(retrieved, Some(b"second".to_vec()));
    }
}
