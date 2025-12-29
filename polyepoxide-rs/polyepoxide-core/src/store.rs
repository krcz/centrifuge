use cid::Cid;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::RwLock;

/// A simple CID-keyed store for oxide bytes.
///
/// Stores operate on raw bytes — serialization/deserialization is handled
/// by higher layers (Solvent). Stores have no knowledge of oxide types,
/// schemas, or sync configuration.
///
/// All methods take `&self` to support stores with internal locking (e.g., RocksDB).
pub trait Store {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Retrieves the bytes associated with a CID, or None if not present.
    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Stores bytes at the given CID.
    fn put(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error>;

    /// Checks whether a CID exists in the store.
    fn has(&self, cid: &Cid) -> Result<bool, Self::Error>;
}

impl<S: Store> Store for &S {
    type Error = S::Error;

    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        (*self).get(cid)
    }

    fn put(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        (*self).put(cid, value)
    }

    fn has(&self, cid: &Cid) -> Result<bool, Self::Error> {
        (*self).has(cid)
    }
}

/// An in-memory store backed by a HashMap.
///
/// Useful for testing and as a reference implementation.
#[derive(Debug, Default)]
pub struct MemoryStore {
    data: RwLock<HashMap<Cid, Vec<u8>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemoryStore {
    type Error = Infallible;

    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.data.read().unwrap().get(cid).cloned())
    }

    fn put(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        self.data.write().unwrap().insert(*cid, value.to_vec());
        Ok(())
    }

    fn has(&self, cid: &Cid) -> Result<bool, Self::Error> {
        Ok(self.data.read().unwrap().contains_key(cid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::compute_cid;

    #[test]
    fn memory_store_put_get() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");
        let value = b"hello world";

        store.put(&cid, value).unwrap();
        let retrieved = store.get(&cid).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn memory_store_get_missing() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"nonexistent");

        let retrieved = store.get(&cid).unwrap();

        assert_eq!(retrieved, None);
    }

    #[test]
    fn memory_store_has() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");

        assert!(!store.has(&cid).unwrap());

        store.put(&cid, b"value").unwrap();

        assert!(store.has(&cid).unwrap());
    }

    #[test]
    fn memory_store_overwrite() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");

        store.put(&cid, b"first").unwrap();
        store.put(&cid, b"second").unwrap();

        let retrieved = store.get(&cid).unwrap();
        assert_eq!(retrieved, Some(b"second".to_vec()));
    }
}
