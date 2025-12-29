use cid::Cid;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::bond::Bond;
use crate::cell::Cell;
use crate::oxide::{BondMapper, Oxide};
use crate::schema::Structure;
use crate::store::Store;

/// Error type for solvent operations.
#[derive(Debug, thiserror::Error)]
pub enum SolventError {
    #[error("oxide not found: {0}")]
    NotFound(Cid),
    #[error("type mismatch for CID {0}")]
    TypeMismatch(Cid),
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
    cells: HashMap<Cid, Arc<dyn Any + Send + Sync>>,
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
    /// If an oxide with the same CID already exists, returns the existing cell.
    /// All nested bonds are recursively added to the solvent, achieving
    /// deduplication of shared sub-structures.
    pub fn add<T: Oxide>(&mut self, value: T) -> Arc<Cell<T>> {
        // Compute CID first - this is the same whether bonds are resolved or not,
        // since bonds serialize to just their CID
        let cid = value.compute_cid();

        // Check if already exists - return existing cell
        if let Some(existing) = self.cells.get(&cid) {
            if let Some(cell) = existing.clone().downcast::<Cell<T>>().ok() {
                return cell;
            }
            // Type mismatch - this shouldn't happen with correct usage
            // but we handle it by inserting the new value anyway
        }

        // Recursively add all nested bond targets to the solvent
        let value = value.map_bonds(&mut SolventBondMapper { solvent: self });

        // Create and store the cell
        let cell = Arc::new(Cell::with_cid(value, cid));
        self.cells.insert(cid, cell.clone());
        cell
    }

    /// Adds an oxide and returns a resolved bond to it.
    fn add_and_bond<T: Oxide>(&mut self, value: T) -> Bond<T> {
        let cell = self.add(value);
        Bond::from_cell(cell)
    }

    /// Gets an oxide by CID, if it exists and has the correct type.
    pub fn get<T: Oxide>(&self, cid: &Cid) -> Option<Arc<Cell<T>>> {
        self.cells
            .get(cid)
            .and_then(|any| any.clone().downcast::<Cell<T>>().ok())
    }

    /// Checks if an oxide with the given CID exists.
    pub fn contains(&self, cid: &Cid) -> bool {
        self.cells.contains_key(cid)
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
            Bond::Unresolved(cid) => {
                if let Some(cell) = self.get::<T>(cid) {
                    Bond::Resolved(cell)
                } else {
                    bond.clone()
                }
            }
        }
    }

    /// Persists a cell and all its transitive bond dependencies to a store.
    ///
    /// Also persists the schema tree for the value's type.
    /// Returns the value CID and schema CID.
    pub fn persist_cell<T: Oxide, S: Store>(
        &self,
        cell: &Cell<T>,
        store: &S,
    ) -> Result<(Cid, Cid), S::Error> {
        let mut visited = HashSet::new();

        // Persist the schema tree first
        // Use a temporary solvent to resolve schema bonds
        let mut schema_solvent = Solvent::new();
        let schema = T::schema();
        let schema_cell = schema_solvent.add(schema);
        let schema_cid = schema_cell.cid();

        // Persist all schemas from the solvent
        for (cid, any_cell) in &schema_solvent.cells {
            if let Some(structure_cell) = any_cell.clone().downcast::<Cell<Structure>>().ok() {
                let bytes = structure_cell.value().to_bytes();
                store.put(cid, &bytes)?;
                visited.insert(*cid);
            }
        }

        // Persist the value and all bond dependencies
        self.persist_value(cell.value(), store, &mut visited)?;

        Ok((cell.cid(), schema_cid))
    }

    /// Persists a value and all its bond dependencies.
    /// Uses dependency-first order: children are stored before parents.
    fn persist_value<T: Oxide, S: Store>(
        &self,
        value: &T,
        store: &S,
        visited: &mut HashSet<Cid>,
    ) -> Result<(), S::Error> {
        let cid = value.compute_cid();
        if visited.contains(&cid) {
            return Ok(());
        }
        visited.insert(cid);

        // First persist all bond dependencies (children before parent)
        let mut mapper = PersistingMapper {
            solvent: self,
            store,
            visited,
            error: None,
        };
        value.map_bonds(&mut mapper);

        if let Some(e) = mapper.error {
            return Err(e);
        }

        // Then persist this value
        let bytes = value.to_bytes();
        store.put(&cid, &bytes)?;

        Ok(())
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
            Bond::Unresolved(cid) => {
                // Try to resolve by looking up in the solvent
                if let Some(cell) = self.solvent.get::<T>(&cid) {
                    Bond::from_cell(cell)
                } else {
                    // Not found - keep as unresolved
                    Bond::Unresolved(cid)
                }
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

/// Bond mapper that persists bond targets to a store.
struct PersistingMapper<'a, S: Store> {
    solvent: &'a Solvent,
    store: &'a S,
    visited: &'a mut HashSet<Cid>,
    error: Option<S::Error>,
}

impl<S: Store> BondMapper for PersistingMapper<'_, S> {
    fn map_bond<T: Oxide>(&mut self, bond: Bond<T>) -> Bond<T> {
        if self.error.is_some() {
            return bond;
        }

        let cid = bond.cid();
        if self.visited.contains(&cid) {
            return bond;
        }
        self.visited.insert(cid);

        // Get the cell from solvent and persist it
        if let Some(cell) = self.solvent.get::<T>(&cid) {
            let bytes = cell.value().to_bytes();
            if let Err(e) = self.store.put(&cid, &bytes) {
                self.error = Some(e);
                return bond;
            }

            // Recursively persist nested bonds
            cell.value().map_bonds(self);
        }

        bond
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::compute_cid;
    use crate::schema::Structure;

    #[test]
    fn solvent_add_and_get() {
        let mut solvent = Solvent::new();
        let value = "hello".to_string();
        let cell = solvent.add(value.clone());

        assert_eq!(cell.value(), &value);
        assert_eq!(solvent.len(), 1);

        // Get by CID
        let retrieved = solvent.get::<String>(&cell.cid()).unwrap();
        assert_eq!(retrieved.value(), &value);
    }

    #[test]
    fn solvent_deduplication() {
        let mut solvent = Solvent::new();
        let value = "duplicate".to_string();

        let cell1 = solvent.add(value.clone());
        let cell2 = solvent.add(value.clone());

        // Same CID, same cell (Arc pointer equality)
        assert_eq!(cell1.cid(), cell2.cid());
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
        let cid = cell.cid();

        let unresolved: Bond<String> = Bond::from_cid(cid);
        assert!(!unresolved.is_resolved());

        let resolved = solvent.resolve(&unresolved);
        assert!(resolved.is_resolved());
        assert_eq!(resolved.value(), Some(&"target".to_string()));
    }

    #[test]
    fn solvent_resolve_missing() {
        let solvent = Solvent::new();
        let fake_cid = compute_cid(b"nonexistent");
        let unresolved: Bond<String> = Bond::from_cid(fake_cid);

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
            let inner_cid = inner_bond.cid();
            assert!(solvent.contains(&inner_cid));
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
