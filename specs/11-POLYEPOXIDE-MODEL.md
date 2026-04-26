# POLYEPOXIDE-MODEL

## Dependencies

- none

## Intent

Define the portable Polyepoxide data model: content-addressed oxide nodes, schema nodes, CIDs, immutable cells, bonds, erased/dynamic bonds, catalogues, and ligation. This is the common vocabulary every language implementation must share before it can implement solvents, stores, import/export, or synchronization.

Polyepoxide is designed for local-first applications that need portable, inspectable, content-addressed state. The core choice is to keep normal runtime objects easy to build while making the transition into immutable graph state explicit. Once committed, values are identified by content rather than by storage location or mutable database row identity.

## Conceptual Model

Polyepoxide has two first-class graphs:

- The value graph contains application data.
- The schema graph contains `Structure` nodes that explain how value bytes should be interpreted.

The two graphs are traversed together. This matters because a client may know only generic Polyepoxide rules, not the concrete host-language type that originally produced a value. Schema-guided traversal lets such a client discover bonds, export documents, and sync dependencies without linking application code.

## CID and Encoding Model

Persisted data nodes are encoded as deterministic IPLD-compatible bytes, currently DAG-CBOR. A content CID is derived from the serialized bytes, not from object identity, insertion order, or store location.

CID codec and multihash have distinct roles:

- The codec tells a reader how to interpret a payload.
- The multihash identifies the payload bytes.

Two CIDs may share the same multihash while using different codecs. Polyepoxide uses this for content/reflexive dual addressing: the same ligation payload may be viewed as normal DAG-CBOR data or as a reflexive reference by changing only the codec.

Bond serialization stores only CID identity. Deserializing a bond yields an unresolved reference because serialized bytes cannot contain in-memory pointers.

## Content CIDs and Reflexive CIDs

A content CID addresses ordinary oxide bytes. It uses the DAG-CBOR codec and a cryptographic multihash of the serialized oxide payload.

A reflexive CID addresses a ligation term. It uses the Polyepoxide reflexive codec and resolves relative to traversal scope. Reflexive CIDs are still CIDs, but their codec marks the reference as a scope-sensitive term rather than ordinary value data.

There are two reflexive CID forms:

- `Slot(i)` uses an identity multihash. The serialized slot term is embedded directly in the multihash digest.
- `Ligase(args)` uses a Blake3-256 multihash of its serialized DAG-CBOR payload, with the codec changed to the Polyepoxide reflexive codec.

A non-identity reflexive CID can be converted to the corresponding data CID by replacing the reflexive codec with DAG-CBOR while preserving the multihash. Store-level components can then fetch the ligation payload by multihash key.

## Structure

`Structure` is the portable schema language. It is itself an oxide and must be content-addressable. Supported forms are:

| Structure | Meaning | Portable encoding shape | WIT-style mapping |
| --- | --- | --- | --- |
| `Bool` | boolean | CBOR bool | `bool` |
| `Char` | one Unicode scalar | CBOR text containing one scalar | `char` |
| `Unicode` | UTF-8 text | CBOR text | `string` |
| `ByteString` | bytes | CBOR bytes | `list<u8>` or `bytes` |
| `Cid` | CID value | IPLD link | opaque `cid` handle or string representation |
| `Int(width)` | `U8`, `U16`, `U32`, `U64`, `I8`, `I16`, `I32`, or `I64` | CBOR integer | `u8`, `u16`, `u32`, `u64`, `s8`, `s16`, `s32`, `s64` |
| `Float(width)` | `F32` or `F64` | CBOR float | `f32` or `f64` |
| `Unit` | unit value | CBOR null/unit representation | `unit` |
| `Option(T)` | optional value | non-record: list length 0 or 1; record field: omitted/direct inner value | `option<T>` |
| `Sequence(T)` | homogeneous sequence | CBOR list | `list<T>` |
| `Tuple([T...])` | fixed heterogeneous sequence | CBOR list | `tuple<T...>` |
| `Record(fields)` | ordered named fields | CBOR map or field-ordered record encoding | `record { ... }` |
| `Tagged(variants)` | variant with payload | single-key tagged map/form | `variant { case(T), ... }` or `result<T, E>` |
| `Enum(variants)` | payload-free variant | symbolic/index variant | `enum { ... }` |
| `Map { key, value }` | unordered map | CBOR map | `map<K, V>` |
| `OrderedMap { key, value }` | order-preserving map | list of `[key, value]` pairs | `list<tuple<K, V>>` |
| `Bond(T)` | graph reference | CID link | opaque reference/resource to `T` |

Record and tagged field order is part of schema identity. Implementations must preserve it when computing schema CIDs and when converting to ordered document formats.

`Map` is for unordered maps. `OrderedMap` is the portable representation when key order matters or when keys are not naturally supported by document-object syntax.

`Option<T>` maps to `Structure::Option`. Non-record positions use a list of length 0 or 1. Named record fields use omitted/direct inner-value encoding so record documents can remain compact while preserving the same schema.

`Result<T, E>` is not a separate `Structure` variant. It is represented as `Tagged` with `ok` and `err` variants.

`ByteString` is distinct from `Sequence(Int(U8))` so implementations can preserve byte-oriented encoding and avoid ambiguity with generic sequences.

## Bond and Cell Semantics

`Bond<T>` has three logical states:

- `Unresolved(CID)`: only the identity is known.
- `Link(Cell<T>)`: the target is materialized in memory.
- `Ligation(Ligation)`: the target is a reflexive term that resolves relative to scope.

Bonds serialize as CID values. Deserializing a bond produces `Unresolved(CID)`. Resolved state is reconstructed later by a solvent, cursor, or equivalent graph runtime.

`Cell<T>` is immutable. A language implementation may expose constructors for values with known CIDs, but it must not expose mutation through shared solvent-managed handles.

## Ligation

Ligation is the reflexive mechanism used for templates, generic schema parameters, and cyclic data while preserving a DAG-compatible persistence layer.

A `Ligase(args)` establishes a scope and resolves first to its entry point, `args[0]`. A `Slot(i)` resolves by looking up index `i` in the current scope.

The exact slot rules are:

- `Slot(0)` denotes the ligase entry point, commonly used for self-reference and recursion.
- `Slot(1)`, `Slot(2)`, and later slots denote generic or open parameters in declaration order.
- Entering a `Ligase(args)` returns `args[0]` as the next target and replaces the current scope with `args`.
- Entering a `Slot(i)` returns `scope[i]` as the next target and keeps the existing scope.
- Resolving `Slot(i)` is scope-dependent. The same stored slot can resolve to different CIDs depending on the path used to reach it.
- A slot whose index is outside the current scope is an invalid ligation reference for that traversal.
- A ligase without an entry point cannot be traversed as a value root.

There are two equivalent mental models:

- In the open-DAG model, `Slot(i)` is an open edge and `Ligase(args)` plugs concrete bonds into open positions.
- In the scope model, `Ligase(args)` is a variable binder and `Slot(i)` is a variable lookup.

The scope model is the one traversal algorithms normally implement.

## Standard Interfaces

Language implementations should expose interfaces equivalent to the following pseudocode. Names may follow host-language conventions, but behavior and graph semantics must match.

```text
interface Oxide<T>:
  static schema() -> Bond<Structure>
  static schema_template() -> Bond<Structure>

  to_bytes(value: T) -> bytes
  from_bytes(data: bytes) -> Result<T, PolyepoxideError>
  compute_cid(value: T) -> CID

  visit_bonds(value: T, visitor: BondVisitor) -> void
  dissolve_in(value: T, solvent: Solvent) -> T
```

`schema()` returns the closed schema for a concrete type. `schema_template()` returns the open form before generic parameters are substituted; non-generic implementations may return `schema()`.

`visit_bonds` is an outbound identity-discovery operation. The visitor receives target CIDs, not full bond objects. This keeps persistence, sync, and generic traversal independent from current in-memory resolution state.

`dissolve_in` converts nested bonds into the representation appropriate for a target solvent. Resolved child values become solvent-owned cells where possible; unresolved CIDs remain unresolved; ligation stays ligation.

`dissolve_in` is a temporary circular interface with the solvent component. It is included here because oxide implementations need a common hook for solvent insertion, but the actual solvent API belongs to `12-POLYEPOXIDE-SOLVENT`. A later split should remove the circular dependency by moving this operation behind a solvent-side visitor/resolver interface or an equivalent adapter.

```text
interface BondVisitor:
  visit_bond(cid: CID) -> void
```

```text
enum Structure:
  Bool
  Char
  Unicode
  ByteString
  Cid
  Int(width: IntType)
  Float(width: FloatType)
  Unit
  Option(inner: Bond<Structure>)
  Sequence(inner: Bond<Structure>)
  Tuple(elements: list<Bond<Structure>>)
  Record(fields: ordered_map<string, Bond<Structure>>)
  Tagged(variants: ordered_map<string, Bond<Structure>>)
  Enum(variants: list<string>)
  Map(key: Bond<Structure>, value: Bond<Structure>)
  OrderedMap(key: Bond<Structure>, value: Bond<Structure>)
  Bond(target: Bond<Structure>)

enum IntType:
  U8
  U16
  U32
  U64
  I8
  I16
  I32
  I64

enum FloatType:
  F32
  F64
```

`Structure` is itself an oxide. Recursive and generic schema references are represented with `Bond<Structure>` and ligation rather than with special schema-only syntax.

```text
enum Bond<T>:
  Unresolved(cid: CID)
  Link(cell: Cell<T>)
  Ligation(term: Ligation)

interface Bond<T>:
  cid(self) -> CID
  is_resolved(self) -> bool
  cell(self) -> optional<Cell<T>>
  value(self) -> optional<T>
  erase(self) -> Bond<?>
```

`cid()` returns the identity of the target regardless of bond state: the stored CID for unresolved bonds, the cell CID for links, and the ligation CID for reflexive terms.

`cell()` and `value()` return a result only for `Link`. `Ligation` is resolved by traversal scope, not by direct cell access.

`erase()` is required only in languages that cannot use wildcard generic bonds directly. Erased bonds and erased cells have the same logical states and operations as their typed counterparts, but without a concrete type parameter.

```text
interface Cell<T>:
  new(value: T) -> Cell<T>
  with_cid(value: T, cid: CID) -> Cell<T>
  value(self) -> T
  cid(self) -> CID
```

`Cell<T>` is an immutable wrapper containing a value and its CID. Cells may cache CIDs lazily, but once a cell is exposed as solvent-managed content, mutation must not be possible.

```text
type DynamicBond:
  schema: Bond<Structure>
  bond: Bond<?>
```

`DynamicBond` is the schema-carrying erased reference. Typed resolution of a dynamic bond must compare the expected schema CID with `schema.cid()` before converting to a typed bond.

```text
type Catalogue:
  items: map<string, DynamicBond>
```

`Catalogue` is the standard named collection. It is useful as an application root when a single mutable bookmark should point to many named content-addressed entries.

```text
enum Ligation:
  Ligase(args: list<Bond<?>>)
  Slot(index: uint16)

interface Ligation:
  cid(self) -> CID
  visit_bonds(self, visitor: BondVisitor) -> void
  dissolve_in(self, solvent: Solvent) -> Ligation

function slot_cid(index: uint16) -> CID
function ligase_cid(args: list<Bond<?>>) -> CID
function ligation_cid(term: Ligation) -> CID
function resolve_ligation(term: Ligation, scope: list<Bond<?>>) -> Result<(Bond<?>, list<Bond<?>>), LigationError>
function with_codec(cid: CID, codec: uint64) -> CID
function data_to_reflexive_cid(cid: CID) -> CID
function reflexive_to_data_cid(cid: CID) -> CID
```

Erased bonds and erased cells are required in languages that cannot express wildcard generic bonds directly. They must preserve the same CID and traversal behavior as typed bonds.

`resolve_ligation` implements the scope rules described above: `Ligase(args)` returns `args[0]` with `args` as the new scope, and `Slot(i)` returns `scope[i]` while preserving the current scope.

## Error Typology

Implementations may expose host-language-specific error types, but they should preserve these semantic categories:

```text
enum PolyepoxideError:
  NotFound(cid: CID)
  TypeMismatch(expected: SchemaRef, found: SchemaRef?)
  DecodeError(message: string)
  FormatError(message: string)
  BookmarkDecodeError(name: string, source: DecodeError)
  UnresolvedBond(cid: CID)
  LigationError(reason: LigationError)
  StoreError(source: BackendError)
  SyncError(source: SyncFailure)

enum LigationError:
  InvalidPayload(cid: CID?)
  EmptyLigase
  SlotOutOfRange(index: uint16)
  MissingScope
```

`NotFound` means a required CID is absent from the available graph or store.

`TypeMismatch` means bytes exist but cannot be interpreted as the expected oxide or schema.

`DecodeError` means bytes are invalid for the selected codec. `FormatError` means decoded data is structurally valid but violates the expected Polyepoxide shape.

`UnresolvedBond` means traversal or materialization required a resolved target but only CID identity was available.

`LigationError` covers invalid reflexive payloads, empty ligases, slots outside the current scope, and traversal that encounters a slot without an active scope.

`StoreError` and `SyncError` are boundary categories. Detailed backend and synchronization failures belong to the store and sync specs.

## Normative Addressing Rules

Implementations must use these constants and addressing rules unless a later version of this spec explicitly changes them:

```text
const DAG_CBOR_CODEC = 0x71
const POLYEPOXIDE_REFLEXIVE_CODEC = 0x300001
const MULTIHASH_IDENTITY = 0x00
const CONTENT_HASH = blake3-256
const CID_VERSION = 1
```

Rules:

1. Content CIDs are CIDv1 values with codec `DAG_CBOR_CODEC` and Blake3-256 multihash over deterministic DAG-CBOR bytes.
2. Reflexive CIDs are CIDv1 values with codec `POLYEPOXIDE_REFLEXIVE_CODEC`.
3. `Slot(i)` CIDs use identity multihash and embed the serialized `Slot(i)` ligation bytes directly in the digest.
4. `Ligase(args)` CIDs use Blake3-256 over the serialized `Ligase(args)` ligation bytes.
5. Codec conversion preserves the multihash and changes only the CID codec.
6. Store keys are multihash bytes, not full CID bytes. This makes data/reflexive codec views address the same payload.
7. Identity-multihash keys are virtual: reading returns the digest bytes, existence checks succeed, and writes are no-ops.
8. To resolve a non-identity reflexive CID to payload bytes, swap the codec to `DAG_CBOR_CODEC` and read by multihash key.
9. `Char` is encoded as CBOR text containing exactly one Unicode scalar value.
10. Bond serialization emits the bond CID. Bond deserialization yields `Unresolved(CID)`.

## Key Design Choices

- Schemas are stored as graph data so clients can traverse unknown values.
- CIDs are derived from bytes, so graph identity is portable across runtimes.
- Runtime resolution state is intentionally separate from serialized identity.
- Content CIDs and reflexive CIDs share the same CID machinery while keeping ordinary data and scope-dependent references distinguishable by codec.
- Ligation is used instead of ad-hoc recursive schema syntax so recursion and generics use the same mechanism across languages.
- The model allows language-specific ergonomics, but the serialized graph and schema behavior must remain portable.

## Out of Scope

- In-memory graph management belongs to `12-POLYEPOXIDE-SOLVENT`.
- Store APIs and synchronization belong to `13-POLYEPOXIDE-STORE`.
- Document import/export belongs to `14-POLYEPOXIDE-EXPORT`.
