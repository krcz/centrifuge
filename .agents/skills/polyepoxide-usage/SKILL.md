---
name: polyepoxide-usage
description: >
  Integrate polyepoxide into existing Rust applications and build new
  polyepoxide-based integrations in this repo. Covers oxide modeling,
  solvent/store usage, traversal, persistence, and sync. Not for changing
  polyepoxide-core internals.
---

# Polyepoxide Usage Patterns

Use this skill for application work that depends on `polyepoxide-rs` crates,
including integrating polyepoxide into existing codebases.

Do not use this skill for `polyepoxide-core` internals (codecs, schema engine, sync
algorithm internals) unless the user explicitly asks for core changes.

## Read First

1. Read `docs/polyepoxide.md` for model and invariants.
2. Treat this file as the default app-level playbook.
3. When details are unclear, confirm patterns in repo examples such as:
   `silane-rs/silane-goog/src/convert.rs` and crates under `aldehyde-rs`.

## Project Setup

Use `polyepoxide-core` as the primary dependency. The default `derive` feature
re-exports `#[oxide]` and `#[derive(Oxide)]`, so do not add
`polyepoxide-derive` directly in app crates.

```toml
[dependencies]
polyepoxide-core = { path = "../../polyepoxide-rs/polyepoxide-core" }
serde = { version = "1", features = ["derive"] }
```

Common imports:

```rust
use polyepoxide_core::{oxide, Bond, Catalogue, Cell, Cursor, DynBond, MemoryStore, Oxide, Solvent};
use polyepoxide_core::{Store, key_from_cid, pull, push};
```

## Core Rules

- Define app data types with `#[oxide]` by default.
- Keep domain values as normal Rust structs; use `Bond<T>` for graph edges.
- Build and mutate values ephemerally first.
- Insert into `Solvent` (`add` / `bond`) to deduplicate and stabilize references.
- Persist and sync using `(value_cid, schema_cid)` pairs.
- Give each app its own default bookmark, with a config override for the bookmark name.
- Prefer `Cursor` for traversal of solvent-managed data.
- Prefer compact end-to-end integration tests over many tiny unit tests.

## Oxide Modeling

Preferred style:

```rust
use polyepoxide_core::{Bond, oxide};

#[derive(PartialEq, Eq)]
#[oxide]
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub previous: Option<Bond<Ticket>>,
}
```

Notes:

- `#[oxide]` auto-derives `Debug`, `Clone`, `Serialize`, `Deserialize`, and `Oxide`.
- If you need extra derives, place them before `#[oxide]`.
- Field-level Oxide attributes are `#[oxide(rename = "...")]` and `#[oxide(skip)]`.
- For runtime-only cached fields, pair `#[oxide(skip)]` with `#[serde(skip)]`.
- In `polyepoxide-core` internals/tests, `#[oxide(crate = crate)]` may be needed.

### Supported Field Shapes (App-Level)

- Primitives: `bool`, integer types, floats, `String`, `()`
- Containers: `Vec<T>`, `Option<T>`, tuples
- References: `Bond<T>` where `T: Oxide`
- Bytes and IDs: `ByteString`, `Cid`
- Enums: unit enums and payload enums

## Ephemeral vs Solvent-Managed Values

- Ephemeral value: normal Rust value, freely mutable, not deduplicated.
- Solvent-managed value: immutable `Cell<T>` with CID identity and dedup semantics.

Preferred flow:

1. Build/modify values as regular Rust data.
2. Internalize with `solvent.add(...)` or `solvent.bond(...)`.
3. Keep references as `Bond<T>` for later traversal/persistence.

Two valid construction styles:

- Bottom-up (preferred for integrations): add children first with `solvent.bond`.
- Top-down: use `Bond::new(...)` in ephemeral trees, then internalize at root add.

## Solvent and Store Patterns

Basic setup:

```rust
let solvent = Solvent::new();
let store = MemoryStore::new();

let cell = solvent.add(my_root_value);
let (value_cid, schema_cid) = solvent.persist_cell(&cell, &store)?;
```

Useful operations:

- `solvent.add(value) -> Arc<Cell<T>>`
- `solvent.bond(value) -> Bond<T>`
- `solvent.get::<T>(&cid) -> Option<Arc<Cell<T>>>`
- `solvent.resolve(&bond) -> Bond<T>`

Store backends:

- `MemoryStore` for tests
- `FjallStore` and `RocksStore` from sibling crates for persistence

Bookmarks:

- Use bookmarks for mutable app entry points.
- Store `DynBond`, not bare CIDs.
- Each app should own one default top-level bookmark.
- If the app has many named roots, point that bookmark at a `Catalogue`.
- Per-entry names then live inside the `Catalogue`, not as shared global bookmarks.

## Loading from Store

Load a root value:

```rust
let bytes = store.get(&key_from_cid(&value_cid))?.ok_or("missing value")?;
let value: MyType = Oxide::from_bytes(&bytes)?;
let cell = solvent.add(value);
```

For linked graphs, load dependencies before parents so bonds resolve on add.

When loading via bookmark, read the bookmark first and use its `cid()` and
`schema_cid()`. If the bookmark points to a `Catalogue`, load the catalogue and
then pick the desired entry.

## Sync Pattern

Use `pull` and `push` with CID pairs:

```rust
let transferred = pull(&source_store, &dest_store, value_cid, schema_cid).await?;
let transferred_back = push(&dest_store, &source_store, value_cid, schema_cid).await?;
```

Both transfer transitive dependencies. Stores must satisfy `AsyncStore`
(sync `Store` backends work via blanket impls).

## Traversal Patterns

### Default traversal (most app code)

Use `Cursor` and `follow(...)` for child bonds. This is especially important for
store-loaded data where bonds may start unresolved.

```rust
use polyepoxide_core::Cursor;

let cursor = Cursor::new(&solvent, root_cell);

// follow takes a closure returning &Bond<U> from the current value.
let next = cursor.follow(|v| &v.next)?;
let value = next.value();
```

Use direct `bond.value()` access only in already-resolved in-memory flows where
you know the bond is resolved.

### Schema-guided traversal (rare/advanced)

Most applications should not need schema-guided traversal.

- Parse bytes with `traverse::parse_to_ipld`
- Walk value and schema together using `Structure` variants
- Resolve/load schema nodes in a separate schema solvent, then recurse by shape
- For generic dependency extraction, reuse `traverse::collect_bonds`

## Data Integration Recipe

1. Define oxide domain types.
2. Convert external records into ephemeral Rust values.
3. While converting, create child references with `solvent.bond(child)`.
4. Return ephemeral parents and add them to solvent at orchestration boundaries.
5. Persist root cells with `persist_cell`.
6. Update the app bookmark after persisting.
7. Store and propagate `(value_cid, schema_cid)` as the app state handle when needed.

## Testing Guidance

Minimum useful coverage for feature work:

- Oxide roundtrip (`to_bytes` / `from_bytes`)
- Solvent insertion and bond resolution behavior
- Persist/load roundtrip using selected store backend
- Bookmark roundtrip for the app's default bookmark

Sync tests:

- Prefer small smoke tests in app crates only when new integration risk exists.
- Leave deep sync algorithm coverage to core-library tests.

## Continuous Improvement Loop

If behavior differs from this skill or `docs/polyepoxide.md`:

1. Call out the mismatch explicitly.
2. Propose a Markdown bug report in `docs/bugs/` with date and topic.
3. Include expected vs actual behavior, reproduction snippet, and file references.

If this skill is unclear or missing a reusable pattern:

- Propose concrete edits to this file and/or `docs/polyepoxide.md`.
