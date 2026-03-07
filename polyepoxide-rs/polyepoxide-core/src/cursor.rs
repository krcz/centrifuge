use cid::Cid;
use std::sync::Arc;

use crate::bond::{Bond, ErasedBond};
use crate::cell::Cell;
use crate::oxide::Oxide;
use crate::reflexive::resolve_ligation_bond;
use crate::solvent::Solvent;

/// Errors raised during cursor traversal.
#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("bond is unresolved: {0}")]
    Unresolved(Cid),
    #[error("ligation has no entry point")]
    EmptyLigase,
    #[error("slot out of range: {0}")]
    SlotOutOfRange(u16),
    #[error("type mismatch for CID {0}")]
    TypeMismatch(Cid),
}

/// A cursor combines a typed cell, solvent reference, and reflexive scope.
#[derive(Debug, Clone)]
pub struct Cursor<'a, T: Oxide> {
    solvent: &'a Solvent,
    cell: Arc<Cell<T>>,
    scope: Vec<ErasedBond>,
}

impl<'a, T: Oxide> Cursor<'a, T> {
    pub fn new(solvent: &'a Solvent, cell: Arc<Cell<T>>) -> Self {
        Self {
            solvent,
            cell,
            scope: Vec::new(),
        }
    }

    pub(crate) fn with_scope(
        solvent: &'a Solvent,
        cell: Arc<Cell<T>>,
        scope: Vec<ErasedBond>,
    ) -> Self {
        Self {
            solvent,
            cell,
            scope,
        }
    }

    /// Returns a reference to the underlying value.
    pub fn value(&self) -> &T {
        self.cell.value()
    }

    /// Resolves a bond in this cursor context, including reflexive ligations.
    pub fn resolve_bond<U: Oxide>(&self, bond: &Bond<U>) -> Result<Cursor<'a, U>, CursorError> {
        self.resolve_bond_in_scope(bond, self.scope.clone())
    }

    /// Follows a child bond selected from the current value.
    pub fn follow<U: Oxide>(
        &self,
        pick: impl FnOnce(&T) -> &Bond<U>,
    ) -> Result<Cursor<'a, U>, CursorError> {
        self.resolve_bond(pick(self.value()))
    }

    fn resolve_bond_in_scope<U: Oxide>(
        &self,
        bond: &Bond<U>,
        scope: Vec<ErasedBond>,
    ) -> Result<Cursor<'a, U>, CursorError> {
        let resolved = self.solvent.resolve(bond);
        match resolved {
            Bond::Link(cell) => Ok(Cursor::with_scope(self.solvent, cell, scope)),
            Bond::Unresolved(cid) => {
                if let Some(cell) = self.solvent.get::<U>(&cid) {
                    Ok(Cursor::with_scope(self.solvent, cell, scope))
                } else {
                    Err(CursorError::Unresolved(cid))
                }
            }
            Bond::Ligation(ligation) => {
                let ligation_value = *ligation;
                let Some((target, next_scope)) =
                    resolve_ligation_bond(Some(ligation_value), &scope)
                else {
                    return match bond {
                        Bond::Ligation(inner) => match &**inner {
                            crate::Ligation::Ligase(_) => Err(CursorError::EmptyLigase),
                            crate::Ligation::Slot(index) => {
                                Err(CursorError::SlotOutOfRange(*index))
                            }
                        },
                        _ => Err(CursorError::Unresolved(bond.cid())),
                    };
                };

                self.resolve_erased_target::<U>(target, next_scope)
            }
        }
    }

    fn resolve_erased_target<U: Oxide>(
        &self,
        target: ErasedBond,
        scope: Vec<ErasedBond>,
    ) -> Result<Cursor<'a, U>, CursorError> {
        match target {
            ErasedBond::Unresolved(cid) => {
                self.resolve_bond_in_scope::<U>(&Bond::from_cid(cid), scope)
            }
            ErasedBond::Ligation(ligation) => {
                self.resolve_bond_in_scope::<U>(&Bond::from_ligation(*ligation), scope)
            }
            ErasedBond::Link(cell) => {
                let cid = cell.cid();
                let Some(cell) = cell.into_any_arc().downcast::<Cell<U>>().ok() else {
                    return Err(CursorError::TypeMismatch(cid));
                };
                Ok(Cursor::with_scope(self.solvent, cell, scope))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bond, ErasedBond, Ligation, Solvent, slot_cid};

    #[crate::oxide(crate = crate)]
    struct Ring {
        name: String,
        next: Bond<Ring>,
    }

    #[test]
    fn cursor_follow_link() {
        let solvent = Solvent::new();
        let tail = solvent.add(Ring {
            name: "tail".into(),
            next: Bond::from_cid(slot_cid(0)),
        });
        let head = solvent.add(Ring {
            name: "head".into(),
            next: Bond::from_cell(tail),
        });

        let cursor = Cursor::new(&solvent, head);
        let next = cursor.follow(|r| &r.next).unwrap();
        assert_eq!(next.value().name, "tail");
    }

    #[test]
    fn cursor_resolves_unresolved_from_solvent() {
        let solvent = Solvent::new();
        let tail_value = Ring {
            name: "tail".into(),
            next: Bond::from_cid(slot_cid(0)),
        };
        let tail_cid = tail_value.compute_cid();

        // Add root first - bond remains unresolved because target is not in solvent yet.
        let root = solvent.add(Ring {
            name: "root".into(),
            next: Bond::from_cid(tail_cid),
        });

        // Add target afterwards and resolve through cursor.
        let _tail = solvent.add(tail_value);
        let cursor = Cursor::new(&solvent, root);
        let next = cursor.follow(|r| &r.next).unwrap();
        assert_eq!(next.value().name, "tail");
    }

    #[test]
    fn cursor_resolves_ligase_slot() {
        let solvent = Solvent::new();
        let a = solvent.add(Ring {
            name: "A".into(),
            next: Bond::from_cid(slot_cid(1)),
        });
        let b = solvent.add(Ring {
            name: "B".into(),
            next: Bond::from_cid(slot_cid(0)),
        });
        let root = solvent.add(Ring {
            name: "root".into(),
            next: Bond::from_ligation(Ligation::Ligase(vec![
                ErasedBond::from_cell(a.clone()),
                ErasedBond::from_cell(b.clone()),
            ])),
        });

        let cursor = Cursor::new(&solvent, root);
        let a_cursor = cursor.follow(|r| &r.next).unwrap();
        assert_eq!(a_cursor.value().name, "A");

        let b_cursor = a_cursor.follow(|r| &r.next).unwrap();
        assert_eq!(b_cursor.value().name, "B");
    }
}
