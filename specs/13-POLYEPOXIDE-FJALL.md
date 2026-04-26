# POLYEPOXIDE-FJALL

## Dependencies

- `12-POLYEPOXIDE-STORE`

## Intent

Define the expected behavior of a Fjall-backed persistent Polyepoxide store.

Fjall is an embedded disk-backed backend for local-first prototypes, desktop tools, and other deployments that need a small durable store without changing the Polyepoxide store interface. It should behave like any other store from `12-POLYEPOXIDE-STORE` while preserving block data and bookmarks across process restarts.

This spec describes the portable backend behavior. Language-specific specs should describe concrete constructor names, error types, handle lifetimes, and host-language wrapping of Fjall APIs.

## Storage Model

A Fjall-backed store uses one Fjall database with separate storage namespaces for content-addressed blocks and mutable bookmarks.

- The data namespace stores raw block bytes keyed by CID multihash bytes.
- The bookmark namespace stores serialized `DynamicBond` bookmark values keyed by bookmark-name bytes.
- Data and bookmark namespaces must be separate so application-chosen bookmark names cannot collide with content-addressed block keys.
- Opening a missing database creates the required namespaces.
- Reopening an existing database preserves both block data and bookmarks.

The default data namespace should be named `data`. The default bookmark namespace should be named `bookmarks`.

Implementations may allow callers to select a non-default data namespace so multiple block stores can share one Fjall database path. If bookmark namespaces are not configurable as part of the same option, bookmarks remain shared by those logical stores and must not be described as isolated.

## Contract

- The backend implements the store contract from `12-POLYEPOXIDE-STORE`.
- Store operations are raw-byte operations and do not decode oxides, schemas, DAG-CBOR values, or synchronization state.
- Content blocks are addressed only by multihash bytes.
- Different CIDs with the same multihash address the same physical block entry.
- Codec differences must not create distinct physical entries when multihash bytes are equal.
- Bookmark operations expose the logical `name -> DynamicBond` behavior required by the store spec.
- Bookmark serialization is backend storage detail; callers observe `DynamicBond`, not backend bytes.
- Backend-specific tuning, compaction, caching, and file-management choices must not change the public store behavior.

Identity-multihash virtual handling belongs to the store layer described by `12-POLYEPOXIDE-STORE`. A Fjall backend may rely on a store overlay or equivalent adapter for identity keys; it does not need to persist synthetic identity blocks.

## Standard Interfaces

Language implementations should expose an opening surface equivalent to the following pseudocode. Names may follow host-language conventions.

```text
type FjallStoreOptions:
  data_namespace: optional<string>
  bookmark_namespace: optional<string>

function open_fjall_store(path: path) -> Result<Store>

function open_fjall_store(path: path, options: FjallStoreOptions) -> Result<Store>
```

If a language implementation exposes only data namespace configuration, its options should make that limitation clear:

```text
type FjallStoreOptions:
  data_namespace: optional<string>
```

The returned value must implement the normal `Store` interface:

```text
interface Store:
  get(key: bytes) -> optional<bytes>
  put(key: bytes, value: bytes) -> void
  has(key: bytes) -> bool

  get_bookmark(name: string) -> optional<DynamicBond>
  put_bookmark(name: string, value: DynamicBond) -> void
```

Implementations may expose lower-level raw bookmark byte methods internally when that matches the host-language store API, but the public logical behavior remains `DynamicBond` bookmarks.

## Compatibility Checks

A Fjall-backed store should be checked against the generic store contract and the following backend-specific cases:

- block put/get round trip
- missing block returns missing
- `has` is false before put and true after put
- reopening preserves block data
- reopening preserves bookmarks
- bookmark names use a namespace separate from block keys
- a DAG-CBOR CID and a reflexive CID with the same multihash read the same stored block
- a configurable data namespace does not change bookmark behavior unless bookmark namespace configuration is also provided

## Out of Scope

- host-language Fjall API details
- typed graph management and schema interpretation
- import/export document formats
- store-to-store synchronization algorithms
- network transports exposing stores remotely
