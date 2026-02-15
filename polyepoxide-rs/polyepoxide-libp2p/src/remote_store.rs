//! RemoteStore - wraps a libp2p peer as an AsyncStore.

use std::collections::HashMap;

use libp2p::PeerId;
use libp2p::request_response::ResponseChannel;
use polyepoxide_core::{AsyncStore, identity_digest_from_key};
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

    async fn async_get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        let results = self.async_get_many(&[key.to_vec()]).await?;
        Ok(results.into_iter().next().flatten())
    }

    async fn async_get_many(&self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>, Self::Error> {
        let mut results = vec![None; keys.len()];
        let mut remote_keys = Vec::new();
        let mut remote_indexes = Vec::new();

        for (idx, key) in keys.iter().enumerate() {
            if let Some(digest) = identity_digest_from_key(key) {
                results[idx] = Some(digest);
            } else {
                remote_indexes.push(idx);
                remote_keys.push(key.clone());
            }
        }

        if remote_keys.is_empty() {
            return Ok(results);
        }

        let response = self
            .send_request(Request::Get {
                keys: remote_keys.clone(),
            })
            .await?;

        match response {
            Response::Nodes { found, missing: _ } => {
                let found_map: HashMap<Vec<u8>, Vec<u8>> = found.into_iter().collect();
                for (idx, key) in remote_indexes.into_iter().zip(remote_keys.into_iter()) {
                    results[idx] = found_map.get(&key).cloned();
                }
                Ok(results)
            }
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }

    async fn async_put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.async_put_many(&[(key, value)]).await
    }

    async fn async_put_many(&self, nodes: &[(&[u8], &[u8])]) -> Result<(), Self::Error> {
        let nodes_owned: Vec<(Vec<u8>, Vec<u8>)> = nodes
            .iter()
            .filter_map(|(k, v)| {
                if identity_digest_from_key(k).is_some() {
                    None
                } else {
                    Some(((*k).to_vec(), v.to_vec()))
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
            Response::Stored { keys: _ } => Ok(()),
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }

    async fn async_has(&self, key: &[u8]) -> Result<bool, Self::Error> {
        let results = self.async_has_many(&[key.to_vec()]).await?;
        Ok(results.into_iter().next().unwrap_or(false))
    }

    async fn async_has_many(&self, keys: &[Vec<u8>]) -> Result<Vec<bool>, Self::Error> {
        let mut results = vec![false; keys.len()];
        let mut remote_keys = Vec::new();
        let mut remote_indexes = Vec::new();

        for (idx, key) in keys.iter().enumerate() {
            if identity_digest_from_key(key).is_some() {
                results[idx] = true;
            } else {
                remote_indexes.push(idx);
                remote_keys.push(key.clone());
            }
        }

        if remote_keys.is_empty() {
            return Ok(results);
        }

        let response = self
            .send_request(Request::Has {
                keys: remote_keys.clone(),
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
