//! Request handler for serving local store data to peers.

use polyepoxide_core::AsyncStore;

use crate::protocol::{Request, Response};

/// Handle an incoming request against a local store.
pub async fn handle_request<S: AsyncStore>(store: &S, request: Request) -> Response {
    match request {
        Request::Get { cids } => match store.async_get_many(&cids).await {
            Ok(results) => {
                let mut found = Vec::new();
                let mut missing = Vec::new();

                for (cid, result) in cids.into_iter().zip(results) {
                    match result {
                        Some(data) => found.push((cid, data)),
                        None => missing.push(cid),
                    }
                }

                Response::Nodes { found, missing }
            }
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::Has { cids } => match store.async_has_many(&cids).await {
            Ok(present) => Response::Has { present },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::Put { nodes } => {
            let refs: Vec<_> = nodes.iter().map(|(k, v)| (k, v.as_slice())).collect();
            match store.async_put_many(&refs).await {
                Ok(()) => Response::Stored {
                    cids: nodes.into_iter().map(|(k, _)| k).collect(),
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
    use polyepoxide_core::{compute_cid, MemoryStore, Store};

    #[tokio::test]
    async fn handle_get_found() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"test");
        store.put(&cid, b"data").unwrap();

        let response = handle_request(&store, Request::Get { cids: vec![cid] }).await;

        if let Response::Nodes { found, missing } = response {
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].0, cid);
            assert_eq!(found[0].1, b"data");
            assert!(missing.is_empty());
        } else {
            panic!("Expected Nodes response");
        }
    }

    #[tokio::test]
    async fn handle_get_missing() {
        let store = MemoryStore::new();
        let cid = compute_cid(b"missing");

        let response = handle_request(&store, Request::Get { cids: vec![cid] }).await;

        if let Response::Nodes { found, missing } = response {
            assert!(found.is_empty());
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], cid);
        } else {
            panic!("Expected Nodes response");
        }
    }

    #[tokio::test]
    async fn handle_has() {
        let store = MemoryStore::new();
        let cid1 = compute_cid(b"exists");
        let cid2 = compute_cid(b"missing");
        store.put(&cid1, b"data").unwrap();

        let response = handle_request(
            &store,
            Request::Has {
                cids: vec![cid1, cid2],
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
        let cid = compute_cid(b"new");

        let response = handle_request(
            &store,
            Request::Put {
                nodes: vec![(cid, b"value".to_vec())],
            },
        )
        .await;

        if let Response::Stored { cids } = response {
            assert_eq!(cids, vec![cid]);
        } else {
            panic!("Expected Stored response");
        }

        // Verify it was actually stored
        assert_eq!(store.get(&cid).unwrap(), Some(b"value".to_vec()));
    }
}
