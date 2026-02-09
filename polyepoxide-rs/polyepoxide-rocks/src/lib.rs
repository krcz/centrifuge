//! RocksDB-backed store for Polyepoxide.

use std::path::Path;

use cid::Cid;
use polyepoxide_core::Store;
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

    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.db.get(cid.to_bytes())?)
    }

    fn put(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        self.db.put(cid.to_bytes(), value)?;
        Ok(())
    }

    fn has(&self, cid: &Cid) -> Result<bool, Self::Error> {
        Ok(self.db.get_pinned(cid.to_bytes())?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::compute_cid;
    use tempfile::TempDir;

    fn temp_store() -> (RocksStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = RocksStore::open(dir.path()).unwrap();
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
            let store = RocksStore::open(dir.path()).unwrap();
            store.put(&cid, value).unwrap();
        }

        {
            let store = RocksStore::open(dir.path()).unwrap();
            let retrieved = store.get(&cid).unwrap();
            assert_eq!(retrieved, Some(value.to_vec()));
        }
    }
}
