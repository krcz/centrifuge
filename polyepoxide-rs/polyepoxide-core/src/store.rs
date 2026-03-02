use cid::Cid;
use cid::multihash::Multihash;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::RwLock;

use crate::dyn_bond::DynBond;
use crate::oxide::Oxide;
use crate::reflexive::MULTIHASH_IDENTITY;

/// Returns store key bytes derived from CID multihash.
#[inline]
pub fn key_from_cid(cid: &Cid) -> Vec<u8> {
    cid.hash().to_bytes()
}

/// Returns identity payload bytes if `key` encodes an identity multihash.
pub fn identity_digest_from_key(key: &[u8]) -> Option<Vec<u8>> {
    let hash = Multihash::<64>::from_bytes(key).ok()?;
    (hash.code() == MULTIHASH_IDENTITY).then(|| hash.digest().to_vec())
}

#[derive(Debug, thiserror::Error)]
pub enum BookmarkError<E: std::error::Error + Send + Sync + 'static> {
    #[error(transparent)]
    Store(#[from] E),
    #[error("invalid bookmark `{name}`")]
    Decode {
        name: String,
        #[source]
        source: serde_ipld_dagcbor::DecodeError<Infallible>,
    },
}

/// A simple store for oxide bytes, indexed by CID multihash.
///
/// Stores operate on raw bytes — serialization/deserialization is handled
/// by higher layers (Solvent). Stores have no knowledge of oxide types,
/// schemas, or sync configuration.
///
/// Stores may also expose mutable named bookmarks. The bookmark value is a
/// serialized `DynBond`, but implementations are free to represent that
/// mapping however they want internally.
///
/// All methods take `&self` to support stores with internal locking (e.g., RocksDB).
pub trait Store {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Retrieves bytes associated with a multihash key.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Stores bytes at the given multihash key.
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;

    /// Checks whether a multihash key exists in the store.
    fn has(&self, key: &[u8]) -> Result<bool, Self::Error>;

    /// Retrieves raw bookmark bytes for a name.
    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Stores raw bookmark bytes for a name.
    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error>;

    /// Retrieves a bookmark as a dynamic bond.
    fn get_bookmark(&self, name: &str) -> Result<Option<DynBond>, BookmarkError<Self::Error>> {
        self.get_bookmark_bytes(name)?
            .map(|bytes| {
                DynBond::from_bytes(&bytes).map_err(|source| BookmarkError::Decode {
                    name: name.to_string(),
                    source,
                })
            })
            .transpose()
    }

    /// Stores a bookmark as a dynamic bond.
    fn put_bookmark(
        &self,
        name: &str,
        bookmark: &DynBond,
    ) -> Result<(), BookmarkError<Self::Error>> {
        self.put_bookmark_bytes(name, &bookmark.to_bytes())?;
        Ok(())
    }
}

/// Wraps a store with identity-multihash virtual CID handling.
pub struct IdentityStoreOverlay<'a, S: Store + ?Sized> {
    inner: &'a S,
}

impl<'a, S: Store + ?Sized> IdentityStoreOverlay<'a, S> {
    pub fn new(inner: &'a S) -> Self {
        Self { inner }
    }
}

pub fn identity_overlay<S: Store + ?Sized>(inner: &S) -> IdentityStoreOverlay<'_, S> {
    IdentityStoreOverlay::new(inner)
}

impl<S: Store + ?Sized> Store for IdentityStoreOverlay<'_, S> {
    type Error = S::Error;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        if let Some(digest) = identity_digest_from_key(key) {
            return Ok(Some(digest));
        }
        self.inner.get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        if identity_digest_from_key(key).is_some() {
            let _ = value;
            return Ok(());
        }
        self.inner.put(key, value)
    }

    fn has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        if identity_digest_from_key(key).is_some() {
            return Ok(true);
        }
        self.inner.has(key)
    }

    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        self.inner.get_bookmark_bytes(name)
    }

    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error> {
        self.inner.put_bookmark_bytes(name, value)
    }
}

impl<S: Store + ?Sized> Store for &S {
    type Error = S::Error;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        (*self).get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        (*self).put(key, value)
    }

    fn has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        (*self).has(key)
    }

    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        (*self).get_bookmark_bytes(name)
    }

    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error> {
        (*self).put_bookmark_bytes(name, value)
    }
}

/// An in-memory store backed by a HashMap.
///
/// Useful for testing and as a reference implementation.
#[derive(Debug, Default)]
pub struct MemoryStore {
    data: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    bookmarks: RwLock<HashMap<String, Vec<u8>>>,
}

impl MemoryStore {
    /// Creates an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemoryStore {
    type Error = Infallible;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.data.read().unwrap().get(key).cloned())
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.data
            .write()
            .unwrap()
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        Ok(self.data.read().unwrap().contains_key(key))
    }

    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.bookmarks.read().unwrap().get(name).cloned())
    }

    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error> {
        self.bookmarks
            .write()
            .unwrap()
            .insert(name.to_string(), value.to_vec());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bond;
    use crate::DynBond;
    use crate::oxide::compute_cid;
    use crate::reflexive::{POLYEPOXIDE_REFLEXIVE_CODEC, make_identity_cid, with_codec};

    #[test]
    fn memory_store_put_get() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");
        let key = key_from_cid(&cid);
        let value = b"hello world";

        store.put(&key, value).unwrap();
        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn memory_store_get_missing() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"nonexistent");
        let key = key_from_cid(&cid);

        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, None);
    }

    #[test]
    fn memory_store_has() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");
        let key = key_from_cid(&cid);

        assert!(!store.has(&key).unwrap());

        store.put(&key, b"value").unwrap();

        assert!(store.has(&key).unwrap());
    }

    #[test]
    fn memory_store_overwrite() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");
        let key = key_from_cid(&cid);

        store.put(&key, b"first").unwrap();
        store.put(&key, b"second").unwrap();

        let retrieved = store.get(&key).unwrap();
        assert_eq!(retrieved, Some(b"second".to_vec()));
    }

    #[test]
    fn memory_store_identity_is_virtual() {
        let store = MemoryStore::new();
        let bytes = b"virtual".to_vec();
        let cid = make_identity_cid(0x71, &bytes).unwrap();
        let key = key_from_cid(&cid);
        let overlay = identity_overlay(&store);

        assert_eq!(overlay.get(&key).unwrap(), Some(bytes.clone()));
        assert!(overlay.has(&key).unwrap());

        overlay.put(&key, b"ignored").unwrap();
        assert_eq!(store.len_impl(), 0);
    }

    #[test]
    fn memory_store_uses_multihash_key() {
        let store = MemoryStore::new();
        let dag_cbor_cid = compute_cid(b"same multihash");
        let reflexive_cid = with_codec(&dag_cbor_cid, POLYEPOXIDE_REFLEXIVE_CODEC);
        let dag_key = key_from_cid(&dag_cbor_cid);
        let reflexive_key = key_from_cid(&reflexive_cid);

        store.put(&dag_key, b"value-a").unwrap();
        assert_eq!(
            store.get(&reflexive_key).unwrap(),
            Some(b"value-a".to_vec())
        );
        assert!(store.has(&reflexive_key).unwrap());

        store.put(&reflexive_key, b"value-b").unwrap();
        assert_eq!(store.get(&dag_key).unwrap(), Some(b"value-b".to_vec()));
        assert_eq!(store.len_impl(), 1);
    }

    #[test]
    fn memory_store_bookmark_roundtrip() {
        let store = MemoryStore::new();
        let bookmark = DynBond::from_typed(Bond::new("hello".to_string()));

        store.put_bookmark("greeting", &bookmark).unwrap();

        let loaded = store.get_bookmark("greeting").unwrap();
        assert_eq!(loaded.unwrap().cid(), bookmark.cid());
    }

    #[test]
    fn memory_store_reports_invalid_bookmark_bytes() {
        let store = MemoryStore::new();
        store.put_bookmark_bytes("broken", b"not dag-cbor").unwrap();

        let err = store.get_bookmark("broken").unwrap_err();
        assert!(matches!(err, BookmarkError::Decode { .. }));
    }
}

impl MemoryStore {
    #[cfg(test)]
    fn len_impl(&self) -> usize {
        self.data.read().unwrap().len()
    }
}
