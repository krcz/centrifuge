//! Protocol messages for Polyepoxide sync over libp2p.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_NAME: &str = "/polyepoxide/sync/0.1.0";

/// Request types for the sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    /// Get values for the given multihash keys.
    Get { keys: Vec<Vec<u8>> },
    /// Check which multihash keys exist.
    Has { keys: Vec<Vec<u8>> },
    /// Store values at the given multihash keys.
    Put { nodes: Vec<(Vec<u8>, Vec<u8>)> },
}

/// Response types for the sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    /// Response to Get: found nodes and missing multihash keys.
    Nodes {
        found: Vec<(Vec<u8>, Vec<u8>)>,
        missing: Vec<Vec<u8>>,
    },
    /// Response to Has: presence flags in same order as request.
    Has { present: Vec<bool> },
    /// Response to Put: multihash keys that were stored.
    Stored { keys: Vec<Vec<u8>> },
    /// Error response.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::{compute_cid, key_from_cid};

    #[test]
    fn request_serialization_roundtrip() {
        let key = key_from_cid(&compute_cid(b"test"));
        let request = Request::Get {
            keys: vec![key.clone()],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&request).unwrap();
        let recovered: Request = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        if let Request::Get { keys } = recovered {
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0], key);
        } else {
            panic!("Expected Get request");
        }
    }

    #[test]
    fn response_serialization_roundtrip() {
        let key = key_from_cid(&compute_cid(b"test"));
        let response = Response::Nodes {
            found: vec![(key.clone(), b"data".to_vec())],
            missing: vec![],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&response).unwrap();
        let recovered: Response = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        if let Response::Nodes { found, missing } = recovered {
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].0, key);
            assert_eq!(found[0].1, b"data".to_vec());
            assert!(missing.is_empty());
        } else {
            panic!("Expected Nodes response");
        }
    }
}
