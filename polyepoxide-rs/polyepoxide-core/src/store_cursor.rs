use cid::Cid;
use ipld_core::ipld::Ipld;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

use crate::cell::Cell;
use crate::oxide::Oxide;
use crate::reflexive::{
    is_reflexive_cid, parse_ligation_bytes, reflexive_to_data_cid, resolve_ligation,
};
use crate::store::{Store, identity_overlay, key_from_cid};
use crate::{Ligation, Solvent, Structure};

use crate::export::ExportError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, crate::Oxide)]
#[oxide(crate = crate)]
pub struct CursorState {
    pub value_cid: Cid,
    pub schema_cid: Cid,
    pub scope: Vec<Cid>,
    pub schema_scope: Vec<Cid>,
}

/// A store-backed cursor for traversing oxide data without loading values into a solvent.
#[derive(Debug, Clone)]
pub struct StoreCursor<'a, S: Store + ?Sized> {
    store: &'a S,
    schemas: &'a Solvent,
    value_cid: Cid,
    schema_cid: Cid,
    scope: Vec<Cid>,
    schema_scope: Vec<Cid>,
}

impl<'a, S: Store + ?Sized> StoreCursor<'a, S> {
    pub fn new(
        store: &'a S,
        schemas: &'a Solvent,
        value_cid: Cid,
        schema_cid: Cid,
    ) -> Result<Self, ExportError<S::Error>> {
        let (value_cid, scope) = resolve_reflexive_edge(store, value_cid, &[])?;
        let (schema_cid, schema_scope) = resolve_schema_cid(store, schemas, schema_cid, &[])?;
        Ok(Self {
            store,
            schemas,
            value_cid,
            schema_cid,
            scope,
            schema_scope,
        })
    }

    pub(crate) fn with_state(
        store: &'a S,
        schemas: &'a Solvent,
        value_cid: Cid,
        schema_cid: Cid,
        scope: Vec<Cid>,
        schema_scope: Vec<Cid>,
    ) -> Self {
        Self {
            store,
            schemas,
            value_cid,
            schema_cid,
            scope,
            schema_scope,
        }
    }

    pub fn from_state(store: &'a S, schemas: &'a Solvent, state: CursorState) -> Self {
        Self::with_state(
            store,
            schemas,
            state.value_cid,
            state.schema_cid,
            state.scope,
            state.schema_scope,
        )
    }

    pub(crate) fn with_schema(&self, schema_cid: Cid, schema_scope: Vec<Cid>) -> Self {
        Self {
            store: self.store,
            schemas: self.schemas,
            value_cid: self.value_cid,
            schema_cid,
            scope: self.scope.clone(),
            schema_scope,
        }
    }

    pub fn value_cid(&self) -> Cid {
        self.value_cid
    }

    pub fn schema_cid(&self) -> Cid {
        self.schema_cid
    }

    pub fn scope(&self) -> &[Cid] {
        &self.scope
    }

    pub fn schema_scope(&self) -> &[Cid] {
        &self.schema_scope
    }

    pub(crate) fn store(&self) -> &'a S {
        self.store
    }

    pub(crate) fn schemas(&self) -> &'a Solvent {
        self.schemas
    }

    pub fn occurrence_id(&self) -> String {
        let state = self.state();
        format!("urn:px-occ:{}", state.compute_cid())
    }

    pub fn state(&self) -> CursorState {
        CursorState {
            value_cid: self.value_cid,
            schema_cid: self.schema_cid,
            scope: self.scope.clone(),
            schema_scope: self.schema_scope.clone(),
        }
    }

    pub fn schema(&self) -> Result<Arc<Cell<Structure>>, ExportError<S::Error>> {
        ensure_schema(self.store, self.schemas, self.schema_cid)
    }

    pub(crate) fn resolve_child_schema(
        &self,
        cid: Cid,
    ) -> Result<(Arc<Cell<Structure>>, Vec<Cid>), ExportError<S::Error>> {
        let (schema_cid, schema_scope) =
            resolve_schema_cid(self.store, self.schemas, cid, &self.schema_scope)?;
        let schema = ensure_schema(self.store, self.schemas, schema_cid)?;
        Ok((schema, schema_scope))
    }

    pub fn child_schema_cursor(&self, cid: Cid) -> Result<Self, ExportError<S::Error>> {
        let (schema, schema_scope) = self.resolve_child_schema(cid)?;
        Ok(self.with_schema(schema.cid(), schema_scope))
    }

    pub fn ipld(&self) -> Result<Ipld, ExportError<S::Error>> {
        let store = identity_overlay(self.store);
        let key = key_from_cid(&self.value_cid);
        let bytes = store
            .get(&key)
            .map_err(ExportError::Store)?
            .ok_or(ExportError::NotFound(self.value_cid))?;
        serde_ipld_dagcbor::from_slice(&bytes).map_err(|e| {
            ExportError::Format(format!("value parse error for {}: {}", self.value_cid, e))
        })
    }

    pub fn ligation_term(&self, cid: Cid) -> Result<Option<Ligation>, ExportError<S::Error>> {
        load_ligation(self.store, cid)
    }

    pub fn follow_bond(
        &self,
        target_cid: Cid,
        inner_schema_cid: Cid,
    ) -> Result<Self, ExportError<S::Error>> {
        let (value_cid, scope) = resolve_reflexive_edge(self.store, target_cid, &self.scope)?;
        let (schema_cid, schema_scope) = resolve_schema_cid(
            self.store,
            self.schemas,
            inner_schema_cid,
            &self.schema_scope,
        )?;
        Ok(Self::with_state(
            self.store,
            self.schemas,
            value_cid,
            schema_cid,
            scope,
            schema_scope,
        ))
    }
}

pub fn load_schema_recursive<S: Store + ?Sized>(
    store: &S,
    schemas: &Solvent,
    cid: Cid,
) -> Result<Arc<Cell<Structure>>, ExportError<S::Error>> {
    let mut visited = HashSet::new();
    load_schema_recursive_inner(store, schemas, cid, &[], &mut visited)
}

fn load_schema_recursive_inner<S: Store + ?Sized>(
    store: &S,
    schemas: &Solvent,
    cid: Cid,
    scope: &[Cid],
    visited: &mut HashSet<(Cid, Vec<Cid>)>,
) -> Result<Arc<Cell<Structure>>, ExportError<S::Error>> {
    let (resolved_cid, resolved_scope) = resolve_schema_cid(store, schemas, cid, scope)?;
    if !visited.insert((resolved_cid, resolved_scope.clone())) {
        return ensure_schema(store, schemas, resolved_cid);
    }

    let cell = ensure_schema(store, schemas, resolved_cid)?;
    match cell.value() {
        Structure::Option(inner) | Structure::Sequence(inner) | Structure::Bond(inner) => {
            let _ =
                load_schema_recursive_inner(store, schemas, inner.cid(), &resolved_scope, visited)?;
        }
        Structure::Tuple(elements) => {
            for element in elements {
                let _ = load_schema_recursive_inner(
                    store,
                    schemas,
                    element.cid(),
                    &resolved_scope,
                    visited,
                )?;
            }
        }
        Structure::Record(fields) | Structure::Tagged(fields) => {
            for bond in fields.values() {
                let _ = load_schema_recursive_inner(
                    store,
                    schemas,
                    bond.cid(),
                    &resolved_scope,
                    visited,
                )?;
            }
        }
        Structure::Map { key, value } | Structure::OrderedMap { key, value } => {
            let _ =
                load_schema_recursive_inner(store, schemas, key.cid(), &resolved_scope, visited)?;
            let _ =
                load_schema_recursive_inner(store, schemas, value.cid(), &resolved_scope, visited)?;
        }
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

    Ok(cell)
}

pub(crate) fn ensure_schema<S: Store + ?Sized>(
    store: &S,
    schemas: &Solvent,
    cid: Cid,
) -> Result<Arc<Cell<Structure>>, ExportError<S::Error>> {
    if let Some(cell) = schemas.get::<Structure>(&cid) {
        return Ok(cell);
    }

    let overlay = identity_overlay(store);
    let key = key_from_cid(&cid);
    let bytes = overlay
        .get(&key)
        .map_err(ExportError::Store)?
        .ok_or(ExportError::NotFound(cid))?;
    let schema: Structure = serde_ipld_dagcbor::from_slice(&bytes)
        .map_err(|e| ExportError::Format(format!("schema parse error for {}: {}", cid, e)))?;

    Ok(schemas.add(schema))
}

pub(crate) fn resolve_schema_cid<S: Store + ?Sized>(
    store: &S,
    schemas: &Solvent,
    cid: Cid,
    scope: &[Cid],
) -> Result<(Cid, Vec<Cid>), ExportError<S::Error>> {
    let mut resolved_cid = cid;
    let mut resolved_scope = scope.to_vec();

    while is_reflexive_cid(&resolved_cid) {
        let ligation = if crate::is_identity_cid(&resolved_cid) {
            parse_ligation_bytes(resolved_cid.hash().digest())
        } else {
            let data_cid = reflexive_to_data_cid(&resolved_cid);
            if let Some(cell) = schemas.get::<Ligation>(&data_cid) {
                Some(cell.value().clone())
            } else if let Some(ligation) = load_ligation(store, resolved_cid)? {
                let _ = schemas.add(ligation.clone());
                Some(ligation)
            } else {
                None
            }
        };
        let (next_cid, next_scope) = resolve_ligation(ligation, &resolved_scope).ok_or_else(
            || match parse_ligation_bytes(resolved_cid.hash().digest()) {
                Some(Ligation::Ligase(_)) => ExportError::EmptyLigase,
                Some(Ligation::Slot(index)) => ExportError::SlotOutOfRange(index),
                None => {
                    ExportError::Format(format!("invalid ligation payload for {}", resolved_cid))
                }
            },
        )?;
        resolved_cid = next_cid;
        resolved_scope = next_scope;
    }

    let _ = ensure_schema(store, schemas, resolved_cid)?;
    Ok((resolved_cid, resolved_scope))
}

pub(crate) fn resolve_reflexive_edge<S: Store + ?Sized>(
    store: &S,
    cid: Cid,
    scope: &[Cid],
) -> Result<(Cid, Vec<Cid>), ExportError<S::Error>> {
    if !is_reflexive_cid(&cid) {
        return Ok((cid, scope.to_vec()));
    }

    let ligation = load_ligation(store, cid)?;
    resolve_ligation(ligation, scope).ok_or_else(|| {
        match parse_ligation_bytes(cid.hash().digest()) {
            Some(Ligation::Ligase(_)) => ExportError::EmptyLigase,
            Some(Ligation::Slot(index)) => ExportError::SlotOutOfRange(index),
            None => ExportError::Format(format!("invalid ligation payload for {}", cid)),
        }
    })
}

pub(crate) fn load_ligation<S: Store + ?Sized>(
    store: &S,
    cid: Cid,
) -> Result<Option<Ligation>, ExportError<S::Error>> {
    if !is_reflexive_cid(&cid) {
        return Ok(None);
    }

    if crate::is_identity_cid(&cid) {
        return Ok(parse_ligation_bytes(cid.hash().digest()));
    }

    let overlay = identity_overlay(store);
    let data_cid = reflexive_to_data_cid(&cid);
    let key = key_from_cid(&data_cid);
    let bytes = overlay
        .get(&key)
        .map_err(ExportError::Store)?
        .ok_or(ExportError::NotFound(data_cid))?;
    Ok(parse_ligation_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::CursorState;
    use crate::Oxide;

    #[test]
    fn cursor_state_roundtrips_as_oxide() {
        let state = CursorState {
            value_cid: "bafyreibwzifm4bbm3f56fz2o7rt4fukjcm7t3qta77fxl3jh4mjqz4nq64"
                .parse()
                .unwrap(),
            schema_cid: "bafyreibwzifm4bbm3f56fz2o7rt4fukjcm7t3qta77fxl3jh4mjqz4nq64"
                .parse()
                .unwrap(),
            scope: vec![],
            schema_scope: vec![],
        };

        let bytes = state.to_bytes();
        let decoded = CursorState::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, state);
    }
}
