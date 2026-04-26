# POLYEPOXIDE-SOLVENT

## Dependencies

- `11-POLYEPOXIDE-MODEL`

## Intent

Define the in-memory runtime that turns ordinary host-language values into immutable, deduplicated Polyepoxide graph state.

The solvent is the boundary between convenient mutable construction and content-addressed graph semantics. It owns committed cells, internalizes bonds, and provides the runtime used by typed traversal.

## Conceptual Model

Application code should be able to construct values naturally, including nested values and temporary references. Polyepoxide still needs committed graph state to be immutable, content-addressed, and safe to share across many references.

A solvent provides that transition:

- insert a value
- dissolve its outbound bonds into solvent-managed form
- compute its CID
- reuse an existing cell when equivalent content is already present

The result is an in-memory graph of immutable cells connected by typed or erased bonds. Resolved links are always local to one solvent. Unresolved links may still exist as CID-only references when the target has not been materialized.

## Contract

- A solvent owns cells keyed by CID.
- Adding a value dissolves all nested bonds into that solvent before insertion completes.
- Equivalent values deduplicate by CID, not by pointer identity.
- Resolved links must only point to cells in the same solvent.
- Unresolved links may remain CID-only when the target has not been materialized.
- Ligation bonds are internalized as ligation terms and are not eagerly dereferenced as ordinary data edges.
- Cells exposed by the solvent are immutable.
- Solvents must support typed and erased access paths, although runtimes with native wildcard generic erasure may not need a separate erased-bond API.
- Identity reflexive CIDs may be handled virtually rather than as stored cells, as long as their observable behavior matches ordinary ligation resolution.

## Standard Interfaces

Language implementations should expose interfaces equivalent to the following pseudocode. Names may follow host-language conventions, but the runtime behavior should match.

```text
interface Solvent:
  new() -> Solvent

  add(value: T) -> Cell<T>
  bond(value: T) -> Bond<T>

  get(cid: CID) -> optional<Cell<?>>
  contains(cid: CID) -> bool

  add_bond(bond: Bond<T>) -> Bond<T>
  add_erased_bond(bond: Bond<?>) -> Bond<?>   # optional in runtimes with native wildcard erasure
  resolve(bond: Bond<T>) -> Bond<T>
```

`add` inserts a value into the solvent and returns the deduplicated cell. `bond` is convenience sugar for `Link(add(value))`.

`add_bond` internalizes a bond target into the solvent when possible:

- `Link(cell)` becomes a link to an equivalent cell owned by this solvent
- `Unresolved(cid)` becomes `Link(cell)` if the solvent can materialize or already contains the target
- `Ligation(term)` remains a ligation bond, with nested bonds dissolved into this solvent as needed

`resolve` attempts to resolve an existing bond using only information already available through the solvent. It must not insert new application values that were not already represented by the supplied bond or existing solvent content.

`11-POLYEPOXIDE-MODEL` currently includes `dissolve_in(value, solvent)` on `Oxide`. This creates a temporary circular boundary on purpose: oxide implementations need a shared hook for solvent insertion, while solvent owns the graph runtime that performs insertion.

## Insertion Semantics

Insertion is transitive. Before a value becomes part of solvent-managed graph state, its outbound bonds are dissolved into the target solvent.

```text
function add_to_solvent(value, solvent):
  dissolved = value.dissolve_in(solvent)
  cid = compute_cid(dissolved)
  if solvent.contains(cid):
    return solvent.get(cid)
  cell = Cell.with_cid(dissolved, cid)
  solvent.insert(cell)
  return cell
```

The precise order of "dissolve first" versus "compute CID first" may vary internally if the implementation can prove equivalent behavior, but the externally visible result must be the CID of the solvent-managed representation after bond dissolution.

## Cursor

`Cursor` is the traversal helper for in-memory solvent graphs. It keeps three pieces of state together:

- the current cell
- a reference to the solvent
- the current ligation scope

This avoids scattering bond-resolution and ligation logic across call sites.

```text
interface Cursor<T>:
  value() -> T
  resolve_bond(bond: Bond<U>) -> Result<Cursor<U>, CursorError>
  follow(select: T -> Bond<U>) -> Result<Cursor<U>, CursorError>
```

`follow` is convenience sugar for selecting a bond from the current value and immediately resolving it.

When a cursor resolves a bond:

- if solvent resolution yields `Link(cell)`, traversal continues with that cell and the same scope
- if the bond is `Ligation(term)`, traversal resolves it against the current scope and continues with the updated scope
- if the bond remains `Unresolved(cid)`, traversal returns an unresolved-bond error

```text
function resolve_bond(cursor, bond):
  resolved = cursor.solvent.resolve(bond)
  if resolved is Link(cell):
    return Cursor(cell, cursor.solvent, cursor.scope)
  if resolved is Unresolved(cid):
    fail UnresolvedBond(cid)
  if resolved is Ligation(term):
    (next_bond, next_scope) = resolve_ligation(term, cursor.scope)
    return resolve_bond(Cursor(cursor.cell, cursor.solvent, next_scope), next_bond)
```

Cursor traversal is typed when the host language supports it. Type mismatch during resolution should be surfaced explicitly rather than silently coerced.

## Ligation Resolution

Cursor ligation behavior follows the model rules from `11-POLYEPOXIDE-MODEL`:

- `Ligase(args)` establishes a new scope and resolves first to `args[0]`
- `Slot(i)` resolves to `scope[i]` and keeps the current scope

```text
function resolve_ligation(term, scope):
  Ligase(args) => (args[0], args)
  Slot(index)  => (scope[index], scope)
```

An empty ligase entry point or an out-of-range slot is an invalid ligation reference for traversal.

## Immutability Requirements

The solvent must enforce immutability structurally, not just by convention. A mutation through any shared cell handle would invalidate its CID and every reference that depends on that identity.

The expected cross-language rule is:

- in languages with ownership, insertion should consume or isolate the value so no aliased mutable path remains
- in languages without ownership, insertion should deep-copy or otherwise prevent caller-held mutable references from affecting the stored cell
- cell handles returned by the solvent must expose read-only access only

## Error Semantics

The exact error type is language-specific, but solvent and cursor operations should distinguish at least:

- unresolved CID
- invalid ligation
- type mismatch

Implementations may refine these further, for example by separating empty ligase entry, slot out of range, or decode failure.

## Out of Scope

- raw byte stores and bookmark storage
- store-backed traversal helpers such as `StoreCursor`
- synchronization algorithms
- document import/export formats
