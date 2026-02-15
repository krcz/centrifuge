use cid::Cid;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::bond::{Bond, ErasedBond};
use crate::cell::{Cell, ErasedCell};
use crate::oxide::{BondVisitor, Oxide};
use crate::reflexive::{
    Ligation, is_identity_cid, is_reflexive_cid, parse_ligation_bytes, reflexive_to_data_cid,
};
use crate::store::{Store, identity_overlay, key_from_cid};

/// Error type for solvent operations.
#[derive(Debug, thiserror::Error)]
pub enum SolventError {
    #[error("oxide not found: {0}")]
    NotFound(Cid),
    #[error("type mismatch for CID {0}")]
    TypeMismatch(Cid),
}

/// Solvent manages oxides in memory and coordinates with backing stores.
#[derive(Debug)]
pub struct Solvent {
    cells: RwLock<HashMap<Cid, Arc<dyn ErasedCell>>>,
}

impl Solvent {
    /// Creates a new empty solvent.
    pub fn new() -> Self {
        Solvent {
            cells: RwLock::new(HashMap::new()),
        }
    }

    fn get_erased(&self, cid: &Cid) -> Option<Arc<dyn ErasedCell>> {
        self.cells.read().unwrap().get(cid).cloned()
    }

    fn all_cells(&self) -> Vec<Arc<dyn ErasedCell>> {
        self.cells.read().unwrap().values().cloned().collect()
    }

    fn resolve_ligation_by_cid(&self, cid: &Cid) -> Option<Ligation> {
        if !is_reflexive_cid(cid) {
            return None;
        }

        if is_identity_cid(cid) {
            return parse_ligation_bytes(cid.hash().digest());
        }

        let data_cid = reflexive_to_data_cid(cid);
        self.get::<Ligation>(&data_cid)
            .map(|cell| cell.value().clone())
    }

    fn add_erased_cell(&self, cell: Arc<dyn ErasedCell>) -> Arc<dyn ErasedCell> {
        let cid = cell.cid();
        if let Some(existing) = self.get_erased(&cid) {
            return existing;
        }

        let dissolved = cell.dissolve_in(self);
        let dissolved_cid = dissolved.cid();
        debug_assert_eq!(cid, dissolved_cid);

        let mut cells = self.cells.write().unwrap();
        if let Some(existing) = cells.get(&dissolved_cid) {
            return existing.clone();
        }
        cells.insert(dissolved_cid, dissolved.clone());
        dissolved
    }

    /// Adds an oxide to the solvent, returning its cell.
    pub fn add<T: Oxide>(&self, value: T) -> Arc<Cell<T>> {
        let cid = value.compute_cid();
        debug!("Adding {:?}", cid);

        if let Some(existing) = self.get::<T>(&cid) {
            return existing;
        }

        let value = value.dissolve_in(self);
        let cell = Arc::new(Cell::with_cid(value, cid));

        let mut cells = self.cells.write().unwrap();
        if let Some(existing) = cells.get(&cid) {
            if let Some(existing_typed) = downcast_cell::<T>(existing.clone()) {
                return existing_typed;
            }
        }

        cells.insert(cid, cell.clone());
        cell
    }

    fn add_and_bond<T: Oxide>(&self, value: T) -> Bond<T> {
        Bond::from_cell(self.add(value))
    }

    /// Gets an oxide by CID, if it exists and has the correct type.
    pub fn get<T: Oxide>(&self, cid: &Cid) -> Option<Arc<Cell<T>>> {
        if let Some(existing) = self.get_erased(cid) {
            if let Some(cell) = downcast_cell::<T>(existing) {
                return Some(cell);
            }
        }

        if is_identity_cid(cid) {
            let value = T::from_bytes(cid.hash().digest()).ok()?;
            return Some(Arc::new(Cell::with_cid(value, *cid)));
        }

        None
    }

    /// Checks if an oxide with the given CID exists.
    pub fn contains(&self, cid: &Cid) -> bool {
        self.cells.read().unwrap().contains_key(cid) || is_identity_cid(cid)
    }

    /// Returns the number of oxides in the solvent.
    pub fn len(&self) -> usize {
        self.cells.read().unwrap().len()
    }

    /// Returns true if the solvent is empty.
    pub fn is_empty(&self) -> bool {
        self.cells.read().unwrap().is_empty()
    }

    /// Creates a resolved bond for the given value.
    pub fn bond<T: Oxide>(&self, value: T) -> Bond<T> {
        self.add_and_bond(value)
    }

    /// Adds the target of the provided bond into this solvent.
    pub fn add_bond<T: Oxide>(&self, bond: &Bond<T>) -> Bond<T> {
        match bond {
            Bond::Unresolved(cid) => {
                if let Some(ligation) = self.resolve_ligation_by_cid(cid) {
                    Bond::Ligation(Box::new(ligation))
                } else if let Some(cell) = self.get::<T>(cid) {
                    Bond::Link(cell)
                } else {
                    Bond::Unresolved(*cid)
                }
            }
            Bond::Link(cell) => Bond::Link(self.add(cell.value().clone())),
            Bond::Ligation(ligation) => {
                let dissolved = ligation.dissolve_in(self);
                let _ = self.add(dissolved.clone());
                Bond::Ligation(Box::new(dissolved))
            }
        }
    }

    /// Adds the target of the provided erased bond into this solvent.
    pub fn add_erased_bond(&self, bond: &ErasedBond) -> ErasedBond {
        match bond {
            ErasedBond::Unresolved(cid) => {
                if let Some(ligation) = self.resolve_ligation_by_cid(cid) {
                    ErasedBond::Ligation(Box::new(ligation))
                } else if let Some(cell) = self.get_erased(cid) {
                    ErasedBond::Link(cell)
                } else {
                    ErasedBond::Unresolved(*cid)
                }
            }
            ErasedBond::Link(cell) => ErasedBond::Link(self.add_erased_cell(cell.clone())),
            ErasedBond::Ligation(ligation) => {
                let dissolved = ligation.dissolve_in(self);
                let _ = self.add(dissolved.clone());
                ErasedBond::Ligation(Box::new(dissolved))
            }
        }
    }

    /// Attempts to resolve an unresolved bond.
    pub fn resolve<T: Oxide>(&self, bond: &Bond<T>) -> Bond<T> {
        match bond {
            Bond::Unresolved(cid) => {
                if let Some(ligation) = self.resolve_ligation_by_cid(cid) {
                    Bond::Ligation(Box::new(ligation))
                } else if let Some(cell) = self.get::<T>(cid) {
                    Bond::Link(cell)
                } else {
                    Bond::Unresolved(*cid)
                }
            }
            Bond::Link(_) | Bond::Ligation(_) => bond.clone(),
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
        let store = identity_overlay(store);
        let mut visited = HashSet::new();

        // Persist schema tree using a temporary solvent.
        let schemas = Solvent::new();
        let schema_root = schemas.add_bond(&T::schema());
        let schema_cid = schema_root.cid();

        for schema_erased in schemas.all_cells() {
            let cid = schema_erased.cid();
            if !visited.insert(cid) {
                continue;
            }
            let key = key_from_cid(&cid);
            store.put(&key, &schema_erased.to_bytes())?;
        }

        // Persist value graph from root.
        self.persist_typed_value(cell.value(), &store, &mut visited)?;

        Ok((cell.cid(), schema_cid))
    }

    fn persist_typed_value<T: Oxide, S: Store>(
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

        let mut collector = CidCollector::default();
        value.visit_bonds(&mut collector);
        for dep in collector.cids {
            if let Some(dep_cell) = self.get_erased(&dep) {
                self.persist_erased_cell(dep_cell, store, visited)?;
            }
        }

        let key = key_from_cid(&cid);
        store.put(&key, &value.to_bytes())?;
        Ok(())
    }

    fn persist_erased_cell<S: Store>(
        &self,
        cell: Arc<dyn ErasedCell>,
        store: &S,
        visited: &mut HashSet<Cid>,
    ) -> Result<(), S::Error> {
        let cid = cell.cid();
        if visited.contains(&cid) {
            return Ok(());
        }
        visited.insert(cid);

        let mut collector = CidCollector::default();
        cell.visit_bonds(&mut collector);
        for dep in collector.cids {
            if let Some(dep_cell) = self.get_erased(&dep) {
                self.persist_erased_cell(dep_cell, store, visited)?;
            }
        }

        let key = key_from_cid(&cid);
        store.put(&key, &cell.to_bytes())?;
        Ok(())
    }
}

impl Default for Solvent {
    fn default() -> Self {
        Self::new()
    }
}

fn downcast_cell<T: Oxide>(cell: Arc<dyn ErasedCell>) -> Option<Arc<Cell<T>>> {
    cell.into_any_arc().downcast::<Cell<T>>().ok()
}

#[derive(Default)]
struct CidCollector {
    cids: Vec<Cid>,
}

impl BondVisitor for CidCollector {
    fn visit_bond(&mut self, cid: &Cid) {
        self.cids.push(*cid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::compute_cid;
    use crate::schema::Structure;

    #[test]
    fn solvent_add_and_get() {
        let solvent = Solvent::new();
        let value = "hello".to_string();
        let cell = solvent.add(value.clone());

        assert_eq!(cell.value(), &value);
        assert_eq!(solvent.len(), 1);

        let retrieved = solvent.get::<String>(&cell.cid()).unwrap();
        assert_eq!(retrieved.value(), &value);
    }

    #[test]
    fn solvent_deduplication() {
        let solvent = Solvent::new();
        let value = "duplicate".to_string();

        let cell1 = solvent.add(value.clone());
        let cell2 = solvent.add(value.clone());

        assert_eq!(cell1.cid(), cell2.cid());
        assert!(Arc::ptr_eq(&cell1, &cell2));
        assert_eq!(solvent.len(), 1);
    }

    #[test]
    fn solvent_different_values() {
        let solvent = Solvent::new();

        solvent.add("one".to_string());
        solvent.add("two".to_string());
        solvent.add(42u64);

        assert_eq!(solvent.len(), 3);
    }

    #[test]
    fn solvent_bond_creation() {
        let solvent = Solvent::new();
        let bond = solvent.bond("bonded value".to_string());

        assert!(bond.is_resolved());
        assert_eq!(bond.value(), Some(&"bonded value".to_string()));
    }

    #[test]
    fn solvent_resolve_existing() {
        let solvent = Solvent::new();
        let cell = solvent.add("target".to_string());
        let cid = cell.cid();

        let unresolved: Bond<String> = Bond::from_cid(cid);
        assert!(!unresolved.is_resolved());

        let resolved = solvent.resolve(&unresolved);
        assert!(matches!(resolved, Bond::Link(_)));
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
    fn solvent_resolve_identity_virtual_ligation() {
        let solvent = Solvent::new();
        let slot_cid = crate::slot_cid(2);
        let unresolved: Bond<String> = Bond::from_cid(slot_cid);

        let resolved = solvent.resolve(&unresolved);
        assert!(matches!(
            resolved,
            Bond::Ligation(ref ligation) if **ligation == crate::Ligation::Slot(2)
        ));
    }

    #[test]
    fn solvent_recursive_add_structure() {
        let solvent = Solvent::new();

        let nested = Structure::sequence(Structure::Unicode);
        let cell = solvent.add(nested);

        assert_eq!(solvent.len(), 3);

        let Bond::Link(struct_cell) = cell.value() else {
            panic!("Expected root schema link");
        };
        let Structure::Sequence(inner_bond) = struct_cell.value() else {
            panic!("Expected Sequence");
        };
        assert!(inner_bond.is_resolved());
        let inner_cid = inner_bond.cid();
        assert!(solvent.contains(&inner_cid));
    }

    #[test]
    fn solvent_deduplication_nested() {
        let solvent = Solvent::new();

        let s1 = Structure::sequence(Structure::Unicode);
        let s2 = Structure::sequence(Structure::Unicode);

        solvent.add(s1);
        solvent.add(s2);

        assert_eq!(solvent.len(), 3);
    }

    #[test]
    fn solvent_deep_nesting() {
        let solvent = Solvent::new();

        let deep = Structure::sequence(Structure::sequence(Structure::sequence(Structure::Bool)));

        solvent.add(deep);

        assert_eq!(solvent.len(), 5);
    }
}
