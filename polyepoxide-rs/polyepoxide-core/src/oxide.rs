use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt::Debug;

use crate::bond::Bond;
use crate::schema::Structure;
use crate::solvent::Solvent;

/// DAG-CBOR codec code (0x71).
pub const DAG_CBOR_CODEC: u64 = 0x71;

/// Computes a CID for DAG-CBOR encoded data using Blake3.
pub fn compute_cid(data: &[u8]) -> Cid {
    let hash = Code::Blake3_256.digest(data);
    Cid::new_v1(DAG_CBOR_CODEC, hash)
}

/// A visitor for traversing bonds in an oxide.
pub trait BondVisitor {
    /// Visits a bond CID with type information erased.
    fn visit_bond(&mut self, cid: &Cid);
}

/// An oxide is a value that can be stored in the Polyepoxide DAG.
///
/// To be an oxide, a value must be:
/// - Serializable to a canonical byte representation (CBOR)
/// - Content-addressable (identity is the hash of serialized form)
/// - Schema-aware (can describe its own structure)
pub trait Oxide: Debug + Serialize + DeserializeOwned + Clone + Send + Sync + 'static {
    /// Returns the structure describing this oxide's type.
    fn schema() -> Bond<Structure>;

    /// Visits all bonds contained in this oxide.
    fn visit_bonds(&self, visitor: &mut dyn BondVisitor);

    /// Creates a new oxide with all nested bonds dissolved in the given solvent.
    fn dissolve_in(&self, solvent: &Solvent) -> Self;

    /// Computes the content-addressed CID of this oxide.
    fn compute_cid(&self) -> Cid {
        let data = self.to_bytes();
        compute_cid(&data)
    }

    /// Serializes this oxide to DAG-CBOR bytes.
    fn to_bytes(&self) -> Vec<u8> {
        serde_ipld_dagcbor::to_vec(self).expect("serialization should not fail")
    }

    /// Deserializes an oxide from DAG-CBOR bytes.
    fn from_bytes(
        data: &[u8],
    ) -> Result<Self, serde_ipld_dagcbor::DecodeError<std::convert::Infallible>> {
        serde_ipld_dagcbor::from_slice(data)
    }
}

// Primitive implementations

impl Oxide for bool {
    fn schema() -> Bond<Structure> {
        Bond::new(Structure::Bool)
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn dissolve_in(&self, _solvent: &Solvent) -> Self {
        *self
    }
}

impl Oxide for String {
    fn schema() -> Bond<Structure> {
        Bond::new(Structure::Unicode)
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn dissolve_in(&self, _solvent: &Solvent) -> Self {
        self.clone()
    }
}

impl Oxide for Cid {
    fn schema() -> Bond<Structure> {
        Bond::new(Structure::Cid)
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn dissolve_in(&self, _solvent: &Solvent) -> Self {
        *self
    }
}

/// A wrapper for byte sequences to distinguish from Vec<T>.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteString(pub Vec<u8>);

impl ByteString {
    pub fn new(data: Vec<u8>) -> Self {
        ByteString(data)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl From<Vec<u8>> for ByteString {
    fn from(v: Vec<u8>) -> Self {
        ByteString(v)
    }
}

impl From<&[u8]> for ByteString {
    fn from(v: &[u8]) -> Self {
        ByteString(v.to_vec())
    }
}

impl Oxide for ByteString {
    fn schema() -> Bond<Structure> {
        Bond::new(Structure::ByteString)
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn dissolve_in(&self, _solvent: &Solvent) -> Self {
        self.clone()
    }
}

macro_rules! impl_oxide_int {
    ($t:ty, $variant:ident) => {
        impl Oxide for $t {
            fn schema() -> Bond<Structure> {
                Bond::new(Structure::Int(crate::schema::IntType::$variant))
            }

            fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

            fn dissolve_in(&self, _solvent: &Solvent) -> Self {
                *self
            }
        }
    };
}

impl_oxide_int!(u8, U8);
impl_oxide_int!(u16, U16);
impl_oxide_int!(u32, U32);
impl_oxide_int!(u64, U64);
impl_oxide_int!(i8, I8);
impl_oxide_int!(i16, I16);
impl_oxide_int!(i32, I32);
impl_oxide_int!(i64, I64);

macro_rules! impl_oxide_float {
    ($t:ty, $variant:ident) => {
        impl Oxide for $t {
            fn schema() -> Bond<Structure> {
                Bond::new(Structure::Float(crate::schema::FloatType::$variant))
            }

            fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

            fn dissolve_in(&self, _solvent: &Solvent) -> Self {
                *self
            }
        }
    };
}

impl_oxide_float!(f32, F32);
impl_oxide_float!(f64, F64);

impl Oxide for () {
    fn schema() -> Bond<Structure> {
        Bond::new(Structure::Unit)
    }

    fn visit_bonds(&self, _visitor: &mut dyn BondVisitor) {}

    fn dissolve_in(&self, _solvent: &Solvent) -> Self {}
}

impl<T: Oxide> Oxide for Vec<T> {
    fn schema() -> Bond<Structure> {
        Structure::sequence(T::schema())
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        for item in self {
            item.visit_bonds(visitor);
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        self.iter().map(|item| item.dissolve_in(solvent)).collect()
    }
}

impl<T: Oxide> Oxide for Option<T> {
    fn schema() -> Bond<Structure> {
        Structure::option(T::schema())
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        if let Some(inner) = self {
            inner.visit_bonds(visitor);
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        self.as_ref().map(|inner| inner.dissolve_in(solvent))
    }
}

impl<T: Oxide, E: Oxide> Oxide for Result<T, E> {
    fn schema() -> Bond<Structure> {
        Structure::result(T::schema(), E::schema())
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        match self {
            Ok(v) => v.visit_bonds(visitor),
            Err(e) => e.visit_bonds(visitor),
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        match self {
            Ok(v) => Ok(v.dissolve_in(solvent)),
            Err(e) => Err(e.dissolve_in(solvent)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_cid_deterministic() {
        let c1 = 42u64.compute_cid();
        let c2 = 42u64.compute_cid();
        assert_eq!(c1, c2);
    }

    #[test]
    fn string_roundtrip() {
        let s = "hello world".to_string();
        let bytes = s.to_bytes();
        let recovered: String = Oxide::from_bytes(&bytes).unwrap();
        assert_eq!(s, recovered);
    }

    #[test]
    fn vec_schema() {
        let schema = <Vec<u32>>::schema();
        assert!(matches!(schema.value(), Some(Structure::Sequence(_))));
    }

    #[test]
    fn bytestring_roundtrip() {
        let bs = ByteString::new(vec![1, 2, 3, 4]);
        let bytes = bs.to_bytes();
        let recovered: ByteString = Oxide::from_bytes(&bytes).unwrap();
        assert_eq!(bs, recovered);
    }
}
