# POLYEPOXIDE-EXPORT

## Dependencies

- `11-POLYEPOXIDE-MODEL`
- `12-POLYEPOXIDE-SOLVENT`
- `12-POLYEPOXIDE-STORE`

## Intent

Define schema-guided import and export between stored Polyepoxide graphs and editable document formats such as YAML, JSON-LD, and YAML-LD.

Raw DAG-CBOR is the canonical content-addressed byte representation, but it is not convenient for inspection, hand-written fixtures, editing, or interchange with document-oriented tools. Export presents stored graph data as typed documents by walking value bytes together with stored schema data. Import performs the inverse operation: it parses a typed document, materializes content-addressed blocks into a store, and returns the imported root CID.

This spec defines the portable document semantics and the common store-backed traversal interface. Language-specific specs should describe concrete API names, renderer libraries, and host-language error types.

## Conceptual Model

Import/export operates at the store boundary. It does not require application-specific oxide types to be compiled into the process. Instead, it uses:

- a value store containing persisted oxide bytes
- a schema solvent containing `Structure` cells
- a root value CID
- a root schema CID
- a document format
- import or export options

The schema solvent is the cache used to resolve and traverse schema nodes while decoding stored value bytes. Before export, implementations must load the schema graph reachable from the root schema CID into this solvent.

Store-backed traversal is cursor-based. A cursor position is not just a value CID: it also includes the schema CID and both value and schema ligation scopes. This matters because a reflexive `Slot` can resolve differently under different scopes, so the same cell CID can produce distinct document occurrences.

## Formats

Supported surface formats are:

| Format | Occurrence identity | Reference form | Notes |
| --- | --- | --- | --- |
| `YAML` | `id` field and optional YAML anchor | YAML alias or `{ id: ... }` reference object | Human-oriented fixture syntax. |
| `JSON-LD` | `@id` field | `{ "@id": ... }` reference object | Top-level output includes `@context` with `@vocab: "urn:px:"`. |
| `YAML-LD` | `@id` field | `{ "@id": ... }` reference object in YAML syntax | Same linked-data object model as JSON-LD. |

Format syntax may differ, but the typed graph semantics are the same. Importers should treat occurrence IDs and YAML anchors as document-local labels. They are not CIDs and they are not semantic graph identity.

## Export Profiles

Export supports three profiles:

- `canonical`: preserves bond kind with `$link` or `$ligation`. Regular links may include `$value` when materializable. Ligation bonds do not include `$value`.
- `full`: preserves bond kind with `$link` or `$ligation` and may include hydrated `$value` for both regular and ligation bonds.
- `direct`: emits hydrated values directly at bond positions instead of bond envelopes.

`canonical` is the strictest round-trippable fixture profile. `full` is best for graph inspection because it exposes both stored bond identity and hydrated values. `direct` is convenient for editing, but it cannot preserve whether the source bond was a regular link or a ligation term and cannot represent unresolved bonds.

## Import Modes

Import supports three modes:

- `lenient`: accepts direct values at bond positions and treats them as implicit `$value` envelopes.
- `faithful`: requires every bond position to carry `$link` or `$ligation`; `$value` may also be present.
- `canonical`: same as `faithful`, and additionally forbids `$value` on ligation bonds.

If an envelope contains both `$link` and `$ligation`, import must reject it. If an envelope contains both a stored bond identity and `$value`, import must materialize the hydrated value and verify that its computed CID matches the explicit `$link` or `$ligation` result when that comparison is meaningful.

## Bond Envelope

At any `Bond(T)` position, a document may use an explicit envelope object:

```text
type BondEnvelope:
  $link?: CID
  $ligation?: LigationDocument
  $schema?: CID
  $value?: OccurrenceOrValue
```

`$link` is a normal persisted target CID. `$ligation` is a reflexive `Ligation` term, rendered as either:

```text
{ "Slot": index }
{ "Ligase": [erased_bond_envelope, ...] }
```

`$schema` gives the schema CID for erased or dynamic bond positions. It is required when a hydrated erased bond has no inherited schema context. Inside a `Ligase`, argument `0` may inherit the enclosing schema context; arguments at index `1` and later need schema information to hydrate or validate their values.

`$value` is a hydrated view reached through the current traversal cursor. In faithful formats it is not a replacement for `$link` or `$ligation`; it is additional inspectable content.

## Occurrences

An occurrence is a hydrated value reached at a specific store cursor state. Occurrence IDs should be deterministic for a cursor state and should include enough state to distinguish:

- value CID
- schema CID
- value ligation scope
- schema ligation scope

Repeated occurrences may be emitted as references rather than duplicating the same hydrated subtree. In plain YAML, aliases may be used for this. In JSON-LD and YAML-LD, references use occurrence ID objects.

Importers must resolve occurrence labels locally within the document. A cyclic hydrated occurrence cannot be materialized from direct `$value` structure alone; cycles must be represented explicitly through ligation.

## Store-Backed Traversal

Import/export uses a store-backed cursor equivalent to:

```text
type CursorState:
  value_cid: CID
  schema_cid: CID
  scope: list<CID>
  schema_scope: list<CID>

interface StoreCursor:
  new(store: Store, schemas: Solvent, value_cid: CID, schema_cid: CID) -> Result<StoreCursor>
  from_state(store: Store, schemas: Solvent, state: CursorState) -> StoreCursor

  state() -> CursorState
  occurrence_id() -> string

  value_cid() -> CID
  schema_cid() -> CID
  scope() -> list<CID>
  schema_scope() -> list<CID>

  value_data() -> IpldValue
  schema() -> Cell<Structure>
  child_schema_cursor(schema_cid: CID) -> Result<StoreCursor>
  follow_bond(target_cid: CID, inner_schema_cid: CID) -> Result<StoreCursor>
  ligation_term(cid: CID) -> optional<Ligation>
```

`StoreCursor` traverses stored bytes without requiring concrete host-language oxide types. `value_data` reads the current value bytes from the store and decodes them into a generic IPLD-like value. `schema` reads the current schema from the schema solvent. `follow_bond` resolves ordinary and reflexive value edges while also resolving the child schema edge, updating the two scopes independently.

The exact generic value representation is language-specific, but it must preserve the DAG-CBOR shapes described by `11-POLYEPOXIDE-MODEL`.

## Schema Loading

Implementations must provide a helper equivalent to:

```text
function load_schema_recursive(
  store: Store,
  schemas: Solvent,
  schema_cid: CID,
) -> Result<Cell<Structure>>
```

`load_schema_recursive` loads the schema graph rooted at `schema_cid` into the schema solvent. It must follow `Structure` child bonds, resolve reflexive schema edges with schema ligation scope, and avoid infinite recursion by tracking visited `(schema_cid, schema_scope)` states.

Identity reflexive schema terms may be handled virtually according to the store and model rules. Non-identity reflexive schema terms must be resolved by reading their DAG-CBOR payload through the corresponding data CID.

## Standard Interfaces

Language implementations should expose interfaces equivalent to the following pseudocode. Names may follow host-language conventions, but behavior should match.

```text
enum ExportFormat:
  YAML
  JSON_LD
  YAML_LD

enum ExportProfile:
  canonical
  full
  direct

type ExportOptions:
  profile: ExportProfile
  pretty: bool
  unwrap_top_level_occurrence: bool
  exclude_top_level_fields: list<string>

function export(
  store: Store,
  schemas: Solvent,
  root_cid: CID,
  schema_cid: CID,
  format: ExportFormat,
  options: ExportOptions,
) -> Result<string>
```

`schemas` must contain the schema graph needed for traversal. Callers can satisfy this by calling `load_schema_recursive` before export.

```text
enum ImportFormat:
  YAML
  JSON_LD
  YAML_LD

enum ImportMode:
  lenient
  faithful
  canonical

type ImportOptions:
  mode: ImportMode

function import(
  document: string,
  format: ImportFormat,
  schema_cid: CID,
  store: Store,
  schemas: Solvent,
  options: ImportOptions,
) -> Result<CID>
```

Import parses the document relative to `schema_cid`, writes materialized value and non-identity ligation blocks into `store`, and returns the imported root value CID.

## Typed Value Mapping

Document values are interpreted through `Structure`:

| `Structure` | Document representation |
| --- | --- |
| `Bool` | boolean scalar |
| `Char` | string containing one Unicode scalar value |
| `Unicode` | string |
| `ByteString` | base64 string |
| `Cid` | CID string |
| `Int` | integer scalar |
| `Float` | numeric scalar |
| `Unit` | null |
| `Option(T)` in record field | omitted when absent; direct inner representation when present |
| `Option(T)` outside record fields | array with zero or one element |
| `Sequence(T)` | array |
| `Tuple([...])` | array |
| `Record(fields)` | object using schema field names and schema field order for rendering |
| `Tagged(variants)` | single-key object for payload variants; string shorthand for unit variants |
| `Enum(variants)` | variant name string |
| `Map { key, value }` | document object when keys can be represented as document object keys |
| `OrderedMap { key, value }` | array of two-element `[key, value]` arrays |
| `Bond(T)` | bond envelope, or direct hydrated value in direct/lenient forms |

For `Map`, implementations may restrict document-object rendering to key forms that round-trip through the target document format. `OrderedMap` is the portable representation when order matters or when keys are not naturally document-object keys.

## Error Semantics

The exact error type is language-specific, but import/export behavior should distinguish at least:

- missing required value or schema block
- invalid document syntax
- typed value/schema mismatch
- invalid bond envelope
- duplicate or missing occurrence reference
- cyclic hydrated value without explicit ligation
- CID mismatch between explicit bond identity and hydrated value
- invalid ligation payload, empty ligase, or slot out of range
- store read/write failure
- render failure

## Out of Scope

- Concrete renderer and parser libraries.
- CLI file handling and presentation.
- Store synchronization, which belongs to `12-POLYEPOXIDE-STORE`.
- Host-language API details, which belong to language-specific export specs.
