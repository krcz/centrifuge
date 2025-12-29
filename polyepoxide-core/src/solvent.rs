use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::bond::Bond;
use crate::cell::Cell;
use crate::key::Key;
use crate::oxide::{BondMapper, Oxide};

/// Error type for solvent operations.
#[derive(Debug, thiserror::Error)]
pub enum SolventError {
    #[error("oxide not found: {0}")]
    NotFound(Key),
    #[error("type mismatch for key {0}")]
    TypeMismatch(Key),
}

/// Solvent manages oxides in memory and coordinates with backing stores.
///
/// Responsibilities:
/// - Deduplication: identical oxides share the same cell
/// - Type-erased storage: stores heterogeneous oxide types
///
/// When adding an oxide, all nested bond targets are also added to the solvent,
/// achieving deduplication of shared sub-structures.
///
/// Future: will coordinate with disk/remote stores for loading.
pub struct Solvent {
    cells: HashMap<Key, Arc<dyn Any + Send + Sync>>,
}

impl Solvent {
    /// Creates a new empty solvent.
    pub fn new() -> Self {
        Solvent {
            cells: HashMap::new(),
        }
    }

    /// Adds an oxide to the solvent, returning its cell.
    ///
    /// If an oxide with the same key already exists, returns the existing cell.
    /// All nested bonds are recursively added to the solvent, achieving
    /// deduplication of shared sub-structures.
    pub fn add<T: Oxide>(&mut self, value: T) -> Arc<Cell<T>> {
        // Compute key first - this is the same whether bonds are resolved or not,
        // since bonds serialize to just their key
        let key = value.compute_key();

        // Check if already exists - return existing cell
        if let Some(existing) = self.cells.get(&key) {
            if let Some(cell) = existing.clone().downcast::<Cell<T>>().ok() {
                return cell;
            }
            // Type mismatch - this shouldn't happen with correct usage
            // but we handle it by inserting the new value anyway
        }

        // Recursively add all nested bond targets to the solvent
        let value = value.map_bonds(&mut SolventBondMapper { solvent: self });

        // Create and store the cell
        let cell = Arc::new(Cell::with_key(value, key));
        self.cells.insert(key, cell.clone());
        cell
    }

    /// Adds an oxide and returns a resolved bond to it.
    fn add_and_bond<T: Oxide>(&mut self, value: T) -> Bond<T> {
        let cell = self.add(value);
        Bond::from_cell(cell)
    }

    /// Gets an oxide by key, if it exists and has the correct type.
    pub fn get<T: Oxide>(&self, key: &Key) -> Option<Arc<Cell<T>>> {
        self.cells
            .get(key)
            .and_then(|any| any.clone().downcast::<Cell<T>>().ok())
    }

    /// Checks if an oxide with the given key exists.
    pub fn contains(&self, key: &Key) -> bool {
        self.cells.contains_key(key)
    }

    /// Returns the number of oxides in the solvent.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Returns true if the solvent is empty.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Creates a resolved bond for the given value.
    ///
    /// Adds the value (and all nested bonds) to the solvent and returns
    /// a bond pointing to it.
    pub fn bond<T: Oxide>(&mut self, value: T) -> Bond<T> {
        self.add_and_bond(value)
    }

    /// Attempts to resolve an unresolved bond.
    ///
    /// If the target exists in the solvent, returns a resolved bond.
    /// Otherwise returns the original unresolved bond.
    pub fn resolve<T: Oxide>(&self, bond: &Bond<T>) -> Bond<T> {
        match bond {
            Bond::Resolved(_) => bond.clone(),
            Bond::Unresolved(key) => {
                if let Some(cell) = self.get::<T>(key) {
                    Bond::Resolved(cell)
                } else {
                    bond.clone()
                }
            }
        }
    }
}

impl Default for Solvent {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal bond mapper that recursively adds bond targets to the solvent.
struct SolventBondMapper<'a> {
    solvent: &'a mut Solvent,
}

impl BondMapper for SolventBondMapper<'_> {
    fn map_bond<T: Oxide>(&mut self, bond: Bond<T>) -> Bond<T> {
        match bond {
            Bond::Unresolved(key) => {
                // Unresolved bond - keep as-is (can't access the value)
                Bond::Unresolved(key)
            }
            Bond::Resolved(cell) => {
                // Resolved bond - recursively add the value to the solvent
                // The value's bonds will also be recursively processed
                let value = cell.value().clone();
                self.solvent.add_and_bond(value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Structure;

    #[test]
    fn solvent_add_and_get() {
        let mut solvent = Solvent::new();
        let value = "hello".to_string();
        let cell = solvent.add(value.clone());

        assert_eq!(cell.value(), &value);
        assert_eq!(solvent.len(), 1);

        // Get by key
        let retrieved = solvent.get::<String>(&cell.key()).unwrap();
        assert_eq!(retrieved.value(), &value);
    }

    #[test]
    fn solvent_deduplication() {
        let mut solvent = Solvent::new();
        let value = "duplicate".to_string();

        let cell1 = solvent.add(value.clone());
        let cell2 = solvent.add(value.clone());

        // Same key, same cell (Arc pointer equality)
        assert_eq!(cell1.key(), cell2.key());
        assert!(Arc::ptr_eq(&cell1, &cell2));
        assert_eq!(solvent.len(), 1);
    }

    #[test]
    fn solvent_different_values() {
        let mut solvent = Solvent::new();

        solvent.add("one".to_string());
        solvent.add("two".to_string());
        solvent.add(42u64);

        assert_eq!(solvent.len(), 3);
    }

    #[test]
    fn solvent_bond_creation() {
        let mut solvent = Solvent::new();
        let bond = solvent.bond("bonded value".to_string());

        assert!(bond.is_resolved());
        assert_eq!(bond.value(), Some(&"bonded value".to_string()));
    }

    #[test]
    fn solvent_resolve_existing() {
        let mut solvent = Solvent::new();
        let cell = solvent.add("target".to_string());
        let key = cell.key();

        let unresolved: Bond<String> = Bond::from_key(key);
        assert!(!unresolved.is_resolved());

        let resolved = solvent.resolve(&unresolved);
        assert!(resolved.is_resolved());
        assert_eq!(resolved.value(), Some(&"target".to_string()));
    }

    #[test]
    fn solvent_resolve_missing() {
        let solvent = Solvent::new();
        let fake_key = Key::from_data(b"nonexistent");
        let unresolved: Bond<String> = Bond::from_key(fake_key);

        let still_unresolved = solvent.resolve(&unresolved);
        assert!(!still_unresolved.is_resolved());
    }

    #[test]
    fn solvent_recursive_add_structure() {
        let mut solvent = Solvent::new();

        // Create a nested structure: Sequence(Unicode)
        let nested = Structure::sequence(Structure::Unicode);

        // Add it to the solvent
        let cell = solvent.add(nested);

        // Should have 2 entries: the Sequence and the Unicode inside
        assert_eq!(solvent.len(), 2);

        // The inner Unicode should also be in the solvent
        if let Structure::Sequence(inner_bond) = cell.value() {
            assert!(inner_bond.is_resolved());
            let inner_key = inner_bond.key();
            assert!(solvent.contains(&inner_key));
        } else {
            panic!("Expected Sequence");
        }
    }

    #[test]
    fn solvent_deduplication_nested() {
        let mut solvent = Solvent::new();

        // Create two structures that share the same inner type
        let s1 = Structure::sequence(Structure::Unicode);
        let s2 = Structure::sequence(Structure::Unicode);

        solvent.add(s1);
        solvent.add(s2);

        // Should have 2 entries: one Sequence and one Unicode (shared)
        assert_eq!(solvent.len(), 2);
    }

    #[test]
    fn solvent_deep_nesting() {
        let mut solvent = Solvent::new();

        // Create a deeply nested structure
        let deep = Structure::sequence(Structure::sequence(Structure::sequence(Structure::Bool)));

        solvent.add(deep);

        // Should have 4 entries: 3 Sequences and 1 Bool
        assert_eq!(solvent.len(), 4);
    }
}
