//! JSON/YAML export with $ref for bonds.

use cid::Cid;
use ipld_core::ipld::Ipld;
use polyepoxide_core::traverse::parse_to_ipld;
use polyepoxide_core::{Solvent, Store, Structure};
use serde_json::{Map, Number, Value as JsonValue};

use crate::store::AnyStore;

/// Export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Yaml,
}

/// Export options.
pub struct ExportOptions {
    /// Maximum depth to expand bonds (0 = only $ref).
    pub depth: usize,
    /// Whether to pretty print.
    pub pretty: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            depth: 2,
            pretty: true,
        }
    }
}

/// Export a value to JSON or YAML.
pub fn export(
    store: &AnyStore,
    schemas: &Solvent,
    cid: Cid,
    schema_cid: Cid,
    format: ExportFormat,
    options: &ExportOptions,
) -> Result<String, Box<dyn std::error::Error>> {
    let json = export_to_json(store, schemas, cid, schema_cid, options.depth)?;

    match format {
        ExportFormat::Json => {
            if options.pretty {
                Ok(serde_json::to_string_pretty(&json)?)
            } else {
                Ok(serde_json::to_string(&json)?)
            }
        }
        ExportFormat::Yaml => Ok(serde_yaml::to_string(&json)?),
    }
}

fn export_to_json(
    store: &AnyStore,
    schemas: &Solvent,
    cid: Cid,
    schema_cid: Cid,
    depth: usize,
) -> Result<JsonValue, Box<dyn std::error::Error>> {
    let bytes = store
        .get(&cid)?
        .ok_or_else(|| format!("value not found: {}", cid))?;

    let ipld = parse_to_ipld(&bytes)?;

    let schema_cell = schemas
        .get::<Structure>(&schema_cid)
        .ok_or_else(|| format!("schema not found: {}", schema_cid))?;

    ipld_to_json(store, schemas, &ipld, schema_cell.value(), depth)
}

fn ipld_to_json(
    store: &AnyStore,
    schemas: &Solvent,
    ipld: &Ipld,
    schema: &Structure,
    depth: usize,
) -> Result<JsonValue, Box<dyn std::error::Error>> {
    match (ipld, schema) {
        (Ipld::Link(target_cid), Structure::Bond(inner)) => {
            if depth == 0 {
                // Just output $ref
                let mut obj = Map::new();
                obj.insert("$ref".to_string(), JsonValue::String(target_cid.to_string()));
                Ok(JsonValue::Object(obj))
            } else {
                // Expand the bond
                if let Ok(Some(target_bytes)) = store.get(target_cid) {
                    if let Ok(target_ipld) = parse_to_ipld(&target_bytes) {
                        if let Some(inner_schema) = inner.value() {
                            let mut result =
                                ipld_to_json(store, schemas, &target_ipld, inner_schema, depth - 1)?;
                            // Add $ref as metadata for expanded objects
                            if let JsonValue::Object(ref mut obj) = result {
                                obj.insert(
                                    "$ref".to_string(),
                                    JsonValue::String(target_cid.to_string()),
                                );
                            }
                            return Ok(result);
                        }
                    }
                }
                // Fallback to just $ref
                let mut obj = Map::new();
                obj.insert("$ref".to_string(), JsonValue::String(target_cid.to_string()));
                Ok(JsonValue::Object(obj))
            }
        }
        (Ipld::Map(map), Structure::Record(fields)) => {
            let mut obj = Map::new();
            for (name, field_schema_bond) in fields {
                if let Some(fv) = map.get(name) {
                    if let Some(field_schema) = field_schema_bond.value() {
                        let json_val = ipld_to_json(store, schemas, fv, field_schema, depth)?;
                        obj.insert(name.clone(), json_val);
                    }
                }
            }
            Ok(JsonValue::Object(obj))
        }
        (Ipld::List(arr), Structure::Sequence(inner)) => {
            if let Some(inner_schema) = inner.value() {
                let items: Result<Vec<_>, _> = arr
                    .iter()
                    .map(|elem| ipld_to_json(store, schemas, elem, inner_schema, depth))
                    .collect();
                Ok(JsonValue::Array(items?))
            } else {
                Ok(ipld_to_json_raw(ipld))
            }
        }
        (Ipld::List(arr), Structure::Tuple(elems)) => {
            let items: Result<Vec<_>, _> = arr
                .iter()
                .zip(elems.iter())
                .map(|(v, s)| {
                    if let Some(schema) = s.value() {
                        ipld_to_json(store, schemas, v, schema, depth)
                    } else {
                        Ok(ipld_to_json_raw(v))
                    }
                })
                .collect();
            Ok(JsonValue::Array(items?))
        }
        (Ipld::Map(map), Structure::Tagged(variants)) => {
            if map.len() == 1 {
                if let Some((name, val)) = map.iter().next() {
                    if let Some(variant_schema_bond) = variants.get(name) {
                        if let Some(variant_schema) = variant_schema_bond.value() {
                            let mut obj = Map::new();
                            let json_val =
                                ipld_to_json(store, schemas, val, variant_schema, depth)?;
                            obj.insert(name.clone(), json_val);
                            return Ok(JsonValue::Object(obj));
                        }
                    }
                }
            }
            Ok(ipld_to_json_raw(ipld))
        }
        (Ipld::Map(map), Structure::Map { value: v, .. })
        | (Ipld::Map(map), Structure::OrderedMap { value: v, .. }) => {
            if let Some(vs) = v.value() {
                let mut obj = Map::new();
                for (mk, mv) in map {
                    let json_val = ipld_to_json(store, schemas, mv, vs, depth)?;
                    obj.insert(mk.clone(), json_val);
                }
                Ok(JsonValue::Object(obj))
            } else {
                Ok(ipld_to_json_raw(ipld))
            }
        }
        _ => Ok(ipld_to_json_raw(ipld)),
    }
}

fn ipld_to_json_raw(ipld: &Ipld) -> JsonValue {
    match ipld {
        Ipld::Null => JsonValue::Null,
        Ipld::Bool(b) => JsonValue::Bool(*b),
        Ipld::Integer(n) => {
            // i128 doesn't implement Into<Number>, so we need to convert via i64/u64
            if *n >= 0 {
                Number::from(*n as u64).into()
            } else {
                Number::from(*n as i64).into()
            }
        }
        Ipld::Float(f) => Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Ipld::String(s) => JsonValue::String(s.clone()),
        Ipld::Bytes(b) => {
            // Encode as base64
            use base64::{engine::general_purpose::STANDARD, Engine};
            JsonValue::String(STANDARD.encode(b))
        }
        Ipld::List(arr) => JsonValue::Array(arr.iter().map(ipld_to_json_raw).collect()),
        Ipld::Map(map) => {
            let obj: Map<String, JsonValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), ipld_to_json_raw(v)))
                .collect();
            JsonValue::Object(obj)
        }
        Ipld::Link(cid) => {
            let mut obj = Map::new();
            obj.insert("$ref".to_string(), JsonValue::String(cid.to_string()));
            JsonValue::Object(obj)
        }
    }
}
