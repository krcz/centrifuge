# POLYEPOXIDE-RS-MODEL

## Dependencies

- `11-POLYEPOXIDE-MODEL`

## Intent

Describe how the portable Polyepoxide model is represented in Rust through `polyepoxide-core` and `polyepoxide-derive`.

This spec is not a complete API reference. It records the Rust encodings of the language-agnostic interfaces from `11-POLYEPOXIDE-MODEL`, the serialization rules that affect content identity, and the derive behavior needed to generate compatible schemas and traversal implementations.

## Crates and Modules

- `polyepoxide-core/src/oxide.rs`: `Oxide`, `BondVisitor`, `ByteString`, `compute_cid`, schema template instantiation.
- `polyepoxide-core/src/schema.rs`: `Structure`, `IntType`, `FloatType`.
- `polyepoxide-core/src/bond.rs`: `Bond<T>`, `ErasedBond`.
- `polyepoxide-core/src/cell.rs`: `Cell<T>`, `ErasedCell`.
- `polyepoxide-core/src/dyn_bond.rs`: `DynBond`, `DynBondError`.
- `polyepoxide-core/src/common.rs`: `Catalogue`.
- `polyepoxide-core/src/reflexive.rs`: `Ligation`, reflexive CID helpers, codec conversion helpers.
- `polyepoxide-derive`: `#[derive(Oxide)]` and `#[oxide]`.

## Oxide Interface

Rust represents the portable `Oxide<T>` interface as a trait implemented by the oxide type itself:

```rust
pub trait Oxide:
    Debug + Serialize + DeserializeOwned + Clone + Send + Sync + 'static
{
    fn schema() -> Bond<Structure>;
    fn schema_template() -> Bond<Structure> { Self::schema() }
    fn visit_bonds(&self, visitor: &mut dyn BondVisitor);
    fn dissolve_in(&self, solvent: &Solvent) -> Self;
    fn compute_cid(&self) -> Cid { compute_cid(&self.to_bytes()) }
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(data: &[u8]) -> Result<Self, DecodeError>;
}

pub trait BondVisitor {
    fn visit_bond(&mut self, cid: &Cid);
}
```

`to_bytes` and `from_bytes` use `serde_ipld_dagcbor`. `compute_cid` hashes those bytes with Blake3-256 and returns CIDv1 with DAG-CBOR codec `0x71`.

`dissolve_in` currently references `Solvent`, matching the temporary circular dependency described in `11-POLYEPOXIDE-MODEL`. The Rust implementation should follow the model spec if that hook is later moved behind a solvent-side adapter or resolver boundary.

## Structure Encoding

Rust represents the portable `Structure` ADT directly:

```rust
pub enum Structure {
    Bool,
    Char,
    Unicode,
    ByteString,
    Cid,
    Int(IntType),
    Float(FloatType),
    Unit,
    Option(Bond<Structure>),
    Sequence(Bond<Structure>),
    Tuple(Vec<Bond<Structure>>),
    Record(IndexMap<String, Bond<Structure>>),
    Tagged(IndexMap<String, Bond<Structure>>),
    Enum(Vec<String>),
    Map { key: Bond<Structure>, value: Bond<Structure> },
    OrderedMap { key: Bond<Structure>, value: Bond<Structure> },
    Bond(Bond<Structure>),
}
```

`IntType` is `U8`, `U16`, `U32`, `U64`, `I8`, `I16`, `I32`, or `I64`. `FloatType` is `F32` or `F64`.

`Record` and `Tagged` use `IndexMap` because field and variant order is part of schema identity. Their internal serde representation uses ordered key/value pairs, but application structs and document formats may still use map/object-like record encodings where the schema determines field order.

`Structure::schema()` is a self-describing tagged schema. Recursive references to `Structure` use `Ligation::Slot(0)` and the root is wrapped in `Ligation::Ligase`.

## Bond and Cell Encoding

Rust represents typed bonds and cells as:

```rust
pub enum Bond<T: Oxide> {
    Unresolved(Cid),
    Link(Arc<Cell<T>>),
    Ligation(Box<Ligation>),
}

pub struct Cell<T: Oxide> {
    value: T,
    cid: OnceLock<Cid>,
}
```

`Bond<T>` serializes as `self.cid()` and deserializes as `Bond::Unresolved(cid)`. `Bond<T>::schema()` is `Structure::Bond(T::schema())`.

`Cell<T>` is the immutable in-memory wrapper for a value. The CID is computed lazily and cached. A precomputed-CID constructor is part of the model contract for known-CID decode/load paths.

Rust also defines erased forms because the language does not have a direct `Bond<?>` runtime representation:

```rust
pub enum ErasedBond {
    Unresolved(Cid),
    Link(Arc<dyn ErasedCell>),
    Ligation(Box<Ligation>),
}

pub trait ErasedCell: Any + Send + Sync + Debug {
    fn cid(&self) -> Cid;
    fn as_any(&self) -> &dyn Any;
    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
    fn dissolve_in(&self, solvent: &Solvent) -> Arc<dyn ErasedCell>;
    fn to_bytes(&self) -> Vec<u8>;
    fn visit_bonds(&self, visitor: &mut dyn BondVisitor);
}
```

`ErasedBond` mirrors the logical states of `Bond<T>`. Its schema is `Structure::Cid`, because serialized erased bonds carry only reference identity. Equality is CID-based.

## Dynamic Types

Rust represents `DynamicBond` as `DynBond`:

```rust
pub struct DynBond {
    pub schema: Bond<Structure>,
    pub bond: ErasedBond,
}
```

`DynBond::from_typed<T>` pairs `T::schema()` with the erased form of the supplied bond. Typed resolution compares `self.schema.cid()` with `T::schema().cid()` before converting to `Bond<T>`.

Rust represents `Catalogue` as:

```rust
pub struct Catalogue {
    pub items: HashMap<String, DynBond>,
}
```

`Catalogue::schema()` uses `Structure::Record` with an `items` field whose value is `Structure::Map { key: Unicode, value: DynBond::schema() }`.

## Ligation and Reflexive Addressing

Rust represents ligation as:

```rust
pub enum Ligation {
    Ligase(Vec<ErasedBond>),
    Slot(u16),
}
```

`Ligation` implements `Oxide`. `Ligase` visits its argument bonds; `Slot` has no outbound bonds by itself. Ligation CIDs use the constants from `11-POLYEPOXIDE-MODEL`:

```rust
pub const POLYEPOXIDE_REFLEXIVE_CODEC: u64 = 0x300001;
pub const MULTIHASH_IDENTITY: u64 = 0x00;
```

Rust exposes helpers equivalent to the model-level ligation operations:

```rust
pub fn with_codec(cid: &Cid, codec: u64) -> Cid;
pub fn data_to_reflexive_cid(cid: &Cid) -> Cid;
pub fn reflexive_to_data_cid(cid: &Cid) -> Cid;
pub fn slot_cid(index: u16) -> Cid;
pub fn ligase_cid(args: Vec<ErasedBond>) -> Cid;
pub fn ligation_cid(ligation: &Ligation) -> Cid;
pub fn resolve_ligation_bond(
    ligation: Option<Ligation>,
    scope: &[ErasedBond],
) -> Option<(ErasedBond, Vec<ErasedBond>)>;
```

`slot_cid` uses identity multihash with the serialized slot bytes embedded in the digest. `ligase_cid` hashes the serialized ligase bytes with Blake3-256 and uses the Polyepoxide reflexive codec. Codec conversion preserves the multihash and changes only the CID codec.

## Built-In Oxide Implementations

Rust provides built-in `Oxide` implementations for the standard model types:

| Rust type | `Structure` mapping | Encoding note |
| --- | --- | --- |
| `bool` | `Bool` | |
| `String` | `Unicode` | |
| `Cid` | `Cid` | DAG-CBOR link |
| `ByteString` | `ByteString` | newtype distinct from `Vec<u8>` |
| `u8`, `u16`, `u32`, `u64` | `Int(U*)` | |
| `i8`, `i16`, `i32`, `i64` | `Int(I*)` | |
| `f32`, `f64` | `Float(F*)` | |
| `()` | `Unit` | |
| `Vec<T>` | `Sequence(T::schema())` | |
| `Option<T>` | `Option(T::schema())` | standalone encoding is `[]` or `[x]` |
| `Result<T, E>` | `Tagged { ok, err }` | lowercase serde keys |
| `IndexMap<K, V>` | `OrderedMap { key, value }` | ordered pair encoding |
| `HashMap<K, V>` | `Map { key, value }` | unordered map |
| `Bond<T>` | `Bond(T::schema())` | serializes as CID |
| `ErasedBond` | `Cid` | serializes as CID |
| `Structure` | self-describing `Tagged` schema | recursive via ligation |
| `DynBond` | `Record { schema, bond }` | schema-carrying erased bond |
| `Catalogue` | `Record { items }` | named dynamic-bond map |

`char` has a portable `Structure::Char` variant, but it is not currently listed among the built-in Rust `Oxide` implementations.

## Rust Serde Rules

Standalone `Option<T>` overrides `to_bytes` and `from_bytes` to serialize as a length-0-or-1 CBOR list: `[]` for `None`, `[x]` for `Some(x)`.

Named `Option<T>` fields generated by `#[oxide]` use `serde(default, skip_serializing_if = "Option::is_none", with = "option_as_field")`. This omits `None` fields and encodes `Some(x)` as the direct inner value.

`Result<T, E>` fields generated by `#[oxide]` use lowercase tagged encoding: `{"ok": x}` or `{"err": e}`.

`IndexMap<K, V>` fields generated by `#[oxide]` use ordered pair encoding: `[[key, value], ...]`.

`HashMap<K, V>` maps to `Structure::Map`. The current implementation relies on `serde_ipld_dagcbor` and Rust container behavior for bytes. Fully canonical map-key ordering is an intended model property, but not yet enforced uniformly by all Rust map paths.

## Schema Templates

Rust exposes:

```rust
pub fn instantiate_schema_template(
    template: Bond<Structure>,
    args: &[Bond<Structure>],
) -> Bond<Structure>;
```

Instantiation replaces `Slot(i)` with `args[i - 1]` for `i >= 1`. `Slot(0)` is preserved as the self-reference slot. Nested ligases are preserved because they introduce their own scope.

## Derive Macros

`#[derive(Oxide)]` generates implementations of:

- `schema`
- `schema_template`
- `visit_bonds`
- `dissolve_in`

Supported oxide attributes are:

- `#[oxide(rename = "...")]`
- `#[oxide(skip)]`
- `#[oxide(crate = path)]`

Structs derive `Structure::Record` schemas. Tuple structs derive `Structure::Tuple`. Unit structs derive `Structure::Unit`.

Enums with payload variants derive `Structure::Tagged`. Enums with only unit variants derive `Structure::Enum`.

Generic type parameters receive `Oxide` bounds. Schema templates map references to the derived type itself to `Slot(0)` and generic parameters to `Slot(1)`, `Slot(2)`, and later slots in declaration order. `schema()` instantiates generic slots with the generic argument schemas. A ligase wrapper is introduced when self-reference is present.

`#[oxide]` is an attribute macro that adds:

- `Debug`
- `Clone`
- `Serialize`
- `Deserialize`
- `Oxide`

It also installs serde helper attributes for named `Option<T>` fields, `Result<T, E>`, and `IndexMap<K, V>` fields.

## Rust Design Choices

- `Arc<Cell<T>>` is the shared immutable handle for resolved typed links.
- `Arc<dyn ErasedCell>` is the Rust representation of erased cell storage.
- `serde_ipld_dagcbor` is the current oxide byte codec.
- `indexmap::IndexMap` is used where schema or document field order matters.
- Proc macros keep application oxide definitions close to ordinary Rust structs and enums.

## Out of Scope

- Solvent ownership and traversal behavior are specified in `32-POLYEPOXIDE-RS-SOLVENT`.
- Store traits and synchronization are specified in `32-POLYEPOXIDE-RS-STORE`.
- Document import/export behavior is specified in `33-POLYEPOXIDE-RS-EXPORT`.
