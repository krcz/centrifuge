use std::future::Future;

use crate::Store;
use crate::store::identity_digest_from_key;

/// Async multihash-keyed store for oxide bytes.
///
/// Mirrors the `Store` trait but with async methods, enabling network-capable
/// implementations (e.g., remote peers over libp2p). Methods are prefixed with
/// `async_` to avoid name collisions when a type implements both `Store` and
/// `AsyncStore`.
pub trait AsyncStore: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Retrieves bytes by multihash key.
    fn async_get(
        &self,
        key: &[u8],
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send;

    /// Stores bytes by multihash key.
    fn async_put(
        &self,
        key: &[u8],
        value: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Checks whether a multihash key exists.
    fn async_has(&self, key: &[u8]) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// Batch get - default impl calls async_get() in sequence.
    fn async_get_many(
        &self,
        keys: &[Vec<u8>],
    ) -> impl Future<Output = Result<Vec<Option<Vec<u8>>>, Self::Error>> + Send {
        let keys = keys.to_vec();
        async move {
            let mut results = Vec::with_capacity(keys.len());
            for key in &keys {
                results.push(self.async_get(key).await?);
            }
            Ok(results)
        }
    }

    /// Batch put - default impl calls async_put() in sequence.
    fn async_put_many(
        &self,
        nodes: &[(&[u8], &[u8])],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let nodes: Vec<(Vec<u8>, Vec<u8>)> = nodes
            .iter()
            .map(|(k, v)| ((*k).to_vec(), v.to_vec()))
            .collect();
        async move {
            for (key, value) in &nodes {
                self.async_put(key, value).await?;
            }
            Ok(())
        }
    }

    /// Batch has - default impl calls async_has() in sequence.
    fn async_has_many(
        &self,
        keys: &[Vec<u8>],
    ) -> impl Future<Output = Result<Vec<bool>, Self::Error>> + Send {
        let keys = keys.to_vec();
        async move {
            let mut results = Vec::with_capacity(keys.len());
            for key in &keys {
                results.push(self.async_has(key).await?);
            }
            Ok(results)
        }
    }
}

/// Wraps an async store with identity-multihash virtual CID handling.
pub struct IdentityAsyncStoreOverlay<'a, S: AsyncStore + ?Sized> {
    inner: &'a S,
}

impl<'a, S: AsyncStore + ?Sized> IdentityAsyncStoreOverlay<'a, S> {
    pub fn new(inner: &'a S) -> Self {
        Self { inner }
    }
}

pub fn identity_overlay_async<S: AsyncStore + ?Sized>(
    inner: &S,
) -> IdentityAsyncStoreOverlay<'_, S> {
    IdentityAsyncStoreOverlay::new(inner)
}

impl<S: AsyncStore + ?Sized> AsyncStore for IdentityAsyncStoreOverlay<'_, S> {
    type Error = S::Error;

    async fn async_get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        if let Some(digest) = identity_digest_from_key(key) {
            return Ok(Some(digest));
        }
        self.inner.async_get(key).await
    }

    async fn async_put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        if identity_digest_from_key(key).is_some() {
            let _ = value;
            return Ok(());
        }
        self.inner.async_put(key, value).await
    }

    async fn async_has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        if identity_digest_from_key(key).is_some() {
            return Ok(true);
        }
        self.inner.async_has(key).await
    }
}

/// Blanket impl: any sync `Store` is also an `AsyncStore`.
impl<S: Store + Send + Sync> AsyncStore for S {
    type Error = S::Error;

    async fn async_get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        self.get(key)
    }

    async fn async_put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.put(key, value)
    }

    async fn async_has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        self.has(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStore;
    use crate::oxide::compute_cid;
    use crate::reflexive::make_identity_cid;
    use crate::store::key_from_cid;

    #[tokio::test]
    async fn store_as_async_store_basic() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");
        let key = key_from_cid(&cid);
        let value = b"hello world";

        store.async_put(&key, value).await.unwrap();
        let retrieved = store.async_get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value.to_vec()));
        assert!(store.async_has(&key).await.unwrap());
    }

    #[tokio::test]
    async fn store_as_async_store_batch() {
        let store = MemoryStore::new();
        let keys: Vec<Vec<u8>> = (0..3).map(|i| key_from_cid(&compute_cid(&[i]))).collect();
        let values: Vec<&[u8]> = vec![b"a", b"b", b"c"];

        let nodes: Vec<_> = keys
            .iter()
            .zip(values.iter())
            .map(|(k, v)| (k.as_slice(), *v))
            .collect();
        store.async_put_many(&nodes).await.unwrap();

        let results = store.async_get_many(&keys).await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Some(b"a".to_vec()));
        assert_eq!(results[1], Some(b"b".to_vec()));
        assert_eq!(results[2], Some(b"c".to_vec()));

        let has_results = store.async_has_many(&keys).await.unwrap();
        assert_eq!(has_results, vec![true, true, true]);
    }

    #[tokio::test]
    async fn async_identity_short_circuit() {
        let store = MemoryStore::new();
        let cid = make_identity_cid(0x71, b"abc").unwrap();
        let key = key_from_cid(&cid);
        let overlay = identity_overlay_async(&store);

        assert_eq!(
            overlay.async_get(&key).await.unwrap(),
            Some(b"abc".to_vec())
        );
        assert!(overlay.async_has(&key).await.unwrap());
        overlay.async_put(&key, b"ignored").await.unwrap();
    }
}
