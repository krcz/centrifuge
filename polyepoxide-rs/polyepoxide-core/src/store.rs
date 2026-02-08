use cid::Cid;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::RwLock;

use crate::reflexive::is_identity_cid;

/// A simple CID-keyed store for oxide bytes.
///
/// Stores operate on raw bytes — serialization/deserialization is handled
/// by higher layers (Solvent). Stores have no knowledge of oxide types,
/// schemas, or sync configuration.
///
/// All methods take `&self` to support stores with internal locking (e.g., RocksDB).
pub trait Store {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Retrieves bytes associated with a CID from the backend implementation.
    fn get_impl(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Stores bytes at the given CID in the backend implementation.
    fn put_impl(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error>;

    /// Checks whether a CID exists in the backend implementation.
    fn has_impl(&self, cid: &Cid) -> Result<bool, Self::Error>;

    /// Retrieves bytes associated with a CID.
    ///
    /// Identity-multihash CIDs are materialized from the digest bytes and never
    /// hit the underlying store.
    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        if is_identity_cid(cid) {
            return Ok(Some(cid.hash().digest().to_vec()));
        }
        self.get_impl(cid)
    }

    /// Stores bytes at the given CID.
    ///
    /// Identity-multihash CIDs are never persisted.
    fn put(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        if is_identity_cid(cid) {
            let _ = value;
            return Ok(());
        }
        self.put_impl(cid, value)
    }

    /// Checks whether a CID exists.
    ///
    /// Identity-multihash CIDs are always considered present.
    fn has(&self, cid: &Cid) -> Result<bool, Self::Error> {
        if is_identity_cid(cid) {
            return Ok(true);
        }
        self.has_impl(cid)
    }
}

impl<S: Store> Store for &S {
    type Error = S::Error;

    fn get_impl(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        (*self).get_impl(cid)
    }

    fn put_impl(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        (*self).put_impl(cid, value)
    }

    fn has_impl(&self, cid: &Cid) -> Result<bool, Self::Error> {
        (*self).has_impl(cid)
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
    /// Creates an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemoryStore {
    type Error = Infallible;

    fn get_impl(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.data.read().unwrap().get(cid).cloned())
    }

    fn put_impl(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        self.data.write().unwrap().insert(*cid, value.to_vec());
        Ok(())
    }

    fn has_impl(&self, cid: &Cid) -> Result<bool, Self::Error> {
        Ok(self.data.read().unwrap().contains_key(cid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::compute_cid;
    use crate::reflexive::make_identity_cid;

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

    #[test]
    fn memory_store_identity_is_virtual() {
        let store = MemoryStore::new();
        let bytes = b"virtual".to_vec();
        let cid = make_identity_cid(0x71, &bytes).unwrap();

        assert_eq!(store.get(&cid).unwrap(), Some(bytes.clone()));
        assert!(store.has(&cid).unwrap());

        store.put(&cid, b"ignored").unwrap();
        assert_eq!(store.len_impl(), 0);
    }
}

impl MemoryStore {
    #[cfg(test)]
    fn len_impl(&self) -> usize {
        self.data.read().unwrap().len()
    }
}
