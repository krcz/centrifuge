use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::bond::Bond;
use crate::oxide::{BondMapper, BondVisitor, Oxide};

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
    fn schema() -> Structure {
        Structure::Enum(Self::variant_names().iter().map(|s| s.to_string()).collect())
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn map_bonds(&self, _mapper: &mut impl BondMapper) -> Self {
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
    fn schema() -> Structure {
        Structure::Enum(Self::variant_names().iter().map(|s| s.to_string()).collect())
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn map_bonds(&self, _mapper: &mut impl BondMapper) -> Self {
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
    /// Integer types (signed and unsigned, various sizes).
    Int(IntType),
    /// Floating-point types.
    Float(FloatType),
    /// Unit type (single value, like `()` in Rust).
    Unit,

    // Compound types
    /// Homogeneous list.
    Sequence(Bond<Structure>),
    /// Heterogeneous fixed-size tuple.
    Tuple(Vec<Bond<Structure>>),
    /// Record with ordered named fields. Encodes as array (field order from schema).
    Record(IndexMap<String, Bond<Structure>>),
    /// Tagged union with payloads. Encodes as map with single key.
    Tagged(IndexMap<String, Bond<Structure>>),
    /// C-style enum (unit variants only). Encodes as variant index.
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
    /// Reference to n-th ancestor in schema tree (for recursive types).
    /// 0 = immediate parent, 1 = grandparent, etc.
    SelfRef(u32),
}

impl Structure {
    /// Creates an optional type (sequence constrained to 0 or 1 elements).
    pub fn option(inner: Structure) -> Self {
        Structure::Sequence(Bond::new(inner))
    }

    /// Creates a result type (tagged union of ok/err).
    pub fn result(ok: Structure, err: Structure) -> Self {
        let mut variants = IndexMap::new();
        variants.insert("ok".to_string(), Bond::new(ok));
        variants.insert("err".to_string(), Bond::new(err));
        Structure::Tagged(variants)
    }

    /// Creates a sequence type.
    pub fn sequence(inner: Structure) -> Self {
        Structure::Sequence(Bond::new(inner))
    }

    /// Creates a bond type (reference to another oxide).
    pub fn bond(inner: Structure) -> Self {
        Structure::Bond(Bond::new(inner))
    }

    /// Creates an unordered map type.
    pub fn map(key: Structure, value: Structure) -> Self {
        Structure::Map {
            key: Bond::new(key),
            value: Bond::new(value),
        }
    }

    /// Creates an ordered map type.
    pub fn ordered_map(key: Structure, value: Structure) -> Self {
        Structure::OrderedMap {
            key: Bond::new(key),
            value: Bond::new(value),
        }
    }

    /// Creates a record type from field definitions.
    pub fn record(fields: impl IntoIterator<Item = (&'static str, Structure)>) -> Self {
        Structure::Record(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), Bond::new(v)))
                .collect(),
        )
    }

    /// Creates a tagged union type from variant definitions.
    pub fn tagged(variants: impl IntoIterator<Item = (&'static str, Structure)>) -> Self {
        Structure::Tagged(
            variants
                .into_iter()
                .map(|(k, v)| (k.to_string(), Bond::new(v)))
                .collect(),
        )
    }

    /// Creates a tuple type.
    pub fn tuple(elements: impl IntoIterator<Item = Structure>) -> Self {
        Structure::Tuple(elements.into_iter().map(Bond::new).collect())
    }
}

impl Oxide for Structure {
    /// Returns the schema of Structure itself.
    ///
    /// This is a tagged union describing all variants of the Structure enum.
    /// Recursive references use SelfRef(0) to refer back to Structure.
    fn schema() -> Structure {
        // SelfRef(0) refers to the Structure type itself (breaks recursion)
        let self_ref = Structure::SelfRef(0);

        // Schema for Map/OrderedMap payload: Record { key: Structure, value: Structure }
        let map_payload = Structure::record([("key", self_ref.clone()), ("value", self_ref.clone())]);

        Structure::tagged([
            // Primitives (unit payloads)
            ("Bool", Structure::Unit),
            ("Char", Structure::Unit),
            ("Unicode", Structure::Unit),
            ("ByteString", Structure::Unit),
            ("Int", IntType::schema()),
            ("Float", FloatType::schema()),
            ("Unit", Structure::Unit),
            // Compound types
            ("Sequence", self_ref.clone()),
            ("Tuple", Structure::sequence(self_ref.clone())),
            ("Record", Structure::ordered_map(Structure::Unicode, self_ref.clone())),
            ("Tagged", Structure::ordered_map(Structure::Unicode, self_ref.clone())),
            ("Enum", Structure::sequence(Structure::Unicode)),
            // Map types
            ("Map", map_payload.clone()),
            ("OrderedMap", map_payload),
            // Polyepoxide-specific
            ("Bond", self_ref),
            ("SelfRef", Structure::Int(IntType::U32)),
        ])
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        match self {
            Structure::Sequence(inner) => inner.visit_bonds(visitor),
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
            // Primitives and SelfRef have no bonds
            Structure::Bool
            | Structure::Char
            | Structure::Unicode
            | Structure::ByteString
            | Structure::Int(_)
            | Structure::Float(_)
            | Structure::Unit
            | Structure::Enum(_)
            | Structure::SelfRef(_) => {}
        }
    }

    fn map_bonds(&self, mapper: &mut impl BondMapper) -> Self {
        match self {
            Structure::Sequence(inner) => Structure::Sequence(inner.map_bonds(mapper)),
            Structure::Tuple(elements) => {
                Structure::Tuple(elements.iter().map(|el| el.map_bonds(mapper)).collect())
            }
            Structure::Record(fields) => Structure::Record(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.map_bonds(mapper)))
                    .collect(),
            ),
            Structure::Tagged(variants) => Structure::Tagged(
                variants
                    .iter()
                    .map(|(k, v)| (k.clone(), v.map_bonds(mapper)))
                    .collect(),
            ),
            Structure::Map { key, value } => Structure::Map {
                key: key.map_bonds(mapper),
                value: value.map_bonds(mapper),
            },
            Structure::OrderedMap { key, value } => Structure::OrderedMap {
                key: key.map_bonds(mapper),
                value: value.map_bonds(mapper),
            },
            Structure::Bond(inner) => Structure::Bond(inner.map_bonds(mapper)),
            // Primitives and SelfRef are copied as-is
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
            (Structure::Int(a), Structure::Int(b)) => a == b,
            (Structure::Float(a), Structure::Float(b)) => a == b,
            (Structure::Unit, Structure::Unit) => true,
            (Structure::Sequence(a), Structure::Sequence(b)) => a.key() == b.key(),
            (Structure::Tuple(a), Structure::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.key() == y.key())
            }
            (Structure::Record(a), Structure::Record(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && v1.key() == v2.key())
            }
            (Structure::Tagged(a), Structure::Tagged(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && v1.key() == v2.key())
            }
            (Structure::Enum(a), Structure::Enum(b)) => a == b,
            (
                Structure::Map {
                    key: k1,
                    value: v1,
                },
                Structure::Map {
                    key: k2,
                    value: v2,
                },
            ) => k1.key() == k2.key() && v1.key() == v2.key(),
            (
                Structure::OrderedMap {
                    key: k1,
                    value: v1,
                },
                Structure::OrderedMap {
                    key: k2,
                    value: v2,
                },
            ) => k1.key() == k2.key() && v1.key() == v2.key(),
            (Structure::Bond(a), Structure::Bond(b)) => a.key() == b.key(),
            (Structure::SelfRef(a), Structure::SelfRef(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Structure {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::Oxide;

    #[test]
    fn structure_record_preserves_order() {
        let schema = Structure::record([
            ("first", Structure::Bool),
            ("second", Structure::Int(IntType::U32)),
            ("third", Structure::Unicode),
        ]);

        if let Structure::Record(f) = schema {
            let keys: Vec<_> = f.keys().collect();
            assert_eq!(keys, vec!["first", "second", "third"]);
        } else {
            panic!("Expected Record");
        }
    }

    #[test]
    fn structure_option_sugar() {
        let opt = Structure::option(Structure::Unicode);
        assert!(matches!(opt, Structure::Sequence(_)));
    }

    #[test]
    fn structure_result_sugar() {
        let res = Structure::result(Structure::Unicode, Structure::Int(IntType::I32));
        if let Structure::Tagged(variants) = res {
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
        if let Structure::Enum(variants) = schema {
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
        if let Structure::Enum(variants) = schema {
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
        if let Structure::Tagged(variants) = &schema {
            // Should have all 16 variants
            assert_eq!(variants.len(), 16);
            assert!(variants.contains_key("Bool"));
            assert!(variants.contains_key("Record"));
            assert!(variants.contains_key("SelfRef"));
        } else {
            panic!("Expected Tagged, got {:?}", schema);
        }
    }

    #[test]
    fn structure_content_addressable() {
        let s1 = Structure::Bool;
        let s2 = Structure::Bool;
        let s3 = Structure::Unicode;

        assert_eq!(s1.compute_key(), s2.compute_key());
        assert_ne!(s1.compute_key(), s3.compute_key());
    }

    #[test]
    fn structure_with_bonds_content_addressable() {
        let s1 = Structure::sequence(Structure::Unicode);
        let s2 = Structure::sequence(Structure::Unicode);
        let s3 = Structure::sequence(Structure::Bool);

        // Same structure should produce same key
        assert_eq!(s1.compute_key(), s2.compute_key());
        // Different inner type should produce different key
        assert_ne!(s1.compute_key(), s3.compute_key());
    }

    #[test]
    fn structure_roundtrip() {
        let original = Structure::record([
            ("name", Structure::Unicode),
            ("age", Structure::Int(IntType::U32)),
        ]);

        let bytes = original.to_bytes();
        let recovered: Structure = Oxide::from_bytes(&bytes).unwrap();

        // After roundtrip, bonds are unresolved but keys should match
        assert_eq!(original.compute_key(), recovered.compute_key());
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
        assert_eq!(s1.compute_key(), s2.compute_key());
    }
}
