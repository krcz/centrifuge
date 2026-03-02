use cid::Cid;
use indexmap::IndexMap;
use ipld_core::ipld::Ipld;
use serde_json::Value as JsonValue;
use serde_yaml_bw::Value as YamlValue;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::bond::Bond;
use crate::cell::Cell;
use crate::oxide::{Oxide, compute_cid};
use crate::reflexive::{
    Ligation, is_identity_cid, is_reflexive_cid, ligation_cid, parse_ligation_bytes,
    reflexive_to_data_cid, resolve_ligation,
};
use crate::schema::Structure;
use crate::store::{Store, key_from_cid};
use crate::{ErasedBond, Solvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Yaml,
    JsonLd,
    YamlLd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Lenient,
    Faithful,
    Canonical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    pub mode: ImportMode,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            mode: ImportMode::Lenient,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("YAML parse error: {0}")]
    Yaml(String),
    #[error("invalid document: {0}")]
    Invalid(String),
    #[error("duplicate occurrence reference: {0}")]
    DuplicateReference(String),
    #[error("missing occurrence reference: {0}")]
    MissingReference(String),
    #[error("profile violation: {0}")]
    ProfileViolation(String),
    #[error("occurrence cycle requires explicit ligation: {0}")]
    CyclicHydratedValue(String),
    #[error(
        "CID mismatch between stored link and hydrated value: expected {expected}, got {actual}"
    )]
    CidMismatch { expected: Cid, actual: Cid },
    #[error("schema resolution failed: {0}")]
    Schema(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("store error: {0}")]
    Store(String),
}

#[derive(Debug, Clone)]
struct InputNode {
    anchor: Option<String>,
    kind: InputKind,
}

#[derive(Debug, Clone)]
enum InputKind {
    Null,
    Bool(bool),
    Integer(i128),
    Float(f64),
    String(String),
    Array(Vec<InputNode>),
    Object(IndexMap<String, InputNode>),
    Alias(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RefName {
    Anchor(String),
    Id(String),
}

#[derive(Debug, Clone)]
struct OccurrenceDef {
    node: InputNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SchemaState {
    schema_cid: Cid,
    scope: Vec<Cid>,
}

#[derive(Debug, Clone)]
struct MaterializedNode {
    cid: Cid,
}

struct ImportEnv<'a, S: Store + ?Sized> {
    mode: ImportMode,
    store: &'a S,
    schemas: &'a Solvent,
    occurrences: Vec<OccurrenceDef>,
    names: HashMap<RefName, usize>,
    cache: HashMap<(usize, SchemaState), MaterializedNode>,
    in_progress: HashSet<(usize, SchemaState)>,
}

const VALUE_KEY: &str = "$value";
const LINK_KEY: &str = "$link";
const LIGATION_KEY: &str = "$ligation";
const SCHEMA_KEY: &str = "$schema";

#[derive(Clone)]
struct SchemaCursor<'a> {
    solvent: &'a Solvent,
    cell: Arc<Cell<Structure>>,
    scope: Vec<Cid>,
}

impl<'a> SchemaCursor<'a> {
    fn from_cid(solvent: &'a Solvent, schema_cid: Cid) -> Result<Self, ImportError> {
        let (cell, scope) = resolve_schema_cid(solvent, schema_cid, &[])?;
        Ok(Self {
            solvent,
            cell,
            scope,
        })
    }

    fn from_state(solvent: &'a Solvent, state: &SchemaState) -> Result<Self, ImportError> {
        let (cell, scope) = resolve_schema_cid(solvent, state.schema_cid, &state.scope)?;
        Ok(Self {
            solvent,
            cell,
            scope,
        })
    }

    fn structure(&self) -> &Structure {
        self.cell.value()
    }

    fn state(&self) -> SchemaState {
        SchemaState {
            schema_cid: self.cell.cid(),
            scope: self.scope.clone(),
        }
    }

    fn child(&self, bond: &Bond<Structure>) -> Result<Self, ImportError> {
        let (cell, scope) = resolve_schema_bond(self.solvent, bond, &self.scope)?;
        Ok(Self {
            solvent: self.solvent,
            cell,
            scope,
        })
    }
}

pub fn import<S: Store + ?Sized>(
    input: &str,
    format: ImportFormat,
    schema_cid: Cid,
    store: &S,
    schemas: &Solvent,
    options: &ImportOptions,
) -> Result<Cid, ImportError> {
    let root = parse_input(input, format)?;
    let mut occurrences = Vec::new();
    let mut names = HashMap::new();
    collect_occurrences(&root, &mut occurrences, &mut names)?;

    let schema_cursor = SchemaCursor::from_cid(schemas, schema_cid)?;
    let mut env = ImportEnv {
        mode: options.mode,
        store,
        schemas,
        occurrences,
        names,
        cache: HashMap::new(),
        in_progress: HashSet::new(),
    };

    import_root_bond(&root, &schema_cursor, &mut env)
}

fn parse_input(input: &str, format: ImportFormat) -> Result<InputNode, ImportError> {
    match format {
        ImportFormat::JsonLd => {
            let value: JsonValue =
                serde_json::from_str(input).map_err(|e| ImportError::Json(e.to_string()))?;
            input_from_json(value)
        }
        ImportFormat::Yaml | ImportFormat::YamlLd => {
            let value = serde_yaml_bw::from_str_value_preserve(input)
                .map_err(|e| ImportError::Yaml(e.to_string()))?;
            input_from_yaml(value)
        }
    }
}

fn input_from_json(value: JsonValue) -> Result<InputNode, ImportError> {
    Ok(InputNode {
        anchor: None,
        kind: match value {
            JsonValue::Null => InputKind::Null,
            JsonValue::Bool(value) => InputKind::Bool(value),
            JsonValue::Number(value) => {
                if let Some(value) = value.as_i64() {
                    InputKind::Integer(i128::from(value))
                } else if let Some(value) = value.as_u64() {
                    InputKind::Integer(i128::from(value))
                } else if let Some(value) = value.as_f64() {
                    InputKind::Float(value)
                } else {
                    return Err(ImportError::Invalid("unsupported JSON number".to_string()));
                }
            }
            JsonValue::String(value) => InputKind::String(value),
            JsonValue::Array(values) => {
                let mut out = Vec::with_capacity(values.len());
                for value in values {
                    out.push(input_from_json(value)?);
                }
                InputKind::Array(out)
            }
            JsonValue::Object(map) => {
                let mut out = IndexMap::new();
                for (key, value) in map {
                    out.insert(key, input_from_json(value)?);
                }
                InputKind::Object(out)
            }
        },
    })
}

fn input_from_yaml(value: YamlValue) -> Result<InputNode, ImportError> {
    match value {
        YamlValue::Tagged(tagged) => input_from_yaml(tagged.value),
        YamlValue::Alias(name) => Ok(InputNode {
            anchor: None,
            kind: InputKind::Alias(name),
        }),
        YamlValue::Null(anchor) => Ok(InputNode {
            anchor,
            kind: InputKind::Null,
        }),
        YamlValue::Bool(value, anchor) => Ok(InputNode {
            anchor,
            kind: InputKind::Bool(value),
        }),
        YamlValue::Number(value, anchor) => {
            let kind = if let Some(value) = value.as_i64() {
                InputKind::Integer(i128::from(value))
            } else if let Some(value) = value.as_u64() {
                InputKind::Integer(i128::from(value))
            } else if let Some(value) = value.as_f64() {
                InputKind::Float(value)
            } else {
                return Err(ImportError::Invalid("unsupported YAML number".to_string()));
            };
            Ok(InputNode { anchor, kind })
        }
        YamlValue::String(value, anchor) => Ok(InputNode {
            anchor,
            kind: InputKind::String(value),
        }),
        YamlValue::Sequence(sequence) => {
            let mut out = Vec::with_capacity(sequence.elements.len());
            for value in sequence.elements {
                out.push(input_from_yaml(value)?);
            }
            Ok(InputNode {
                anchor: sequence.anchor,
                kind: InputKind::Array(out),
            })
        }
        YamlValue::Mapping(map) => {
            let mut out = IndexMap::new();
            for (key, value) in map.iter() {
                let key = scalar_key_to_string(key)?;
                out.insert(key, input_from_yaml(value.clone())?);
            }
            Ok(InputNode {
                anchor: map.anchor.clone(),
                kind: InputKind::Object(out),
            })
        }
    }
}

fn scalar_key_to_string(value: &YamlValue) -> Result<String, ImportError> {
    match value {
        YamlValue::String(value, _) => Ok(value.clone()),
        YamlValue::Number(value, _) => {
            if let Some(value) = value.as_i64() {
                Ok(value.to_string())
            } else if let Some(value) = value.as_u64() {
                Ok(value.to_string())
            } else if let Some(value) = value.as_f64() {
                Ok(value.to_string())
            } else {
                Err(ImportError::Invalid("unsupported YAML map key".to_string()))
            }
        }
        YamlValue::Bool(value, _) => Ok(value.to_string()),
        YamlValue::Null(_) => Ok("null".to_string()),
        YamlValue::Tagged(tagged) => scalar_key_to_string(&tagged.value),
        other => Err(ImportError::Invalid(format!(
            "unsupported YAML map key: {:?}",
            other
        ))),
    }
}

fn collect_occurrences(
    node: &InputNode,
    occurrences: &mut Vec<OccurrenceDef>,
    names: &mut HashMap<RefName, usize>,
) -> Result<(), ImportError> {
    let mut local_names = Vec::new();
    if let Some(anchor) = &node.anchor {
        local_names.push(RefName::Anchor(anchor.clone()));
    }
    if let Some(id) = occurrence_id(node)? {
        local_names.push(RefName::Id(id));
    }

    if !local_names.is_empty() {
        let index = occurrences.len();
        occurrences.push(OccurrenceDef { node: node.clone() });
        for name in local_names {
            if let Some(existing) = names.insert(name.clone(), index) {
                if existing != index {
                    return Err(ImportError::DuplicateReference(match name {
                        RefName::Anchor(name) => format!("anchor:{name}"),
                        RefName::Id(name) => format!("id:{name}"),
                    }));
                }
            }
        }
    }

    match &node.kind {
        InputKind::Array(values) => {
            for value in values {
                collect_occurrences(value, occurrences, names)?;
            }
        }
        InputKind::Object(map) => {
            for value in map.values() {
                collect_occurrences(value, occurrences, names)?;
            }
        }
        InputKind::Null
        | InputKind::Bool(_)
        | InputKind::Integer(_)
        | InputKind::Float(_)
        | InputKind::String(_)
        | InputKind::Alias(_) => {}
    }

    Ok(())
}

fn materialize_occurrence<S: Store + ?Sized>(
    node: &InputNode,
    schema: &SchemaCursor<'_>,
    env: &mut ImportEnv<'_, S>,
) -> Result<MaterializedNode, ImportError> {
    if let Some(index) = occurrence_index(node, env)? {
        let state = schema.state();
        if let Some(existing) = env.cache.get(&(index, state.clone())) {
            return Ok(existing.clone());
        }
        if !env.in_progress.insert((index, state.clone())) {
            return Err(ImportError::CyclicHydratedValue(
                schema.state().schema_cid.to_string(),
            ));
        }

        let node = env.occurrences[index].node.clone();
        let materialized = materialize_inline(occurrence_data(&node), schema, env)?;
        env.cache
            .insert((index, state.clone()), materialized.clone());
        env.in_progress.remove(&(index, state));
        return Ok(materialized);
    }

    materialize_inline(occurrence_data(node), schema, env)
}

fn materialize_inline<S: Store + ?Sized>(
    node: &InputNode,
    schema: &SchemaCursor<'_>,
    env: &mut ImportEnv<'_, S>,
) -> Result<MaterializedNode, ImportError> {
    let ipld = import_data(node, schema, env)?;
    let bytes =
        serde_ipld_dagcbor::to_vec(&ipld).map_err(|e| ImportError::Decode(e.to_string()))?;
    let cid = compute_cid(&bytes);
    env.store
        .put(&key_from_cid(&cid), &bytes)
        .map_err(|e| ImportError::Store(e.to_string()))?;
    Ok(MaterializedNode { cid })
}

fn import_data<S: Store + ?Sized>(
    node: &InputNode,
    schema: &SchemaCursor<'_>,
    env: &mut ImportEnv<'_, S>,
) -> Result<Ipld, ImportError> {
    match schema.structure() {
        Structure::Bool => match &node.kind {
            InputKind::Bool(value) => Ok(Ipld::Bool(*value)),
            _ => Err(type_mismatch("bool", node)),
        },
        Structure::Char | Structure::Unicode => match &node.kind {
            InputKind::String(value) => Ok(Ipld::String(value.clone())),
            _ => Err(type_mismatch("string", node)),
        },
        Structure::ByteString => match &node.kind {
            InputKind::String(value) => {
                use base64::{Engine, engine::general_purpose::STANDARD};
                let bytes = STANDARD
                    .decode(value)
                    .map_err(|e| ImportError::Invalid(format!("invalid base64 bytes: {}", e)))?;
                Ok(Ipld::Bytes(bytes))
            }
            _ => Err(type_mismatch("base64 string", node)),
        },
        Structure::Cid => match &node.kind {
            InputKind::String(value) => Ok(Ipld::Link(parse_cid(value)?)),
            _ => Err(type_mismatch("CID string", node)),
        },
        Structure::Int(_) => match &node.kind {
            InputKind::Integer(value) => Ok(Ipld::Integer(*value)),
            _ => Err(type_mismatch("integer", node)),
        },
        Structure::Float(_) => match &node.kind {
            InputKind::Float(value) => Ok(Ipld::Float(*value)),
            InputKind::Integer(value) => Ok(Ipld::Float(*value as f64)),
            _ => Err(type_mismatch("float", node)),
        },
        Structure::Unit => match &node.kind {
            InputKind::Null => Ok(Ipld::Null),
            _ => Err(type_mismatch("null", node)),
        },
        Structure::Sequence(inner) => {
            let child = schema.child(inner)?;
            match &node.kind {
                InputKind::Array(values) => {
                    let mut out = Vec::with_capacity(values.len());
                    for value in values {
                        out.push(import_data(value, &child, env)?);
                    }
                    Ok(Ipld::List(out))
                }
                _ => Err(type_mismatch("array", node)),
            }
        }
        Structure::Tuple(elements) => match &node.kind {
            InputKind::Array(values) => {
                let mut out = Vec::with_capacity(values.len());
                for (value, bond) in values.iter().zip(elements.iter()) {
                    let child = schema.child(bond)?;
                    out.push(import_data(value, &child, env)?);
                }
                Ok(Ipld::List(out))
            }
            _ => Err(type_mismatch("array", node)),
        },
        Structure::Record(fields) => match &node.kind {
            InputKind::Object(map) => {
                let mut out = BTreeMap::new();
                for (name, bond) in fields {
                    if let Some(value) = map.get(name) {
                        let child = schema.child(bond)?;
                        out.insert(name.clone(), import_data(value, &child, env)?);
                    }
                }
                Ok(Ipld::Map(out))
            }
            _ => Err(type_mismatch("object", node)),
        },
        Structure::Tagged(variants) => match &node.kind {
            InputKind::Object(map) => {
                if let Some((name, value)) = first_non_meta(map) {
                    let bond = variants.get(name).ok_or_else(|| {
                        ImportError::Invalid(format!("unknown tagged variant: {}", name))
                    })?;
                    let child = schema.child(bond)?;
                    let mut out = BTreeMap::new();
                    out.insert(name.clone(), import_data(value, &child, env)?);
                    Ok(Ipld::Map(out))
                } else {
                    Err(ImportError::Invalid(
                        "tagged value missing variant".to_string(),
                    ))
                }
            }
            InputKind::String(name) => {
                let bond = variants.get(name).ok_or_else(|| {
                    ImportError::Invalid(format!("unknown tagged variant: {}", name))
                })?;
                let child = schema.child(bond)?;
                if matches!(child.structure(), Structure::Unit) {
                    Ok(Ipld::String(name.clone()))
                } else {
                    Err(type_mismatch("tagged object", node))
                }
            }
            _ => Err(type_mismatch("tagged object", node)),
        },
        Structure::Enum(variants) => match &node.kind {
            InputKind::String(name) => {
                variants
                    .iter()
                    .find(|variant| *variant == name)
                    .ok_or_else(|| {
                        ImportError::Invalid(format!("unknown enum variant: {}", name))
                    })?;
                Ok(Ipld::String(name.clone()))
            }
            InputKind::Integer(index) => {
                let name = variants
                    .get(usize::try_from(*index).map_err(|_| {
                        ImportError::Invalid(format!("unknown enum variant index: {}", index))
                    })?)
                    .ok_or_else(|| {
                        ImportError::Invalid(format!("unknown enum variant index: {}", index))
                    })?;
                Ok(Ipld::String(name.clone()))
            }
            _ => Err(type_mismatch("enum string", node)),
        },
        Structure::Map { .. } => match &node.kind {
            InputKind::Object(map) => {
                let mut out = BTreeMap::new();
                for (key, value) in map {
                    if key.starts_with('@') {
                        continue;
                    }
                    if key == "id" || key == "data" {
                        continue;
                    }
                    out.insert(key.clone(), raw_ipld_from_value(value)?);
                }
                Ok(Ipld::Map(out))
            }
            _ => Err(type_mismatch("object map", node)),
        },
        Structure::OrderedMap { key, value } => match &node.kind {
            InputKind::Array(entries) => {
                let key_schema = schema.child(key)?;
                let value_schema = schema.child(value)?;
                let mut out = Vec::with_capacity(entries.len());
                for entry in entries {
                    match &entry.kind {
                        InputKind::Array(pair) if pair.len() == 2 => out.push(Ipld::List(vec![
                            import_data(&pair[0], &key_schema, env)?,
                            import_data(&pair[1], &value_schema, env)?,
                        ])),
                        _ => {
                            return Err(ImportError::Invalid(
                                "ordered map entries must be two-element arrays".to_string(),
                            ));
                        }
                    }
                }
                Ok(Ipld::List(out))
            }
            _ => Err(type_mismatch("ordered map array", node)),
        },
        Structure::Bond(inner) => import_bond(node, schema, inner, env),
    }
}

fn import_bond<S: Store + ?Sized>(
    node: &InputNode,
    schema: &SchemaCursor<'_>,
    inner: &Bond<Structure>,
    env: &mut ImportEnv<'_, S>,
) -> Result<Ipld, ImportError> {
    let envelope = match &node.kind {
        InputKind::Object(map)
            if map.contains_key(LINK_KEY)
                || map.contains_key(LIGATION_KEY)
                || map.contains_key(VALUE_KEY) =>
        {
            Some(map)
        }
        _ => None,
    };

    let link = envelope.and_then(|map| map.get(LINK_KEY));
    let ligation = envelope.and_then(|map| map.get(LIGATION_KEY));
    let value = envelope.and_then(|map| map.get(VALUE_KEY));

    if link.is_some() && ligation.is_some() {
        return Err(ImportError::Invalid(
            "bond envelope cannot contain both $link and $ligation".to_string(),
        ));
    }

    if matches!(env.mode, ImportMode::Canonical) && ligation.is_some() && value.is_some() {
        return Err(ImportError::ProfileViolation(
            "canonical import forbids hydrated value on ligation bonds".to_string(),
        ));
    }

    let inner_schema = schema.child(inner)?;

    if let Some(link) = link {
        let InputKind::String(link) = &link.kind else {
            return Err(type_mismatch("CID string", link));
        };
        let cid = parse_cid(link)?;
        if let Some(value) = value {
            let hydrated = materialize_occurrence(value, &inner_schema, env)?;
            if hydrated.cid != cid {
                return Err(ImportError::CidMismatch {
                    expected: cid,
                    actual: hydrated.cid,
                });
            }
        }
        return Ok(Ipld::Link(cid));
    }

    if let Some(ligation) = ligation {
        let ligation = parse_ligation(ligation, env, Some(inner_schema.state()))?;
        if let Some(value) = value {
            match materialize_occurrence(value, &inner_schema, env) {
                Ok(_) | Err(ImportError::CyclicHydratedValue(_)) => {}
                Err(err) => return Err(err),
            }
        }
        store_ligation(env.store, &ligation)?;
        return Ok(Ipld::Link(ligation_cid(&ligation)));
    }

    let value = value.unwrap_or(node);

    if matches!(env.mode, ImportMode::Faithful | ImportMode::Canonical) {
        return Err(ImportError::ProfileViolation(
            "import mode requires $link or $ligation on every bond".to_string(),
        ));
    }

    let hydrated = materialize_occurrence(value, &inner_schema, env)?;
    Ok(Ipld::Link(hydrated.cid))
}

fn import_root_bond<S: Store + ?Sized>(
    node: &InputNode,
    schema: &SchemaCursor<'_>,
    env: &mut ImportEnv<'_, S>,
) -> Result<Cid, ImportError> {
    let envelope = match &node.kind {
        InputKind::Object(map)
            if map.contains_key(LINK_KEY)
                || map.contains_key(LIGATION_KEY)
                || map.contains_key(VALUE_KEY) =>
        {
            Some(map)
        }
        _ => None,
    };

    let link = envelope.and_then(|map| map.get(LINK_KEY));
    let ligation = envelope.and_then(|map| map.get(LIGATION_KEY));
    let value = envelope.and_then(|map| map.get(VALUE_KEY));

    if link.is_some() && ligation.is_some() {
        return Err(ImportError::Invalid(
            "bond envelope cannot contain both $link and $ligation".to_string(),
        ));
    }

    if matches!(env.mode, ImportMode::Canonical) && ligation.is_some() && value.is_some() {
        return Err(ImportError::ProfileViolation(
            "canonical import forbids hydrated value on ligation bonds".to_string(),
        ));
    }

    if let Some(link) = link {
        let InputKind::String(link) = &link.kind else {
            return Err(type_mismatch("CID string", link));
        };
        let cid = parse_cid(link)?;
        if let Some(value) = value {
            let hydrated = materialize_occurrence(value, schema, env)?;
            if hydrated.cid != cid {
                return Err(ImportError::CidMismatch {
                    expected: cid,
                    actual: hydrated.cid,
                });
            }
        }
        return Ok(cid);
    }

    if let Some(ligation) = ligation {
        let ligation = parse_ligation(ligation, env, Some(schema.state()))?;
        if let Some(value) = value {
            match materialize_occurrence(value, schema, env) {
                Ok(_) | Err(ImportError::CyclicHydratedValue(_)) => {}
                Err(err) => return Err(err),
            }
        }
        store_ligation(env.store, &ligation)?;
        return Ok(ligation_cid(&ligation));
    }

    let value = value.unwrap_or(node);

    if matches!(env.mode, ImportMode::Faithful | ImportMode::Canonical) {
        return Err(ImportError::ProfileViolation(
            "import mode requires $link or $ligation on every bond".to_string(),
        ));
    }

    let hydrated = materialize_occurrence(value, schema, env)?;
    Ok(hydrated.cid)
}

fn raw_ipld_from_value(node: &InputNode) -> Result<Ipld, ImportError> {
    match &node.kind {
        InputKind::Null => Ok(Ipld::Null),
        InputKind::Bool(value) => Ok(Ipld::Bool(*value)),
        InputKind::Integer(value) => Ok(Ipld::Integer(*value)),
        InputKind::Float(value) => Ok(Ipld::Float(*value)),
        InputKind::String(value) => Ok(Ipld::String(value.clone())),
        InputKind::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for value in values {
                out.push(raw_ipld_from_value(value)?);
            }
            Ok(Ipld::List(out))
        }
        InputKind::Object(map) => {
            let mut out = BTreeMap::new();
            for (key, value) in map {
                if key.starts_with('@') {
                    continue;
                }
                out.insert(key.clone(), raw_ipld_from_value(value)?);
            }
            Ok(Ipld::Map(out))
        }
        InputKind::Alias(name) => Err(ImportError::MissingReference(name.clone())),
    }
}

fn parse_ligation<S: Store + ?Sized>(
    node: &InputNode,
    env: &mut ImportEnv<'_, S>,
    default_schema: Option<SchemaState>,
) -> Result<Ligation, ImportError> {
    let InputKind::Object(map) = &node.kind else {
        return Err(ImportError::Invalid(
            "ligation must be an object".to_string(),
        ));
    };

    if let Some(slot) = map.get("Slot") {
        let index = match &slot.kind {
            InputKind::Integer(value) if *value >= 0 && *value <= i128::from(u16::MAX) => {
                *value as u16
            }
            _ => return Err(type_mismatch("Slot integer", slot)),
        };
        return Ok(Ligation::Slot(index));
    }

    if let Some(args) = map.get("Ligase") {
        let InputKind::Array(args) = &args.kind else {
            return Err(type_mismatch("Ligase array", args));
        };
        let mut bonds = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let inherited_schema = if index == 0 {
                default_schema.clone()
            } else {
                None
            };
            bonds.push(parse_erased_bond(arg, env, inherited_schema)?);
        }
        return Ok(Ligation::Ligase(bonds));
    }

    Err(ImportError::Invalid(
        "ligation object must contain Slot or Ligase".to_string(),
    ))
}

fn parse_erased_bond<S: Store + ?Sized>(
    node: &InputNode,
    env: &mut ImportEnv<'_, S>,
    default_schema: Option<SchemaState>,
) -> Result<ErasedBond, ImportError> {
    let envelope = match &node.kind {
        InputKind::Object(map)
            if map.contains_key(LINK_KEY)
                || map.contains_key(LIGATION_KEY)
                || map.contains_key(VALUE_KEY)
                || map.contains_key(SCHEMA_KEY) =>
        {
            Some(map)
        }
        _ => None,
    };

    if envelope.is_none() {
        return match &node.kind {
            InputKind::String(cid) => Ok(ErasedBond::from_cid(parse_cid(cid)?)),
            _ => Err(type_mismatch("erased bond envelope or CID string", node)),
        };
    }

    let map = envelope.unwrap();
    let schema_state = if let Some(schema) = map.get(SCHEMA_KEY) {
        let InputKind::String(schema) = &schema.kind else {
            return Err(type_mismatch("schema CID string", schema));
        };
        Some(SchemaState {
            schema_cid: parse_cid(schema)?,
            scope: Vec::new(),
        })
    } else {
        default_schema
    };

    let link = map.get(LINK_KEY);
    let ligation = map.get(LIGATION_KEY);
    let value = map.get(VALUE_KEY);

    if link.is_some() && ligation.is_some() {
        return Err(ImportError::Invalid(
            "bond envelope cannot contain both $link and $ligation".to_string(),
        ));
    }

    if matches!(env.mode, ImportMode::Canonical) && ligation.is_some() && value.is_some() {
        return Err(ImportError::ProfileViolation(
            "canonical import forbids hydrated value on ligation bonds".to_string(),
        ));
    }

    if let Some(link) = link {
        let InputKind::String(link) = &link.kind else {
            return Err(type_mismatch("CID string", link));
        };
        let cid = parse_cid(link)?;
        if let Some(value) = value {
            let schema_state = schema_state.clone().ok_or_else(|| {
                ImportError::Invalid("hydrated erased bond requires $schema".to_string())
            })?;
            let schema = SchemaCursor::from_state(env.schemas, &schema_state)?;
            let hydrated = materialize_occurrence(value, &schema, env)?;
            if hydrated.cid != cid {
                return Err(ImportError::CidMismatch {
                    expected: cid,
                    actual: hydrated.cid,
                });
            }
        }
        return Ok(ErasedBond::from_cid(cid));
    }

    if let Some(ligation) = ligation {
        let ligation = parse_ligation(ligation, env, schema_state.clone())?;
        if let Some(value) = value {
            let schema_state = schema_state.clone().ok_or_else(|| {
                ImportError::Invalid("hydrated erased bond requires $schema".to_string())
            })?;
            let schema = SchemaCursor::from_state(env.schemas, &schema_state)?;
            match materialize_occurrence(value, &schema, env) {
                Ok(_) | Err(ImportError::CyclicHydratedValue(_)) => {}
                Err(err) => return Err(err),
            }
        }
        store_ligation(env.store, &ligation)?;
        return Ok(ErasedBond::from_cid(ligation_cid(&ligation)));
    }

    let value = value.unwrap_or(node);

    if matches!(env.mode, ImportMode::Faithful | ImportMode::Canonical) {
        return Err(ImportError::ProfileViolation(
            "import mode requires $link or $ligation on every bond".to_string(),
        ));
    }

    let schema_state = schema_state
        .ok_or_else(|| ImportError::Invalid("direct erased bond requires $schema".to_string()))?;
    let schema = SchemaCursor::from_state(env.schemas, &schema_state)?;
    let hydrated = materialize_occurrence(value, &schema, env)?;
    Ok(ErasedBond::from_cid(hydrated.cid))
}

fn store_ligation<S: Store + ?Sized>(store: &S, ligation: &Ligation) -> Result<(), ImportError> {
    let cid = ligation_cid(ligation);
    if is_identity_cid(&cid) {
        return Ok(());
    }
    let data_cid = reflexive_to_data_cid(&cid);
    let key = key_from_cid(&data_cid);
    let bytes = ligation.to_bytes();
    store
        .put(&key, &bytes)
        .map_err(|e| ImportError::Store(e.to_string()))
}

fn resolve_schema_bond(
    solvent: &Solvent,
    bond: &Bond<Structure>,
    scope: &[Cid],
) -> Result<(Arc<Cell<Structure>>, Vec<Cid>), ImportError> {
    match solvent.add_bond(bond) {
        Bond::Link(cell) => Ok((cell, scope.to_vec())),
        Bond::Unresolved(cid) => resolve_schema_cid(solvent, cid, scope),
        Bond::Ligation(ligation) => {
            let (cid, scope) = resolve_ligation(Some(*ligation), scope).ok_or(
                ImportError::Schema("schema ligation could not be resolved".to_string()),
            )?;
            resolve_schema_cid(solvent, cid, &scope)
        }
    }
}

fn resolve_schema_cid(
    solvent: &Solvent,
    cid: Cid,
    scope: &[Cid],
) -> Result<(Arc<Cell<Structure>>, Vec<Cid>), ImportError> {
    let mut cid = cid;
    let mut scope = scope.to_vec();

    while is_reflexive_cid(&cid) {
        let ligation = if is_identity_cid(&cid) {
            parse_ligation_bytes(cid.hash().digest())
        } else {
            solvent
                .get::<Ligation>(&reflexive_to_data_cid(&cid))
                .map(|cell| cell.value().clone())
        };
        let Some((next_cid, next_scope)) = resolve_ligation(ligation, &scope) else {
            return Err(ImportError::Schema(format!(
                "schema slot could not be resolved: {}",
                cid
            )));
        };
        cid = next_cid;
        scope = next_scope;
    }

    let cell = solvent
        .get::<Structure>(&cid)
        .ok_or_else(|| ImportError::Schema(format!("missing schema cell {}", cid)))?;
    Ok((cell, scope))
}

fn occurrence_index<S: Store + ?Sized>(
    node: &InputNode,
    env: &ImportEnv<'_, S>,
) -> Result<Option<usize>, ImportError> {
    match &node.kind {
        InputKind::Alias(name) => env
            .names
            .get(&RefName::Anchor(name.clone()))
            .copied()
            .ok_or_else(|| ImportError::MissingReference(name.clone()))
            .map(Some),
        InputKind::Object(map) => {
            if let Some(id) = occurrence_ref_id(map)? {
                return env
                    .names
                    .get(&RefName::Id(id.clone()))
                    .copied()
                    .ok_or_else(|| ImportError::MissingReference(id.clone()))
                    .map(Some);
            }
            if let Some(anchor) = &node.anchor {
                if let Some(index) = env.names.get(&RefName::Anchor(anchor.clone())) {
                    return Ok(Some(*index));
                }
            }
            if let Some(id) = occurrence_id(node)? {
                if let Some(index) = env.names.get(&RefName::Id(id)) {
                    return Ok(Some(*index));
                }
            }
            Ok(None)
        }
        _ => {
            if let Some(anchor) = &node.anchor {
                return Ok(env.names.get(&RefName::Anchor(anchor.clone())).copied());
            }
            Ok(None)
        }
    }
}

fn occurrence_data(node: &InputNode) -> &InputNode {
    if let InputKind::Object(map) = &node.kind {
        if let Some(data) = map.get("data") {
            if occurrence_id(node).ok().flatten().is_some() || node.anchor.is_some() {
                return data;
            }
        }
    }
    node
}

fn occurrence_id(node: &InputNode) -> Result<Option<String>, ImportError> {
    let InputKind::Object(map) = &node.kind else {
        return Ok(None);
    };
    if let Some(id) = map.get("@id").or_else(|| map.get("id")) {
        if map.contains_key("data") {
            let InputKind::String(id) = &id.kind else {
                return Err(type_mismatch("string occurrence id", id));
            };
            return Ok(Some(id.clone()));
        }
    }
    Ok(None)
}

fn occurrence_ref_id(map: &IndexMap<String, InputNode>) -> Result<Option<String>, ImportError> {
    if let Some(id) = map.get("@id").or_else(|| map.get("id")) {
        if !map.contains_key("data")
            && map
                .keys()
                .all(|key| key == "@id" || key == "id" || key == "@context")
        {
            let InputKind::String(id) = &id.kind else {
                return Err(type_mismatch("string reference id", id));
            };
            return Ok(Some(id.clone()));
        }
    }
    Ok(None)
}

fn first_non_meta(map: &IndexMap<String, InputNode>) -> Option<(&String, &InputNode)> {
    map.iter()
        .find(|(key, _)| *key != "@context" && *key != "@id" && *key != "id" && *key != "data")
}

fn parse_cid(value: &str) -> Result<Cid, ImportError> {
    value
        .parse()
        .map_err(|e| ImportError::Invalid(format!("invalid CID {}: {}", value, e)))
}

fn type_mismatch(expected: &str, node: &InputNode) -> ImportError {
    ImportError::Invalid(format!("expected {}, found {:?}", expected, node.kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Bond, ErasedBond, ExportFormat, ExportOptions, ExportProfile, Ligation, MemoryStore, Oxide,
        export, key_from_cid, resolve_reflexive_with_store,
    };

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, crate::Oxide)]
    #[oxide(crate = crate)]
    struct Ring {
        name: String,
        next: Bond<Ring>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, crate::Oxide)]
    #[oxide(crate = crate)]
    struct Pair {
        left: Bond<String>,
        right: Bond<String>,
    }

    fn export_fixture<T: Oxide>(
        value: T,
        format: ExportFormat,
        profile: ExportProfile,
    ) -> (String, Cid, Cid) {
        let solvent = Solvent::new();
        let store = MemoryStore::new();
        let cell = solvent.add(value);
        let (value_cid, schema_cid) = solvent.persist_cell(&cell, &store).unwrap();
        let schemas = Solvent::new();
        let _ = crate::load_schema_recursive(&store, &schemas, schema_cid).unwrap();
        let text = export(
            &store,
            &schemas,
            value_cid,
            schema_cid,
            format,
            &ExportOptions {
                profile,
                pretty: false,
            },
        )
        .unwrap();
        (text, value_cid, schema_cid)
    }

    fn schema_env<T: Oxide>() -> (Solvent, Cid) {
        let schemas = Solvent::new();
        let schema_cid = schemas.add_bond(&T::schema()).cid();
        (schemas, schema_cid)
    }

    fn decode_root<T: Oxide>(store: &MemoryStore, root_cid: Cid) -> T {
        let (cid, _) = resolve_reflexive_with_store(store, root_cid, &[])
            .unwrap()
            .expect("root bond should resolve");
        let bytes = store.get(&key_from_cid(&cid)).unwrap().unwrap();
        T::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn lenient_import_roundtrips_direct_links() {
        let shared = Bond::new("shared".to_string());
        let (text, value_cid, _) = export_fixture(
            Pair {
                left: shared.clone(),
                right: shared,
            },
            ExportFormat::JsonLd,
            ExportProfile::Direct,
        );

        let store = MemoryStore::new();
        let (schemas, schema_cid) = schema_env::<Pair>();
        let imported_root_cid = import(
            &text,
            ImportFormat::JsonLd,
            schema_cid,
            &store,
            &schemas,
            &ImportOptions {
                mode: ImportMode::Lenient,
            },
        )
        .unwrap();

        let imported = decode_root::<Pair>(&store, imported_root_cid);
        assert_eq!(imported.compute_cid(), value_cid);
        assert_eq!(imported_root_cid, value_cid);
    }

    #[test]
    fn faithful_import_rejects_direct_bonds() {
        let (text, _, _) = export_fixture(
            Pair {
                left: Bond::new("left".to_string()),
                right: Bond::new("right".to_string()),
            },
            ExportFormat::JsonLd,
            ExportProfile::Direct,
        );

        let store = MemoryStore::new();
        let (schemas, schema_cid) = schema_env::<Pair>();
        let err = import(
            &text,
            ImportFormat::JsonLd,
            schema_cid,
            &store,
            &schemas,
            &ImportOptions {
                mode: ImportMode::Faithful,
            },
        )
        .unwrap_err();

        assert!(matches!(err, ImportError::ProfileViolation(_)));
    }

    #[test]
    fn canonical_import_rejects_hydrated_ligation() {
        let (text, _, _) = export_fixture(
            Ring {
                name: "root".into(),
                next: Bond::from_ligation(Ligation::Ligase(vec![ErasedBond::from(&Bond::new(
                    Ring {
                        name: "child".into(),
                        next: Bond::from_cid(crate::slot_cid(0)),
                    },
                ))])),
            },
            ExportFormat::JsonLd,
            ExportProfile::Full,
        );

        let store = MemoryStore::new();
        let (schemas, schema_cid) = schema_env::<Ring>();
        let err = import(
            &text,
            ImportFormat::JsonLd,
            schema_cid,
            &store,
            &schemas,
            &ImportOptions {
                mode: ImportMode::Canonical,
            },
        )
        .unwrap_err();

        assert!(matches!(err, ImportError::ProfileViolation(_)));
    }

    #[test]
    fn yaml_import_accepts_aliases() {
        let shared = Bond::new("shared".to_string());
        let (text, value_cid, schema_cid) = export_fixture(
            Pair {
                left: shared.clone(),
                right: shared,
            },
            ExportFormat::Yaml,
            ExportProfile::Full,
        );

        let store = MemoryStore::new();
        let (schemas, imported_schema_cid) = schema_env::<Pair>();
        let imported_root_cid = import(
            &text,
            ImportFormat::Yaml,
            imported_schema_cid,
            &store,
            &schemas,
            &ImportOptions {
                mode: ImportMode::Faithful,
            },
        )
        .unwrap();

        assert_eq!(imported_root_cid, value_cid);
        assert_eq!(imported_schema_cid, schema_cid);
    }

    #[test]
    fn imported_store_exports_again() {
        let (text, value_cid, schema_cid) = export_fixture(
            Ring {
                name: "root".into(),
                next: Bond::from_ligation(Ligation::Ligase(vec![ErasedBond::from(&Bond::new(
                    Ring {
                        name: "child".into(),
                        next: Bond::from_cid(crate::slot_cid(0)),
                    },
                ))])),
            },
            ExportFormat::JsonLd,
            ExportProfile::Full,
        );

        let store = MemoryStore::new();
        let (schemas, imported_schema_cid) = schema_env::<Ring>();
        let imported_root_cid = import(
            &text,
            ImportFormat::JsonLd,
            imported_schema_cid,
            &store,
            &schemas,
            &ImportOptions {
                mode: ImportMode::Faithful,
            },
        )
        .unwrap();

        let exported = export(
            &store,
            &schemas,
            imported_root_cid,
            imported_schema_cid,
            ExportFormat::JsonLd,
            &ExportOptions {
                profile: ExportProfile::Full,
                pretty: false,
            },
        )
        .unwrap();

        assert_eq!(imported_root_cid, value_cid);
        assert_eq!(imported_schema_cid, schema_cid);
        assert!(exported.contains("\"$ligation\""));
    }

    #[test]
    fn lenient_import_accepts_implicit_value_object() {
        let text = r#"{
          "@id":"urn:px-occ:root",
          "data":{
            "left":{"@id":"urn:px-occ:shared","data":"shared"},
            "right":{"@id":"urn:px-occ:shared"}
          },
          "@context":{"@vocab":"urn:px:"}
        }"#;

        let store = MemoryStore::new();
        let (schemas, schema_cid) = schema_env::<Pair>();
        let imported_root_cid = import(
            text,
            ImportFormat::JsonLd,
            schema_cid,
            &store,
            &schemas,
            &ImportOptions {
                mode: ImportMode::Lenient,
            },
        )
        .unwrap();

        let imported = decode_root::<Pair>(&store, imported_root_cid);
        assert_eq!(imported.left.cid(), imported.right.cid());
    }
}
