use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::bond::Bond;
use crate::oxide::{BondVisitor, Oxide};
use crate::reflexive::Ligation;
use crate::solvent::Solvent;

/// Integer type variants for the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

impl IntType {
    /// Returns all variant names in order.
    pub fn variant_names() -> &'static [&'static str] {
        &["U8", "U16", "U32", "U64", "I8", "I16", "I32", "I64"]
    }
}

impl Oxide for IntType {
    fn schema() -> Bond<Structure> {
        Bond::new(Structure::Enum(
            Self::variant_names()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ))
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn dissolve_in(&self, _solvent: &Solvent) -> Self {
        *self
    }
}

/// Floating-point type variants for the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatType {
    F32,
    F64,
}

impl FloatType {
    /// Returns all variant names in order.
    pub fn variant_names() -> &'static [&'static str] {
        &["F32", "F64"]
    }
}

impl Oxide for FloatType {
    fn schema() -> Bond<Structure> {
        Bond::new(Structure::Enum(
            Self::variant_names()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ))
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn dissolve_in(&self, _solvent: &Solvent) -> Self {
        *self
    }
}

/// Structure type system for Polyepoxide.
///
/// Defines the structure of oxides. Structures are themselves content-addressed
/// and can be stored in the DAG (Structure implements Oxide).
///
/// Nested structures are referenced via `Bond<Structure>`, enabling deduplication
/// and lazy loading when stored in a Solvent.
///
/// NOTE: Canonical CBOR serialization is not yet implemented. Map key ordering
/// must be handled explicitly when deterministic hashing is required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Structure {
    // Primitives
    /// Boolean value.
    Bool,
    /// Unicode scalar value (u32 subset).
    Char,
    /// UTF-8 text string.
    Unicode,
    /// Byte sequence.
    ByteString,
    /// CID value encoded as DAG-CBOR link.
    Cid,
    /// Integer types (signed and unsigned, various sizes).
    Int(IntType),
    /// Floating-point types.
    Float(FloatType),
    /// Unit type (single value, like `()` in Rust).
    Unit,

    // Compound types
    /// Optional value. Outside record fields this follows the underlying direct/null encoding;
    /// record import/export may use omitted/direct field syntax.
    Option(Bond<Structure>),
    /// Homogeneous list.
    Sequence(Bond<Structure>),
    /// Heterogeneous fixed-size tuple.
    Tuple(Vec<Bond<Structure>>),
    /// Record with ordered named fields. Encodes as array (field order from schema).
    #[serde(with = "crate::serde_helpers::indexmap_as_ordered_map")]
    Record(IndexMap<String, Bond<Structure>>),
    /// Tagged union with payloads. Encodes as map with single key.
    #[serde(with = "crate::serde_helpers::indexmap_as_ordered_map")]
    Tagged(IndexMap<String, Bond<Structure>>),
    /// C-style enum (unit variants only). With the current Serde representation,
    /// unit variants encode by name.
    Enum(Vec<String>),

    // Map types
    /// Unordered map. Keys sorted for canonical encoding.
    Map {
        key: Bond<Structure>,
        value: Bond<Structure>,
    },
    /// Ordered map. Preserves insertion order.
    OrderedMap {
        key: Bond<Structure>,
        value: Bond<Structure>,
    },

    // Polyepoxide-specific
    /// Reference to another oxide (lazy-loadable).
    Bond(Bond<Structure>),
}

impl Structure {
    /// Creates an optional type (sequence constrained to 0 or 1 elements).
    pub fn option(inner: impl Into<Bond<Structure>>) -> Bond<Structure> {
        Bond::new(Structure::Option(inner.into()))
    }

    /// Creates a result type (tagged union of ok/err).
    pub fn result(
        ok: impl Into<Bond<Structure>>,
        err: impl Into<Bond<Structure>>,
    ) -> Bond<Structure> {
        let mut variants = IndexMap::new();
        variants.insert("ok".to_string(), ok.into());
        variants.insert("err".to_string(), err.into());
        Bond::new(Structure::Tagged(variants))
    }

    /// Creates a sequence type.
    pub fn sequence(inner: impl Into<Bond<Structure>>) -> Bond<Structure> {
        Bond::new(Structure::Sequence(inner.into()))
    }

    /// Creates a bond type (reference to another oxide).
    pub fn bond(inner: impl Into<Bond<Structure>>) -> Bond<Structure> {
        Bond::new(Structure::Bond(inner.into()))
    }

    /// Creates an unordered map type.
    pub fn map(
        key: impl Into<Bond<Structure>>,
        value: impl Into<Bond<Structure>>,
    ) -> Bond<Structure> {
        Bond::new(Structure::Map {
            key: key.into(),
            value: value.into(),
        })
    }

    /// Creates an ordered map type.
    pub fn ordered_map(
        key: impl Into<Bond<Structure>>,
        value: impl Into<Bond<Structure>>,
    ) -> Bond<Structure> {
        Bond::new(Structure::OrderedMap {
            key: key.into(),
            value: value.into(),
        })
    }

    /// Creates a record type from field definitions.
    pub fn record<V>(fields: impl IntoIterator<Item = (&'static str, V)>) -> Bond<Structure>
    where
        V: Into<Bond<Structure>>,
    {
        Bond::new(Structure::Record(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.into()))
                .collect(),
        ))
    }

    /// Creates a tagged union type from variant definitions.
    pub fn tagged<V>(variants: impl IntoIterator<Item = (&'static str, V)>) -> Bond<Structure>
    where
        V: Into<Bond<Structure>>,
    {
        Bond::new(Structure::Tagged(
            variants
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.into()))
                .collect(),
        ))
    }

    /// Creates a tuple type.
    pub fn tuple<V>(elements: impl IntoIterator<Item = V>) -> Bond<Structure>
    where
        V: Into<Bond<Structure>>,
    {
        Bond::new(Structure::Tuple(
            elements.into_iter().map(|v| v.into()).collect(),
        ))
    }
}

impl Oxide for Structure {
    /// Returns the schema of Structure itself.
    ///
    /// This is a tagged union describing all variants of the Structure enum.
    /// Recursive references use Slot(0) and are wrapped in Ligase.
    fn schema() -> Bond<Structure> {
        let self_ref: Bond<Structure> = Bond::from_ligation(Ligation::Slot(0));
        // Variant payload schemas are themselves stored as Bond<Structure>. `self_ref` is the
        // recursive schema of `Structure`, while `self_bond` is the schema of `Bond<Structure>`.
        let self_bond = Bond::new(Structure::Bond(self_ref.clone()));

        let map_payload = Bond::new(Structure::Record(
            [
                ("key".to_string(), self_bond.clone()),
                ("value".to_string(), self_bond.clone()),
            ]
            .into_iter()
            .collect(),
        ));

        let root = Bond::new(Structure::Tagged(
            [
                // Primitives (unit payloads)
                ("Bool".to_string(), Bond::new(Structure::Unit)),
                ("Char".to_string(), Bond::new(Structure::Unit)),
                ("Unicode".to_string(), Bond::new(Structure::Unit)),
                ("ByteString".to_string(), Bond::new(Structure::Unit)),
                ("Cid".to_string(), Bond::new(Structure::Unit)),
                ("Int".to_string(), IntType::schema()),
                ("Float".to_string(), FloatType::schema()),
                ("Unit".to_string(), Bond::new(Structure::Unit)),
                // Compound types
                ("Option".to_string(), self_bond.clone()),
                ("Sequence".to_string(), self_bond.clone()),
                (
                    "Tuple".to_string(),
                    Bond::new(Structure::Sequence(self_bond.clone())),
                ),
                (
                    "Record".to_string(),
                    Bond::new(Structure::OrderedMap {
                        key: Bond::new(Structure::Unicode),
                        value: self_bond.clone(),
                    }),
                ),
                (
                    "Tagged".to_string(),
                    Bond::new(Structure::OrderedMap {
                        key: Bond::new(Structure::Unicode),
                        value: self_bond.clone(),
                    }),
                ),
                (
                    "Enum".to_string(),
                    Bond::new(Structure::Sequence(Bond::new(Structure::Unicode))),
                ),
                // Map types
                ("Map".to_string(), map_payload.clone()),
                ("OrderedMap".to_string(), map_payload),
                // Polyepoxide-specific
                ("Bond".to_string(), self_bond),
            ]
            .into_iter()
            .collect(),
        ));

        Bond::from_ligation(Ligation::Ligase(vec![root.erased()]))
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        match self {
            Structure::Option(inner) | Structure::Sequence(inner) => inner.visit_bonds(visitor),
            Structure::Tuple(elements) => {
                for el in elements {
                    el.visit_bonds(visitor);
                }
            }
            Structure::Record(fields) | Structure::Tagged(fields) => {
                for bond in fields.values() {
                    bond.visit_bonds(visitor);
                }
            }
            Structure::Map { key, value } | Structure::OrderedMap { key, value } => {
                key.visit_bonds(visitor);
                value.visit_bonds(visitor);
            }
            Structure::Bond(inner) => inner.visit_bonds(visitor),
            // Primitives have no bonds
            Structure::Bool
            | Structure::Char
            | Structure::Unicode
            | Structure::ByteString
            | Structure::Cid
            | Structure::Int(_)
            | Structure::Float(_)
            | Structure::Unit
            | Structure::Enum(_) => {}
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        match self {
            Structure::Option(inner) => Structure::Option(inner.dissolve_in(solvent)),
            Structure::Sequence(inner) => Structure::Sequence(inner.dissolve_in(solvent)),
            Structure::Tuple(elements) => {
                Structure::Tuple(elements.iter().map(|el| el.dissolve_in(solvent)).collect())
            }
            Structure::Record(fields) => Structure::Record(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.dissolve_in(solvent)))
                    .collect(),
            ),
            Structure::Tagged(variants) => Structure::Tagged(
                variants
                    .iter()
                    .map(|(k, v)| (k.clone(), v.dissolve_in(solvent)))
                    .collect(),
            ),
            Structure::Map { key, value } => Structure::Map {
                key: key.dissolve_in(solvent),
                value: value.dissolve_in(solvent),
            },
            Structure::OrderedMap { key, value } => Structure::OrderedMap {
                key: key.dissolve_in(solvent),
                value: value.dissolve_in(solvent),
            },
            Structure::Bond(inner) => Structure::Bond(inner.dissolve_in(solvent)),
            // Primitives are copied as-is
            other => other.clone(),
        }
    }
}

// Manual PartialEq implementation that compares by key for bonds
impl PartialEq for Structure {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Structure::Bool, Structure::Bool) => true,
            (Structure::Char, Structure::Char) => true,
            (Structure::Unicode, Structure::Unicode) => true,
            (Structure::ByteString, Structure::ByteString) => true,
            (Structure::Cid, Structure::Cid) => true,
            (Structure::Int(a), Structure::Int(b)) => a == b,
            (Structure::Float(a), Structure::Float(b)) => a == b,
            (Structure::Unit, Structure::Unit) => true,
            (Structure::Option(a), Structure::Option(b)) => a.cid() == b.cid(),
            (Structure::Sequence(a), Structure::Sequence(b)) => a.cid() == b.cid(),
            (Structure::Tuple(a), Structure::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.cid() == y.cid())
            }
            (Structure::Record(a), Structure::Record(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && v1.cid() == v2.cid())
            }
            (Structure::Tagged(a), Structure::Tagged(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && v1.cid() == v2.cid())
            }
            (Structure::Enum(a), Structure::Enum(b)) => a == b,
            (Structure::Map { key: k1, value: v1 }, Structure::Map { key: k2, value: v2 }) => {
                k1.cid() == k2.cid() && v1.cid() == v2.cid()
            }
            (
                Structure::OrderedMap { key: k1, value: v1 },
                Structure::OrderedMap { key: k2, value: v2 },
            ) => k1.cid() == k2.cid() && v1.cid() == v2.cid(),
            (Structure::Bond(a), Structure::Bond(b)) => a.cid() == b.cid(),
            _ => false,
        }
    }
}

impl Eq for Structure {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::Oxide;
    use crate::{Cell, ErasedBond, Ligation};

    fn resolve_schema_root(schema: Bond<Structure>) -> Structure {
        let solvent = Solvent::new();
        match solvent.add_bond(&schema) {
            Bond::Link(cell) => cell.value().clone(),
            Bond::Ligation(ligation) => match *ligation {
                Ligation::Ligase(args) => {
                    let Some(entry) = args.first() else {
                        panic!("ligase args should not be empty");
                    };
                    let ErasedBond::Link(cell) = solvent.add_erased_bond(entry) else {
                        panic!("ligase entry should resolve to a link");
                    };
                    let Some(cell) = cell.into_any_arc().downcast::<Cell<Structure>>().ok() else {
                        panic!("ligase entry should resolve to Structure");
                    };
                    cell.value().clone()
                }
                Ligation::Slot(_) => panic!("schema root should not be Slot"),
            },
            Bond::Unresolved(_) => panic!("schema should resolve"),
        }
    }

    #[test]
    fn structure_record_preserves_order() {
        let schema = Structure::record([
            ("first", Structure::Bool),
            ("second", Structure::Int(IntType::U32)),
            ("third", Structure::Unicode),
        ]);

        if let Some(Structure::Record(f)) = schema.value() {
            let keys: Vec<_> = f.keys().collect();
            assert_eq!(keys, vec!["first", "second", "third"]);
        } else {
            panic!("Expected Record");
        }
    }

    #[test]
    fn structure_option_sugar() {
        let opt = Structure::option(Structure::Unicode);
        assert!(matches!(opt.value(), Some(Structure::Option(_))));
    }

    #[test]
    fn structure_result_sugar() {
        let res = Structure::result(Structure::Unicode, Structure::Int(IntType::I32));
        if let Some(Structure::Tagged(variants)) = res.value() {
            assert_eq!(variants.len(), 2);
            assert!(variants.contains_key("ok"));
            assert!(variants.contains_key("err"));
        } else {
            panic!("Expected Tagged");
        }
    }

    #[test]
    fn int_type_schema() {
        let schema = IntType::schema();
        if let Some(Structure::Enum(variants)) = schema.value() {
            assert_eq!(variants.len(), 8);
            assert_eq!(variants[0], "U8");
            assert_eq!(variants[7], "I64");
        } else {
            panic!("Expected Enum");
        }
    }

    #[test]
    fn float_type_schema() {
        let schema = FloatType::schema();
        if let Some(Structure::Enum(variants)) = schema.value() {
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0], "F32");
            assert_eq!(variants[1], "F64");
        } else {
            panic!("Expected Enum");
        }
    }

    #[test]
    fn structure_schema_is_tagged() {
        let schema = Structure::schema();
        let root = resolve_schema_root(schema);
        if let Structure::Tagged(variants) = root {
            // Should have all 17 variants
            assert_eq!(variants.len(), 17);
            assert!(variants.contains_key("Bool"));
            assert!(variants.contains_key("Cid"));
            assert!(variants.contains_key("Option"));
            assert!(variants.contains_key("Record"));
        } else {
            panic!("Expected Tagged");
        }
    }

    #[test]
    fn structure_content_addressable() {
        let s1 = Structure::Bool;
        let s2 = Structure::Bool;
        let s3 = Structure::Unicode;

        assert_eq!(s1.compute_cid(), s2.compute_cid());
        assert_ne!(s1.compute_cid(), s3.compute_cid());
    }

    #[test]
    fn structure_with_bonds_content_addressable() {
        let s1 = Structure::sequence(Structure::Unicode);
        let s2 = Structure::sequence(Structure::Unicode);
        let s3 = Structure::sequence(Structure::Bool);

        // Same structure should produce same key
        assert_eq!(s1.compute_cid(), s2.compute_cid());
        // Different inner type should produce different key
        assert_ne!(s1.compute_cid(), s3.compute_cid());
    }

    #[test]
    fn structure_roundtrip() {
        let original = Structure::record([
            ("name", Structure::Unicode),
            ("age", Structure::Int(IntType::U32)),
        ]);

        let bytes = original.to_bytes();
        let recovered: Bond<Structure> = Oxide::from_bytes(&bytes).unwrap();

        // After roundtrip, bonds are unresolved but keys should match
        assert_eq!(original.compute_cid(), recovered.compute_cid());
    }

    #[test]
    fn int_type_roundtrip() {
        let original = IntType::I64;
        let bytes = original.to_bytes();
        let recovered: IntType = Oxide::from_bytes(&bytes).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn nested_structure_deduplication() {
        // Create two structures that share the same inner type
        let inner = Structure::Unicode;
        let s1 = Structure::sequence(inner.clone());
        let s2 = Structure::sequence(inner);

        // Both should have the same key
        assert_eq!(s1.compute_cid(), s2.compute_cid());
    }
}
