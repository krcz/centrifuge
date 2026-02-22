use cid::Cid;
use serde::{Deserialize, Serialize};

use crate::bond::{Bond, ErasedBond};
use crate::cell::Cell;
use crate::oxide::{BondVisitor, Oxide};
use crate::schema::Structure;
use crate::solvent::Solvent;

/// A schema-carrying erased bond.
///
/// `schema` describes the value referenced by `bond`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynBond {
    pub schema: Bond<Structure>,
    pub bond: ErasedBond,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DynBondError {
    #[error("schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: Cid, found: Cid },
    #[error("type mismatch for CID {0}")]
    TypeMismatch(Cid),
}

impl DynBond {
    pub fn new(schema: Bond<Structure>, bond: ErasedBond) -> Self {
        Self { schema, bond }
    }

    pub fn from_typed<T: Oxide>(bond: Bond<T>) -> Self {
        Self {
            schema: T::schema(),
            bond: bond.erased(),
        }
    }

    pub fn cid(&self) -> Cid {
        self.bond.cid()
    }

    pub fn schema_cid(&self) -> Cid {
        self.schema.cid()
    }

    pub fn resolve(&self, solvent: &Solvent) -> Self {
        Self {
            schema: solvent.add_bond(&self.schema),
            bond: solvent.add_erased_bond(&self.bond),
        }
    }

    pub fn resolve_schema(&self, solvent: &Solvent) -> Bond<Structure> {
        solvent.add_bond(&self.schema)
    }

    pub fn resolve_bond(&self, solvent: &Solvent) -> ErasedBond {
        solvent.add_erased_bond(&self.bond)
    }

    pub fn matches_schema<T: Oxide>(&self) -> bool {
        self.schema_cid() == T::schema().cid()
    }

    pub fn resolve_as<T: Oxide>(&self, solvent: &Solvent) -> Result<Bond<T>, DynBondError> {
        let expected = T::schema().cid();
        let found = self.schema_cid();
        if expected != found {
            return Err(DynBondError::SchemaMismatch { expected, found });
        }

        let resolved = solvent.add_erased_bond(&self.bond);
        match resolved {
            ErasedBond::Unresolved(cid) => {
                if let Some(cell) = solvent.get::<T>(&cid) {
                    Ok(Bond::from_cell(cell))
                } else {
                    Ok(Bond::Unresolved(cid))
                }
            }
            ErasedBond::Link(cell) => {
                let cid = cell.cid();
                let typed = cell
                    .into_any_arc()
                    .downcast::<Cell<T>>()
                    .map_err(|_| DynBondError::TypeMismatch(cid))?;
                Ok(Bond::from_cell(typed))
            }
            ErasedBond::Ligation(ligation) => Ok(Bond::Ligation(ligation)),
        }
    }
}

impl Oxide for DynBond {
    fn schema() -> Bond<Structure> {
        Structure::record([
            ("schema", <Bond<Structure> as Oxide>::schema()),
            ("bond", ErasedBond::schema()),
        ])
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        self.schema.visit_bonds(visitor);
        self.bond.visit_bonds(visitor);
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        Self {
            schema: self.schema.dissolve_in(solvent),
            bond: self.bond.dissolve_in(solvent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oxide::compute_cid;

    #[test]
    fn from_typed_resolves_back_to_typed() {
        let solvent = Solvent::new();
        let dyn_bond = DynBond::from_typed(Bond::new("hello".to_string()));

        let resolved = dyn_bond.resolve_as::<String>(&solvent).unwrap();
        assert_eq!(resolved.value(), Some(&"hello".to_string()));
        assert!(dyn_bond.matches_schema::<String>());
    }

    #[test]
    fn resolve_as_reports_schema_mismatch() {
        let solvent = Solvent::new();
        let dyn_bond = DynBond::from_typed(Bond::new("hello".to_string()));

        let err = dyn_bond.resolve_as::<u64>(&solvent).unwrap_err();
        assert!(matches!(err, DynBondError::SchemaMismatch { .. }));
    }

    #[test]
    fn unresolved_bond_stays_unresolved_when_value_missing() {
        let solvent = Solvent::new();
        let cid = compute_cid(b"missing");
        let dyn_bond = DynBond::new(String::schema(), ErasedBond::from_cid(cid));

        let resolved = dyn_bond.resolve_as::<String>(&solvent).unwrap();
        assert!(matches!(resolved, Bond::Unresolved(found) if found == cid));
    }

    #[test]
    fn resolve_as_reports_type_mismatch_for_incorrect_link_type() {
        let solvent = Solvent::new();
        let wrong = Bond::new(42u64);
        let dyn_bond = DynBond::new(String::schema(), wrong.erased());

        let err = dyn_bond.resolve_as::<String>(&solvent).unwrap_err();
        assert!(matches!(err, DynBondError::TypeMismatch(_)));
    }
}
