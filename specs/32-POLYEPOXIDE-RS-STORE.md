# POLYEPOXIDE-RS-STORE

## Dependencies

- `12-POLYEPOXIDE-STORE`
- `31-POLYEPOXIDE-RS-MODEL`
- `32-POLYEPOXIDE-RS-SOLVENT`

## Intent

Describe how the language-agnostic store interface from `12-POLYEPOXIDE-STORE` is represented in Rust in `polyepoxide-core`.

This spec focuses on the Rust API surface and the Rust implementation choices that matter for compatibility: store traits, bookmark helpers, identity overlays, in-memory reference backend, async adaptation, and sync entry points.

It does not restate the general store contract, bookmark semantics, or sync rules from the portable spec except where the Rust representation adds important detail.

## Modules

- `polyepoxide-core/src/store.rs`: `Store`, `MemoryStore`, `BookmarkError`, `key_from_cid`, `identity_digest_from_key`, `IdentityStoreOverlay`.
- `polyepoxide-core/src/async_store.rs`: `AsyncStore`, batch defaults, `IdentityAsyncStoreOverlay`.
- `polyepoxide-core/src/sync.rs`: `pull`, `push`, `SyncError`, internal traversal helpers.

The Rust store layer operates on `Cid`, `DynBond`, `Structure`, `Solvent`, and reflexive helpers defined in `31-POLYEPOXIDE-RS-MODEL` and `32-POLYEPOXIDE-RS-SOLVENT`.

## Store Trait

Rust represents the portable store interface as a trait with shared-reference methods and an associated backend error type:

```rust
pub trait Store {
    type Error: std::error::Error + Send + Sync + 'static;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    fn has(&self, key: &[u8]) -> Result<bool, Self::Error>;

    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error>;
    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error>;

    fn get_bookmark(&self, name: &str) -> Result<Option<DynBond>, BookmarkError<Self::Error>>;
    fn put_bookmark(
        &self,
        name: &str,
        bookmark: &DynBond,
    ) -> Result<(), BookmarkError<Self::Error>>;
}
```

All methods take `&self`. This lets Rust stores use interior mutability or database handles while still fitting a simple shared-reference API.

Rust also implements `Store` for `&S` when `S: Store + ?Sized`. This keeps borrowed store values usable without wrapper boilerplate.

## Bookmark Helpers

Rust keeps raw bookmark bytes at the trait boundary and provides typed bookmark helpers as default methods.

The default bookmark representation is DAG-CBOR bytes of `DynBond`:

- `get_bookmark` reads raw bytes and decodes them with `DynBond::from_bytes`
- `put_bookmark` encodes a `DynBond` with `to_bytes` and writes the resulting bytes

This matches the portable rule that the logical bookmark value is `DynamicBond` while still allowing backends to store bookmark data however they want internally.

Rust uses a dedicated error type for typed bookmark access:

```rust
pub enum BookmarkError<E: std::error::Error + Send + Sync + 'static> {
    Store(E),
    Decode {
        name: String,
        source: serde_ipld_dagcbor::DecodeError<Infallible>,
    },
}
```

`Store` errors and bookmark decode errors are therefore separated at the Rust API boundary.

## Key Helpers and Identity Overlay

Rust exposes two helpers from `store.rs`:

```rust
pub fn key_from_cid(cid: &Cid) -> Vec<u8>;
pub fn identity_digest_from_key(key: &[u8]) -> Option<Vec<u8>>;
```

`key_from_cid` returns `cid.hash().to_bytes()`. This is the concrete Rust encoding of the portable rule that store keys are multihash bytes rather than full CID bytes.

`identity_digest_from_key` attempts to parse a multihash key and returns the embedded digest payload when the key uses the identity multihash code.

Rust realizes virtual identity-key handling through a wrapper type:

```rust
pub struct IdentityStoreOverlay<'a, S: Store + ?Sized> { ... }

pub fn identity_overlay<S: Store + ?Sized>(
    inner: &S,
) -> IdentityStoreOverlay<'_, S>;
```

`IdentityStoreOverlay` intercepts identity keys:

- `get` returns the embedded digest bytes
- `has` returns `true`
- `put` is a no-op

Bookmark operations pass through to the wrapped store unchanged.

## MemoryStore

Rust includes an in-memory reference backend:

```rust
pub struct MemoryStore {
    data: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    bookmarks: RwLock<HashMap<String, Vec<u8>>>,
}
```

`MemoryStore` implements `Store<Error = Infallible>` and is the default test and smoke-check backend.

Its structure reflects the portable split between:

- content-addressed block bytes
- bookmark metadata bytes

Because keys are multihash bytes, ordinary data CIDs and reflexive/data CID views with the same multihash naturally map to the same stored payload.

## AsyncStore

Rust represents the optional non-blocking store surface as a separate trait:

```rust
pub trait AsyncStore: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn async_get(
        &self,
        key: &[u8],
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send;

    fn async_put(
        &self,
        key: &[u8],
        value: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn async_has(
        &self,
        key: &[u8],
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn async_get_many(
        &self,
        keys: &[Vec<u8>],
    ) -> impl Future<Output = Result<Vec<Option<Vec<u8>>>, Self::Error>> + Send;

    fn async_put_many(
        &self,
        nodes: &[(&[u8], &[u8])],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn async_has_many(
        &self,
        keys: &[Vec<u8>],
    ) -> impl Future<Output = Result<Vec<bool>, Self::Error>> + Send;
}
```

Rust prefixes these methods with `async_` so one type can implement both `Store` and `AsyncStore` without method-name collisions.

The batch methods are provided defaults. Their default behavior is sequential; they are optimization hooks, not separate semantics.

Rust also provides a blanket implementation:

```rust
impl<S: Store + Send + Sync> AsyncStore for S { ... }
```

This allows synchronous embedded stores such as `MemoryStore` to participate in sync and other async-facing code without a separate adapter type.

The async identity wrapper mirrors the synchronous one:

```rust
pub struct IdentityAsyncStoreOverlay<'a, S: AsyncStore + ?Sized> { ... }

pub fn identity_overlay_async<S: AsyncStore + ?Sized>(
    inner: &S,
) -> IdentityAsyncStoreOverlay<'_, S>;
```

## Sync Entry Points

Rust exposes synchronization as async functions in `sync.rs`:

```rust
pub async fn pull<S: AsyncStore, D: AsyncStore>(
    source: &S,
    dest: &D,
    value_cid: Cid,
    schema_cid: Cid,
) -> Result<Vec<Cid>, SyncError<S::Error, D::Error>>;

pub async fn push<S: AsyncStore, D: AsyncStore>(
    source: &S,
    dest: &D,
    value_cid: Cid,
    schema_cid: Cid,
) -> Result<Vec<Cid>, SyncError<S::Error, D::Error>>;
```

`push` delegates to `pull`. Both operate on the same `(value_cid, schema_cid)` root pair as the portable spec.

The return value is the list of CIDs that were actually transferred into the destination store.

## Sync Structure in Rust

The Rust implementation follows the portable sync rules with these concrete choices:

- both source and destination are wrapped in `identity_overlay_async`
- schema traversal uses a local `Solvent` cache for loaded schema cells
- visited states are tracked as `TraversalState { value_cid, schema_cid, scope, schema_scope }`
- raw value bytes are parsed to `Ipld` with `serde_ipld_dagcbor`
- dependency discovery walks the decoded IPLD value together with `Structure`
- non-identity reflexive edges are resolved by following the reflexive term and transferring its payload block as needed
- destination `async_has` short-circuits a whole subtree using the dependency-first invariant

The core recursive helpers are internal rather than public API. Rust uses `Box::pin` in recursive async traversal paths where needed.

Current Rust behavior is intentionally permissive for local schema-shape or value-shape mismatches encountered during dependency discovery: such branches are skipped rather than failing the whole transfer. Missing required root blocks and decode/format failures still surface as sync errors.

## SyncError

Rust uses a source/destination-parameterized error enum for sync:

```rust
pub enum SyncError<S, D> {
    NotFound(Cid),
    Format(String),
    Source(S),
    Dest(D),
}
```

This keeps store backend failures separate from traversal-level failures:

- `NotFound` means a required block is missing
- `Format` means bytes could not be interpreted as the expected persisted structure
- `Source` and `Dest` preserve backend-specific errors from the two stores

## Key Rust Choices

- `Store` keeps raw bookmark byte methods at the trait boundary and layers typed `DynBond` helpers above them.
- Shared-reference `&self` methods let stores use interior mutability and database handles naturally.
- `AsyncStore` is a separate trait with `async_`-prefixed methods rather than an async reinterpretation of the same method names.
- The blanket `AsyncStore` implementation makes sync independent from any specific remote transport or backend family.
- Sync is schema-guided over decoded IPLD rather than based on Rust application types, which keeps the Rust implementation aligned with the portable model.

## Out of Scope

- concrete persistent backend crates such as RocksDB or Fjall wrappers
- network transport protocols
- store-backed import/export formats
- application-level bookmark naming conventions
