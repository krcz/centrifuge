# POLYEPOXIDE-RS-FJALL

## Dependencies

- `13-POLYEPOXIDE-FJALL`
- `32-POLYEPOXIDE-RS-STORE`

## Intent

Describe how the portable Fjall backend contract from `13-POLYEPOXIDE-FJALL` is represented in Rust by the `polyepoxide-fjall` crate.

This spec records the Rust API surface, the mapping from `Store` methods to Fjall operations, and the implementation choices needed for compatibility with the Rust store traits. It does not restate the general store contract from `32-POLYEPOXIDE-RS-STORE` except where the Fjall backend adds concrete behavior.

## Crate Surface

The `polyepoxide-fjall` crate exposes:

```rust
pub const DEFAULT_KEYSPACE: &str = "data";
pub const BOOKMARKS_KEYSPACE: &str = "bookmarks";

pub struct FjallStore { ... }

pub struct FjallError(...);
```

`DEFAULT_KEYSPACE` is the default data keyspace. `BOOKMARKS_KEYSPACE` is the fixed bookmarks keyspace used by the current Rust implementation.

## Types

`FjallStore` holds the opened Fjall database and the two keyspaces used by the `Store` implementation:

```rust
pub struct FjallStore {
    keyspace: Keyspace,
    bookmarks: Keyspace,
    _database: Database,
}
```

`keyspace` stores content-addressed block bytes. `bookmarks` stores raw bookmark bytes. The `Database` handle is retained so the keyspace handles remain valid for the lifetime of the store.

Rust wraps Fjall backend errors in a single error newtype:

```rust
#[derive(Debug, Error)]
#[error("Fjall error: {0}")]
pub struct FjallError(#[from] fjall::Error);
```

`FjallError` is the associated `Store::Error` type for `FjallStore`.

## Constructors

Rust exposes two constructors:

```rust
impl FjallStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, FjallError>;

    pub fn open_keyspace(
        path: impl AsRef<Path>,
        keyspace: &str,
    ) -> Result<Self, FjallError>;
}
```

`open(path)` delegates to `open_keyspace(path, DEFAULT_KEYSPACE)`.

`open_keyspace(path, keyspace)` opens or creates the Fjall database at `path`, opens or creates the named data keyspace, and opens or creates the fixed bookmarks keyspace named by `BOOKMARKS_KEYSPACE`.

Only the data keyspace is configurable in the current Rust API. Multiple `FjallStore` values with different data keyspaces at the same database path still share the same bookmarks keyspace.

## Store Implementation

`FjallStore` implements the synchronous `Store` trait from `polyepoxide-core`.

| `Store` method | Fjall-backed behavior |
| --- | --- |
| `get(key)` | reads `keyspace.get(key)` and converts the value to `Vec<u8>` |
| `put(key, value)` | writes `keyspace.insert(key, value)` |
| `has(key)` | checks `keyspace.contains_key(key)` |
| `get_bookmark_bytes(name)` | reads `bookmarks.get(name.as_bytes())` and converts the value to `Vec<u8>` |
| `put_bookmark_bytes(name, value)` | writes `bookmarks.insert(name.as_bytes(), value)` |

Typed bookmark methods are inherited from the default `Store` implementation:

```rust
fn get_bookmark(&self, name: &str) -> Result<Option<DynBond>, BookmarkError<Self::Error>>;

fn put_bookmark(
    &self,
    name: &str,
    bookmark: &DynBond,
) -> Result<(), BookmarkError<Self::Error>>;
```

Those helpers encode and decode `DynBond` as DAG-CBOR bytes through the raw bookmark byte methods. The Fjall backend stores those bytes but does not interpret their schema or bond contents.

## Keying and Identity Handling

`FjallStore` receives store keys as byte slices from the `Store` API. Callers use `key_from_cid` from `polyepoxide-core` to produce multihash-byte keys.

Because Fjall stores exactly the supplied key bytes, codec-independent addressing comes from the Rust store convention:

- a DAG-CBOR CID and a reflexive CID with the same multihash produce the same `key_from_cid` bytes
- putting through one CID view and getting through the other CID view must read the same block

`FjallStore` does not special-case identity multihashes. Identity-key virtual behavior belongs to `IdentityStoreOverlay` or an equivalent caller-side adapter from `32-POLYEPOXIDE-RS-STORE`.

## Rust Design Choices

- The backend is intentionally thin: it maps store operations to Fjall keyspace operations and does not decode DAG-CBOR blocks.
- The data and bookmark keyspaces are separate to preserve the content-addressed data namespace and mutable metadata namespace split.
- Bookmark values are stored as bytes at the backend boundary; typed `DynBond` behavior is supplied by the shared Rust store trait.
- `&self` store methods rely on Fjall handles and fit the shared-reference `Store` API.
- Backend tuning and Fjall-specific options are kept below this component boundary unless they alter observable store behavior.

## Compatibility Checks

The Rust crate should cover the portable Fjall checks from `13-POLYEPOXIDE-FJALL` with tests equivalent to:

- block put/get round trip
- missing block returns `None`
- `has` is false before put and true after put
- data survives reopening the same database path
- bookmark survives reopening the same database path
- same multihash works across different CID codecs
- `open(path)` uses the default data keyspace
- `open_keyspace(path, name)` uses the named data keyspace and the fixed bookmarks keyspace

## Out of Scope

- asynchronous store wrappers, which come from the blanket `AsyncStore` implementation in `32-POLYEPOXIDE-RS-STORE`
- graph synchronization logic
- typed oxide loading or schema interpretation
- document import/export behavior
- changing Fjall storage layout or migration policy
