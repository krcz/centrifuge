# POLYEPOXIDE-STORE

## Dependencies

- `11-POLYEPOXIDE-MODEL`

## Intent

Define the persistence boundary for Polyepoxide: raw content-addressed byte storage, mutable bookmarks, virtual handling of identity-multihash terms, and store-to-store synchronization.

The store deliberately knows less than the model. It stores bytes by hash and does not decode oxides, interpret schemas, or understand host-language types. This small boundary makes it possible to use many storage engines and transports while keeping graph semantics above the storage layer.

## Conceptual Model

Polyepoxide persistence has two layers:

- content-addressed blocks keyed by multihash bytes
- mutable bookmark metadata keyed by application-chosen names

The content-addressed layer is intentionally minimal. It stores raw bytes and answers presence queries. CID construction, byte serialization, schema interpretation, and graph traversal all happen outside the store.

This boundary matters because Polyepoxide uses both ordinary content CIDs and reflexive CIDs. Different CIDs may share the same multihash while using different codecs. A multihash-keyed store naturally treats such CIDs as different views of the same payload.

## Contract

- Blocks are keyed by CID multihash bytes, not by full CID bytes.
- `get(key)` returns stored bytes or missing.
- `put(key, bytes)` stores bytes at the key.
- `has(key)` tests key presence.
- Store operations are raw-byte operations; serialization and CID construction happen outside the store.
- Different CIDs with the same multihash address the same store payload.
- Implementations may overwrite bytes at an existing key, but correct callers only write bytes whose content hash matches that key.
- Store identity is multihash-based and therefore independent of CID codec.

## Bookmarks

Bookmarks are mutable metadata outside the content-addressed keyspace.

- Bookmark keys are application-chosen strings.
- Bookmark values are `DynamicBond`, not bare CIDs.
- Updating a bookmark does not rewrite existing content-addressed blocks.
- Bookmark operations are store-level metadata operations, not graph traversal operations.
- Bookmark data is not discovered by normal bond traversal unless the bookmarked object itself points to persisted oxide content such as a `Catalogue`.

The logical bookmark value is `DynamicBond` because application roots often need both:

- the value reference
- the schema reference needed to interpret that value

This allows a bookmark to name an application root directly without requiring a separate schema lookup table.

Implementations may realize bookmarks in different ways as long as the observable behavior remains `name -> DynamicBond`. Common patterns include:

- storing serialized bookmark bytes in a separate metadata namespace
- storing one mutable bookmark that points to a persisted `Catalogue`
- any equivalent representation with the same external semantics

The `Catalogue` pattern is useful when an application wants one mutable top-level bookmark while keeping many named entries inside content-addressed data.

## Identity-Multihash Handling

Identity multihashes embed their payload directly in the multihash digest. Store-facing code should be able to treat such keys as ordinary store keys even when no backend block exists for them.

The observable behavior should therefore be:

- `get(identity_key)` returns the digest payload
- `has(identity_key)` returns true
- `put(identity_key, bytes)` is a no-op

This keeps small embedded terms and reflexive slot references usable without forcing every backend to persist synthetic blocks.

## Language-Specific Async Variants

Some languages and platforms need a non-blocking or asynchronous store interface. That is an execution detail, not a different storage model.

Such variants should expose operations equivalent to the synchronous store contract above. Batch operations such as `get_many`, `put_many`, and `has_many` are optional optimizations, not separate semantics.

## Synchronization

Synchronization moves a rooted Polyepoxide graph from one store to another. The entry point is a pair `(value_cid, schema_cid)` because correct traversal requires both content identity and schema interpretation.

Synchronization must:

- traverse the value graph and schema graph together
- resolve schema nodes before using them to interpret value bytes
- resolve reflexive value and schema edges using the current ligation scopes
- transfer dependencies before parents
- track visited traversal states using `(value_cid, schema_cid, value_scope, schema_scope)` or an equivalent state key
- transfer schema blocks alongside value blocks
- treat non-identity reflexive payloads as transferable dependencies
- treat identity-multihash terms as virtual rather than stored blocks

Synchronization may skip a subtree when the destination already has the current value block. This relies on the dependency-first invariant: if a parent block is already present in a correctly synchronized destination, its dependencies are already present as well.

Current reference behavior is permissive for local shape mismatches encountered while discovering optional or nested dependencies. Such branches may be skipped. Missing required root blocks and decode or format failures are synchronization errors.

`push` and `pull` are the same graph-transfer behavior viewed from opposite directions. This spec defines the traversal and transfer contract, not separate semantics for those names.

### Non-Normative Sketch

```text
function sync_pull(source_store, destination_store, value_cid, schema_cid):
  walk(value_cid, schema_cid, value_scope=[], schema_scope=[])

function walk(value_cid, schema_cid, value_scope, schema_scope):
  if already_visited(value_cid, schema_cid, value_scope, schema_scope):
    return
  if destination_store.has(key_from_cid(value_cid)):
    return

  schema = resolve_schema(source_store, destination_store, schema_cid, schema_scope)
  value_bytes = source_store.get(key_from_cid(value_cid)) or fail NotFound(value_cid)
  dependencies = discover_dependencies(value_bytes, schema, value_scope, schema_scope)

  for dependency in dependencies:
    walk(
      dependency.value_cid,
      dependency.schema_cid,
      dependency.value_scope,
      dependency.schema_scope,
    )

  destination_store.put(key_from_cid(value_cid), value_bytes)
```

## Standard Interfaces

Language implementations should expose interfaces equivalent to the following pseudocode. Names may follow host-language conventions, but behavior should match.

```text
interface Store:
  get(key: bytes) -> optional<bytes>
  put(key: bytes, value: bytes) -> void
  has(key: bytes) -> bool

  get_bookmark(name: string) -> optional<DynamicBond>
  put_bookmark(name: string, value: DynamicBond) -> void

function key_from_cid(cid: CID) -> bytes
```

Implementations may also expose lower-level bookmark byte operations internally, as long as the logical bookmark interface above is preserved.

Languages that distinguish synchronous and asynchronous I/O should additionally expose an equivalent non-blocking interface. Optional batch forms such as `get_many`, `put_many`, and `has_many` may be added when they improve performance.

## Error Semantics

The exact error type is language-specific, but store-facing behavior should distinguish at least:

- missing required block
- bookmark decode failure
- invalid value or schema bytes
- backend read or write failure
- synchronization traversal or transfer failure

Identity-multihash virtual handling is not an error case by itself. It is part of the normal store contract.

## Out of Scope

- in-memory typed graph management, which belongs to `12-POLYEPOXIDE-SOLVENT`
- concrete backend implementations
- network protocols that expose stores
- document import and export formats
