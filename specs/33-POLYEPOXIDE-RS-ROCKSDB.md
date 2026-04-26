# POLYEPOXIDE-RS-ROCKSDB

## Dependencies

- `13-POLYEPOXIDE-ROCKSDB`
- `32-POLYEPOXIDE-RS-STORE`

## Intent

Describe how the portable RocksDB backend contract from `13-POLYEPOXIDE-ROCKSDB` is represented in Rust by the `polyepoxide-rocks` crate.

This spec records the Rust API surface, the mapping from `Store` methods to RocksDB operations, and the implementation choices needed for compatibility with the Rust store traits. It does not restate the general store contract from `32-POLYEPOXIDE-RS-STORE` except where the RocksDB backend adds concrete behavior.

The Rust crate is named `polyepoxide-rocks`, while the component spec uses `ROCKSDB` to name the backend clearly.

## Crate Surface

The `polyepoxide-rocks` crate exposes:

```rust
pub struct RocksStore { ... }

pub struct RocksError(...);
```

The crate also uses an internal bookmarks column-family constant:

```rust
const BOOKMARKS_CF: &str = "bookmarks";
```

`BOOKMARKS_CF` is not part of the public API, but it is part of the storage layout that must remain compatible with persisted stores.

## Types

`RocksStore` holds a single opened RocksDB database:

```rust
pub struct RocksStore {
    db: DB,
}
```

The database contains both the default column family for block data and the `bookmarks` column family for raw bookmark bytes. The bookmarks column-family handle is retrieved from the database when bookmark operations run.

Rust wraps RocksDB backend errors in a single error newtype:

```rust
#[derive(Debug, Error)]
#[error("RocksDB error: {0}")]
pub struct RocksError(#[from] rocksdb::Error);
```

`RocksError` is the associated `Store::Error` type for `RocksStore`.

## Constructor

Rust exposes one constructor:

```rust
impl RocksStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RocksError>;
}
```

`open(path)` opens or creates the RocksDB database at `path` with:

```rust
opts.create_if_missing(true);
opts.create_missing_column_families(true);
```

The database is opened with column-family descriptors for:

- `"default"` for content-addressed block bytes
- `"bookmarks"` for raw bookmark bytes

The current Rust API does not expose configurable column-family names.

## Store Implementation

`RocksStore` implements the synchronous `Store` trait from `polyepoxide-core`.

| `Store` method | RocksDB-backed behavior |
| --- | --- |
| `get(key)` | reads `db.get(key)` |
| `put(key, value)` | writes `db.put(key, value)` |
| `has(key)` | checks `db.get_pinned(key)?.is_some()` |
| `get_bookmark_bytes(name)` | reads `db.get_cf(bookmarks_cf, name.as_bytes())` |
| `put_bookmark_bytes(name, value)` | writes `db.put_cf(bookmarks_cf, name.as_bytes(), value)` |

`has` uses `get_pinned` rather than `get` so existence checks do not allocate and copy value bytes.

Typed bookmark methods are inherited from the default `Store` implementation:

```rust
fn get_bookmark(&self, name: &str) -> Result<Option<DynBond>, BookmarkError<Self::Error>>;

fn put_bookmark(
    &self,
    name: &str,
    bookmark: &DynBond,
) -> Result<(), BookmarkError<Self::Error>>;
```

Those helpers encode and decode `DynBond` as DAG-CBOR bytes through the raw bookmark byte methods. The RocksDB backend stores those bytes but does not interpret their schema or bond contents.

## Column-Family Invariant

Bookmark operations retrieve the bookmarks column-family handle with:

```rust
db.cf_handle(BOOKMARKS_CF).expect("missing bookmarks cf")
```

Missing `bookmarks` after a successful `RocksStore::open` is treated as a programming invariant violation, not a recoverable store operation error. The constructor declares the column family and enables missing-column-family creation, so normal callers should not observe this panic.

## Keying and Identity Handling

`RocksStore` receives store keys as byte slices from the `Store` API. Callers use `key_from_cid` from `polyepoxide-core` to produce multihash-byte keys.

Because RocksDB stores exactly the supplied key bytes, codec-independent addressing comes from the Rust store convention:

- a DAG-CBOR CID and a reflexive CID with the same multihash produce the same `key_from_cid` bytes
- putting through one CID view and getting through the other CID view must read the same block

`RocksStore` does not special-case identity multihashes. Identity-key virtual behavior belongs to `IdentityStoreOverlay` or an equivalent caller-side adapter from `32-POLYEPOXIDE-RS-STORE`.

## Rust Design Choices

- The backend is intentionally thin: it maps store operations to RocksDB operations and does not decode DAG-CBOR blocks.
- The default and bookmarks column families are separate to preserve the content-addressed data namespace and mutable metadata namespace split.
- Bookmark values are stored as bytes at the backend boundary; typed `DynBond` behavior is supplied by the shared Rust store trait.
- `&self` store methods rely on the RocksDB handle and fit the shared-reference `Store` API.
- The backend does not validate that stored bytes hash to the supplied key; that invariant belongs to correct callers and compatibility checks.
- RocksDB-specific options remain below this component boundary unless they alter observable store behavior.

## Compatibility Checks

The Rust crate should cover the portable RocksDB checks from `13-POLYEPOXIDE-ROCKSDB` with tests equivalent to:

- block put/get round trip
- missing block returns `None`
- `has` is false before put and true after put
- data survives reopening the same database path
- bookmark survives reopening the same database path
- same multihash works across different CID codecs
- opening a missing database creates the default and bookmarks column families
- bookmark operations use the `bookmarks` column family, not the default column family

## Out of Scope

- asynchronous store wrappers, which come from the blanket `AsyncStore` implementation in `32-POLYEPOXIDE-RS-STORE`
- graph synchronization logic
- typed oxide loading or schema interpretation
- document import/export behavior
- changing RocksDB storage layout or migration policy
