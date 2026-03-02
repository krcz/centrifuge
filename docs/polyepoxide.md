# Polyepoxide

## Overview

Polyepoxide is an experimental synchronization database for applications that want local-first behavior without giving up content addressing. The project starts from a practical problem: user data is increasingly fragmented across many cloud applications, each with its own storage model and synchronization semantics. That fragmentation creates long-term lock-in, complicates interoperability, and weakens privacy guarantees because storage and access are controlled by service providers, making portability expensive in practice.

The goal of Polyepoxide is to provide a graph-native, content-addressed model that is still convenient for everyday product development. In particular, the design intentionally narrows the gap between "normal runtime objects" and "persisted graph objects." Developers should be able to construct and mutate data in memory as usual, then commit it into an immutable graph when needed. Polyepoxide does not attempt to hide that persistence is content-addressed; instead, it makes that boundary explicit and operationally simple.

At a high level, Polyepoxide stores immutable nodes in a directed graph, identifies them by CID, and serializes them in IPLD-compatible formats (primarily DAG-CBOR). It also introduces a reflexive overlay codec (`polyepoxide-reflexive`) that enables templates and cycles while preserving a DAG-compatible persistence layer.

### Pre-Alpha Disclaimer

Polyepoxide is pre-alpha. Format details, schema representation, and APIs may change at any time, and no compatibility guarantees should be assumed. There is currently no promise of migration support across breaking versions.

### Glossary

- `Oxide`: interface implemented by graph node types. It defines schema, encoding/decoding, CID derivation, bond traversal, and dissolution into a solvent.
- `Structure`: schema language used to describe oxide shapes, including recursive and parametric forms through ligation.
- `CID`: content identifier composed of multicodec and multihash. Two CIDs can share the same multihash while using different codecs.
- `Cell`: immutable in-memory wrapper around an oxide value plus its CID.
- `Bond`: reference to another node with three states: unresolved, link, or ligation.
- **erased bond**: `Bond<?>`, a type-erased/wildcard/existential view of a bond used when concrete type parameters are unknown during traversal.
- `DynamicBond`: schema-carrying erased bond pairing `Bond<Structure>` with an **erased bond**.
- `Catalogue`: oxide record containing named dynamic bonds (`items: map<string, DynamicBond>`).
- `Bookmark`: mutable store-level mapping from `string` to `DynamicBond`, stored outside the content-addressed keyspace.
- `BondVisitor`: callback/visitor used for outbound reference discovery; receives bond target CIDs, not full bond state.
- `Ligation`: reflexive mechanism represented by `Ligase` and `Slot`, used for templates and cyclic data.
- `Solvent`: in-memory deduplicating graph manager that internalizes and resolves bonds.
- `Cursor`: traversal helper combining solvent access, current cell, and current ligation scope.
- `Store`: content-addressable byte storage indexed by multihash bytes.

## Conceptual Model

Polyepoxide works with two graphs that are traversed together. The first graph is the value graph containing application data. The second graph is the schema graph containing `Structure` nodes that describe how values should be interpreted. Operations such as sync and generic traversal are most accurate when they run as a zipped walk across both graphs: value bytes are interpreted through schema context at each step. This allows compact binary data without requiring every client to have every concrete type compiled in.

This dual-graph model is central to the design. Value graph traversal alone is not enough when a client does not know concrete types, because schema context is needed to interpret data correctly. Polyepoxide therefore treats value and schema as equally first-class, both content-addressed and both synchronized.

## Core Concepts

### Oxide

An oxide is the base abstraction for graph nodes. Each oxide type may contain direct fields (embedded in serialization) and bond fields (references to other oxides). The oxide interface is responsible for schema definition, binary encoding/decoding, CID computation, and enumeration/dissolution of bonds.

In practical terms, an implementation should expose methods equivalent to: `schema()`, `to_bytes()`, `from_bytes()`, `compute_cid()`, bond visitation, and `dissolve_in(solvent)`. The last method is critical: before a value becomes part of a solvent-managed graph, child bonds must be converted into the internal bond form that will later serialize as CID or reflexive references.

`visit_bonds` is an outbound identity-discovery operation: the visitor should receive referenced target CIDs (or equivalent identity values), not full bond objects. This keeps traversal/persistence/sync dependency discovery independent from in-memory bond state.

### Structure

`Structure` describes the shape of an oxide and enables schema-driven traversal for clients that do not have concrete type implementations. This is important both for interoperability and for tooling that inspects arbitrary data.

`Structure` supports primitive and composite forms: booleans, characters, text, bytes, integers, floats, `Unit`, sequences, tuples, records, tagged unions, enums, and bond-bearing forms. Recursive and parametric structures are represented through ligation rather than ad-hoc language features, so the same schema model can work across languages.

#### Type Mapping (Current Intent)

| Polyepoxide `Structure` | IPLD / DAG-CBOR shape | WIT-style shape | Typical Rust-style shape |
| --- | --- | --- | --- |
| `Bool` | CBOR bool | `bool` | `bool` |
| `Char` | CBOR text (single Unicode scalar value) | `char` | `char` |
| `Unicode` | CBOR text | `string` | `String` |
| `ByteString` | CBOR bytes | `list<u8>` / `bytes` | `Vec<u8>` |
| `Cid` | IPLD link | opaque CID/id | `Cid` |
| `Int(...)` | CBOR integer | signed and unsigned integer widths | `u8..u64`, `i8..i64` |
| `Float(F32/F64)` | CBOR float | `f32` / `f64` | `f32` / `f64` |
| `Unit` | unit value | `unit` | `()` |
| `Sequence(T)` | list | `list<T>` | `Vec<T>` |
| `Tuple([A,B,..])` | fixed list | tuple | `(A,B,...)` |
| `Record` | map or field-ordered record encoding | record | `struct` |
| `Tagged` | tagged map/form | variant with payload | data-carrying enum |
| `Enum` | symbolic/index variant | enum | unit enum |
| `Map` | CBOR map (unordered key/value entries) | map | hash/dictionary map |
| `OrderedMap` | list of `[key, value]` pairs (order-preserving) | ordered map / list of pairs | ordered map / vector of pairs |
| `Bond(T)` | CID link | reference | `Bond<T>` |

In practical IPLD terms, `Map` is best suited for unordered text-key maps. `OrderedMap` is the portable way to preserve order and support non-text keys, because it is encoded as explicit key/value pairs.

For cross-language parity, language-specific encoding and mapping rules should also document container encodings for host-language sum/optional types (for example, `Option`-like and `Result`-like values) when those are mapped through `Structure::Sequence`/`Structure::Tagged`.

#### Polyepoxide Data Structures

Some applications need to store and index very large collections. In those cases, we prefer not to keep all data in a single oxide node. Instead, Polyepoxide defines standard, composable data structures that spread content across multiple linked nodes while preserving deterministic, content-addressed behavior.

See [polyepoxide-data.rs] specification.

### Solvent

Solvent is the in-memory graph runtime. It owns immutable cells, deduplicates them by CID, resolves references, and provides a controlled boundary between mutable runtime objects and immutable graph state.

For convenience, oxides can exist outside solvent as normal mutable values. Once inserted into solvent, the inserted representation is immutable and deduplicated transitively. During insertion, bonds are dissolved so that references point to solvent-managed cells whenever possible.

The key invariant is that resolved solvent links only point to cells in the same solvent. Unresolved bonds may still exist as CID-only references when data is not currently materialized, but resolved links must not cross solvent boundaries.

Because solvent cells are deduplicated and shared, implementations must enforce immutability at the type level, not just by convention. Handles returned by solvent operations must expose only read access to the contained value; no path to mutate through a shared handle should exist.

### Bond and Erased Bonds

`Bond` is a typed reference abstraction in languages that support generic types. It has three states: unresolved CID-only reference, resolved in-memory link, and ligation reference.

An erased bond is the type-agnostic counterpart used where concrete type information is unknown. In some languages, this is represented directly as a wildcard/existential form of `Bond<T>` (for example `Bond<?>`/`Bond<*>`) rather than a separate runtime type. In others, explicit erased bond/cell wrapper types remain the clearest design for heterogenous solvent storage and traversal.

Correspondingly, a dedicated solvent API such as `add_erased_bond` is optional in runtimes with native wildcard erasure. Those runtimes can often use `Bond<?>` directly with the regular bond insertion/resolution APIs.

Binary persistence behavior is intentionally simple and stable: in DAG-CBOR and related raw oxide codecs, bonds serialize as CID values. Deserialization yields `Unresolved`, because a byte stream only carries identity, not in-memory pointers. Resolved links are reconstructed later via solvent or cursor resolution. Higher-level import/export profiles may expose additional hydrated structure for inspection and editing.

`DynamicBond` (Rust type: `DynBond`) is an oxide wrapper with:

- `schema: Bond<Structure>`
- `bond: Bond<?>`

It represents a schema-bound dynamic reference. Typed resolution must compare schema CIDs (`T::schema()` vs `DynamicBond.schema`) before converting to `Bond<T>`.

`Catalogue` is an oxide map wrapper:

- `items: map<string, DynamicBond>`

### Ligation

Ligation is the overlay that allows Polyepoxide to describe templates and cyclic data without giving up DAG-oriented storage. It is represented by two forms: `Ligase` (carrying an ordered list of erased bonds that establishes scope) and `Slot` (carrying an index into that scope).

There are two equally valid ways to understand ligation, and both are useful.

#### View 1: Open-DAG / Graph-Plugging View

In graph terms, ligation behaves like composition of open DAG fragments. A `Slot(i)` acts as an unbound output edge. A `Ligase(args)` provides an ordered set of open inputs. Resolution connects output and input edges by index, effectively "closing" the graph fragment. This view is useful when designing reusable templates, because it makes composition explicit and index-based.

#### View 2: Scope-and-Variables View

During traversal, passing through `Ligase(args)` establishes a reflexive scope. Encountering `Slot(i)` means: resolve as if traversal stepped into `scope[i]`. In this interpretation, `Slot` behaves like a variable lookup and `Ligase` behaves like a variable binder.

Index `0` has special meaning: it corresponds to the current ligase entry-point. This is why zero-based indexing is semantically meaningful in Polyepoxide ligation, not merely conventional. A direct consequence is that slot resolution is scope-dependent. The same `Slot(i)` can resolve to different content when reached along different paths.

For schema-generation profiles that support generic parameters, slot indexing is normative:

- `Slot(0)` denotes self-reference / ligase entry-point.
- Generic parameters map to `Slot(1)`, `Slot(2)`, ... in declaration order.

#### Main Uses

The first use is templating. For example, `Pair<T, U>` can be represented as an open schema using `Slot(1)` where `T` should appear and `Slot(2)` where `U` should appear. A concrete instance such as `Pair<String, Bool>` is then represented by wrapping that open graph in `Ligase([pair_template, string_schema, bool_schema])`.

The second use is self-reference and recursion. Because the entry-point is available at `Slot(0)`, recursive definitions can be expressed naturally. A list-like schema can be written as a ligase whose payload has two tagged variants, such as `Cons` (containing element plus `Slot(0)` as the tail) and `None` (containing `Unit`). This mirrors the sketch-style recursive example directly: recursion is just another slot lookup.

### Cursor

`Cursor` is a traversal helper that carries three pieces of state together: the current cell, a solvent reference, and the current ligation scope. In languages with generics it is typically typed (`Cursor<T>`). This avoids scattering resolution logic across call sites.

When asked to resolve a bond, cursor first attempts solvent-based CID resolution for unresolved links. If the bond is ligation-based, cursor resolves it against scope rules (`Ligase`/`Slot`) and continues traversal with the appropriate scope updates. The `follow` helper further streamlines typed traversal by selecting a bond from `T` and resolving it in one step.

### Store

Store is intentionally minimal on the content-addressed side: it maps multihash bytes to raw bytes. It does not own CID semantics and does not perform codec conversions. CID-to-multihash conversion happens outside store operations.

This boundary is important. Because store indexing is hash-based, different CIDs with the same multihash naturally map to the same stored payload. That property is used by reflexive/data dual-CID workflows where codec differs but content hash remains identical. Many store implementations can coexist behind this interface, including in-memory stores, filesystem stores, embedded databases, and remote/object stores.

Stores may additionally expose a mutable bookmark namespace that maps names to `DynamicBond` values. Bookmarks are not content-addressed themselves. They are convenience references layered on top of the multihash-indexed block store, and implementations may realize them either through a dedicated metadata mechanism or by storing a single content-addressed `Catalogue` root plus whatever local pointer is needed to find it.

#### Store Bookmarks

Bookmark storage is intentionally separate from block storage.

- Content-addressed blocks are keyed by multihash bytes.
- Bookmarks are keyed by application-chosen names.
- Updating a bookmark does not rewrite existing content-addressed blocks; it only changes the mutable name-to-reference mapping.

The logical bookmark value is `DynamicBond`, not just a bare CID. That allows a bookmark to carry both:

- the value reference (`bond`)
- the schema reference (`schema`)

This matters because many store-level operations need both roots together. A bookmark can therefore point directly at an application root without requiring callers to maintain a separate schema lookup table.

Implementations may choose different internal layouts:

- store serialized `DynamicBond` bytes directly in a metadata namespace
- store a single bookmark that points to a `Catalogue`, then keep many named entries inside that `Catalogue`
- use any equivalent mechanism that presents the same external behavior

The second pattern is useful when an application wants one mutable top-level bookmark such as `calendars`, while the actual per-object names live as entries inside the bookmarked `Catalogue`.

From the caller's perspective, bookmark operations are store-level metadata operations. They are not part of DAG traversal, do not participate in CID derivation, and are not discovered by `visit_bonds` unless the bookmarked object itself is persisted as normal oxide content.

## Operations and Algorithms

### Inserting Into Solvent (`dissolve_in`)

Insertion starts by dissolving every outbound bond of a value into the target solvent. Resolved links are internalized as solvent-owned cells; unresolved references remain unresolved. Ligation bonds are dissolved as ligation values, not eagerly dereferenced as normal data edges. After dissolution, CID is computed and deduplication either reuses an existing cell or inserts a new one.

Implementations should preserve a typed fast path when concrete types are known, and use erased/dynamic traversal where type information is unavailable. This mixed strategy keeps common operations efficient while supporting schema-driven generic behavior.

Cells may compute and cache CID lazily. Implementations may also provide a constructor for cells with precomputed CID when decoding from known-CID contexts.

```text
function add_to_solvent(value, solvent):
  dissolved = value.dissolve_in(solvent)
  cid = compute_cid(dissolved)
  if solvent.contains(cid):
    return solvent.get(cid)
  cell = Cell(cid, dissolved)
  solvent.insert(cell)
  return cell
```

### Resolving Bonds During Traversal

Cursor-based resolution unifies ordinary CID links and reflexive references. For unresolved CID links, cursor asks solvent to materialize the target cell. For ligation references, it resolves against current scope. If neither path can resolve, traversal returns an explicit unresolved or ligation error.

```text
function resolve_bond(cursor, bond):
  b = cursor.solvent.resolve_if_possible(bond)
  if b is Link(cell):
    return Cursor(cursor.solvent, cell, cursor.scope)
  if b is Unresolved(cid):
    fail UnresolvedBond(cid)
  if b is Ligation(l):
    (next_bond, next_scope) = resolve_ligation(l, cursor.scope)
    return resolve_bond(Cursor(cursor.solvent, cursor.cell, next_scope), next_bond)
```

### Persisting Graphs

Persistence operates on both value and schema graphs. A root cell is persisted together with its schema root, and the public result is a pair `(value_cid, schema_cid)`. Traversal is dependency-first so references are available before parent nodes are stored.

For ligation, codec distinction matters. The data payload for a ligation value is DAG-CBOR-addressable, while reflexive references use the `polyepoxide-reflexive` codec. They share multihash but differ in multicodec. Resolution of reflexive CIDs therefore converts codec when fetching payload bytes.

```text
function persist_root(root_cell, solvent, store):
  schema_cell = dissolve_schema(root_cell.type_schema, solvent)
  persist_transitively(schema_cell, solvent, store)
  persist_transitively(root_cell, solvent, store)
  return (root_cell.cid, schema_cell.cid)
```

### Synchronization

Synchronization entry points are CID pairs: one for value root and one for schema root. Even though store operations are hash-based, sync remains CID-based because traversal decisions require codec and schema semantics.

The algorithm walks value and schema together, resolves reflexive edges with scope, transfers dependencies before parents, and tracks visited state to avoid repeated work. A robust visited key includes `(value_cid, schema_cid, value_scope, schema_scope)`.

Current reference behavior is permissive for local shape mismatches encountered during schema-guided dependency discovery (such branches are skipped). Missing required blocks and root-level decode/format failures remain sync errors.

Because transfer is dependency-first (children before parent), implementations may safely skip recursion for a node when destination already has that node CID. This optimization relies on the invariant that stored parent presence implies dependency presence.

```text
function sync_pull(src_store, dst_store, value_cid, schema_cid):
  walk(value_cid, schema_cid, value_scope=[], schema_scope=[])

function walk(vcid, scid, value_scope, schema_scope):
  if already_visited(vcid, scid, value_scope, schema_scope):
    return
  if dst_store.has(multihash(vcid)):
    return
  schema = resolve_schema(scid, schema_scope)
  value_bytes = src_store.get(multihash(vcid)) or fail NotFound(vcid)
  deps = discover_dependencies(value_bytes, schema, value_scope, schema_scope)
  for dep in deps:
    walk(dep.vcid, dep.scid, dep.vscope, dep.sscope)
  dst_store.put(multihash(vcid), value_bytes)
```

## Implementation Details

This section is intentionally language-agnostic. The goal is to preserve model semantics across runtimes, not to prescribe Rust-specific syntax (for example, lifetimes, trait objects, or proc-macro mechanics).

Different languages use different naming and object-model conventions, but Polyepoxide implementations should keep these interfaces as close as possible in behavior and shape. Similar interfaces make specs, tests, and cross-language debugging much easier.

### Suggested Core Shapes (Pseudocode)

```text
type CID
type Hash = bytes
type Bytes = bytes
# Use Bond<?> and Cell<?> directly in languages with wildcard-style generic erasure.
# In languages without built-in wildcard erasure, define equivalent erased bond/cell wrapper types.

enum Ligation:
  Ligase(args: list<Bond<?>>)
  Slot(index: uint16)

enum Bond<T>:
  Unresolved(cid: CID)
  Link(cell: Cell<T>)
  Ligation(payload: Ligation)

class DynamicBond:
  schema: Bond<Structure>
  bond: Bond<?>

class Catalogue:
  items: map<string, DynamicBond>

interface BondVisitor:
  visit_cid(self, cid: CID)

class Cell<T>:
  cid_cache: optional<CID>
  value: T
  cid(self) -> CID           # may compute lazily and cache
  with_cid(value: T, cid: CID) -> Cell<T>
  value(self) -> T

interface Oxide:
  schema() -> Bond<Structure>    # type-level/static or companion/registry-based
  to_bytes(self) -> Bytes
  from_bytes(data: Bytes) -> Self  # type-level/static or codec/registry-based
  compute_cid(self) -> CID
  dissolve_in(self, solvent) -> Self
  visit_bonds(self, visitor: BondVisitor)  # visitor receives outbound target CIDs

interface Store:
  get(self, hash: Hash) -> Optional<Bytes>
  put(self, hash: Hash, bytes: Bytes)
  has(self, hash: Hash) -> bool
  get_bookmark(self, name: string) -> Optional<DynamicBond>
  put_bookmark(self, name: string, value: DynamicBond)

# Implementations may also expose lower-level bookmark byte operations
# internally, as long as the logical interface above is preserved.

interface AsyncStore:
  async_get(self, hash: Hash) -> Optional<Bytes>
  async_put(self, hash: Hash, bytes: Bytes)
  async_has(self, hash: Hash) -> bool

class Solvent:
  add(self, value) -> Cell<value_type>
  get(self, cid) -> Optional<Cell<?>>
  add_bond(self, bond<T>) -> Bond<T>
  add_erased_bond(self, bond: Bond<?>) -> Bond<?>  # optional in runtimes with native wildcard erasure
  resolve_bond(self, bond<T>) -> Bond<T>
  persist(self, cell, store) -> (value_cid: CID, schema_cid: CID)

class Cursor<T>:
  value(self) -> T
  resolve_bond(self, bond<U>) -> Result<Cursor<U>, CursorError>
  follow(self, select: T -> Bond<U>) -> Result<Cursor<U>, CursorError>
```

### Invariants

Implementations should preserve these invariants:

1. Cells are immutable once inserted into solvent.
2. Deduplication is by CID, not pointer identity.
3. Resolved solvent links point only to cells in the same solvent.
4. Raw bond serialization emits CID form; deserialization yields unresolved bonds.
5. Store identity is multihash-based, independent of CID codec.
6. `Slot` resolution depends on traversal scope and may vary by path.

### Immutability Enforcement

Solvent cells are deduplicated by CID and may be referenced from many places in the graph simultaneously. A mutation through any one reference would silently corrupt all other holders, potentially invalidating CID stability, breaking traversal, and causing subtle sync errors.

Implementations must therefore make mutation structurally difficult, not merely undocumented. The expected pattern is:

- **Isolation on insertion**: in languages with ownership semantics, the caller surrenders the value so no aliased mutable copy can remain outside the solvent. In languages without ownership, the solvent must deep-copy the value on insertion, ensuring that any reference the caller retains points to a separate copy it cannot use to affect the stored cell.
- **Read-only shared handles**: handles returned to callers (typed or erased) expose only immutable borrows of the contained value. There is no public mutable accessor on cells.
- **Encapsulation**: internal cell and solvent storage is hidden behind an interface that exposes only the operations above.

In Rust, `Arc<Cell<T>>` with a `value() -> &T` accessor and no `DerefMut` satisfies this. The `ErasedCell` trait takes only `&self`, so downcasting produces only shared references.

### Error Taxonomy

A practical error model should separate structural failures from runtime/backend failures.

- `NotFound`: referenced hash/CID is absent.
- `TypeMismatch`: content exists but cannot be interpreted as the requested type.
- `DecodeError` / `FormatError`: encoded bytes are invalid for expected decoding rules.
- `BookmarkDecodeError`: bookmark bytes exist but do not decode as `DynamicBond`.
- `UnresolvedBond`: traversal requires a link that remains unresolved.
- `LigationError`: invalid reflexive payload, slot out of bounds, malformed ligase scope (common subcases: empty ligase entry, slot out-of-range).
- `StoreError`: backend failure while reading/writing bytes.
- `SyncError`: synchronization-level failure (source/destination/traversal mismatch).

## Oxide Import and Export

In addition to raw DAG-CBOR persistence, Polyepoxide may expose higher-level oxide import/export formats for debugging, interchange, manual inspection, and partially self-describing workflows. These formats are graph-oriented and may preserve hydrated traversal results that do not appear in the raw binary representation.

Allowed surface syntaxes are:

- `YAML`
- `JSON-LD`
- `YAML-LD`

`JSON-LD` and `YAML-LD` use explicit node identifiers and references (`@id`). `YAML` may additionally use native anchors and aliases as a presentation convenience, but those anchors are not part of the semantic model and importers must not rely on any specific anchor naming scheme.

### Store-Level Import/Export

Oxide import/export operates at the store boundary. The caller provides:

- a root CID
- a root schema CID
- a value store
- a schema solvent

Export renders the root bond identified by `(root_cid, schema_cid)`. Import parses a document relative to `schema_cid`, writes any materialized value and ligation blocks into the caller-provided store, and returns the imported root CID.

The schema solvent is the typing context for traversal. Import/export must not depend on exported cursor-state metadata being embedded in the document.

Self-describing interchange is achieved by supplying a schema-carrying wrapper such as `DynamicBond` / `DynBond`, rather than by requiring every serialized document to carry schema metadata inline.

### Bond Envelope

At oxide import/export level, every bond position, including the root, may be represented either by an explicit envelope object or, in `direct` export mode, by its hydrated value directly.

The explicit envelope object matches the stored `Bond` variant shape, optionally augmented with a hydrated value:

- `$link`: a regular CID-bearing bond
- `$ligation`: a reflexive `Ligation` term
- `$schema`: schema CID for erased bond positions that do not otherwise carry type
- `$value`: an optional resolved occurrence reached by traversing that bond in the current cursor context

Faithful exports must preserve the stored term. `$value` is an additional hydrated view; it is not a replacement for `$link` or `$ligation` in round-trippable profiles.

If a bond position is encoded without any of `$link`, `$ligation`, or `$value`, import treats it as if it had been wrapped in `$value`. This allows the same mechanism to serve as a generic typed data import surface.

In particular:

- `$link` identifies the persisted bond target directly.
- `$ligation` mirrors the stored reflexive term (`Slot` or `Ligase`), not a post-resolution target.
- `$schema` is used for erased bond positions, such as `Ligase` arguments, so import can validate or hydrate them without out-of-band Rust type information.
- `$value` represents a resolved occurrence under the current cursor state. Two occurrences may therefore share the same underlying cell CID while remaining distinct export nodes because they arise under different ligation scopes.

Within a `Ligase`, each argument is serialized as an erased bond envelope. In faithful forms, that means at least:

- `$link` or `$ligation`
- `$schema`

`full` export may also attach `$value` to such erased bond arguments. `canonical` keeps them unhydrated.

Exception: argument `0` is the ligase entry point and may omit `$schema`, inheriting the enclosing schema context instead.

### Export Profiles

Three export profiles are supported. They apply equally to the root bond and to nested bonds:

1. `canonical`

   Each bond object must preserve its stored term:

   - regular bonds use `$link`
   - ligation bonds use `$ligation`

   For regular bonds, exporters should also include `$value` when the linked target is already available in the current solvent/store. For ligation bonds, `$value` is omitted in this profile.

   This profile preserves the original bond kind and remains suitable for faithful round-trip import.

2. `full`

   Each bond object preserves its stored term exactly as in `canonical`, and may additionally include `$value` for both regular and ligation bonds.

   This profile is intended for fully hydrated graph inspection. It remains round-trippable because the stored term is still present.

3. `direct`

   Each bond position is exported as its hydrated value directly, without a wrapper object. Shared hydrated occurrences are still deduplicated through `@id` / YAML aliases.

   On import, such values are treated as if they were wrapped in `$value`.

   This profile is convenient for presentation and editing of materialized data, but it is not faithful: it does not preserve whether the original stored bond was a regular link or a ligation term, and it cannot represent unresolved bonds exactly.

### Import Modes

Import uses validation modes rather than export profiles:

1. `lenient`

   Any bond position without `$link`, `$ligation`, or `$value` is treated as an implicit `$value`.

   This mode accepts mixed documents containing explicit envelopes and direct hydrated values.

2. `faithful`

   Every bond must carry `$link` or `$ligation`.

   `$value` may be present in addition to the stored term.

3. `canonical`

   Same as `faithful`, and additionally forbids `$value` on ligation bonds.

   This corresponds to the strictest round-trippable interpretation of the export format.

### References and Occurrences

Hydrated `$value` objects, or direct hydrated values in the `direct` profile, may be defined at first occurrence and referenced from later positions in the same export document. This is especially useful when a graph contains sharing, cycles, or multiple scoped occurrences of the same cell.

Reference identifiers are transport-level labels, not semantic identity. Importers must treat them as opaque and must support arbitrary valid document-local references. Exporters should use identifiers derived from cursor state when determinism matters, because the same cell CID may appear as multiple distinct hydrated occurrences under different ligation scopes.

For deterministic full exports:

- exporters should derive occurrence reference ids from cursor state
- the same cursor state should always map to the same exported reference id
- different cursor states should map to different exported reference ids, even when they point at the same underlying cell CID

For non-deterministic or convenience exports, arbitrary reference ids are acceptable.

### Format Mapping Rules

The same oxide import/export model may be rendered through different surface syntaxes:

- `YAML`: values may be defined inline, with aliases used for later references.
- `JSON-LD`: hydrated occurrences use `@id` for referenceability and may be nested locally at first occurrence.
- `YAML-LD`: same linked-data object model as JSON-LD, expressed with YAML syntax.

Local nesting is a framing choice, not a semantic requirement. Implementations must accept equivalent documents that flatten or reorder definitions, as long as the reference graph and typed content are preserved.

## Interoperation with IPFS Ecosystem

Polyepoxide is designed to integrate cleanly with CID/multihash/multicodec tooling from IPFS/IPLD ecosystems. Implementations should use mature libraries when available rather than reimplementing these primitives.

Data payloads are addressed with DAG-CBOR CIDs (`multicodec 0x71`). Reflexive overlay references use `polyepoxide-reflexive` (`multicodec 0x300001`, currently from the private multicodec range). A reflexive CID and a DAG-CBOR CID can share the same multihash while differing in codec. That distinction is meaningful: codec communicates interpretation, while multihash communicates byte identity.

At the store boundary, addressing is by multihash bytes only. This allows natural aliasing of equal-content blocks regardless of codec view. When a workflow encounters a reflexive CID and needs raw ligation bytes, it translates codec to DAG-CBOR while keeping the same multihash and performs the store lookup by hash.

### Normative Addressing and Encoding Rules (Current Reference Ruleset)

The following rules describe the current reference ruleset used by the Rust implementation and should be treated as normative for compatibility-focused ports:

1. DAG-CBOR content CIDs are computed as CIDv1 with:
   - multicodec `0x71` (DAG-CBOR),
   - multihash `blake3-256` over the serialized DAG-CBOR bytes.
2. Reflexive CIDs use multicodec `0x300001` (`polyepoxide-reflexive`).
3. Identity multihash code is `0x00`.
4. `Ligation::Slot(i)` uses an identity multihash CID whose digest is the serialized ligation payload bytes.
5. `Ligation::Ligase(args)` uses a hashed CID (blake3-256 over serialized ligation payload bytes).
6. Content-addressed store keys are multihash bytes only (`key = cid.multihash_bytes`), independent of codec.
7. Identity multihash keys are virtual at the store overlay boundary:
   - `get(identity_key)` returns digest bytes,
   - `has(identity_key)` returns `true`,
   - `put(identity_key, ...)` is a no-op.
8. When resolving non-identity reflexive CIDs to payload bytes, convert to DAG-CBOR codec while preserving multihash, then read by multihash key.
9. `Char` values are encoded as CBOR text containing exactly one Unicode scalar value.
10. Current optional/result encoding rules:
   - option-like values encode as sequence/cardinality-0-or-1: `[]` for none, `[x]` for some.
11. Bookmark names live in a separate mutable namespace and must not collide with content-addressed multihash keys.
12. The logical value of a bookmark is a schema-carrying `DynamicBond`; implementations may store equivalent lower-level bytes internally.
   - result-like values encode as a single-entry map with lowercase tag key: `{"ok": x}` or `{"err": e}`.

## Conformance and Tests

Because the project is pre-alpha, conformance is intentionally minimal and focused on stability anchors. The current baseline includes one golden vector computed by the reference Rust implementation:

- `Structure::schema()` CID: `bagaybqabdyqnc2pq4a3b4l4ybdl76fgeyaeyc5joew6sasegirhl2tssnq3kyci`

This gives a concrete interoperability checkpoint while the format evolves.

Additional vectors should be added as behavior stabilizes, especially for:

- bond roundtrips (`serialize -> CID only`, `deserialize -> unresolved`);
- ligation scope resolution (`Slot(0)`, slot out-of-range, empty ligase);
- hash-based store aliasing and identity-overlay behavior;
- zipped sync traversal across value/schema graphs, including reflexive edges and ordered-map key/value traversal;
- language-specific optional/result encoding rules.
