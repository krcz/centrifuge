use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::cell::Cell;
use crate::key::Key;
use crate::oxide::{BondMapper, BondVisitor, Oxide};
use crate::schema::Structure;

/// A typed reference from one oxide to another.
///
/// Bonds exist in two states:
/// - **Unresolved**: Contains only the target's key (after deserialization)
/// - **Resolved**: Points to a Cell containing the value (after loading)
///
/// When serialized, bonds always emit just the key.
#[derive(Debug)]
pub enum Bond<T: Oxide> {
    /// Unresolved reference - contains only the key.
    Unresolved(Key),
    /// Resolved reference - points to a cell with the value.
    Resolved(Arc<Cell<T>>),
}

impl<T: Oxide> Bond<T> {
    /// Creates a new resolved bond with an ephemeral cell.
    /// The cell is not added to any Solvent - use this for building structures.
    pub fn new(value: T) -> Self {
        Bond::Resolved(Arc::new(Cell::new(value)))
    }

    /// Creates a new unresolved bond from a key.
    pub fn from_key(key: Key) -> Self {
        Bond::Unresolved(key)
    }

    /// Creates a new resolved bond from a cell.
    pub fn from_cell(cell: Arc<Cell<T>>) -> Self {
        Bond::Resolved(cell)
    }

    /// Returns the key of the referenced oxide.
    pub fn key(&self) -> Key {
        match self {
            Bond::Unresolved(key) => *key,
            Bond::Resolved(cell) => cell.key(),
        }
    }

    /// Returns true if this bond is resolved.
    pub fn is_resolved(&self) -> bool {
        matches!(self, Bond::Resolved(_))
    }

    /// Returns the resolved cell, if available.
    pub fn cell(&self) -> Option<&Arc<Cell<T>>> {
        match self {
            Bond::Unresolved(_) => None,
            Bond::Resolved(cell) => Some(cell),
        }
    }

    /// Returns a reference to the value if resolved.
    pub fn value(&self) -> Option<&T> {
        self.cell().map(|c| c.value())
    }
}

impl<T: Oxide> Clone for Bond<T> {
    fn clone(&self) -> Self {
        match self {
            Bond::Unresolved(key) => Bond::Unresolved(*key),
            Bond::Resolved(cell) => Bond::Resolved(Arc::clone(cell)),
        }
    }
}

impl<T: Oxide> Serialize for Bond<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Always serialize as just the key (32 bytes)
        self.key().serialize(serializer)
    }
}

impl<'de, T: Oxide> Deserialize<'de> for Bond<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as key, creating an unresolved bond
        let key = Key::deserialize(deserializer)?;
        Ok(Bond::Unresolved(key))
    }
}

impl<T: Oxide> Oxide for Bond<T> {
    fn schema() -> Structure {
        Structure::bond(T::schema())
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        visitor.visit_bond(&self.key());
        // If resolved, also visit bonds within the target value
        if let Some(value) = self.value() {
            value.visit_bonds(visitor);
        }
    }

    fn map_bonds(&self, mapper: &mut impl BondMapper) -> Self {
        // Delegate to the mapper, which can access the bond's value
        mapper.map_bond(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bond_unresolved() {
        let key = Key::from_data(b"test");
        let bond: Bond<String> = Bond::from_key(key);
        assert!(!bond.is_resolved());
        assert_eq!(bond.key(), key);
        assert!(bond.value().is_none());
    }

    #[test]
    fn bond_resolved() {
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
        let key = bond.key();

        let bytes = bond.to_bytes();
        let recovered: Bond<String> = Bond::from_bytes(&bytes).unwrap();

        // After deserialization, bond is unresolved but has same key
        assert!(!recovered.is_resolved());
        assert_eq!(recovered.key(), key);
    }
}
