# POLYEPOXIDE-RS-TRANSPORT

## Dependencies

- `13-POLYEPOXIDE-TRANSPORT`
- `32-POLYEPOXIDE-RS-STORE`

## Intent

Describe how the portable remote store transport contract from `13-POLYEPOXIDE-TRANSPORT` is represented in Rust by the `polyepoxide-libp2p` crate.

This spec records the Rust API surface, protocol message types, libp2p request-response integration, and implementation choices needed for compatibility with the Rust `AsyncStore` trait. It does not restate synchronization traversal from `32-POLYEPOXIDE-RS-STORE`; the transport exposes a remote peer as an `AsyncStore` so existing `pull` and `push` can use it directly.

The crate is named `polyepoxide-libp2p`, while the component spec uses `TRANSPORT` to name the remote store transport role.

## Modules

- `polyepoxide-libp2p/src/protocol.rs`: `PROTOCOL_NAME`, `Request`, `Response`.
- `polyepoxide-libp2p/src/codec.rs`: `PolyepoxideCodec`, libp2p codec implementation, length-prefixed DAG-CBOR framing.
- `polyepoxide-libp2p/src/remote_store.rs`: `RemoteStore`, `Command`, `RemoteStoreError`.
- `polyepoxide-libp2p/src/handler.rs`: `handle_request`.
- `polyepoxide-libp2p/src/lib.rs`: crate exports, `PolyepoxideBehaviour`, `run_swarm`.

## Crate Surface

The crate exports:

```rust
pub const PROTOCOL_NAME: &str = "/polyepoxide/sync/0.1.0";

pub enum Request { ... }
pub enum Response { ... }

pub struct PolyepoxideCodec;
pub fn protocol() -> libp2p::StreamProtocol;

pub struct RemoteStore { ... }
pub enum Command { ... }
pub enum RemoteStoreError { ... }

pub async fn handle_request<S: AsyncStore>(store: &S, request: Request) -> Response;

#[derive(NetworkBehaviour)]
pub struct PolyepoxideBehaviour {
    pub sync: request_response::Behaviour<PolyepoxideCodec>,
}

pub async fn run_swarm<S, T>(
    swarm: Swarm<PolyepoxideBehaviour>,
    local_store: S,
    command_rx: mpsc::Receiver<Command>,
)
where
    S: AsyncStore,
    T: Send;
```

`run_swarm` is the current helper for driving the request-response behavior with a local store and a command channel. Applications may drive `PolyepoxideBehaviour` themselves if they need a larger custom swarm loop.

## Protocol Types

Rust represents the portable batched transport requests as serde-serializable enums:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Get { keys: Vec<Vec<u8>> },
    Has { keys: Vec<Vec<u8>> },
    Put { nodes: Vec<(Vec<u8>, Vec<u8>)> },
}
```

Rust represents responses as:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Nodes {
        found: Vec<(Vec<u8>, Vec<u8>)>,
        missing: Vec<Vec<u8>>,
    },
    Has { present: Vec<bool> },
    Stored { keys: Vec<Vec<u8>> },
    Error { message: String },
}
```

The `Vec<u8>` keys are Rust store keys: CID multihash bytes produced by `key_from_cid` from `polyepoxide-core`. Request and response values are raw block bytes and are not decoded by the transport crate.

## Codec and Framing

`PolyepoxideCodec` implements:

```rust
impl libp2p::request_response::Codec for PolyepoxideCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = Request;
    type Response = Response;
}
```

The wire frame is:

- 4-byte big-endian message length
- DAG-CBOR request or response bytes encoded with `serde_ipld_dagcbor`

The current maximum message size is 16 MiB. Reading or writing a larger message returns an `io::Error` with `InvalidData`.

`protocol()` returns a `StreamProtocol` using `PROTOCOL_NAME`.

## RemoteStore

`RemoteStore` exposes one target peer as a Rust `AsyncStore`:

```rust
pub struct RemoteStore {
    peer_id: PeerId,
    command_tx: mpsc::Sender<Command>,
}

impl RemoteStore {
    pub fn new(peer_id: PeerId, command_tx: mpsc::Sender<Command>) -> Self;
    pub fn peer_id(&self) -> PeerId;
}
```

`RemoteStore` does not own the libp2p connection or swarm. It sends commands to a background swarm driver and waits for a one-shot response:

```rust
pub enum Command {
    SendRequest {
        peer: PeerId,
        request: Request,
        response_tx: oneshot::Sender<Result<Response, RemoteStoreError>>,
    },
    SendResponse {
        channel: ResponseChannel<Response>,
        response: Response,
    },
}
```

This keeps the `AsyncStore` implementation separate from libp2p's event loop ownership. One swarm can serve inbound requests, issue outbound requests, and route responses for many `RemoteStore` handles.

## AsyncStore Implementation

`RemoteStore` implements `AsyncStore<Error = RemoteStoreError>`.

Single-key methods delegate to batched methods:

```rust
async fn async_get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RemoteStoreError>;
async fn async_put(&self, key: &[u8], value: &[u8]) -> Result<(), RemoteStoreError>;
async fn async_has(&self, key: &[u8]) -> Result<bool, RemoteStoreError>;
```

Batched methods send the corresponding protocol request:

```rust
async fn async_get_many(
    &self,
    keys: &[Vec<u8>],
) -> Result<Vec<Option<Vec<u8>>>, RemoteStoreError>;

async fn async_put_many(
    &self,
    nodes: &[(&[u8], &[u8])],
) -> Result<(), RemoteStoreError>;

async fn async_has_many(
    &self,
    keys: &[Vec<u8>],
) -> Result<Vec<bool>, RemoteStoreError>;
```

`async_get_many` converts `Response::Nodes` into an ordered `Vec<Option<Vec<u8>>>` by indexing found nodes by key. Missing keys become `None`.

`async_has_many` expects `Response::Has { present }` and maps the returned booleans back into the original key positions.

`async_put_many` treats `Response::Stored` as success and does not verify the acknowledged key list.

Unexpected response variants become `RemoteStoreError::UnexpectedResponse`. `Response::Error { message }` becomes `RemoteStoreError::Remote(message)`.

## Identity-Multihash Handling

`RemoteStore` applies Rust identity-key handling locally with `identity_digest_from_key` from `polyepoxide-core`.

- `async_get_many` returns the embedded digest bytes for identity keys without sending them to the peer.
- `async_put_many` filters identity-key writes out before sending `Put`.
- `async_has_many` returns `true` for identity keys without sending them to the peer.

If every requested key is handled locally, no transport request is sent.

## Errors

Rust exposes remote store failures as:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RemoteStoreError {
    ConnectionClosed,
    RequestFailed(String),
    UnexpectedResponse,
    Remote(String),
}
```

`ConnectionClosed` is used when the command channel or one-shot response channel closes. `RequestFailed` wraps libp2p outbound request failures reported by the swarm driver. `UnexpectedResponse` means the peer returned a response variant that does not match the request operation. `Remote` wraps protocol-level `Response::Error` messages returned by the peer.

Missing blocks are not errors; they are represented as `None` results from `async_get` and `async_get_many`.

## Inbound Handler

`handle_request` adapts incoming protocol requests to a local `AsyncStore`:

```rust
pub async fn handle_request<S: AsyncStore>(
    store: &S,
    request: Request,
) -> Response;
```

Request handling is:

| Request | Store operation | Response |
| --- | --- | --- |
| `Get { keys }` | `async_get_many(&keys)` | `Nodes { found, missing }` |
| `Has { keys }` | `async_has_many(&keys)` | `Has { present }` |
| `Put { nodes }` | `async_put_many(&refs)` | `Stored { keys }` |

Store errors are converted to `Response::Error { message: error.to_string() }`.

For `Get`, found results are paired with their original keys and missing results are returned as the missing-key list.

For `Put`, the handler returns the submitted keys after `async_put_many` succeeds.

## Behaviour and Swarm Driver

`PolyepoxideBehaviour` wraps libp2p request-response behavior:

```rust
#[derive(NetworkBehaviour)]
pub struct PolyepoxideBehaviour {
    pub sync: request_response::Behaviour<PolyepoxideCodec>,
}
```

`PolyepoxideBehaviour::new()` registers the Polyepoxide protocol with `ProtocolSupport::Full`, so the peer can send outbound requests and serve inbound requests.

`run_swarm` drives the current event loop pattern:

- receives `Command::SendRequest`, sends it through `swarm.behaviour_mut().sync`, and records the one-shot sender by `OutboundRequestId`
- receives `Command::SendResponse` and sends the response on the supplied libp2p response channel
- handles inbound request messages by calling `handle_request(&local_store, request)` and sending the result back
- handles inbound response messages by resolving the matching pending one-shot sender
- converts outbound failures into `RemoteStoreError::RequestFailed`
- ignores inbound failures and response-sent notifications

The pending request table has this logical shape:

```rust
HashMap<OutboundRequestId, oneshot::Sender<Result<Response, RemoteStoreError>>>
```

## Rust Design Choices

- `polyepoxide-libp2p` implements remote store transport with libp2p `request_response`.
- The protocol is batched to match Rust `AsyncStore` batch methods and reduce sync request overhead.
- `RemoteStore` implements `AsyncStore`, not `Store`, because network operations are asynchronous.
- `RemoteStore` communicates through `tokio::sync::mpsc` commands and `oneshot` replies rather than owning a swarm.
- The transport crate handles raw multihash-keyed block bytes only; it does not decode oxides, inspect schemas, or run graph traversal.
- Identity-multihash behavior is handled in `RemoteStore` so remote access matches local identity overlays from `32-POLYEPOXIDE-RS-STORE`.
- DAG-CBOR framing uses `serde_ipld_dagcbor` for protocol messages, separate from oxide block contents.

## Compatibility Checks

The Rust crate should cover the portable transport checks from `13-POLYEPOXIDE-TRANSPORT` with tests equivalent to:

- `Request` serialization round trip through `serde_ipld_dagcbor`
- `Response` serialization round trip through `serde_ipld_dagcbor`
- codec read/write round trip for a request and a response
- oversized codec message is rejected
- `handle_request` returns found and missing keys for `Get`
- `handle_request` preserves order for `Has`
- `handle_request` writes nodes for `Put`
- `RemoteStore::async_get_many` preserves caller-visible key order
- `RemoteStore::async_has_many` preserves caller-visible key order
- identity-key `get`, `has`, and `put` are handled without sending a request
- `pull` can use `RemoteStore` as the source or destination store

## Out of Scope

- peer discovery and dialing policy
- authentication, authorization, and encryption policy beyond the libp2p transport selected by the application
- bookmark discovery or bookmark replication
- graph synchronization traversal, which belongs to `32-POLYEPOXIDE-RS-STORE`
- typed oxide loading or schema interpretation
- persistent backend storage behavior
