//! RocksDB-backed store for Polyepoxide.

use std::path::Path;

use polyepoxide_core::Store;
use rocksdb::{ColumnFamilyDescriptor, DB, Options};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("RocksDB error: {0}")]
pub struct RocksError(#[from] rocksdb::Error);

const BOOKMARKS_CF: &str = "bookmarks";

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
        opts.create_missing_column_families(true);
        let db = DB::open_cf_descriptors(
            &opts,
            path,
            vec![
                ColumnFamilyDescriptor::new("default", Options::default()),
                ColumnFamilyDescriptor::new(BOOKMARKS_CF, Options::default()),
            ],
        )?;
        Ok(Self { db })
    }
}

impl Store for RocksStore {
    type Error = RocksError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.db.get(key)?)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.db.put(key, value)?;
        Ok(())
    }

    fn has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        Ok(self.db.get_pinned(key)?.is_some())
    }

    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        let cf = self
            .db
            .cf_handle(BOOKMARKS_CF)
            .expect("missing bookmarks cf");
        Ok(self.db.get_cf(cf, name.as_bytes())?)
    }

    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error> {
        let cf = self
            .db
            .cf_handle(BOOKMARKS_CF)
            .expect("missing bookmarks cf");
        self.db.put_cf(cf, name.as_bytes(), value)?;
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

    fn temp_store() -> (RocksStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = RocksStore::open(dir.path()).unwrap();
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
            let store = RocksStore::open(dir.path()).unwrap();
            store.put(&key, value).unwrap();
        }

        {
            let store = RocksStore::open(dir.path()).unwrap();
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
            let store = RocksStore::open(dir.path()).unwrap();
            store.put_bookmark("greeting", &bookmark).unwrap();
        }

        {
            let store = RocksStore::open(dir.path()).unwrap();
            let loaded = store.get_bookmark("greeting").unwrap();
            assert_eq!(loaded.unwrap().cid(), bookmark.cid());
        }
    }
}
