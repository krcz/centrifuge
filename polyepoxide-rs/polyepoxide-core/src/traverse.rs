//! Traversal utilities for schema-aware IPLD exploration.
//!
//! Provides low-level functions for parsing DAG-CBOR data and extracting
//! bond references using schema information.

use cid::Cid;
use ipld_core::ipld::Ipld;

use crate::{Solvent, Structure};

/// Error during IPLD parsing.
#[derive(Debug, thiserror::Error)]
#[error("parse error: {0}")]
pub struct ParseError(String);

/// Parse DAG-CBOR bytes into IPLD.
pub fn parse_to_ipld(bytes: &[u8]) -> Result<Ipld, ParseError> {
    serde_ipld_dagcbor::from_slice(bytes).map_err(|e| ParseError(e.to_string()))
}

/// Extract bond targets from an IPLD value given its schema.
///
/// Appends (value_cid, schema_cid) pairs to `bonds`.
/// Silently skips malformed data - we only care about finding valid bonds.
pub fn collect_bonds(
    value: &Ipld,
    schema: &Structure,
    schemas: &Solvent,
    bonds: &mut Vec<(Cid, Cid)>,
) {
    match schema {
        Structure::Bond(inner_schema) => {
            // In DAG-CBOR, CIDs are represented as Ipld::Link
            if let Ipld::Link(target_cid) = value {
                bonds.push((*target_cid, inner_schema.cid()));
            }
        }
        Structure::Record(fields) => {
            if let Ipld::Map(map) = value {
                for (name, field_schema_bond) in fields {
                    if let Some(fv) = map.get(name) {
                        if let Some(s) = field_schema_bond.value() {
                            collect_bonds(fv, s, schemas, bonds);
                        }
                    }
                }
            }
        }
        Structure::Sequence(inner) => {
            if let Ipld::List(arr) = value {
                if let Some(inner_schema) = inner.value() {
                    for elem in arr {
                        collect_bonds(elem, inner_schema, schemas, bonds);
                    }
                }
            }
        }
        Structure::Tuple(elems) => {
            if let Ipld::List(arr) = value {
                for (elem_schema_bond, elem_val) in elems.iter().zip(arr.iter()) {
                    if let Some(s) = elem_schema_bond.value() {
                        collect_bonds(elem_val, s, schemas, bonds);
                    }
                }
            }
        }
        Structure::Tagged(variants) => {
            if let Ipld::Map(map) = value {
                if map.len() == 1 {
                    if let Some((name, val)) = map.iter().next() {
                        if let Some(variant_schema_bond) = variants.get(name) {
                            if let Some(s) = variant_schema_bond.value() {
                                collect_bonds(val, s, schemas, bonds);
                            }
                        }
                    }
                }
            }
        }
        Structure::Map { key: k, value: v } | Structure::OrderedMap { key: k, value: v } => {
            if let Ipld::Map(map) = value {
                if let (Some(ks), Some(vs)) = (k.value(), v.value()) {
                    for (mk, mv) in map {
                        // In IPLD maps, keys are strings, not arbitrary values
                        // For now we only recurse into values
                        collect_bonds(mv, vs, schemas, bonds);
                        let _ = (ks, mk); // Acknowledge unused for now
                    }
                }
            }
        }
        _ => {}
    }
}
