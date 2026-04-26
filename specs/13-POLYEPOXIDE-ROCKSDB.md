# POLYEPOXIDE-ROCKSDB

## Dependencies

- `12-POLYEPOXIDE-STORE`

## Intent

Define the expected behavior of a RocksDB-backed persistent Polyepoxide store.

RocksDB is an embedded disk-backed backend for deployments that want a widely used local persistence engine without changing the Polyepoxide store interface. In Polyepoxide it is a backend binding, not a different data model. It should behave like any other store from `12-POLYEPOXIDE-STORE` while preserving block data and bookmarks across process restarts.

This spec describes the portable backend behavior. Language-specific specs should describe concrete constructor names, error types, RocksDB option choices, and host-language wrapping of RocksDB APIs.

## Storage Model

A RocksDB-backed store uses one RocksDB database with separate column families for content-addressed blocks and mutable bookmarks.

- The default column family stores raw block bytes keyed by CID multihash bytes.
- The bookmarks column family stores serialized `DynamicBond` bookmark values keyed by bookmark-name bytes.
- Data and bookmark column families must be separate so application-chosen bookmark names cannot collide with content-addressed block keys.
- Opening a missing database creates the required column families.
- Opening an existing database creates any required column families that are absent, when supported by the host binding.
- Reopening an existing database preserves both block data and bookmarks.

The default block column family should be named `default`. The bookmark column family should be named `bookmarks`.

## Contract

- The backend implements the store contract from `12-POLYEPOXIDE-STORE`.
- Store operations are raw-byte operations and do not decode oxides, schemas, DAG-CBOR values, or synchronization state.
- Content blocks are addressed only by multihash bytes.
- Different CIDs with the same multihash address the same physical block entry.
- Codec differences must not create distinct physical entries when multihash bytes are equal.
- Bookmark operations expose the logical `name -> DynamicBond` behavior required by the store spec.
- Bookmark serialization is backend storage detail; callers observe `DynamicBond`, not backend bytes.
- The backend does not validate that stored bytes hash to the supplied key; correct callers only write bytes whose content hash matches that key.
- Backend-specific tuning, compaction, column-family options, caching, and file-management choices must not change the public store behavior.

Identity-multihash virtual handling belongs to the store layer described by `12-POLYEPOXIDE-STORE`. A RocksDB backend may rely on a store overlay or equivalent adapter for identity keys; it does not need to persist synthetic identity blocks.

## Standard Interfaces

Language implementations should expose an opening surface equivalent to the following pseudocode. Names may follow host-language conventions.

```text
function open_rocksdb_store(path: path) -> Result<Store>
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

A RocksDB-backed store should be checked against the generic store contract and the following backend-specific cases:

- block put/get round trip
- missing block returns missing
- `has` is false before put and true after put
- reopening preserves block data
- reopening preserves bookmarks
- bookmark names use a column family separate from block keys
- a DAG-CBOR CID and a reflexive CID with the same multihash read the same stored block
- opening a missing database creates the default and bookmarks column families
- reopening an existing database opens both required column families

## Out of Scope

- host-language RocksDB API details
- typed graph management and schema interpretation
- import/export document formats
- store-to-store synchronization algorithms
- network transports exposing stores remotely
- database migration policy beyond opening the required column families
