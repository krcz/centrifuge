//! Fjall-backed store for Polyepoxide.

use std::path::Path;

use cid::Cid;
use fjall::{Database, Keyspace, KeyspaceCreateOptions};
use polyepoxide_core::Store;
use thiserror::Error;

pub const DEFAULT_KEYSPACE: &str = "data";

#[derive(Debug, Error)]
#[error("Fjall error: {0}")]
pub struct FjallError(#[from] fjall::Error);

/// A persistent store backed by Fjall.
pub struct FjallStore {
    keyspace: Keyspace,
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
        Ok(Self {
            keyspace,
            _database: database,
        })
    }
}

impl Store for FjallStore {
    type Error = FjallError;

    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.keyspace.get(cid.to_bytes())?.map(|v| v.to_vec()))
    }

    fn put(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        self.keyspace.insert(cid.to_bytes(), value)?;
        Ok(())
    }

    fn has(&self, cid: &Cid) -> Result<bool, Self::Error> {
        self.keyspace
            .contains_key(cid.to_bytes())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::compute_cid;
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
        let value = b"hello world";

        store.put(&cid, value).unwrap();
        let retrieved = store.get(&cid).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn get_missing() {
        let (store, _dir) = temp_store();
        let cid = compute_cid(b"nonexistent");

        let retrieved = store.get(&cid).unwrap();

        assert_eq!(retrieved, None);
    }

    #[test]
    fn has() {
        let (store, _dir) = temp_store();
        let cid = compute_cid(b"test");

        assert!(!store.has(&cid).unwrap());

        store.put(&cid, b"value").unwrap();

        assert!(store.has(&cid).unwrap());
    }

    #[test]
    fn persistence() {
        let dir = TempDir::new().unwrap();
        let cid = compute_cid(b"persistent");
        let value = b"data survives restart";

        {
            let store = FjallStore::open(dir.path()).unwrap();
            store.put(&cid, value).unwrap();
        }

        {
            let store = FjallStore::open(dir.path()).unwrap();
            let retrieved = store.get(&cid).unwrap();
            assert_eq!(retrieved, Some(value.to_vec()));
        }
    }
}
