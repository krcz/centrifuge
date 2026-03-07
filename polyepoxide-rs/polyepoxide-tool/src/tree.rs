//! Lazy tree model for polyepoxide graph exploration.

use std::collections::HashMap;
use std::sync::Arc;

use cid::Cid;
use ipld_core::ipld::Ipld;
use polyepoxide_core::{
    Cell, CursorState, Oxide, Solvent, StoreCursor, Structure, load_schema_recursive,
};
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
    /// Active schema scope for this node.
    pub schema_scope: Vec<Cid>,
    /// Human-readable type hint.
    pub type_hint: String,
    /// Display string for the node.
    pub display: String,
    /// Child node IDs.
    pub children: Vec<NodeId>,
    /// Active reflexive context scope for this node.
    pub context: Vec<Cid>,
    /// Cursor state needed for zooming or resolving nested schemas.
    pub cursor_state: CursorState,
}

/// Breadcrumb entry for zoom navigation.
#[derive(Debug, Clone)]
pub struct Breadcrumb {
    pub cid: Cid,
    pub schema_cid: Cid,
    pub context: Vec<Cid>,
    pub schema_scope: Vec<Cid>,
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
    /// Current reflexive context.
    root_context: Vec<Cid>,
    /// Current schema reflexive context.
    root_schema_scope: Vec<Cid>,
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
            root_context: Vec::new(),
            root_schema_scope: Vec::new(),
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
        Ok(load_schema_recursive(&self.store, &self.schemas, cid)?)
    }

    fn rebuild_tree(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.nodes.clear();
        self.roots.clear();
        let root_cursor =
            StoreCursor::new(&self.store, &self.schemas, self.root_cid, self.root_schema_cid)?;
        let root_state = root_cursor.state();
        self.root_schema_cid = root_state.schema_cid;
        self.root_schema_scope = root_state.schema_scope.clone();
        let ipld = root_cursor.ipld()?;
        let node_id = NodeId::root(&self.root_cid);
        let label = short_cid(&self.root_cid);
        self.build_node(&node_id, &label, &ipld, root_state)?;
        self.roots.push(node_id);

        Ok(())
    }

    fn build_node(
        &mut self,
        node_id: &NodeId,
        label: &str,
        ipld: &Ipld,
        cursor_state: CursorState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cursor = StoreCursor::from_state(&self.store, &self.schemas, cursor_state.clone());
        let schema = cursor.schema()?.value().clone();
        let schema_cid = cursor.schema_cid();
        let schema_scope = cursor.schema_scope().to_vec();
        let context = cursor.scope().to_vec();
        let type_hint = self.schema_to_type_hint(&schema);
        let display = self.format_node_display(label, ipld, &schema);
        let cid = self.extract_cid(ipld, &schema, &context);
        drop(cursor);
        let children = self.collect_children(node_id, ipld, cursor_state.clone())?;

        self.nodes.insert(
            node_id.clone(),
            NodeData {
                cid,
                schema_cid,
                schema_scope,
                type_hint,
                display,
                children,
                context,
                cursor_state,
            },
        );

        Ok(())
    }

    fn collect_children(
        &mut self,
        parent_id: &NodeId,
        ipld: &Ipld,
        cursor_state: CursorState,
    ) -> Result<Vec<NodeId>, Box<dyn std::error::Error>> {
        let mut children = Vec::new();
        let cursor = StoreCursor::from_state(&self.store, &self.schemas, cursor_state.clone());
        let schema = cursor.schema()?.value().clone();
        drop(cursor);

        match &schema {
            Structure::Record(fields) => {
                if let Ipld::Map(map) = ipld {
                    for (name, field_schema_bond) in fields {
                        if let Some(fv) = map.get(name) {
                            let field_state =
                                self.child_schema_state(&cursor_state, field_schema_bond.cid())?;
                            let child_id = NodeId::child(parent_id.as_str(), name);
                            self.build_node(&child_id, name, fv, field_state)?;
                            children.push(child_id);
                        }
                    }
                }
            }
            Structure::Option(inner) => {
                let inner_state = self.child_schema_state(&cursor_state, inner.cid())?;
                match ipld {
                    Ipld::List(arr) => {
                        for (i, elem) in arr.iter().enumerate() {
                            let idx = format!("[{}]", i);
                            let child_id = NodeId::child(parent_id.as_str(), &idx);
                            self.build_node(&child_id, &idx, elem, inner_state.clone())?;
                            children.push(child_id);
                        }
                    }
                    Ipld::Null => {}
                    value => {
                        let child_id = NodeId::child(parent_id.as_str(), "some");
                        self.build_node(&child_id, "some", value, inner_state)?;
                        children.push(child_id);
                    }
                }
            }
            Structure::Sequence(inner) => {
                if let Ipld::List(arr) = ipld {
                    let inner_state = self.child_schema_state(&cursor_state, inner.cid())?;
                    for (i, elem) in arr.iter().enumerate() {
                        let idx = format!("[{}]", i);
                        let child_id = NodeId::child(parent_id.as_str(), &idx);
                        self.build_node(&child_id, &idx, elem, inner_state.clone())?;
                        children.push(child_id);
                    }
                }
            }
            Structure::Tuple(elems) => {
                if let Ipld::List(arr) = ipld {
                    for (i, (elem_schema_bond, elem_val)) in
                        elems.iter().zip(arr.iter()).enumerate()
                    {
                        let elem_state =
                            self.child_schema_state(&cursor_state, elem_schema_bond.cid())?;
                        let idx = format!("[{}]", i);
                        let child_id = NodeId::child(parent_id.as_str(), &idx);
                        self.build_node(&child_id, &idx, elem_val, elem_state)?;
                        children.push(child_id);
                    }
                }
            }
            Structure::Tagged(variants) => {
                if let Ipld::Map(map) = ipld {
                    if map.len() == 1 {
                        if let Some((name, val)) = map.iter().next() {
                            if let Some(variant_schema_bond) = variants.get(name) {
                                let variant_state = self
                                    .child_schema_state(&cursor_state, variant_schema_bond.cid())?;
                                let child_id = NodeId::child(parent_id.as_str(), name);
                                self.build_node(&child_id, name, val, variant_state)?;
                                children.push(child_id);
                            }
                        }
                    }
                }
            }
            Structure::Map { value: v, .. } => {
                if let Ipld::Map(map) = ipld {
                    let value_state = self.child_schema_state(&cursor_state, v.cid())?;
                    for (mk, mv) in map {
                        let child_id = NodeId::child(parent_id.as_str(), mk);
                        self.build_node(&child_id, mk, mv, value_state.clone())?;
                        children.push(child_id);
                    }
                }
            }
            Structure::OrderedMap { key: k, value: v } => {
                if let Ipld::List(entries) = ipld {
                    let key_state = self.child_schema_state(&cursor_state, k.cid())?;
                    let value_state = self.child_schema_state(&cursor_state, v.cid())?;
                    for (i, entry) in entries.iter().enumerate() {
                        if let Ipld::List(pair) = entry {
                            if pair.len() == 2 {
                                let key_label = format!("[{}].key", i);
                                let key_id = NodeId::child(parent_id.as_str(), &key_label);
                                self.build_node(&key_id, &key_label, &pair[0], key_state.clone())?;
                                children.push(key_id);

                                let val_label = format!("[{}].value", i);
                                let val_id = NodeId::child(parent_id.as_str(), &val_label);
                                self.build_node(
                                    &val_id,
                                    &val_label,
                                    &pair[1],
                                    value_state.clone(),
                                )?;
                                children.push(val_id);
                            }
                        }
                    }
                }
            }
            Structure::Bond(inner) => {
                if let Ipld::Link(target_cid) = ipld {
                    let (followed_state, target_ipld) =
                        self.follow_bond_state(&cursor_state, *target_cid, inner.cid())?;
                    let nested = self.collect_children(parent_id, &target_ipld, followed_state)?;
                    children.extend(nested);
                }
            }
            Structure::Enum(_) => {}
            Structure::Bool
            | Structure::Char
            | Structure::Unicode
            | Structure::ByteString
            | Structure::Cid
            | Structure::Int(_)
            | Structure::Float(_)
            | Structure::Unit => {}
        }

        Ok(children)
    }

    fn child_schema_state(
        &self,
        cursor_state: &CursorState,
        child_schema_cid: Cid,
    ) -> Result<CursorState, Box<dyn std::error::Error>> {
        let cursor = StoreCursor::from_state(&self.store, &self.schemas, cursor_state.clone());
        Ok(cursor.child_schema_cursor(child_schema_cid)?.state())
    }

    fn follow_bond_state(
        &self,
        cursor_state: &CursorState,
        target_cid: Cid,
        inner_schema_cid: Cid,
    ) -> Result<(CursorState, Ipld), Box<dyn std::error::Error>> {
        let cursor = StoreCursor::from_state(&self.store, &self.schemas, cursor_state.clone());
        let followed = cursor.follow_bond(target_cid, inner_schema_cid)?;
        let ipld = followed.ipld()?;
        Ok((followed.state(), ipld))
    }

    fn format_node_display(&self, label: &str, ipld: &Ipld, schema: &Structure) -> String {
        let type_hint = self.schema_to_type_hint(schema);

        match (ipld, schema) {
            (Ipld::Link(cid), Structure::Bond(_)) => {
                format!("{}: {} → {}", label, type_hint, short_cid(cid))
            }
            (Ipld::Link(cid), Structure::Cid) => {
                format!("{}: {} = {}", label, type_hint, short_cid(cid))
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
            Structure::Cid => "Cid".to_string(),
            Structure::Int(t) => format!("{:?}", t),
            Structure::Float(t) => format!("{:?}", t),
            Structure::Unit => "Unit".to_string(),
            Structure::Option(inner) => {
                let inner_hint = inner
                    .value()
                    .map(|s| self.schema_to_type_hint(s))
                    .unwrap_or_else(|| "?".to_string());
                format!("Option<{}>", inner_hint)
            }
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
        }
    }

    fn extract_cid(&self, ipld: &Ipld, schema: &Structure, context: &[Cid]) -> Option<Cid> {
        match (ipld, schema) {
            (Ipld::Link(cid), Structure::Bond(_)) => {
                match polyepoxide_core::resolve_reflexive_with_store(&self.store, *cid, context) {
                    Ok(Some((resolved, _))) => Some(resolved),
                    _ => Some(*cid),
                }
            }
            (Ipld::Link(cid), _) => Some(*cid),
            _ => None,
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
            context: self.root_context.clone(),
            schema_scope: self.root_schema_scope.clone(),
            label: short_cid(&self.root_cid),
        });

        let schema_cursor = StoreCursor::from_state(&self.store, &self.schemas, node.cursor_state);
        let schema_cell = schema_cursor.schema()?;
        let target_cursor = if let Structure::Bond(inner) = schema_cell.value() {
            schema_cursor.follow_bond(target_cid, inner.cid())?
        } else {
            schema_cursor
        };

        self.root_cid = target_cid;
        self.root_schema_cid = target_cursor.schema_cid();
        self.root_context = target_cursor.scope().to_vec();
        self.root_schema_scope = target_cursor.schema_scope().to_vec();
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
        self.root_context = crumb.context;
        self.root_schema_scope = crumb.schema_scope;
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
            context: self.root_context.clone(),
            schema_scope: self.root_schema_scope.clone(),
            label: short_cid(&self.root_cid),
        });

        // Use schema CID as both data and schema (schema is self-describing)
        self.root_cid = schema_cid;
        self.root_schema_cid = Structure::schema().cid();
        self.root_context = Vec::new();
        self.root_schema_scope = Vec::new();
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
