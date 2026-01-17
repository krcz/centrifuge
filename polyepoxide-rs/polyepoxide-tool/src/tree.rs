//! Lazy tree model for polyepoxide graph exploration.

use std::collections::HashMap;
use std::sync::Arc;

use cid::Cid;
use ipld_core::ipld::Ipld;
use polyepoxide_core::traverse::parse_to_ipld;
use polyepoxide_core::{Cell, Oxide, Solvent, Store, Structure};
use tui_tree_widget::TreeItem;
use unicode_segmentation::UnicodeSegmentation;

use crate::store::AnyStore;

/// Check if a string has more than N grapheme clusters.
/// This is more efficient than counting all graphemes for long strings.
fn has_more_than_n_graphemes(s: &str, n: usize) -> bool {
    s.grapheme_indices(true).nth(n).is_some()
}

/// Safely truncate a string to a maximum number of grapheme clusters.
/// This avoids splitting multi-byte UTF-8 characters.
fn truncate_str(s: &str, max_graphemes: usize) -> &str {
    match s.grapheme_indices(true).nth(max_graphemes) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Unique identifier for tree nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn root(cid: &Cid) -> Self {
        Self(format!("root:{}", cid))
    }

    fn child(parent: &str, key: &str) -> Self {
        Self(format!("{}:{}", parent, key))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Metadata for a tree node.
#[derive(Debug, Clone)]
pub struct NodeData {
    /// CID of this node's value (for bonds).
    pub cid: Option<Cid>,
    /// CID of the schema for this node.
    pub schema_cid: Cid,
    /// Human-readable type hint.
    pub type_hint: String,
    /// IPLD value for this node (if loaded).
    pub ipld: Option<Ipld>,
    /// Display string for the node.
    pub display: String,
    /// Depth in tree (for indentation).
    pub depth: usize,
    /// Child node IDs.
    pub children: Vec<NodeId>,
}

/// Breadcrumb entry for zoom navigation.
#[derive(Debug, Clone)]
pub struct Breadcrumb {
    pub cid: Cid,
    pub schema_cid: Cid,
    pub label: String,
}

/// Tree model for navigation.
pub struct TreeModel {
    /// Node data by ID.
    pub nodes: HashMap<NodeId, NodeData>,
    /// Root node IDs.
    pub roots: Vec<NodeId>,
    /// Breadcrumb trail for zoom navigation.
    pub breadcrumbs: Vec<Breadcrumb>,
    /// Store for loading data.
    store: AnyStore,
    /// Schema resolver.
    schemas: Solvent,
    /// Current root CID.
    root_cid: Cid,
    /// Current root schema CID.
    root_schema_cid: Cid,
}

impl TreeModel {
    /// Create a new tree model from a root CID and schema CID.
    pub fn new(
        store: AnyStore,
        root_cid: Cid,
        root_schema_cid: Cid,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut model = Self {
            nodes: HashMap::new(),
            roots: Vec::new(),
            breadcrumbs: Vec::new(),
            store,
            schemas: Solvent::new(),
            root_cid,
            root_schema_cid,
        };

        // Load schema
        model.load_schema(root_schema_cid)?;

        // Build initial tree
        model.rebuild_tree()?;

        Ok(model)
    }

    fn load_schema(
        &mut self,
        cid: Cid,
    ) -> Result<Arc<Cell<Structure>>, Box<dyn std::error::Error>> {
        if let Some(cell) = self.schemas.get::<Structure>(&cid) {
            return Ok(cell);
        }

        let bytes = self
            .store
            .get(&cid)?
            .ok_or_else(|| format!("schema not found: {}", cid))?;

        let schema: Structure = serde_ipld_dagcbor::from_slice(&bytes)?;

        // Recursively load nested schemas
        self.load_nested_schemas(&schema)?;

        Ok(self.schemas.add(schema))
    }

    fn load_nested_schemas(
        &mut self,
        schema: &Structure,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match schema {
            Structure::Sequence(inner) | Structure::Bond(inner) => {
                let cid = inner.cid();
                if self.schemas.get::<Structure>(&cid).is_none() {
                    self.load_schema(cid)?;
                }
            }
            Structure::Tuple(elems) => {
                for elem in elems {
                    let cid = elem.cid();
                    if self.schemas.get::<Structure>(&cid).is_none() {
                        self.load_schema(cid)?;
                    }
                }
            }
            Structure::Record(fields) | Structure::Tagged(fields) => {
                for (_, field) in fields {
                    let cid = field.cid();
                    if self.schemas.get::<Structure>(&cid).is_none() {
                        self.load_schema(cid)?;
                    }
                }
            }
            Structure::Map { key: k, value: v } | Structure::OrderedMap { key: k, value: v } => {
                let kk = k.cid();
                let vk = v.cid();
                if self.schemas.get::<Structure>(&kk).is_none() {
                    self.load_schema(kk)?;
                }
                if self.schemas.get::<Structure>(&vk).is_none() {
                    self.load_schema(vk)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn rebuild_tree(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.nodes.clear();
        self.roots.clear();

        let bytes = self
            .store
            .get(&self.root_cid)?
            .ok_or_else(|| format!("value not found: {}", self.root_cid))?;

        let ipld = parse_to_ipld(&bytes)?;
        let schema_cell = self.load_schema(self.root_schema_cid)?;
        let schema = schema_cell.value();

        let node_id = NodeId::root(&self.root_cid);
        let label = short_cid(&self.root_cid);

        self.build_node(&node_id, &label, &ipld, schema, 0)?;
        self.roots.push(node_id);

        Ok(())
    }

    fn build_node(
        &mut self,
        node_id: &NodeId,
        label: &str,
        ipld: &Ipld,
        schema: &Structure,
        depth: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let type_hint = self.schema_to_type_hint(schema);
        let display = self.format_node_display(label, ipld, schema);
        let cid = self.extract_cid(ipld);

        let children = self.collect_children(node_id, ipld, schema, depth + 1)?;

        self.nodes.insert(
            node_id.clone(),
            NodeData {
                cid,
                schema_cid: self.root_schema_cid, // simplified
                type_hint,
                ipld: Some(ipld.clone()),
                display,
                depth,
                children,
            },
        );

        Ok(())
    }

    fn collect_children(
        &mut self,
        parent_id: &NodeId,
        ipld: &Ipld,
        schema: &Structure,
        depth: usize,
    ) -> Result<Vec<NodeId>, Box<dyn std::error::Error>> {
        let mut children = Vec::new();

        match schema {
            Structure::Record(fields) => {
                if let Ipld::Map(map) = ipld {
                    for (name, field_schema_bond) in fields {
                        if let Some(fv) = map.get(name) {
                            if let Some(field_schema) = field_schema_bond.value() {
                                let child_id = NodeId::child(parent_id.as_str(), name);
                                self.build_node(&child_id, name, fv, field_schema, depth)?;
                                children.push(child_id);
                            }
                        }
                    }
                }
            }
            Structure::Sequence(inner) => {
                if let Ipld::List(arr) = ipld {
                    if let Some(inner_schema) = inner.value() {
                        for (i, elem) in arr.iter().enumerate() {
                            let idx = format!("[{}]", i);
                            let child_id = NodeId::child(parent_id.as_str(), &idx);
                            self.build_node(&child_id, &idx, elem, inner_schema, depth)?;
                            children.push(child_id);
                        }
                    }
                }
            }
            Structure::Tuple(elems) => {
                if let Ipld::List(arr) = ipld {
                    for (i, (elem_schema_bond, elem_val)) in
                        elems.iter().zip(arr.iter()).enumerate()
                    {
                        if let Some(elem_schema) = elem_schema_bond.value() {
                            let idx = format!("[{}]", i);
                            let child_id = NodeId::child(parent_id.as_str(), &idx);
                            self.build_node(&child_id, &idx, elem_val, elem_schema, depth)?;
                            children.push(child_id);
                        }
                    }
                }
            }
            Structure::Tagged(variants) => {
                if let Ipld::Map(map) = ipld {
                    if map.len() == 1 {
                        if let Some((name, val)) = map.iter().next() {
                            if let Some(variant_schema_bond) = variants.get(name) {
                                if let Some(variant_schema) = variant_schema_bond.value() {
                                    let child_id = NodeId::child(parent_id.as_str(), name);
                                    self.build_node(&child_id, name, val, variant_schema, depth)?;
                                    children.push(child_id);
                                }
                            }
                        }
                    }
                }
            }
            Structure::Map { value: v, .. } | Structure::OrderedMap { value: v, .. } => {
                if let Ipld::Map(map) = ipld {
                    if let Some(vs) = v.value() {
                        for (mk, mv) in map {
                            let child_id = NodeId::child(parent_id.as_str(), mk);
                            self.build_node(&child_id, mk, mv, vs, depth)?;
                            children.push(child_id);
                        }
                    }
                }
            }
            Structure::Bond(inner) => {
                // For bonds, load the referenced value if available
                if let Ipld::Link(target_cid) = ipld {
                    if let Ok(Some(target_bytes)) = self.store.get(target_cid) {
                        if let Ok(target_ipld) = parse_to_ipld(&target_bytes) {
                            if let Some(inner_schema) = inner.value() {
                                let nested = self.collect_children(
                                    parent_id,
                                    &target_ipld,
                                    inner_schema,
                                    depth,
                                )?;
                                children.extend(nested);
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(children)
    }

    fn format_node_display(&self, label: &str, ipld: &Ipld, schema: &Structure) -> String {
        let type_hint = self.schema_to_type_hint(schema);

        match (ipld, schema) {
            (Ipld::Link(cid), Structure::Bond(_)) => {
                format!("{}: {} → {}", label, type_hint, short_cid(cid))
            }
            (Ipld::String(s), _) => {
                let truncated = if has_more_than_n_graphemes(s, 30) {
                    format!("\"{}...\"", truncate_str(s, 27))
                } else {
                    format!("\"{}\"", s)
                };
                format!("{}: {} = {}", label, type_hint, truncated)
            }
            (Ipld::Integer(n), _) => format!("{}: {} = {}", label, type_hint, n),
            (Ipld::Float(f), _) => format!("{}: {} = {}", label, type_hint, f),
            (Ipld::Bool(b), _) => format!("{}: {} = {}", label, type_hint, b),
            (Ipld::Bytes(b), _) => format!("{}: {} ({} bytes)", label, type_hint, b.len()),
            (Ipld::List(arr), _) => format!("{}: {} ({} items)", label, type_hint, arr.len()),
            (Ipld::Map(_), _) => format!("{}: {}", label, type_hint),
            (Ipld::Null, _) => format!("{}: {} = null", label, type_hint),
            _ => format!("{}: {}", label, type_hint),
        }
    }

    fn schema_to_type_hint(&self, schema: &Structure) -> String {
        match schema {
            Structure::Bool => "Bool".to_string(),
            Structure::Char => "Char".to_string(),
            Structure::Unicode => "String".to_string(),
            Structure::ByteString => "Bytes".to_string(),
            Structure::Int(t) => format!("{:?}", t),
            Structure::Float(t) => format!("{:?}", t),
            Structure::Unit => "Unit".to_string(),
            Structure::Sequence(inner) => {
                let inner_hint = inner
                    .value()
                    .map(|s| self.schema_to_type_hint(s))
                    .unwrap_or_else(|| "?".to_string());
                format!("Seq<{}>", inner_hint)
            }
            Structure::Tuple(elems) => {
                let hints: Vec<_> = elems
                    .iter()
                    .map(|e| {
                        e.value()
                            .map(|s| self.schema_to_type_hint(s))
                            .unwrap_or_else(|| "?".to_string())
                    })
                    .collect();
                format!("({})", hints.join(", "))
            }
            Structure::Record(fields) => {
                let names: Vec<_> = fields.keys().cloned().collect();
                if names.len() <= 3 {
                    format!("Record{{{}}}", names.join(", "))
                } else {
                    format!("Record{{{}...}}", names[..2].join(", "))
                }
            }
            Structure::Tagged(variants) => {
                let names: Vec<_> = variants.keys().cloned().collect();
                if names.len() <= 3 {
                    format!("Tagged{{{}}}", names.join("|"))
                } else {
                    format!("Tagged{{{}|...}}", names[..2].join("|"))
                }
            }
            Structure::Enum(variants) => {
                if variants.len() <= 3 {
                    format!("Enum{{{}}}", variants.join("|"))
                } else {
                    format!("Enum{{{}|...}}", variants[..2].join("|"))
                }
            }
            Structure::Map { .. } => "Map".to_string(),
            Structure::OrderedMap { .. } => "OrderedMap".to_string(),
            Structure::Bond(inner) => {
                let inner_hint = inner
                    .value()
                    .map(|s| self.schema_to_type_hint(s))
                    .unwrap_or_else(|| "?".to_string());
                format!("Bond<{}>", inner_hint)
            }
            Structure::SelfRef(n) => format!("SelfRef({})", n),
        }
    }

    fn extract_cid(&self, ipld: &Ipld) -> Option<Cid> {
        if let Ipld::Link(cid) = ipld {
            Some(*cid)
        } else {
            None
        }
    }

    /// Build tree items for tui-tree-widget.
    pub fn tree_items(&self) -> Vec<TreeItem<'_, NodeId>> {
        self.build_tree_items(&self.roots)
    }

    fn build_tree_items(&self, node_ids: &[NodeId]) -> Vec<TreeItem<'_, NodeId>> {
        node_ids
            .iter()
            .filter_map(|id| self.build_tree_item(id))
            .collect()
    }

    fn build_tree_item(&self, node_id: &NodeId) -> Option<TreeItem<'_, NodeId>> {
        let node = self.nodes.get(node_id)?;

        if node.children.is_empty() {
            Some(TreeItem::new_leaf(node_id.clone(), node.display.as_str()))
        } else {
            let children = self.build_tree_items(&node.children);
            TreeItem::new(node_id.clone(), node.display.as_str(), children).ok()
        }
    }

    /// Get node data by ID.
    pub fn get_node(&self, id: &NodeId) -> Option<&NodeData> {
        self.nodes.get(id)
    }

    /// Zoom into a bond node by ID.
    pub fn zoom_in(&mut self, node_id: &NodeId) -> Result<bool, Box<dyn std::error::Error>> {
        let node = match self.nodes.get(node_id) {
            Some(n) if n.cid.is_some() => n.clone(),
            _ => return Ok(false),
        };

        let target_cid = node.cid.unwrap();

        // Save current state to breadcrumb
        self.breadcrumbs.push(Breadcrumb {
            cid: self.root_cid,
            schema_cid: self.root_schema_cid,
            label: short_cid(&self.root_cid),
        });

        // Load schema for bond target
        let schema_cell = self.load_schema(node.schema_cid)?;
        let inner_schema_cid = if let Structure::Bond(inner) = schema_cell.value() {
            inner.cid()
        } else {
            node.schema_cid
        };

        self.root_cid = target_cid;
        self.root_schema_cid = inner_schema_cid;
        self.rebuild_tree()?;

        Ok(true)
    }

    /// Zoom out to the previous view.
    pub fn zoom_out(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let crumb = match self.breadcrumbs.pop() {
            Some(c) => c,
            None => return Ok(false),
        };

        self.root_cid = crumb.cid;
        self.root_schema_cid = crumb.schema_cid;
        self.rebuild_tree()?;

        Ok(true)
    }

    /// Zoom into the schema of a node. Schema CID is used as both data and schema.
    pub fn zoom_to_schema(&mut self, node_id: &NodeId) -> Result<bool, Box<dyn std::error::Error>> {
        let node = match self.nodes.get(node_id) {
            Some(n) => n.clone(),
            None => return Ok(false),
        };

        let schema_cid = node.schema_cid;

        // Save current state to breadcrumb
        self.breadcrumbs.push(Breadcrumb {
            cid: self.root_cid,
            schema_cid: self.root_schema_cid,
            label: short_cid(&self.root_cid),
        });

        // Use schema CID as both data and schema (schema is self-describing)
        self.root_cid = schema_cid;
        self.root_schema_cid = self.schemas.add(Structure::schema()).cid();
        self.rebuild_tree()?;

        Ok(true)
    }

    /// Get breadcrumb path string.
    pub fn breadcrumb_path(&self) -> String {
        let mut parts: Vec<String> = self.breadcrumbs.iter().map(|b| b.label.clone()).collect();
        parts.push(short_cid(&self.root_cid));
        parts.join(" > ")
    }

    /// Access the store.
    pub fn store(&self) -> &AnyStore {
        &self.store
    }

    /// Access schemas.
    pub fn schemas(&self) -> &Solvent {
        &self.schemas
    }

    /// Get current root CID.
    pub fn root_cid(&self) -> Cid {
        self.root_cid
    }

    /// Get current root schema CID.
    pub fn root_schema_cid(&self) -> Cid {
        self.root_schema_cid
    }
}

/// Format a CID as a short string.
fn short_cid(cid: &Cid) -> String {
    let s = cid.to_string();
    if has_more_than_n_graphemes(&s, 12) {
        format!("{}...", truncate_str(&s, 12))
    } else {
        s
    }
}
