//! Request handler for serving local store data to peers.

use polyepoxide_core::AsyncStore;

use crate::protocol::{Request, Response};

/// Handle an incoming request against a local store.
pub async fn handle_request<S: AsyncStore>(store: &S, request: Request) -> Response {
    match request {
        Request::Get { keys } => match store.async_get_many(&keys).await {
            Ok(results) => {
                let mut found = Vec::new();
                let mut missing = Vec::new();

                for (key, result) in keys.into_iter().zip(results) {
                    match result {
                        Some(data) => found.push((key, data)),
                        None => missing.push(key),
                    }
                }

                Response::Nodes { found, missing }
            }
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::Has { keys } => match store.async_has_many(&keys).await {
            Ok(present) => Response::Has { present },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::Put { nodes } => {
            let refs: Vec<_> = nodes.iter().map(|(k, v)| (k, v.as_slice())).collect();
            match store.async_put_many(&refs).await {
                Ok(()) => Response::Stored {
                    keys: nodes.into_iter().map(|(k, _)| k).collect(),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::{Key, MemoryStore, Store};

    #[tokio::test]
    async fn handle_get_found() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"test");
        store.put(&key, b"data").unwrap();

        let response = handle_request(&store, Request::Get { keys: vec![key] }).await;

        if let Response::Nodes { found, missing } = response {
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].0, key);
            assert_eq!(found[0].1, b"data");
            assert!(missing.is_empty());
        } else {
            panic!("Expected Nodes response");
        }
    }

    #[tokio::test]
    async fn handle_get_missing() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"missing");

        let response = handle_request(&store, Request::Get { keys: vec![key] }).await;

        if let Response::Nodes { found, missing } = response {
            assert!(found.is_empty());
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], key);
        } else {
            panic!("Expected Nodes response");
        }
    }

    #[tokio::test]
    async fn handle_has() {
        let store = MemoryStore::new();
        let key1 = Key::from_data(b"exists");
        let key2 = Key::from_data(b"missing");
        store.put(&key1, b"data").unwrap();

        let response = handle_request(
            &store,
            Request::Has {
                keys: vec![key1, key2],
            },
        )
        .await;

        if let Response::Has { present } = response {
            assert_eq!(present, vec![true, false]);
        } else {
            panic!("Expected Has response");
        }
    }

    #[tokio::test]
    async fn handle_put() {
        let store = MemoryStore::new();
        let key = Key::from_data(b"new");

        let response = handle_request(
            &store,
            Request::Put {
                nodes: vec![(key, b"value".to_vec())],
            },
        )
        .await;

        if let Response::Stored { keys } = response {
            assert_eq!(keys, vec![key]);
        } else {
            panic!("Expected Stored response");
        }

        // Verify it was actually stored
        assert_eq!(store.get(&key).unwrap(), Some(b"value".to_vec()));
    }
}
