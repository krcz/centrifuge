use std::path::{Path, PathBuf};

use polyepoxide_core::{Solvent, Store};
use polyepoxide_fjall::FjallStore;
use polyepoxide_rocks::RocksStore;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnyStoreError {
    #[error("fjall error: {0}")]
    Fjall(#[from] polyepoxide_fjall::FjallError),
    #[error("rocks error: {0}")]
    Rocks(#[from] polyepoxide_rocks::RocksError),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreType {
    #[default]
    Fjall,
    Rocks,
}

impl std::str::FromStr for StoreType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fjall" => Ok(StoreType::Fjall),
            "rocks" | "rocksdb" => Ok(StoreType::Rocks),
            _ => Err(format!("unknown store type: {}", s)),
        }
    }
}

impl std::fmt::Display for StoreType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreType::Fjall => write!(f, "fjall"),
            StoreType::Rocks => write!(f, "rocks"),
        }
    }
}

pub enum AnyStore {
    Fjall(FjallStore),
    Rocks(RocksStore),
}

impl AnyStore {
    pub fn open(store_type: StoreType, path: impl AsRef<Path>) -> Result<Self, AnyStoreError> {
        match store_type {
            StoreType::Fjall => Ok(Self::Fjall(FjallStore::open(path)?)),
            StoreType::Rocks => Ok(Self::Rocks(RocksStore::open(path)?)),
        }
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

    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        match self {
            AnyStore::Fjall(s) => s.get_bookmark_bytes(name).map_err(Into::into),
            AnyStore::Rocks(s) => s.get_bookmark_bytes(name).map_err(Into::into),
        }
    }

    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error> {
        match self {
            AnyStore::Fjall(s) => s.put_bookmark_bytes(name, value).map_err(Into::into),
            AnyStore::Rocks(s) => s.put_bookmark_bytes(name, value).map_err(Into::into),
        }
    }
}

pub struct AppContext {
    pub store: AnyStore,
    pub solvent: Solvent,
}

impl AppContext {
    pub fn open(store_type: StoreType, store_path: PathBuf) -> Result<Self, AnyStoreError> {
        let store = AnyStore::open(store_type, &store_path)?;
        let solvent = Solvent::new();

        Ok(Self { store, solvent })
    }
}

pub fn default_store_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("silane")
        .join("store")
}
