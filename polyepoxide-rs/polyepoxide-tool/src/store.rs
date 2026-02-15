//! Store abstraction for runtime dispatch.

use std::path::Path;

use polyepoxide_core::Store;
use polyepoxide_fjall::FjallStore;
use polyepoxide_rocks::RocksStore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnyStoreError {
    #[error("fjall error: {0}")]
    Fjall(#[from] polyepoxide_fjall::FjallError),
    #[error("rocks error: {0}")]
    Rocks(#[from] polyepoxide_rocks::RocksError),
}

/// Runtime-dispatched store.
pub enum AnyStore {
    Fjall(FjallStore),
    Rocks(RocksStore),
}

impl AnyStore {
    pub fn open_fjall(path: impl AsRef<Path>) -> Result<Self, AnyStoreError> {
        Ok(Self::Fjall(FjallStore::open(path)?))
    }

    pub fn open_rocks(path: impl AsRef<Path>) -> Result<Self, AnyStoreError> {
        Ok(Self::Rocks(RocksStore::open(path)?))
    }
}

impl Store for AnyStore {
    type Error = AnyStoreError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        match self {
            AnyStore::Fjall(s) => s.get(key).map_err(Into::into),
            AnyStore::Rocks(s) => s.get(key).map_err(Into::into),
        }
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        match self {
            AnyStore::Fjall(s) => s.put(key, value).map_err(Into::into),
            AnyStore::Rocks(s) => s.put(key, value).map_err(Into::into),
        }
    }

    fn has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        match self {
            AnyStore::Fjall(s) => s.has(key).map_err(Into::into),
            AnyStore::Rocks(s) => s.has(key).map_err(Into::into),
        }
    }
}
