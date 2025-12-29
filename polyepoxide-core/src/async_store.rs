use std::future::Future;

use crate::{Key, Store};

/// Async key-value store for oxide bytes.
///
/// Mirrors the `Store` trait but with async methods, enabling network-capable
/// implementations (e.g., remote peers over libp2p). Methods are prefixed with
/// `async_` to avoid name collisions when a type implements both `Store` and
/// `AsyncStore`.
pub trait AsyncStore: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn async_get(&self, key: &Key) -> impl Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send;
    fn async_put(&self, key: &Key, value: &[u8]) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn async_has(&self, key: &Key) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// Batch get - default impl calls async_get() in sequence.
    fn async_get_many(
        &self,
        keys: &[Key],
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
        nodes: &[(&Key, &[u8])],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let nodes: Vec<(Key, Vec<u8>)> = nodes.iter().map(|(k, v)| (**k, v.to_vec())).collect();
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
        keys: &[Key],
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

/// Blanket impl: any sync `Store` is also an `AsyncStore`.
impl<S: Store + Send + Sync> AsyncStore for S {
    type Error = S::Error;

    async fn async_get(&self, key: &Key) -> Result<Option<Vec<u8>>, Self::Error> {
        self.get(key)
    }

    async fn async_put(&self, key: &Key, value: &[u8]) -> Result<(), Self::Error> {
        self.put(key, value)
    }

    async fn async_has(&self, key: &Key) -> Result<bool, Self::Error> {
        self.has(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStore;

    #[tokio::test]
    async fn store_as_async_store_basic() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"test");
        let value = b"hello world";

        store.async_put(&key, value).await.unwrap();
        let retrieved = store.async_get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value.to_vec()));
        assert!(store.async_has(&key).await.unwrap());
    }

    #[tokio::test]
    async fn store_as_async_store_batch() {
        let store = MemoryStore::new();
        let keys: Vec<Key> = (0..3).map(|i| Key::from_data(&[i])).collect();
        let values: Vec<&[u8]> = vec![b"a", b"b", b"c"];

        let nodes: Vec<_> = keys.iter().zip(values.iter()).map(|(k, v)| (k, *v)).collect();
        store.async_put_many(&nodes).await.unwrap();

        let results = store.async_get_many(&keys).await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Some(b"a".to_vec()));
        assert_eq!(results[1], Some(b"b".to_vec()));
        assert_eq!(results[2], Some(b"c".to_vec()));

        let has_results = store.async_has_many(&keys).await.unwrap();
        assert_eq!(has_results, vec![true, true, true]);
    }
}
