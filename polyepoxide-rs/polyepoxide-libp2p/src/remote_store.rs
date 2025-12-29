//! RemoteStore - wraps a libp2p peer as an AsyncStore.

use std::collections::HashMap;

use libp2p::request_response::ResponseChannel;
use libp2p::PeerId;
use polyepoxide_core::{AsyncStore, Key};
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

    async fn async_get(&self, key: &Key) -> Result<Option<Vec<u8>>, Self::Error> {
        let results = self.async_get_many(&[*key]).await?;
        Ok(results.into_iter().next().flatten())
    }

    async fn async_get_many(&self, keys: &[Key]) -> Result<Vec<Option<Vec<u8>>>, Self::Error> {
        let response = self
            .send_request(Request::Get {
                keys: keys.to_vec(),
            })
            .await?;

        match response {
            Response::Nodes { found, missing: _ } => {
                let found_map: HashMap<Key, Vec<u8>> = found.into_iter().collect();
                Ok(keys.iter().map(|k| found_map.get(k).cloned()).collect())
            }
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }

    async fn async_put(&self, key: &Key, value: &[u8]) -> Result<(), Self::Error> {
        self.async_put_many(&[(key, value)]).await
    }

    async fn async_put_many(&self, nodes: &[(&Key, &[u8])]) -> Result<(), Self::Error> {
        let nodes_owned: Vec<(Key, Vec<u8>)> =
            nodes.iter().map(|(k, v)| (**k, v.to_vec())).collect();

        let response = self.send_request(Request::Put { nodes: nodes_owned }).await?;

        match response {
            Response::Stored { keys: _ } => Ok(()),
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }

    async fn async_has(&self, key: &Key) -> Result<bool, Self::Error> {
        let results = self.async_has_many(&[*key]).await?;
        Ok(results.into_iter().next().unwrap_or(false))
    }

    async fn async_has_many(&self, keys: &[Key]) -> Result<Vec<bool>, Self::Error> {
        let response = self
            .send_request(Request::Has {
                keys: keys.to_vec(),
            })
            .await?;

        match response {
            Response::Has { present } => Ok(present),
            Response::Error { message } => Err(RemoteStoreError::Remote(message)),
            _ => Err(RemoteStoreError::UnexpectedResponse),
        }
    }
}
