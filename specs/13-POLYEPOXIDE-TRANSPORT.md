# POLYEPOXIDE-TRANSPORT

## Dependencies

- `12-POLYEPOXIDE-STORE`

## Intent

Define remote store transport for Polyepoxide: a request/response protocol and adapter model that exposes a remote peer's content-addressed block store as an asynchronous store.

Synchronization, import/export, and other graph-aware operations already know how to walk value and schema graphs. Remote store transport deliberately sits below those operations. It moves raw store blocks by multihash key and lets existing store-level algorithms such as `pull` and `push` use a remote peer without changing their traversal logic.

This spec defines the portable transport contract and common request/response shapes. Language-specific specs should describe concrete networking libraries, event loops, connection ownership, timeout behavior, and host-language error types.

## Conceptual Model

Remote store transport adapts a peer into the content-addressed portion of `AsyncStore`.

The caller sees ordinary asynchronous block operations:

- fetch block bytes by multihash key
- test block presence by multihash key
- write block bytes by multihash key

The transport does not decode oxides, inspect schemas, construct CIDs, discover dependencies, resolve conflicts, or decide traversal order. Those responsibilities stay with the store and sync layers described by `12-POLYEPOXIDE-STORE`.

Bookmarks are not part of the remote block protocol. A bookmark may be used by an application to discover a `(value_cid, schema_cid)` root, but transport-level synchronization operates after such roots are known.

## Protocol Model

The protocol is a point-to-point request/response protocol over a binary-safe byte channel. Messages are serialized as DAG-CBOR.

The request set is:

- `Get(keys)`: fetch block bytes for multihash keys.
- `Has(keys)`: test block presence for multihash keys.
- `Put(nodes)`: store `(key, bytes)` block pairs.

The response set is:

- `Nodes(found, missing)`: response to `Get`.
- `Has(present)`: response to `Has`.
- `Stored(keys)`: response to `Put`.
- `Error(message)`: request-level failure.

`Get` responses identify both found and missing keys. Found blocks are returned as keyed pairs so callers do not depend on response order. `Has` responses preserve request order. `Put` responses report accepted keys and do not need to recompute or validate hashes.

Implementations should batch requests because synchronization often performs many small reads and presence checks. Single-key store operations can be implemented as one-element batches.

## Contract

- Transport operates on store keys, not full CIDs, schemas, values, bookmarks, or host-language types.
- Store keys are CID multihash bytes as defined by `12-POLYEPOXIDE-STORE`.
- Request and response bodies must preserve arbitrary bytes without text encoding or lossy conversion.
- A remote store adapter must satisfy the asynchronous store contract for raw block operations.
- `Get` must distinguish missing blocks from empty block bytes.
- `Has` must return one boolean for each requested key in the same order.
- `Put` may acknowledge accepted keys without proving that each value hashes to its key.
- Identity-multihash keys should be handled locally when possible: `get` returns the embedded digest, `has` returns true, and `put` is a no-op.
- Message size limits, request timeouts, and retry policies are implementation-specific but should be explicit so callers do not accidentally rely on unbounded buffering.

Transport errors are request-level failures. Missing blocks are normal `Get` results, not transport errors.

## Standard Interfaces

Language implementations should expose interfaces equivalent to the following pseudocode. Names may follow host-language conventions, but behavior should match.

```text
type StoreKey = bytes
type BlockBytes = bytes

type BlockNode:
  key: StoreKey
  bytes: BlockBytes

enum TransportRequest:
  Get(keys: list<StoreKey>)
  Has(keys: list<StoreKey>)
  Put(nodes: list<BlockNode>)

enum TransportResponse:
  Nodes(found: list<BlockNode>, missing: list<StoreKey>)
  Has(present: list<bool>)
  Stored(keys: list<StoreKey>)
  Error(message: string)
```

The remote store adapter exposes a peer through the async block-store surface:

```text
interface RemoteStore:
  async get(key: StoreKey) -> optional<BlockBytes>
  async put(key: StoreKey, value: BlockBytes) -> void
  async has(key: StoreKey) -> bool

  async get_many(keys: list<StoreKey>) -> list<optional<BlockBytes>>
  async put_many(nodes: list<BlockNode>) -> void
  async has_many(keys: list<StoreKey>) -> list<bool>
```

`RemoteStore` may send requests directly on a connection or enqueue commands for a background network driver. In either design, the adapter is responsible for matching responses to requests and translating successful responses into the asynchronous store behavior above.

Inbound peers should expose a handler equivalent to:

```text
function handle_transport_request(
  local_store: AsyncStore,
  request: TransportRequest,
) -> TransportResponse
```

The handler maps transport messages to local store operations:

- `Get(keys)` calls `get_many(keys)` and returns keyed found/missing results.
- `Has(keys)` calls `has_many(keys)` and returns ordered presence flags.
- `Put(nodes)` calls `put_many(nodes)` and returns accepted keys.

If a local store operation fails, the handler returns `Error(message)`.

## Remote Store Behavior

For `get_many`, the adapter should return results in the same order as the input keys. A `Nodes` response can be converted to this ordered result by indexing found blocks by key and leaving missing keys as `none`.

For `put_many`, the adapter may ignore the exact `Stored(keys)` list if the request succeeded. Implementations that need stronger acknowledgement semantics may compare the stored keys with the requested keys, but that is an implementation policy rather than a portable transport requirement.

For identity-multihash keys, the adapter should apply the virtual store rules before contacting the remote peer. This avoids network traffic for data already embedded in the key and keeps remote transport behavior aligned with local identity overlays.

## Compatibility Checks

A remote store transport should be checked against the generic async store contract and the following transport-specific cases:

- request and response serialization round trip
- `Get` returns found block bytes
- `Get` reports missing keys separately from found keys
- `Has` preserves input order
- `Put` writes blocks into the receiving store
- single-key operations work through one-element batches
- batched operations preserve caller-visible ordering
- request-level store failures become `Error` responses
- identity-multihash `get`, `has`, and `put` are handled without requiring remote storage
- a standard `pull` can use a remote store adapter without transport-specific sync logic

## Out of Scope

- peer discovery
- connection establishment and network topology
- authorization and authentication policy
- bookmark discovery or bookmark replication
- graph traversal, dependency completeness, and conflict resolution
- concrete backend storage engines
