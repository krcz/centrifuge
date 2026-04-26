# POLYEPOXIDE-RS-SOLVENT

## Dependencies

- `12-POLYEPOXIDE-SOLVENT`
- `31-POLYEPOXIDE-RS-MODEL`

## Intent

Describe how the language-agnostic solvent and cursor interfaces from `12-POLYEPOXIDE-SOLVENT` are represented in Rust in `polyepoxide-core`.

This spec focuses on the Rust-facing API shape and the Rust idioms used to enforce the solvent contract. It does not restate the general `Cell`, `ErasedCell`, or bond model from `31-POLYEPOXIDE-RS-MODEL`, except where their interaction with solvent behavior matters.

## Modules

- `polyepoxide-core/src/solvent.rs`: `Solvent`, `SolventError`, `persist_cell`.
- `polyepoxide-core/src/cursor.rs`: `Cursor<'a, T>`, `CursorError`.

`Solvent` operates on the `Cell<T>`, `Bond<T>`, `ErasedBond`, `Ligation`, and `ErasedCell` types defined in `31-POLYEPOXIDE-RS-MODEL`.

## Solvent Interface

Rust represents the portable solvent interface as an owned runtime object with methods on `&self`:

```rust
pub struct Solvent { ... }

impl Solvent {
    pub fn new() -> Self;

    pub fn add<T: Oxide>(&self, value: T) -> Arc<Cell<T>>;
    pub fn bond<T: Oxide>(&self, value: T) -> Bond<T>;

    pub fn get<T: Oxide>(&self, cid: &Cid) -> Option<Arc<Cell<T>>>;
    pub fn contains(&self, cid: &Cid) -> bool;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;

    pub fn add_bond<T: Oxide>(&self, bond: &Bond<T>) -> Bond<T>;
    pub fn add_erased_bond(&self, bond: &ErasedBond) -> ErasedBond;
    pub fn resolve<T: Oxide>(&self, bond: &Bond<T>) -> Bond<T>;

    pub fn persist_cell<T: Oxide, S: Store>(
        &self,
        cell: &Cell<T>,
        store: &S,
    ) -> Result<(Cid, Cid), S::Error>;
}
```

Rust uses interior mutability for `Solvent`. This allows the solvent to behave as a shared runtime object: application code, oxide dissolution, and cursor traversal can all hold shared references to the same solvent while still inserting cells, resolving bonds, and performing deduplication.

`add` returns a shared immutable `Arc<Cell<T>>`. This is the main Rust mechanism used to satisfy the solvent requirement that committed cells are immutable and safely shared.

`bond` is convenience sugar for adding a value and returning `Bond::Link` to the resulting cell.

`add_bond` and `add_erased_bond` internalize a supplied bond into this solvent when possible. In Rust, the erased form is explicit because the language does not have a direct runtime `Bond<?>` representation.

`resolve` attempts to resolve an existing bond using the solvent's current in-memory state and reflexive rules. Like the common spec, it is a resolution operation, not a general insertion entry point.

## Solvent Semantics in Rust

Rust follows the common solvent contract with these representation choices:

- committed values are exposed through shared `Arc<Cell<T>>` handles
- typed access uses `Bond<T>` and `Arc<Cell<T>>`
- heterogeneous solvent storage and erased traversal use `ErasedBond` and `ErasedCell`
- unresolved reflexive CIDs may resolve to `Bond::Ligation` rather than to a materialized cell

Identity reflexive CIDs may be handled virtually, as allowed by `12-POLYEPOXIDE-SOLVENT`. In Rust this means solvent APIs may resolve such CIDs without requiring a previously stored cell entry for the same CID.

Type-directed access is explicit. `get<T>` returns a typed cell only when the stored value can be interpreted as `T`.

## Cursor Interface

Rust represents the portable cursor as a typed traversal helper borrowing the solvent:

```rust
pub struct Cursor<'a, T: Oxide> { ... }

impl<'a, T: Oxide> Cursor<'a, T> {
    pub fn new(solvent: &'a Solvent, cell: Arc<Cell<T>>) -> Self;
    pub fn value(&self) -> &T;
    pub fn resolve_bond<U: Oxide>(&self, bond: &Bond<U>) -> Result<Cursor<'a, U>, CursorError>;
    pub fn follow<U: Oxide>(
        &self,
        pick: impl FnOnce(&T) -> &Bond<U>,
    ) -> Result<Cursor<'a, U>, CursorError>;
}
```

The lifetime `'a` ties the cursor to the solvent it traverses. This reflects the common rule that resolved links are meaningful only within one solvent.

`value` returns `&T`, not an owned clone. This keeps traversal read-only and matches the immutability requirement of solvent-managed content.

`follow` is Rust convenience sugar over `resolve_bond`, using a closure to select a bond field from the current value.

The cursor carries the current ligation scope internally. That scope is updated as traversal passes through `Ligase` and `Slot`, but it is not exposed as part of the public traversal API.

## Error Types

Rust uses dedicated error enums for solvent and cursor operations:

```rust
pub enum SolventError {
    NotFound(Cid),
    TypeMismatch(Cid),
}

pub enum CursorError {
    Unresolved(Cid),
    EmptyLigase,
    SlotOutOfRange(u16),
    TypeMismatch(Cid),
}
```

These map the common semantic categories from `12-POLYEPOXIDE-SOLVENT` and `11-POLYEPOXIDE-MODEL` into Rust-specific APIs:

- `Unresolved` is used when traversal needs a target that is still CID-only
- `EmptyLigase` and `SlotOutOfRange` are the main ligation traversal failures
- `TypeMismatch` is used when a resolved identity cannot be viewed as the requested Rust oxide type

## Persistence Helper

Rust includes `Solvent::persist_cell` as a helper attached to the solvent API:

```rust
pub fn persist_cell<T: Oxide, S: Store>(
    &self,
    cell: &Cell<T>,
    store: &S,
) -> Result<(Cid, Cid), S::Error>;
```

This method persists a solvent-managed root together with its schema root and returns `(value_cid, schema_cid)`.

`persist_cell` is related to the store component rather than being purely an in-memory graph concern. It is included here because Rust exposes it as a solvent method, but the store contract, byte layout, and traversal of persisted content belong to the Rust store and export specs.

## Key Rust Choices

- `Solvent` is an owned shared runtime object, while traversal borrows it through `Cursor<'a, T>`.
- Shared graph identities are represented with `Arc<Cell<T>>` rather than copied values.
- Erased traversal is explicit through `ErasedBond` because Rust does not have wildcard generic runtime values.
- Cursor traversal is strongly typed and returns borrowed read access, which makes solvent immutability part of the API shape rather than only a convention.

## Out of Scope

- store-backed traversal helpers such as `StoreCursor`
- schema-loading helpers that require store access
- synchronization behavior
- detailed store persistence mechanics
