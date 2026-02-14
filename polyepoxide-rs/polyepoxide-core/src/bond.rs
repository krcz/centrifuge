use cid::Cid;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::cell::{Cell, ErasedCell};
use crate::oxide::{BondVisitor, Oxide};
use crate::reflexive::{Ligation, ligation_cid};
use crate::schema::Structure;
use crate::solvent::Solvent;

/// A typed reference from one oxide to another.
#[derive(Debug)]
pub enum Bond<T: Oxide> {
    /// Unresolved reference - contains only the CID.
    Unresolved(Cid),
    /// Resolved reference - points to a cell with the value. Corresponds to dag-cbor CIDs.
    Link(Arc<Cell<T>>),
    /// Resolved reference - used to describe cyclic data. Corresponds to polyepoxide-reflexive CIDs.
    Ligation(Box<Ligation>),
}

impl<T: Oxide> Bond<T> {
    /// Creates a new resolved bond with an ephemeral cell.
    /// The cell is not added to any Solvent - use this for building structures.
    pub fn new(value: T) -> Self {
        Bond::Link(Arc::new(Cell::new(value)))
    }

    /// Creates a new unresolved bond from a CID.
    pub fn from_cid(cid: Cid) -> Self {
        Bond::Unresolved(cid)
    }

    /// Creates a new resolved bond from a cell.
    pub fn from_cell(cell: Arc<Cell<T>>) -> Self {
        Bond::Link(cell)
    }

    /// Creates a resolved ligation bond.
    pub fn from_ligation(ligation: Ligation) -> Self {
        Bond::Ligation(Box::new(ligation))
    }

    /// Returns the CID of the referenced oxide.
    pub fn cid(&self) -> Cid {
        match self {
            Bond::Unresolved(cid) => *cid,
            Bond::Link(cell) => cell.cid(),
            Bond::Ligation(ligation) => ligation_cid(ligation),
        }
    }

    /// Returns true if this bond is resolved.
    pub fn is_resolved(&self) -> bool {
        !matches!(self, Bond::Unresolved(_))
    }

    /// Returns the resolved cell, if available.
    pub fn cell(&self) -> Option<&Arc<Cell<T>>> {
        match self {
            Bond::Unresolved(_) | Bond::Ligation(_) => None,
            Bond::Link(cell) => Some(cell),
        }
    }

    /// Returns a reference to the value if this is a typed link.
    pub fn value(&self) -> Option<&T> {
        self.cell().map(|c| c.value())
    }
}

impl<T: Oxide> Clone for Bond<T> {
    fn clone(&self) -> Self {
        match self {
            Bond::Unresolved(cid) => Bond::Unresolved(*cid),
            Bond::Link(cell) => Bond::Link(Arc::clone(cell)),
            Bond::Ligation(ligation) => Bond::Ligation(ligation.clone()),
        }
    }
}

impl<T: Oxide> Serialize for Bond<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.cid().serialize(serializer)
    }
}

impl<'de, T: Oxide> Deserialize<'de> for Bond<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cid = Cid::deserialize(deserializer)?;
        Ok(Bond::Unresolved(cid))
    }
}

impl<T: Oxide> Oxide for Bond<T> {
    fn schema() -> Structure {
        Structure::bond(T::schema())
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        visitor.visit_bond(&self.cid());
        match self {
            Bond::Link(cell) => cell.value().visit_bonds(visitor),
            Bond::Ligation(ligation) => ligation.visit_bonds(visitor),
            Bond::Unresolved(_) => {}
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        solvent.add_bond(self)
    }
}

/// A type-erased alternative to [`Bond`].
#[derive(Debug)]
pub enum ErasedBond {
    Unresolved(Cid),
    Link(Arc<dyn ErasedCell>),
    Ligation(Box<Ligation>),
}

impl PartialEq for ErasedBond {
    fn eq(&self, other: &Self) -> bool {
        self.cid() == other.cid()
    }
}

impl Eq for ErasedBond {}

impl ErasedBond {
    pub fn from_cid(cid: Cid) -> Self {
        ErasedBond::Unresolved(cid)
    }

    pub fn from_cell(cell: Arc<dyn ErasedCell>) -> Self {
        ErasedBond::Link(cell)
    }

    pub fn from_ligation(ligation: Ligation) -> Self {
        ErasedBond::Ligation(Box::new(ligation))
    }

    pub fn cid(&self) -> Cid {
        match self {
            ErasedBond::Unresolved(cid) => *cid,
            ErasedBond::Link(cell) => cell.cid(),
            ErasedBond::Ligation(ligation) => ligation_cid(ligation),
        }
    }

    pub fn cell(&self) -> Option<&Arc<dyn ErasedCell>> {
        match self {
            ErasedBond::Link(cell) => Some(cell),
            ErasedBond::Unresolved(_) | ErasedBond::Ligation(_) => None,
        }
    }

    pub fn is_resolved(&self) -> bool {
        !matches!(self, ErasedBond::Unresolved(_))
    }
}

impl Clone for ErasedBond {
    fn clone(&self) -> Self {
        match self {
            ErasedBond::Unresolved(cid) => ErasedBond::Unresolved(*cid),
            ErasedBond::Link(cell) => ErasedBond::Link(Arc::clone(cell)),
            ErasedBond::Ligation(ligation) => ErasedBond::Ligation(ligation.clone()),
        }
    }
}

impl Serialize for ErasedBond {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.cid().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ErasedBond {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cid = Cid::deserialize(deserializer)?;
        Ok(ErasedBond::Unresolved(cid))
    }
}

impl Oxide for ErasedBond {
    fn schema() -> Structure {
        Structure::Cid
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        visitor.visit_bond(&self.cid());
        match self {
            ErasedBond::Link(cell) => cell.visit_bonds(visitor),
            ErasedBond::Ligation(ligation) => ligation.visit_bonds(visitor),
            ErasedBond::Unresolved(_) => {}
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        solvent.add_erased_bond(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::compute_cid;

    #[test]
    fn bond_unresolved() {
        let cid = compute_cid(b"test");
        let bond: Bond<String> = Bond::from_cid(cid);
        assert!(!bond.is_resolved());
        assert_eq!(bond.cid(), cid);
        assert!(bond.value().is_none());
    }

    #[test]
    fn bond_link() {
        let value = "hello".to_string();
        let cell = Arc::new(Cell::new(value.clone()));
        let bond = Bond::from_cell(cell);
        assert!(bond.is_resolved());
        assert_eq!(bond.value(), Some(&value));
    }

    #[test]
    fn bond_serialization_roundtrip() {
        let value = "test".to_string();
        let cell = Arc::new(Cell::new(value));
        let bond = Bond::from_cell(cell);
        let cid = bond.cid();

        let bytes = bond.to_bytes();
        let recovered: Bond<String> = Bond::from_bytes(&bytes).unwrap();

        assert!(!recovered.is_resolved());
        assert_eq!(recovered.cid(), cid);
    }

    #[test]
    fn erased_bond_roundtrip_is_unresolved() {
        let cid = compute_cid(b"erased");
        let bond = ErasedBond::from_cid(cid);
        let bytes = bond.to_bytes();
        let recovered = ErasedBond::from_bytes(&bytes).unwrap();
        assert!(matches!(recovered, ErasedBond::Unresolved(found) if found == cid));
    }
}
