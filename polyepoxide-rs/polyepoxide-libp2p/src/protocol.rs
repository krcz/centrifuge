//! Protocol messages for Polyepoxide sync over libp2p.

use cid::Cid;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_NAME: &str = "/polyepoxide/sync/0.1.0";

/// Request types for the sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    /// Get values for the given CIDs.
    Get { cids: Vec<Cid> },
    /// Check which CIDs exist.
    Has { cids: Vec<Cid> },
    /// Store values at the given CIDs.
    Put { nodes: Vec<(Cid, Vec<u8>)> },
}

/// Response types for the sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    /// Response to Get: found nodes and missing CIDs.
    Nodes {
        found: Vec<(Cid, Vec<u8>)>,
        missing: Vec<Cid>,
    },
    /// Response to Has: presence flags in same order as request.
    Has { present: Vec<bool> },
    /// Response to Put: CIDs that were stored.
    Stored { cids: Vec<Cid> },
    /// Error response.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::compute_cid;

    #[test]
    fn request_serialization_roundtrip() {
        let cid = compute_cid(b"test");
        let request = Request::Get { cids: vec![cid] };

        let bytes = serde_ipld_dagcbor::to_vec(&request).unwrap();
        let recovered: Request = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        if let Request::Get { cids } = recovered {
            assert_eq!(cids.len(), 1);
            assert_eq!(cids[0], cid);
        } else {
            panic!("Expected Get request");
        }
    }

    #[test]
    fn response_serialization_roundtrip() {
        let cid = compute_cid(b"test");
        let response = Response::Nodes {
            found: vec![(cid, b"data".to_vec())],
            missing: vec![],
        };

        let bytes = serde_ipld_dagcbor::to_vec(&response).unwrap();
        let recovered: Response = serde_ipld_dagcbor::from_slice(&bytes).unwrap();

        if let Response::Nodes { found, missing } = recovered {
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].0, cid);
            assert_eq!(found[0].1, b"data".to_vec());
            assert!(missing.is_empty());
        } else {
            panic!("Expected Nodes response");
        }
    }
}
