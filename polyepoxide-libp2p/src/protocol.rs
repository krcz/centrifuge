//! Protocol messages for Polyepoxide sync over libp2p.

use polyepoxide_core::Key;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_NAME: &str = "/polyepoxide/sync/0.1.0";

/// Request types for the sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    /// Get values for the given keys.
    Get { keys: Vec<Key> },
    /// Check which keys exist.
    Has { keys: Vec<Key> },
    /// Store values at the given keys.
    Put { nodes: Vec<(Key, Vec<u8>)> },
}

/// Response types for the sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    /// Response to Get: found nodes and missing keys.
    Nodes {
        found: Vec<(Key, Vec<u8>)>,
        missing: Vec<Key>,
    },
    /// Response to Has: presence flags in same order as request.
    Has { present: Vec<bool> },
    /// Response to Put: keys that were stored.
    Stored { keys: Vec<Key> },
    /// Error response.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serialization_roundtrip() {
        let key = Key::from_data(b"test");
        let request = Request::Get { keys: vec![key] };

        let mut bytes = Vec::new();
        ciborium::into_writer(&request, &mut bytes).unwrap();

        let recovered: Request = ciborium::from_reader(&bytes[..]).unwrap();
        if let Request::Get { keys } = recovered {
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0], key);
        } else {
            panic!("Expected Get request");
        }
    }

    #[test]
    fn response_serialization_roundtrip() {
        let key = Key::from_data(b"test");
        let response = Response::Nodes {
            found: vec![(key, b"data".to_vec())],
            missing: vec![],
        };

        let mut bytes = Vec::new();
        ciborium::into_writer(&response, &mut bytes).unwrap();

        let recovered: Response = ciborium::from_reader(&bytes[..]).unwrap();
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
