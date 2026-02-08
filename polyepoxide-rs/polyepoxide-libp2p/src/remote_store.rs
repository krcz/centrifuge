//! RemoteStore - wraps a libp2p peer as an AsyncStore.

use std::collections::HashMap;

use cid::Cid;
use libp2p::PeerId;
use libp2p::request_response::ResponseChannel;
use polyepoxide_core::{AsyncStore, is_identity_cid};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::{Request, Response};

/// Error from remote store operations.
#[derive(Debug, thiserror::Error)]
pub enum RemoteStoreError {
    #[error("connection closed")]
    ConnectionClosed,
    #[error("request failed: {0}")]
    RequestFailed(String),
    #[error("unexpected response type")]
    UnexpectedResponse,
    #[error("remote error: {0}")]
    Remote(String),
}

/// Command sent to the swarm driver.
pub enum Command {
    /// Send a request to a peer.
    SendRequest {
        peer: PeerId,
        request: Request,
        response_tx: oneshot::Sender<Result<Response, RemoteStoreError>>,
    },
    /// Respond to an incoming request.
    SendResponse {
        channel: ResponseChannel<Response>,
        response: Response,
    },
}

/// A remote peer exposed as an AsyncStore.
///
/// Sends requests via the command channel and waits for responses.
pub struct RemoteStore {
    peer_id: PeerId,
    command_tx: mpsc::Sender<Command>,
}

impl RemoteStore {
    /// Creates a new RemoteStore for the given peer.
    pub fn new(peer_id: PeerId, command_tx: mpsc::Sender<Command>) -> Self {
        Self {
            peer_id,
            command_tx,
        }
    }

    /// Returns the peer ID this store connects to.
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    async fn send_request(&self, request: Request) -> Result<Response, RemoteStoreError> {
        let (tx, rx) = oneshot::channel();

        self.command_tx
            .send(Command::SendRequest {
                peer: self.peer_id,
                request,
                response_tx: tx,
            })
            .await
            .map_err(|_| RemoteStoreError::ConnectionClosed)?;

        rx.await.map_err(|_| RemoteStoreError::ConnectionClosed)?
    }
}

impl AsyncStore for RemoteStore {
    type Error = RemoteStoreError;

    async fn async_get_impl(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Self::Error> {
        let results = self.async_get_many(&[*cid]).await?;
        Ok(results.into_iter().next().flatten())
    }

    async fn async_get_many(&self, cids: &[Cid]) -> Result<Vec<Option<Vec<u8>>>, Self::Error> {
        let mut results = vec![None; cids.len()];
        let mut remote_cids = Vec::new();
        let mut remote_indexes = Vec::new();

        for (idx, cid) in cids.iter().enumerate() {
            if is_identity_cid(cid) {
                results[idx] = Some(cid.hash().digest().to_vec());
            } else {
                remote_indexes.push(idx);
                remote_cids.push(*cid);
            }
        }

        if remote_cids.is_empty() {
            return Ok(results);
        }

        let response = self
            .send_request(Request::Get {
                cids: remote_cids.clone(),
            })
            .await?;

        match response {
            Response::Nodes { found, missing: _ } => {
                let found_map: HashMap<Cid, Vec<u8>> = found.into_iter().collect();
                for (idx, cid) in remote_indexes.into_iter().zip(remote_cids.into_iter()) {
                    results[idx] = found_map.get(&cid).cloned();
                }
                Ok(results)
            }
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }

    async fn async_put_impl(&self, cid: &Cid, value: &[u8]) -> Result<(), Self::Error> {
        self.async_put_many(&[(cid, value)]).await
    }

    async fn async_put_many(&self, nodes: &[(&Cid, &[u8])]) -> Result<(), Self::Error> {
        let nodes_owned: Vec<(Cid, Vec<u8>)> = nodes
            .iter()
            .filter_map(|(k, v)| {
                if is_identity_cid(k) {
                    None
                } else {
                    Some((**k, v.to_vec()))
                }
            })
            .collect();

        if nodes_owned.is_empty() {
            return Ok(());
        }

        let response = self
            .send_request(Request::Put { nodes: nodes_owned })
            .await?;

        match response {
            Response::Stored { cids: _ } => Ok(()),
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }

    async fn async_has_impl(&self, cid: &Cid) -> Result<bool, Self::Error> {
        let results = self.async_has_many(&[*cid]).await?;
        Ok(results.into_iter().next().unwrap_or(false))
    }

    async fn async_has_many(&self, cids: &[Cid]) -> Result<Vec<bool>, Self::Error> {
        let mut results = vec![false; cids.len()];
        let mut remote_cids = Vec::new();
        let mut remote_indexes = Vec::new();

        for (idx, cid) in cids.iter().enumerate() {
            if is_identity_cid(cid) {
                results[idx] = true;
            } else {
                remote_indexes.push(idx);
                remote_cids.push(*cid);
            }
        }

        if remote_cids.is_empty() {
            return Ok(results);
        }

        let response = self
            .send_request(Request::Has {
                cids: remote_cids.clone(),
            })
            .await?;

        match response {
            Response::Has { present } => {
                for (idx, value) in remote_indexes.into_iter().zip(present.into_iter()) {
                    results[idx] = value;
                }
                Ok(results)
            }
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }
}
