//! Fjall-backed store for Polyepoxide.

use std::path::Path;

use fjall::{Database, Keyspace, KeyspaceCreateOptions};
use polyepoxide_core::Store;
use thiserror::Error;

pub const DEFAULT_KEYSPACE: &str = "data";
pub const BOOKMARKS_KEYSPACE: &str = "bookmarks";

#[derive(Debug, Error)]
#[error("Fjall error: {0}")]
pub struct FjallError(#[from] fjall::Error);

/// A persistent store backed by Fjall.
pub struct FjallStore {
    keyspace: Keyspace,
    bookmarks: Keyspace,
    _database: Database, // Keep keyspace alive
}

impl FjallStore {
    /// Opens a Fjall store at the given path using the default keyspace.
    ///
    /// Creates the database if it doesn't exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, FjallError> {
        Self::open_keyspace(path, DEFAULT_KEYSPACE)
    }

    /// Opens a Fjall store at the given path with a specific keyspace name.
    ///
    /// Creates the database and keyspace if they don't exist.
    pub fn open_keyspace(path: impl AsRef<Path>, keyspace: &str) -> Result<Self, FjallError> {
        let database = Database::builder(path).open()?;
        let keyspace = database.keyspace(keyspace, || KeyspaceCreateOptions::default())?;
        let bookmarks =
            database.keyspace(BOOKMARKS_KEYSPACE, || KeyspaceCreateOptions::default())?;
        Ok(Self {
            keyspace,
            bookmarks,
            _database: database,
        })
    }
}

impl Store for FjallStore {
    type Error = FjallError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.keyspace.get(key)?.map(|v| v.to_vec()))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.keyspace.insert(key, value)?;
        Ok(())
    }

    fn has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        self.keyspace.contains_key(key).map_err(Into::into)
    }

    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.bookmarks.get(name.as_bytes())?.map(|v| v.to_vec()))
    }

    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error> {
        self.bookmarks.insert(name.as_bytes(), value)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::{
        Bond, DynBond, POLYEPOXIDE_REFLEXIVE_CODEC, compute_cid, key_from_cid, with_codec,
    };
    use tempfile::TempDir;

    fn temp_store() -> (FjallStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = FjallStore::open(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn put_get() {
        let (store, _dir) = temp_store();
        let cid = compute_cid(b"test");
        let key = key_from_cid(&cid);
        let value = b"hello world";

        store.put(&key, value).unwrap();
        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn get_missing() {
        let (store, _dir) = temp_store();
        let cid = compute_cid(b"nonexistent");
        let key = key_from_cid(&cid);

        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, None);
    }

    #[test]
    fn has() {
        let (store, _dir) = temp_store();
        let cid = compute_cid(b"test");
        let key = key_from_cid(&cid);

        assert!(!store.has(&key).unwrap());

        store.put(&key, b"value").unwrap();

        assert!(store.has(&key).unwrap());
    }

    #[test]
    fn persistence() {
        let dir = TempDir::new().unwrap();
        let cid = compute_cid(b"persistent");
        let key = key_from_cid(&cid);
        let value = b"data survives restart";

        {
            let store = FjallStore::open(dir.path()).unwrap();
            store.put(&key, value).unwrap();
        }

        {
            let store = FjallStore::open(dir.path()).unwrap();
            let retrieved = store.get(&key).unwrap();
            assert_eq!(retrieved, Some(value.to_vec()));
        }
    }

    #[test]
    fn multihash_keying_across_codecs() {
        let (store, _dir) = temp_store();
        let dag_cbor_cid = compute_cid(b"same multihash");
        let reflexive_cid = with_codec(&dag_cbor_cid, POLYEPOXIDE_REFLEXIVE_CODEC);
        let dag_key = key_from_cid(&dag_cbor_cid);
        let reflexive_key = key_from_cid(&reflexive_cid);

        store.put(&dag_key, b"hello").unwrap();
        assert_eq!(store.get(&reflexive_key).unwrap(), Some(b"hello".to_vec()));
        assert!(store.has(&reflexive_key).unwrap());
    }

    #[test]
    fn bookmark_persistence() {
        let dir = TempDir::new().unwrap();
        let bookmark = DynBond::from_typed(Bond::new("hello".to_string()));

        {
            let store = FjallStore::open(dir.path()).unwrap();
            store.put_bookmark("greeting", &bookmark).unwrap();
        }

        {
            let store = FjallStore::open(dir.path()).unwrap();
            let loaded = store.get_bookmark("greeting").unwrap();
            assert_eq!(loaded.unwrap().cid(), bookmark.cid());
        }
    }
}
