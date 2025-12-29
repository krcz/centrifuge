//! libp2p transport for Polyepoxide sync.
//!
//! This crate provides network transport for syncing Polyepoxide data
//! between peers using libp2p.
//!
//! # Architecture
//!
//! - `RemoteStore` implements `AsyncStore` for a remote peer
//! - `PolyepoxideCodec` handles CBOR serialization over libp2p streams
//! - `handle_request` processes incoming requests against a local store
//!
//! # Example
//!
//! ```ignore
//! use polyepoxide_libp2p::{RemoteStore, Command};
//! use polyepoxide_core::pull;
//!
//! // Create a remote store for a peer
//! let remote = RemoteStore::new(peer_id, command_tx);
//!
//! // Pull data from remote to local
//! pull(&remote, &local_store, value_key, schema_key).await?;
//! ```

mod codec;
mod handler;
mod protocol;
mod remote_store;

pub use codec::{protocol, PolyepoxideCodec};
pub use handler::handle_request;
pub use protocol::{Request, Response, PROTOCOL_NAME};
pub use remote_store::{Command, RemoteStore, RemoteStoreError};

use std::collections::HashMap;

use futures::StreamExt;
use libp2p::request_response::{self, OutboundRequestId};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::Swarm;
use polyepoxide_core::AsyncStore;
use tokio::sync::{mpsc, oneshot};

/// Behaviour combining request_response for sync protocol.
#[derive(NetworkBehaviour)]
pub struct PolyepoxideBehaviour {
    pub sync: request_response::Behaviour<PolyepoxideCodec>,
}

impl PolyepoxideBehaviour {
    /// Create a new behaviour with the sync protocol.
    pub fn new() -> Self {
        let config = request_response::Config::default();
        let sync = request_response::Behaviour::new(
            [(protocol(), request_response::ProtocolSupport::Full)],
            config,
        );
        Self { sync }
    }
}

impl Default for PolyepoxideBehaviour {
    fn default() -> Self {
        Self::new()
    }
}

/// Drive the swarm, processing commands and events.
///
/// This function runs the swarm event loop, handling:
/// - Outbound requests via the command channel
/// - Inbound requests by calling the handler with the local store
/// - Response matching for pending requests
pub async fn run_swarm<S, T>(
    mut swarm: Swarm<PolyepoxideBehaviour>,
    local_store: S,
    mut command_rx: mpsc::Receiver<Command>,
) where
    S: AsyncStore,
    T: Send,
{
    let mut pending_requests: HashMap<OutboundRequestId, oneshot::Sender<Result<Response, RemoteStoreError>>> =
        HashMap::new();

    loop {
        tokio::select! {
            // Handle incoming commands
            Some(cmd) = command_rx.recv() => {
                match cmd {
                    Command::SendRequest { peer, request, response_tx } => {
                        let request_id = swarm.behaviour_mut().sync.send_request(&peer, request);
                        pending_requests.insert(request_id, response_tx);
                    }
                    Command::SendResponse { channel, response } => {
                        let _ = swarm.behaviour_mut().sync.send_response(channel, response);
                    }
                }
            }

            // Handle swarm events
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::Behaviour(PolyepoxideBehaviourEvent::Sync(req_res_event)) => {
                        match req_res_event {
                            request_response::Event::Message { peer: _, message } => {
                                match message {
                                    request_response::Message::Request { request, channel, .. } => {
                                        let response = handle_request(&local_store, request).await;
                                        let _ = swarm.behaviour_mut().sync.send_response(channel, response);
                                    }
                                    request_response::Message::Response { request_id, response } => {
                                        if let Some(tx) = pending_requests.remove(&request_id) {
                                            let _ = tx.send(Ok(response));
                                        }
                                    }
                                }
                            }
                            request_response::Event::OutboundFailure { request_id, error, .. } => {
                                if let Some(tx) = pending_requests.remove(&request_id) {
                                    let _ = tx.send(Err(RemoteStoreError::RequestFailed(error.to_string())));
                                }
                            }
                            request_response::Event::InboundFailure { .. } => {
                                // Log but don't crash
                            }
                            request_response::Event::ResponseSent { .. } => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
