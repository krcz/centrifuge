# POLYEPOXIDE-RS-EXPORT

## Dependencies

- `13-POLYEPOXIDE-EXPORT`
- `31-POLYEPOXIDE-RS-MODEL`
- `32-POLYEPOXIDE-RS-SOLVENT`
- `32-POLYEPOXIDE-RS-STORE`

## Intent

Describe how the language-agnostic import/export contract from `13-POLYEPOXIDE-EXPORT` is implemented in Rust by `polyepoxide-core`.

This spec focuses on the Rust API surface, the Rust-owned store-backed traversal types, and implementation choices that affect compatibility. It does not restate the portable document semantics except where the Rust representation adds important detail.

## Modules

- `polyepoxide-core/src/export.rs`: `export`, `ExportFormat`, `ExportProfile`, `ExportOptions`, `ExportError`, document rendering.
- `polyepoxide-core/src/import.rs`: `import`, `ImportFormat`, `ImportMode`, `ImportOptions`, `ImportError`, document parsing and materialization.
- `polyepoxide-core/src/store_cursor.rs`: `StoreCursor`, `CursorState`, `load_schema_recursive`, store-backed schema and reflexive resolution.
- `polyepoxide-core/src/traverse.rs`: schema-guided IPLD traversal helpers used by store/sync code.

The public items are re-exported from `polyepoxide-core/src/lib.rs`.

## Export API

Rust exposes export as a generic function over the synchronous `Store` trait:

```rust
pub fn export<S: Store + ?Sized>(
    store: &S,
    schemas: &Solvent,
    root_cid: Cid,
    schema_cid: Cid,
    format: ExportFormat,
    options: &ExportOptions,
) -> Result<String, ExportError<S::Error>>;
```

`store` contains persisted value and ligation blocks. `schemas` is the schema solvent used by store-backed traversal. Callers should populate it with `load_schema_recursive(store, schemas, schema_cid)` before calling `export`.

Rust represents export formats and profiles as enums:

```rust
pub enum ExportFormat {
    Yaml,
    JsonLd,
    YamlLd,
}

pub enum ExportProfile {
    Canonical,
    Full,
    Direct,
}
```

Export options are:

```rust
pub struct ExportOptions {
    pub profile: ExportProfile,
    pub pretty: bool,
    pub unwrap_top_level_occurrence: bool,
    pub exclude_top_level_fields: Vec<String>,
}
```

The default options are `profile = ExportProfile::Full`, `pretty = true`, `unwrap_top_level_occurrence = false`, and no top-level field exclusions.

`unwrap_top_level_occurrence` applies only to direct export. It strips the top-level occurrence wrapper and returns the occurrence data directly. `exclude_top_level_fields` removes named fields from the exported top-level object or top-level occurrence data; it is a presentation option and does not alter stored content.

## Import API

Rust exposes import as:

```rust
pub fn import<S: Store + ?Sized>(
    input: &str,
    format: ImportFormat,
    schema_cid: Cid,
    store: &S,
    schemas: &Solvent,
    options: &ImportOptions,
) -> Result<Cid, ImportError>;
```

The import error type is not generic. Store backend failures are converted to strings inside `ImportError::Store`.

Rust represents import formats and modes as:

```rust
pub enum ImportFormat {
    Yaml,
    JsonLd,
    YamlLd,
}

pub enum ImportMode {
    Lenient,
    Faithful,
    Canonical,
}

pub struct ImportOptions {
    pub mode: ImportMode,
}
```

The default import mode is `ImportMode::Lenient`.

Import parses `input` relative to `schema_cid`, materializes DAG-CBOR blocks into `store`, stores non-identity ligation payloads as data-CID blocks, and returns the imported root value CID.

## Store Cursor

Rust represents the portable store-backed traversal state as an oxide:

```rust
pub struct CursorState {
    pub value_cid: Cid,
    pub schema_cid: Cid,
    pub scope: Vec<Cid>,
    pub schema_scope: Vec<Cid>,
}
```

`CursorState` implements `Oxide`. `StoreCursor::occurrence_id` returns `urn:px-occ:{cid}`, where `{cid}` is the CID of this `CursorState`. This makes occurrence IDs sensitive to value identity, schema identity, and both ligation scopes.

`StoreCursor` borrows a store and a schema solvent:

```rust
pub struct StoreCursor<'a, S: Store + ?Sized> { ... }

impl<'a, S: Store + ?Sized> StoreCursor<'a, S> {
    pub fn new(
        store: &'a S,
        schemas: &'a Solvent,
        value_cid: Cid,
        schema_cid: Cid,
    ) -> Result<Self, ExportError<S::Error>>;

    pub fn from_state(
        store: &'a S,
        schemas: &'a Solvent,
        state: CursorState,
    ) -> Self;

    pub fn value_cid(&self) -> Cid;
    pub fn schema_cid(&self) -> Cid;
    pub fn scope(&self) -> &[Cid];
    pub fn schema_scope(&self) -> &[Cid];
    pub fn occurrence_id(&self) -> String;
    pub fn state(&self) -> CursorState;

    pub fn schema(&self) -> Result<Arc<Cell<Structure>>, ExportError<S::Error>>;
    pub fn child_schema_cursor(&self, cid: Cid) -> Result<Self, ExportError<S::Error>>;
    pub fn ipld(&self) -> Result<Ipld, ExportError<S::Error>>;
    pub fn ligation_term(&self, cid: Cid) -> Result<Option<Ligation>, ExportError<S::Error>>;
    pub fn follow_bond(
        &self,
        target_cid: Cid,
        inner_schema_cid: Cid,
    ) -> Result<Self, ExportError<S::Error>>;
}
```

`StoreCursor` traverses stored bytes without decoding them into application Rust types. `ipld` reads the current value block through `identity_overlay` and decodes DAG-CBOR into `ipld_core::ipld::Ipld`. `follow_bond` resolves ordinary and reflexive value edges, resolves the corresponding schema edge, and updates value and schema scopes independently.

The cursor uses `Vec<Cid>` for value and schema scopes because store-backed traversal operates at the persisted CID/byte level rather than on in-memory `ErasedBond` values.

## Schema Loading

Rust implements schema preloading as:

```rust
pub fn load_schema_recursive<S: Store + ?Sized>(
    store: &S,
    schemas: &Solvent,
    cid: Cid,
) -> Result<Arc<Cell<Structure>>, ExportError<S::Error>>;
```

This function loads the schema graph rooted at `cid` into `schemas`. It follows schema bonds for `Option`, `Sequence`, `Bond`, `Tuple`, `Record`, `Tagged`, `Map`, and `OrderedMap`. It tracks visited `(Cid, Vec<Cid>)` schema states to avoid infinite recursion through recursive schemas.

Identity reflexive schema CIDs are handled virtually. Non-identity reflexive schema CIDs are resolved by converting to the data CID and reading the ligation payload from the store.

## Export Implementation

Export builds an internal `DocNode` tree before rendering:

```rust
enum DocNode {
    Null,
    Bool(bool),
    Integer(i128),
    Float(f64),
    String(String),
    Array(Vec<DocNode>),
    Object(IndexMap<String, DocNode>),
    Occurrence { id: String, data: Box<DocNode> },
    Ref(String),
}
```

`GraphBuilder` owns the export profile and a `seen` map keyed by occurrence ID. The main export steps are:

- `build_root` chooses root envelope versus direct occurrence output.
- `build_occurrence` emits an `Occurrence` for a first visit and `Ref` for repeated cursor states.
- `export_value` recursively converts an `Ipld` value using a `Structure`.
- `export_bond` renders a bond envelope or follows the bond directly, depending on profile.
- `ligation_to_doc` renders `Slot` and `Ligase` terms.
- `erased_bond_to_doc` renders ligase argument envelopes and includes `$schema` for arguments after index `0`.

Rust uses `IndexMap` for document objects so schema/rendering order is stable.

## Rendering

Rust uses `serde_json` for JSON-LD and `serde_yaml_bw` for YAML and YAML-LD.

JSON-LD rendering:

- uses `@id` for occurrence IDs and references
- wraps occurrence data under `data`
- adds top-level `@context: { "@vocab": "urn:px:" }`
- honors `ExportOptions::pretty`

YAML-LD rendering:

- uses the same linked-data object model as JSON-LD
- uses YAML syntax
- uses `@id` and `@context`
- does not use YAML aliases for references

Plain YAML rendering:

- uses `id` instead of `@id`
- wraps occurrence data under `data`
- emits YAML anchors for occurrence definitions
- emits YAML aliases for repeated references
- derives anchor names from occurrence IDs by replacing non-alphanumeric characters and prefixing with `occ_`

## Rust Type Encoding Details

Rust export and import operate on `Ipld` values interpreted through `Structure`.

Important Rust-specific details:

- `Record` export accepts both `Ipld::Map` and field-ordered `Ipld::List` encodings.
- `Record` import materializes fields into an IPLD map and omits absent optional fields.
- `Tagged` export accepts single-key IPLD maps for payload variants and strings for unit variants.
- `Enum` export accepts integer indexes or variant-name strings, but document output is the variant name.
- `ByteString` uses base64 with `base64::engine::general_purpose::STANDARD`.
- `Option<T>` in record-field context uses direct/omitted field syntax.
- `Option<T>` outside record-field context uses an IPLD/document array of length `0` or `1`.
- `OrderedMap` uses a list of two-element lists.
- `Map` export renders document objects from IPLD map keys and currently ignores the key schema during rendering.
- `Float` import accepts integer document numbers and converts them to floating-point values.
- JSON integers outside `i64`/`u64` range are rendered as strings during export.
- Non-finite floating-point values render as null.

These details follow the current Rust implementation and should not be treated as a separate portable encoding model beyond the constraints in `13-POLYEPOXIDE-EXPORT`.

## Bond Envelopes in Rust

Rust uses the marker keys from the portable spec:

- `$link`
- `$ligation`
- `$schema`
- `$value`

For `Canonical` and `Full` export, ordinary links are rendered with `$link`; reflexive targets are rendered with `$ligation`.

For `Canonical`, Rust includes `$value` for ordinary links when the target is materializable and omits `$value` for ligation bonds. For `Full`, Rust attempts to include `$value` for both ordinary and ligation bonds. For `Direct`, Rust follows bonds and emits the hydrated occurrence directly.

Import rejects envelopes containing both `$link` and `$ligation`. In `Canonical` import mode, it rejects `$value` on ligation envelopes. In `Faithful` and `Canonical` modes, every bond position must carry `$link` or `$ligation`.

When `$value` accompanies `$link`, Rust materializes the value and compares the computed CID to the explicit link CID. For ligation envelopes with `$value`, Rust materializes the value as validation/inspection support but returns the ligation CID as the stored bond identity.

Erased ligase arguments may use a bare CID string or an envelope. Hydrated erased-bond values require `$schema` unless the argument inherits schema context from ligase argument index `0`.

## Import Implementation

Import first parses the input into an internal `InputNode` representation preserving:

- scalar values
- arrays
- ordered objects
- YAML anchors
- YAML aliases

JSON-LD input is parsed with `serde_json`. YAML and YAML-LD input are parsed with `serde_yaml_bw`.

Import collects occurrence definitions by YAML anchor and by `id`/`@id` objects with `data`. Reference objects are objects containing only an ID field plus optional `@context`. Duplicate occurrence names are rejected.

Materialization is schema-guided:

1. Convert the document subtree into `Ipld` using the current `SchemaCursor`.
2. Encode the IPLD value with `serde_ipld_dagcbor`.
3. Compute the CID with `compute_cid`.
4. Write bytes to the store under `key_from_cid(&cid)`.

Occurrence materialization is cached by `(occurrence_index, schema_state)`. If an occurrence is encountered recursively while already in progress, Rust reports `CyclicHydratedValue`; ligation must be used for cyclic structure.

Non-identity ligation terms imported from `$ligation` are serialized as `Ligation` DAG-CBOR bytes and stored under the corresponding data CID key. Identity ligation terms are virtual and are not written.

## Error Types

Rust export errors are:

```rust
pub enum ExportError<E: std::error::Error + Send + Sync + 'static> {
    NotFound(Cid),
    Store(E),
    Format(String),
    EmptyLigase,
    SlotOutOfRange(u16),
    CannotMaterialize(Cid),
    Render(String),
}
```

Rust import errors are:

```rust
pub enum ImportError {
    Json(String),
    Yaml(String),
    Invalid(String),
    DuplicateReference(String),
    MissingReference(String),
    ProfileViolation(String),
    CyclicHydratedValue(String),
    CidMismatch { expected: Cid, actual: Cid },
    Schema(String),
    Decode(String),
    Store(String),
}
```

`ExportError` preserves the backend store error type. `ImportError` erases backend store errors to strings because import writes materialized data through a non-generic error surface.

## Rust Design Choices

- Import/export is store-backed and schema-guided; it does not require concrete application Rust types.
- A separate schema `Solvent` is used as the schema cache required by `13-POLYEPOXIDE-EXPORT`.
- `CursorState` is an oxide so occurrence IDs can reuse the normal CID machinery.
- `StoreCursor` uses CID scopes instead of erased bond scopes because persisted traversal runs on CIDs and bytes.
- `IndexMap` preserves document field order.
- `serde_json` and `serde_yaml_bw` are renderer/parser choices, not portable format requirements.
- Import materializes blocks directly into a `Store`; it does not insert application values into a value `Solvent`.

## Out of Scope

- CLI commands and TUI presentation.
- Concrete persistent backend crates such as RocksDB and Fjall wrappers.
- Network synchronization protocols.
- Application-specific document framing beyond the generic options exposed by `ExportOptions`.
