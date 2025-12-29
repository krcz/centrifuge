//! RocksDB-backed store for Polyepoxide.

use std::path::Path;

use polyepoxide_core::{Key, Store};
use rocksdb::{DB, Options};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("RocksDB error: {0}")]
pub struct RocksError(#[from] rocksdb::Error);

/// A persistent store backed by RocksDB.
pub struct RocksStore {
    db: DB,
}

impl RocksStore {
    /// Opens a RocksDB store at the given path.
    ///
    /// Creates the database if it doesn't exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RocksError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db })
    }
}

impl Store for RocksStore {
    type Error = RocksError;

    fn get(&self, key: &Key) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.db.get(key.as_bytes())?)
    }

    fn put(&self, key: &Key, value: &[u8]) -> Result<(), Self::Error> {
        self.db.put(key.as_bytes(), value)?;
        Ok(())
    }

    fn has(&self, key: &Key) -> Result<bool, Self::Error> {
        Ok(self.db.get_pinned(key.as_bytes())?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (RocksStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = RocksStore::open(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn put_get() {
        let (store, _dir) = temp_store();
        let key = Key::from_data(b"test");
        let value = b"hello world";

        store.put(&key, value).unwrap();
        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn get_missing() {
        let (store, _dir) = temp_store();
        let key = Key::from_data(b"nonexistent");

        let retrieved = store.get(&key).unwrap();

        assert_eq!(retrieved, None);
    }

    #[test]
    fn has() {
        let (store, _dir) = temp_store();
        let key = Key::from_data(b"test");

        assert!(!store.has(&key).unwrap());

        store.put(&key, b"value").unwrap();

        assert!(store.has(&key).unwrap());
    }

    #[test]
    fn persistence() {
        let dir = TempDir::new().unwrap();
        let key = Key::from_data(b"persistent");
        let value = b"data survives restart";

        {
            let store = RocksStore::open(dir.path()).unwrap();
            store.put(&key, value).unwrap();
        }

        {
            let store = RocksStore::open(dir.path()).unwrap();
            let retrieved = store.get(&key).unwrap();
            assert_eq!(retrieved, Some(value.to_vec()));
        }
    }
}
