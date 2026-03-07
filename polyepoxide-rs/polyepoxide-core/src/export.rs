use cid::Cid;
use indexmap::IndexMap;
use ipld_core::ipld::Ipld;
use serde_json::{Map as JsonMap, Number, Value as JsonValue};
use serde_yaml_bw::{Mapping as YamlMapping, Sequence as YamlSequence, Value as YamlValue};

use crate::bond::ErasedBond;
use crate::store::Store;
use crate::store_cursor::{load_ligation, resolve_reflexive_edge, resolve_schema_cid};
use crate::{Ligation, Solvent, StoreCursor, Structure, is_reflexive_cid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Yaml,
    JsonLd,
    YamlLd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportProfile {
    Canonical,
    Full,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    pub profile: ExportProfile,
    pub pretty: bool,
    pub unwrap_top_level_occurrence: bool,
    pub exclude_top_level_fields: Vec<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            profile: ExportProfile::Full,
            pretty: true,
            unwrap_top_level_occurrence: false,
            exclude_top_level_fields: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError<E: std::error::Error + Send + Sync + 'static> {
    #[error("node not found: {0}")]
    NotFound(Cid),
    #[error("store error: {0}")]
    Store(E),
    #[error("invalid format: {0}")]
    Format(String),
    #[error("ligation has no entry point")]
    EmptyLigase,
    #[error("slot out of range: {0}")]
    SlotOutOfRange(u16),
    #[error("cannot materialize bond target: {0}")]
    CannotMaterialize(Cid),
    #[error("render error: {0}")]
    Render(String),
}

#[derive(Debug, Clone)]
enum DocNode {
    Null,
    Bool(bool),
    Integer(i128),
    Float(f64),
    String(String),
    Array(Vec<DocNode>),
    Object(IndexMap<String, DocNode>),
    Occurrence { id: String, data: Box<DocNode> },
    Ref(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportContext {
    Normal,
    RecordField,
}

struct GraphBuilder {
    profile: ExportProfile,
    seen: IndexMap<String, VisitState>,
}

const VALUE_KEY: &str = "$value";
const LINK_KEY: &str = "$link";
const LIGATION_KEY: &str = "$ligation";
const SCHEMA_KEY: &str = "$schema";

impl GraphBuilder {
    fn new(profile: ExportProfile) -> Self {
        Self {
            profile,
            seen: IndexMap::new(),
        }
    }

    fn build_occurrence<S: Store + ?Sized>(
        &mut self,
        cursor: &StoreCursor<'_, S>,
    ) -> Result<DocNode, ExportError<S::Error>> {
        let id = cursor.occurrence_id();
        if self.seen.contains_key(&id) {
            return Ok(DocNode::Ref(id));
        }

        self.seen.insert(id.clone(), VisitState::Visiting);
        let ipld = cursor.ipld()?;
        let schema_cell = cursor.schema()?;
        let data = self
            .export_value(cursor, &ipld, schema_cell.value(), ExportContext::Normal)?
            .ok_or_else(|| ExportError::Format("root value cannot be omitted".to_string()))?;
        self.seen.insert(id.clone(), VisitState::Done);
        Ok(DocNode::Occurrence {
            id,
            data: Box::new(data),
        })
    }

    fn export_value<S: Store + ?Sized>(
        &mut self,
        cursor: &StoreCursor<'_, S>,
        ipld: &Ipld,
        schema: &Structure,
        context: ExportContext,
    ) -> Result<Option<DocNode>, ExportError<S::Error>> {
        match (ipld, schema) {
            (Ipld::Link(target_cid), Structure::Bond(inner_schema)) => {
                self.export_bond(cursor, *target_cid, inner_schema.cid())
                    .map(Some)
            }
            (Ipld::Link(cid), Structure::Cid) => Ok(Some(DocNode::String(cid.to_string()))),
            (Ipld::Map(map), Structure::Record(fields)) => {
                let mut out = IndexMap::new();
                for (name, field_schema_bond) in fields {
                    if let Some(value) = map.get(name) {
                        let (field_schema, field_schema_scope) =
                            cursor.resolve_child_schema(field_schema_bond.cid())?;
                        let child_cursor =
                            cursor.with_schema(field_schema.cid(), field_schema_scope);
                        if let Some(exported) =
                            self.export_value(
                                &child_cursor,
                                value,
                                field_schema.value(),
                                ExportContext::RecordField,
                            )?
                        {
                            out.insert(name.clone(), exported);
                        }
                    }
                }
                Ok(Some(DocNode::Object(out)))
            }
            (Ipld::List(values), Structure::Record(fields)) => {
                let mut out = IndexMap::new();
                for ((name, field_schema_bond), value) in fields.iter().zip(values.iter()) {
                    let (field_schema, field_schema_scope) =
                        cursor.resolve_child_schema(field_schema_bond.cid())?;
                    let child_cursor = cursor.with_schema(field_schema.cid(), field_schema_scope);
                    if let Some(exported) =
                        self.export_value(
                            &child_cursor,
                            value,
                            field_schema.value(),
                            ExportContext::RecordField,
                        )?
                    {
                        out.insert(name.clone(), exported);
                    }
                }
                Ok(Some(DocNode::Object(out)))
            }
            (value, Structure::Option(inner)) => {
                let (inner_schema, inner_schema_scope) =
                    cursor.resolve_child_schema(inner.cid())?;
                let child_cursor = cursor.with_schema(inner_schema.cid(), inner_schema_scope);
                if matches!(context, ExportContext::RecordField) {
                    return self.export_value(
                        &child_cursor,
                        value,
                        inner_schema.value(),
                        ExportContext::Normal,
                    );
                }
                let Ipld::List(values) = value else {
                    return Err(self.type_mismatch::<S>(ipld, schema));
                };
                if values.len() > 1 {
                    return Err(self.type_mismatch::<S>(ipld, schema));
                }
                let mut out = Vec::with_capacity(values.len());
                for value in values {
                    out.push(
                        self.export_value(
                            &child_cursor,
                            value,
                            inner_schema.value(),
                            ExportContext::Normal,
                        )?
                        .ok_or_else(|| {
                            ExportError::Format("option element cannot be omitted".to_string())
                        })?,
                    );
                }
                Ok(Some(DocNode::Array(out)))
            }
            (Ipld::List(values), Structure::Sequence(inner)) => {
                let (inner_schema, inner_schema_scope) =
                    cursor.resolve_child_schema(inner.cid())?;
                let child_cursor = cursor.with_schema(inner_schema.cid(), inner_schema_scope);
                let mut out = Vec::with_capacity(values.len());
                for value in values {
                    out.push(
                        self.export_value(
                            &child_cursor,
                            value,
                            inner_schema.value(),
                            ExportContext::Normal,
                        )?
                        .ok_or_else(|| {
                            ExportError::Format("sequence element cannot be omitted".to_string())
                        })?,
                    );
                }
                Ok(Some(DocNode::Array(out)))
            }
            (Ipld::List(values), Structure::Tuple(elements)) => {
                let mut out = Vec::with_capacity(values.len());
                for (value, elem_schema_bond) in values.iter().zip(elements.iter()) {
                    let (elem_schema, elem_schema_scope) =
                        cursor.resolve_child_schema(elem_schema_bond.cid())?;
                    let child_cursor = cursor.with_schema(elem_schema.cid(), elem_schema_scope);
                    out.push(
                        self.export_value(
                            &child_cursor,
                            value,
                            elem_schema.value(),
                            ExportContext::Normal,
                        )?
                        .ok_or_else(|| {
                            ExportError::Format("tuple element cannot be omitted".to_string())
                        })?,
                    );
                }
                Ok(Some(DocNode::Array(out)))
            }
            (Ipld::Map(map), Structure::Tagged(variants)) => {
                if let Some((name, value)) = map.iter().next() {
                    if map.len() == 1 {
                        if let Some(variant_schema_bond) = variants.get(name) {
                            let (variant_schema, variant_schema_scope) =
                                cursor.resolve_child_schema(variant_schema_bond.cid())?;
                            let child_cursor =
                                cursor.with_schema(variant_schema.cid(), variant_schema_scope);
                            let mut out = IndexMap::new();
                            out.insert(
                                name.clone(),
                                self.export_value(
                                    &child_cursor,
                                    value,
                                    variant_schema.value(),
                                    ExportContext::Normal,
                                )?
                                .ok_or_else(|| {
                                    ExportError::Format(
                                        "tagged payload cannot be omitted".to_string(),
                                    )
                                })?,
                            );
                            return Ok(Some(DocNode::Object(out)));
                        }
                    }
                }
                Err(self.type_mismatch::<S>(ipld, schema))
            }
            (Ipld::String(name), Structure::Tagged(variants)) => {
                if let Some(variant_schema_bond) = variants.get(name) {
                    let (variant_schema, _) =
                        cursor.resolve_child_schema(variant_schema_bond.cid())?;
                    if matches!(variant_schema.value(), Structure::Unit) {
                        return Ok(Some(DocNode::String(name.clone())));
                    }
                }
                Err(self.type_mismatch::<S>(ipld, schema))
            }
            (Ipld::Integer(index), Structure::Enum(variants)) => {
                if *index >= 0 {
                    if let Some(name) = variants.get(*index as usize) {
                        return Ok(Some(DocNode::String(name.clone())));
                    }
                }
                Err(ExportError::Format(format!(
                    "enum index {} out of range for {:?}",
                    index, variants
                )))
            }
            (Ipld::String(name), Structure::Enum(variants)) => {
                if variants.iter().any(|variant| variant == name) {
                    return Ok(Some(DocNode::String(name.clone())));
                }
                Err(self.type_mismatch::<S>(ipld, schema))
            }
            (Ipld::Map(map), Structure::Map { value, .. }) => {
                let (value_schema, value_schema_scope) =
                    cursor.resolve_child_schema(value.cid())?;
                let child_cursor = cursor.with_schema(value_schema.cid(), value_schema_scope);
                let mut out = IndexMap::new();
                for (key, map_value) in map {
                    out.insert(
                        key.clone(),
                        self.export_value(
                            &child_cursor,
                            map_value,
                            value_schema.value(),
                            ExportContext::Normal,
                        )?
                        .ok_or_else(|| {
                            ExportError::Format("map value cannot be omitted".to_string())
                        })?,
                    );
                }
                Ok(Some(DocNode::Object(out)))
            }
            (Ipld::List(entries), Structure::OrderedMap { key, value }) => {
                let (key_schema, key_schema_scope) = cursor.resolve_child_schema(key.cid())?;
                let (value_schema, value_schema_scope) =
                    cursor.resolve_child_schema(value.cid())?;
                let key_cursor = cursor.with_schema(key_schema.cid(), key_schema_scope);
                let value_cursor = cursor.with_schema(value_schema.cid(), value_schema_scope);
                let mut out = Vec::with_capacity(entries.len());
                for entry in entries {
                    match entry {
                        Ipld::List(pair) if pair.len() == 2 => out.push(DocNode::Array(vec![
                            self.export_value(
                                &key_cursor,
                                &pair[0],
                                key_schema.value(),
                                ExportContext::Normal,
                            )?
                            .ok_or_else(|| {
                                ExportError::Format("ordered map key cannot be omitted".to_string())
                            })?,
                            self.export_value(
                                &value_cursor,
                                &pair[1],
                                value_schema.value(),
                                ExportContext::Normal,
                            )?
                            .ok_or_else(|| {
                                ExportError::Format(
                                    "ordered map value cannot be omitted".to_string(),
                                )
                            })?,
                        ])),
                        other => {
                            return Err(ExportError::Format(format!(
                                "ordered map entry must be a two-element list, found {:?}",
                                other
                            )));
                        }
                    }
                }
                Ok(Some(DocNode::Array(out)))
            }
            (Ipld::Null, Structure::Unit) => Ok(Some(DocNode::Null)),
            (Ipld::Bool(value), Structure::Bool) => Ok(Some(DocNode::Bool(*value))),
            (Ipld::Integer(value), Structure::Int(_)) => Ok(Some(DocNode::Integer(*value))),
            (Ipld::Float(value), Structure::Float(_)) => Ok(Some(DocNode::Float(*value))),
            (Ipld::String(value), Structure::Unicode | Structure::Char) => {
                Ok(Some(DocNode::String(value.clone())))
            }
            (Ipld::Bytes(bytes), Structure::ByteString) => {
                use base64::{Engine, engine::general_purpose::STANDARD};
                Ok(Some(DocNode::String(STANDARD.encode(bytes))))
            }
            _ => Err(self.type_mismatch::<S>(ipld, schema)),
        }
    }

    fn type_mismatch<S: Store + ?Sized>(
        &self,
        ipld: &Ipld,
        schema: &Structure,
    ) -> ExportError<S::Error> {
        ExportError::Format(format!(
            "typed export mismatch: value {:?} does not match schema {:?}",
            ipld, schema
        ))
    }

    fn export_bond<S: Store + ?Sized>(
        &mut self,
        cursor: &StoreCursor<'_, S>,
        target_cid: Cid,
        inner_schema_cid: Cid,
    ) -> Result<DocNode, ExportError<S::Error>> {
        let mut out = IndexMap::new();

        match self.profile {
            ExportProfile::Direct => {
                let resolved = cursor.follow_bond(target_cid, inner_schema_cid)?;
                return self.build_occurrence(&resolved).map_err(|err| match err {
                    ExportError::NotFound(_) => ExportError::CannotMaterialize(target_cid),
                    other => other,
                });
            }
            ExportProfile::Canonical | ExportProfile::Full => {}
        }

        if is_reflexive_cid(&target_cid) {
            let ligation = cursor.ligation_term(target_cid)?.ok_or_else(|| {
                ExportError::Format(format!("invalid ligation payload for {}", target_cid))
            })?;
            out.insert(
                LIGATION_KEY.to_string(),
                self.ligation_to_doc(
                    cursor.store(),
                    cursor.schemas(),
                    &ligation,
                    inner_schema_cid,
                    cursor.schema_scope(),
                )?,
            );
        } else {
            out.insert(
                LINK_KEY.to_string(),
                DocNode::String(target_cid.to_string()),
            );
        }

        let should_include_value = match self.profile {
            ExportProfile::Canonical => !is_reflexive_cid(&target_cid),
            ExportProfile::Full => true,
            ExportProfile::Direct => unreachable!(),
        };

        if should_include_value {
            let resolved = cursor.follow_bond(target_cid, inner_schema_cid)?;
            match self.build_occurrence(&resolved) {
                Ok(value) => {
                    out.insert(VALUE_KEY.to_string(), value);
                }
                Err(ExportError::NotFound(_)) if !is_reflexive_cid(&target_cid) => {}
                Err(err) => return Err(err),
            }
        }

        Ok(DocNode::Object(out))
    }
}

pub fn export<S: Store + ?Sized>(
    store: &S,
    schemas: &Solvent,
    root_cid: Cid,
    schema_cid: Cid,
    format: ExportFormat,
    options: &ExportOptions,
) -> Result<String, ExportError<S::Error>> {
    let mut builder = GraphBuilder::new(options.profile);
    let root = apply_export_options(
        builder.build_root(store, schemas, root_cid, schema_cid)?,
        options,
    );

    match format {
        ExportFormat::JsonLd => {
            render_json_ld(&root, options.pretty).map_err(|err| ExportError::Render(err))
        }
        ExportFormat::YamlLd => render_yaml_ld(&root).map_err(ExportError::Render),
        ExportFormat::Yaml => render_yaml(&root).map_err(ExportError::Render),
    }
}

fn apply_export_options(mut root: DocNode, options: &ExportOptions) -> DocNode {
    if options.profile == ExportProfile::Direct && options.unwrap_top_level_occurrence {
        if let DocNode::Occurrence { data, .. } = root {
            root = *data;
        }
    }

    if options.exclude_top_level_fields.is_empty() {
        return root;
    }

    match &mut root {
        DocNode::Occurrence { data, .. } => {
            exclude_object_fields(data, &options.exclude_top_level_fields)
        }
        DocNode::Object(map) => {
            for field in &options.exclude_top_level_fields {
                map.shift_remove(field);
            }
        }
        _ => {}
    }

    root
}

fn exclude_object_fields(node: &mut DocNode, fields: &[String]) {
    if let DocNode::Object(map) = node {
        for field in fields {
            map.shift_remove(field);
        }
    }
}

impl GraphBuilder {
    fn build_root<S: Store + ?Sized>(
        &mut self,
        store: &S,
        schemas: &Solvent,
        root_cid: Cid,
        schema_cid: Cid,
    ) -> Result<DocNode, ExportError<S::Error>> {
        match self.profile {
            ExportProfile::Direct => {
                let cursor = StoreCursor::new(store, schemas, root_cid, schema_cid)?;
                self.build_occurrence(&cursor).map_err(|err| match err {
                    ExportError::NotFound(_) => ExportError::CannotMaterialize(root_cid),
                    other => other,
                })
            }
            ExportProfile::Canonical | ExportProfile::Full => {
                let mut out = IndexMap::new();
                if is_reflexive_cid(&root_cid) {
                    let ligation = load_ligation(store, root_cid)?.ok_or_else(|| {
                        ExportError::Format(format!("invalid ligation payload for {}", root_cid))
                    })?;
                    out.insert(
                        LIGATION_KEY.to_string(),
                        self.ligation_to_doc(store, schemas, &ligation, schema_cid, &[])?,
                    );
                } else {
                    out.insert(LINK_KEY.to_string(), DocNode::String(root_cid.to_string()));
                }

                let should_include_value = match self.profile {
                    ExportProfile::Canonical => !is_reflexive_cid(&root_cid),
                    ExportProfile::Full => true,
                    ExportProfile::Direct => unreachable!(),
                };

                if should_include_value {
                    let cursor = StoreCursor::new(store, schemas, root_cid, schema_cid)?;
                    match self.build_occurrence(&cursor) {
                        Ok(value) => {
                            out.insert(VALUE_KEY.to_string(), value);
                        }
                        Err(ExportError::NotFound(_)) if !is_reflexive_cid(&root_cid) => {}
                        Err(err) => return Err(err),
                    }
                }

                Ok(DocNode::Object(out))
            }
        }
    }

    fn ligation_to_doc<S: Store + ?Sized>(
        &mut self,
        store: &S,
        schemas: &Solvent,
        ligation: &Ligation,
        arg_schema_cid: Cid,
        schema_scope: &[Cid],
    ) -> Result<DocNode, ExportError<S::Error>> {
        let mut out = IndexMap::new();
        match ligation {
            Ligation::Slot(index) => {
                out.insert("Slot".to_string(), DocNode::Integer(i128::from(*index)));
            }
            Ligation::Ligase(args) => {
                let arg_scope: Vec<Cid> = args.iter().map(|arg| arg.cid()).collect();
                let mut exported_args = Vec::with_capacity(args.len());
                for (index, arg) in args.iter().enumerate() {
                    exported_args.push(self.erased_bond_to_doc(
                        store,
                        schemas,
                        arg,
                        arg_schema_cid,
                        schema_scope,
                        &arg_scope,
                        index != 0,
                    )?);
                }
                out.insert("Ligase".to_string(), DocNode::Array(exported_args));
            }
        }
        Ok(DocNode::Object(out))
    }

    fn erased_bond_to_doc<S: Store + ?Sized>(
        &mut self,
        store: &S,
        schemas: &Solvent,
        bond: &ErasedBond,
        schema_cid: Cid,
        schema_scope: &[Cid],
        value_scope: &[Cid],
        include_schema: bool,
    ) -> Result<DocNode, ExportError<S::Error>> {
        let mut out = IndexMap::new();
        let cid = bond.cid();
        let (resolved_schema_cid, resolved_schema_scope) =
            resolve_schema_cid(store, schemas, schema_cid, schema_scope)?;

        if is_reflexive_cid(&cid) {
            let ligation = load_ligation(store, cid)?.ok_or_else(|| {
                ExportError::Format(format!("invalid ligation payload for {}", cid))
            })?;
            out.insert(
                LIGATION_KEY.to_string(),
                self.ligation_to_doc(store, schemas, &ligation, schema_cid, schema_scope)?,
            );
        } else {
            out.insert(LINK_KEY.to_string(), DocNode::String(cid.to_string()));
        }

        if include_schema {
            out.insert(
                SCHEMA_KEY.to_string(),
                DocNode::String(resolved_schema_cid.to_string()),
            );
        }

        if matches!(self.profile, ExportProfile::Canonical | ExportProfile::Full) {
            let (resolved_value_cid, resolved_scope) =
                resolve_reflexive_edge(store, cid, value_scope)?;
            let cursor = StoreCursor::with_state(
                store,
                schemas,
                resolved_value_cid,
                resolved_schema_cid,
                resolved_scope,
                resolved_schema_scope,
            );
            match self.build_occurrence(&cursor) {
                Ok(value) => {
                    out.insert(VALUE_KEY.to_string(), value);
                }
                Err(ExportError::NotFound(_)) if !is_reflexive_cid(&cid) => {}
                Err(err) => return Err(err),
            }
        }

        Ok(DocNode::Object(out))
    }
}

fn render_json_ld(node: &DocNode, pretty: bool) -> Result<String, String> {
    let mut value = doc_to_json(node, true);
    if let JsonValue::Object(ref mut obj) = value {
        let mut context = JsonMap::new();
        context.insert(
            "@vocab".to_string(),
            JsonValue::String("urn:px:".to_string()),
        );
        obj.insert("@context".to_string(), JsonValue::Object(context));
    }

    if pretty {
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    } else {
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }
}

fn render_yaml_ld(node: &DocNode) -> Result<String, String> {
    let mut value = doc_to_yaml(node, true, false);
    if let YamlValue::Mapping(ref mut map) = value {
        let mut context = YamlMapping::new();
        context.insert(yaml_string("@vocab"), yaml_string("urn:px:"));
        map.insert(yaml_string("@context"), YamlValue::Mapping(context));
    }
    serde_yaml_bw::to_string(&value).map_err(|e| e.to_string())
}

fn doc_to_json(node: &DocNode, linked_data: bool) -> JsonValue {
    match node {
        DocNode::Null => JsonValue::Null,
        DocNode::Bool(value) => JsonValue::Bool(*value),
        DocNode::Integer(value) => {
            if let Ok(value) = i64::try_from(*value) {
                JsonValue::Number(Number::from(value))
            } else if let Ok(value) = u64::try_from(*value) {
                JsonValue::Number(Number::from(value))
            } else {
                JsonValue::String(value.to_string())
            }
        }
        DocNode::Float(value) => Number::from_f64(*value)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        DocNode::String(value) => JsonValue::String(value.clone()),
        DocNode::Array(values) => JsonValue::Array(
            values
                .iter()
                .map(|value| doc_to_json(value, linked_data))
                .collect(),
        ),
        DocNode::Object(map) => {
            let mut out = JsonMap::new();
            for (key, value) in map {
                out.insert(key.clone(), doc_to_json(value, linked_data));
            }
            JsonValue::Object(out)
        }
        DocNode::Occurrence { id, data } => {
            let mut out = JsonMap::new();
            let id_key = if linked_data { "@id" } else { "id" };
            out.insert(id_key.to_string(), JsonValue::String(id.clone()));
            out.insert("data".to_string(), doc_to_json(data, linked_data));
            JsonValue::Object(out)
        }
        DocNode::Ref(id) => {
            let mut out = JsonMap::new();
            let id_key = if linked_data { "@id" } else { "id" };
            out.insert(id_key.to_string(), JsonValue::String(id.clone()));
            JsonValue::Object(out)
        }
    }
}

fn render_yaml(node: &DocNode) -> Result<String, String> {
    let value = doc_to_yaml(node, false, true);
    serde_yaml_bw::to_string(&value).map_err(|e| e.to_string())
}

fn anchor_name(id: &str) -> String {
    let mut out = String::from("occ_");
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn doc_to_yaml(node: &DocNode, linked_data: bool, use_aliases: bool) -> YamlValue {
    match node {
        DocNode::Null => YamlValue::Null(None),
        DocNode::Bool(value) => YamlValue::Bool(*value, None),
        DocNode::Integer(value) => {
            if let Ok(value) = i64::try_from(*value) {
                YamlValue::Number(value.into(), None)
            } else if let Ok(value) = u64::try_from(*value) {
                YamlValue::Number(value.into(), None)
            } else {
                yaml_string(value.to_string())
            }
        }
        DocNode::Float(value) if value.is_finite() => YamlValue::Number((*value).into(), None),
        DocNode::Float(_) => YamlValue::Null(None),
        DocNode::String(value) => yaml_string(value.clone()),
        DocNode::Array(values) => {
            let mut sequence = YamlSequence::with_capacity(values.len());
            for value in values {
                sequence
                    .elements
                    .push(doc_to_yaml(value, linked_data, use_aliases));
            }
            YamlValue::Sequence(sequence)
        }
        DocNode::Object(map) => {
            let mut out = YamlMapping::new();
            for (key, value) in map {
                out.insert(
                    yaml_string(key.clone()),
                    doc_to_yaml(value, linked_data, use_aliases),
                );
            }
            YamlValue::Mapping(out)
        }
        DocNode::Occurrence { id, data } => {
            let id_key = if linked_data { "@id" } else { "id" };
            let mut out = if use_aliases {
                YamlMapping::with_anchor(anchor_name(id))
            } else {
                YamlMapping::new()
            };
            out.insert(yaml_string(id_key), yaml_string(id.clone()));
            out.insert(
                yaml_string("data"),
                doc_to_yaml(data, linked_data, use_aliases),
            );
            YamlValue::Mapping(out)
        }
        DocNode::Ref(id) if use_aliases => YamlValue::Alias(anchor_name(id)),
        DocNode::Ref(id) => {
            let id_key = if linked_data { "@id" } else { "id" };
            let mut out = YamlMapping::new();
            out.insert(yaml_string(id_key), yaml_string(id.clone()));
            YamlValue::Mapping(out)
        }
    }
}

fn yaml_string(value: impl Into<String>) -> YamlValue {
    YamlValue::String(value.into(), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bond, ErasedBond, MemoryStore, Solvent};

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

    #[test]
    fn full_export_hydrates_ligation_bonds() {
        let solvent = Solvent::new();
        let a = solvent.add(Ring {
            name: "A".into(),
            next: Bond::from_cid(crate::slot_cid(1)),
        });
        let b = solvent.add(Ring {
            name: "B".into(),
            next: Bond::from_cid(crate::slot_cid(0)),
        });
        let root = solvent.add(Ring {
            name: "root".into(),
            next: Bond::from_ligation(Ligation::Ligase(vec![
                ErasedBond::from_cell(a.clone()),
                ErasedBond::from_cell(b.clone()),
            ])),
        });
        let store = MemoryStore::new();
        let (value_cid, schema_cid) = solvent.persist_cell(&root, &store).unwrap();

        let schemas = Solvent::new();
        let _ = crate::load_schema_recursive(&store, &schemas, schema_cid).unwrap();
        let text = export(
            &store,
            &schemas,
            value_cid,
            schema_cid,
            ExportFormat::JsonLd,
            &ExportOptions {
                profile: ExportProfile::Full,
                pretty: false,
                unwrap_top_level_occurrence: false,
                exclude_top_level_fields: Vec::new(),
            },
        )
        .unwrap();

        assert!(text.contains("\"$ligation\""));
        assert!(text.contains("\"$schema\""));
        assert!(text.contains("\"$value\""));
        assert!(text.contains("urn:px-occ:"));
    }

    #[test]
    fn canonical_export_keeps_ligation_unhydrated() {
        let solvent = Solvent::new();
        let root = solvent.add(Ring {
            name: "root".into(),
            next: Bond::from_ligation(Ligation::Ligase(vec![ErasedBond::from(&Bond::new(Ring {
                name: "child".into(),
                next: Bond::from_cid(crate::slot_cid(0)),
            }))])),
        });
        let store = MemoryStore::new();
        let (value_cid, schema_cid) = solvent.persist_cell(&root, &store).unwrap();

        let schemas = Solvent::new();
        let _ = crate::load_schema_recursive(&store, &schemas, schema_cid).unwrap();
        let text = export(
            &store,
            &schemas,
            value_cid,
            schema_cid,
            ExportFormat::JsonLd,
            &ExportOptions {
                profile: ExportProfile::Canonical,
                pretty: false,
                unwrap_top_level_occurrence: false,
                exclude_top_level_fields: Vec::new(),
            },
        )
        .unwrap();

        assert!(text.contains("\"$ligation\""));
        assert!(!text.contains("\"$schema\""));
        assert!(text.contains("\"name\":\"child\""));
        assert!(!text.contains("\"Slot\":0,\"$value\""));
    }

    #[test]
    fn direct_export_omits_bond_envelope() {
        let solvent = Solvent::new();
        let shared = solvent.bond("shared".to_string());
        let root = solvent.add(Pair {
            left: shared.clone(),
            right: shared,
        });
        let store = MemoryStore::new();
        let (value_cid, schema_cid) = solvent.persist_cell(&root, &store).unwrap();

        let schemas = Solvent::new();
        let _ = crate::load_schema_recursive(&store, &schemas, schema_cid).unwrap();
        let text = export(
            &store,
            &schemas,
            value_cid,
            schema_cid,
            ExportFormat::JsonLd,
            &ExportOptions {
                profile: ExportProfile::Direct,
                pretty: false,
                unwrap_top_level_occurrence: false,
                exclude_top_level_fields: Vec::new(),
            },
        )
        .unwrap();

        assert!(!text.contains("\"$link\""));
        assert!(!text.contains("\"$value\""));
        assert!(text.contains("urn:px-occ:"));
    }

    #[test]
    fn yaml_export_uses_alias_for_repeated_occurrence() {
        let solvent = Solvent::new();
        let shared = solvent.bond("shared".to_string());
        let root = solvent.add(Pair {
            left: shared.clone(),
            right: shared,
        });
        let store = MemoryStore::new();
        let (value_cid, schema_cid) = solvent.persist_cell(&root, &store).unwrap();

        let schemas = Solvent::new();
        let _ = crate::load_schema_recursive(&store, &schemas, schema_cid).unwrap();
        let text = export(
            &store,
            &schemas,
            value_cid,
            schema_cid,
            ExportFormat::Yaml,
            &ExportOptions::default(),
        )
        .unwrap();

        assert!(text.contains('&'));
        assert!(text.contains('*'));
        assert!(text.contains("id: urn:px-occ:"));
    }

    #[test]
    fn direct_export_can_unwrap_root_and_exclude_fields() {
        let solvent = Solvent::new();
        let shared = solvent.bond("shared".to_string());
        let root = solvent.add(Pair {
            left: shared.clone(),
            right: shared,
        });
        let store = MemoryStore::new();
        let (value_cid, schema_cid) = solvent.persist_cell(&root, &store).unwrap();

        let schemas = Solvent::new();
        let _ = crate::load_schema_recursive(&store, &schemas, schema_cid).unwrap();
        let text = export(
            &store,
            &schemas,
            value_cid,
            schema_cid,
            ExportFormat::Yaml,
            &ExportOptions {
                profile: ExportProfile::Direct,
                pretty: false,
                unwrap_top_level_occurrence: true,
                exclude_top_level_fields: vec!["right".to_string()],
            },
        )
        .unwrap();

        assert!(text.contains("left:"));
        assert!(!text.contains("\nid:"));
        assert!(!text.contains("\ndata:"));
        assert!(!text.contains("\nright:"));
    }
}
