//! Integration tests demonstrating nested structures with bonds.

use polyepoxide_core::{Bond, BondMapper, BondVisitor, Key, Oxide, Structure, Solvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A simple node with a value and optional reference to another node.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinkedNode {
    value: String,
    next: Option<Bond<LinkedNode>>,
}

impl Oxide for LinkedNode {
    fn schema() -> Structure {
        Structure::record([
            ("value", Structure::Unicode),
            ("next", Structure::option(Structure::bond(Structure::SelfRef(0)))),
        ])
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        if let Some(bond) = &self.next {
            bond.visit_bonds(visitor);
        }
    }

    fn map_bonds(&self, mapper: &mut impl BondMapper) -> Self {
        LinkedNode {
            value: self.value.clone(),
            next: self.next.as_ref().map(|b| b.map_bonds(mapper)),
        }
    }
}

#[test]
fn linked_list_single() {
    let mut solvent = Solvent::new();

    let node = LinkedNode {
        value: "only node".to_string(),
        next: None,
    };

    let cell = solvent.add(node);
    assert_eq!(cell.value().value, "only node");
    assert!(cell.value().next.is_none());
    assert_eq!(solvent.len(), 1);
}

#[test]
fn linked_list_chain() {
    let mut solvent = Solvent::new();

    // Create a chain: node3 -> node2 -> node1
    let node1 = LinkedNode {
        value: "first".to_string(),
        next: None,
    };
    let cell1 = solvent.add(node1);

    let node2 = LinkedNode {
        value: "second".to_string(),
        next: Some(Bond::from_cell(Arc::clone(&cell1))),
    };
    let cell2 = solvent.add(node2);

    let node3 = LinkedNode {
        value: "third".to_string(),
        next: Some(Bond::from_cell(Arc::clone(&cell2))),
    };
    let cell3 = solvent.add(node3);

    // Verify the chain
    assert_eq!(cell3.value().value, "third");

    let next = cell3.value().next.as_ref().unwrap();
    assert!(next.is_resolved());
    assert_eq!(next.value().unwrap().value, "second");

    let next_next = next.value().unwrap().next.as_ref().unwrap();
    assert!(next_next.is_resolved());
    assert_eq!(next_next.value().unwrap().value, "first");

    // Should have 3 nodes in solvent
    assert_eq!(solvent.len(), 3);
}

#[test]
fn deduplication_in_dag() {
    let mut solvent = Solvent::new();

    // Create a shared node
    let shared = LinkedNode {
        value: "shared".to_string(),
        next: None,
    };
    let shared_cell = solvent.add(shared);

    // Two nodes pointing to the same shared node
    let left = LinkedNode {
        value: "left".to_string(),
        next: Some(Bond::from_cell(Arc::clone(&shared_cell))),
    };
    let right = LinkedNode {
        value: "right".to_string(),
        next: Some(Bond::from_cell(Arc::clone(&shared_cell))),
    };

    let left_cell = solvent.add(left);
    let right_cell = solvent.add(right);

    // Both should point to the same shared node
    let left_next = left_cell.value().next.as_ref().unwrap();
    let right_next = right_cell.value().next.as_ref().unwrap();

    assert_eq!(left_next.key(), right_next.key());

    // Should have 3 nodes total (shared is not duplicated)
    assert_eq!(solvent.len(), 3);
}

#[test]
fn bond_serialization_preserves_reference() {
    let mut solvent = Solvent::new();

    let target = LinkedNode {
        value: "target".to_string(),
        next: None,
    };
    let target_cell = solvent.add(target);
    let target_key = target_cell.key();

    let source = LinkedNode {
        value: "source".to_string(),
        next: Some(Bond::from_cell(Arc::clone(&target_cell))),
    };

    // Serialize and deserialize
    let bytes = source.to_bytes();
    let recovered: LinkedNode = Oxide::from_bytes(&bytes).unwrap();

    // After deserialization, the bond should be unresolved but have the same key
    let recovered_bond = recovered.next.as_ref().unwrap();
    assert!(!recovered_bond.is_resolved());
    assert_eq!(recovered_bond.key(), target_key);
}

/// A tree node with multiple children.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TreeNode {
    label: String,
    children: Vec<Bond<TreeNode>>,
}

impl Oxide for TreeNode {
    fn schema() -> Structure {
        Structure::record([
            ("label", Structure::Unicode),
            ("children", Structure::sequence(Structure::bond(Structure::SelfRef(0)))),
        ])
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        for child in &self.children {
            child.visit_bonds(visitor);
        }
    }

    fn map_bonds(&self, mapper: &mut impl BondMapper) -> Self {
        TreeNode {
            label: self.label.clone(),
            children: self.children.iter().map(|c| c.map_bonds(mapper)).collect(),
        }
    }
}

#[test]
fn tree_structure() {
    let mut solvent = Solvent::new();

    // Build a tree:
    //       root
    //      /    \
    //    left   right
    //    /
    //  leaf

    let leaf = TreeNode {
        label: "leaf".to_string(),
        children: vec![],
    };
    let leaf_cell = solvent.add(leaf);

    let left = TreeNode {
        label: "left".to_string(),
        children: vec![Bond::from_cell(Arc::clone(&leaf_cell))],
    };
    let left_cell = solvent.add(left);

    let right = TreeNode {
        label: "right".to_string(),
        children: vec![],
    };
    let right_cell = solvent.add(right);

    let root = TreeNode {
        label: "root".to_string(),
        children: vec![
            Bond::from_cell(Arc::clone(&left_cell)),
            Bond::from_cell(Arc::clone(&right_cell)),
        ],
    };
    let root_cell = solvent.add(root);

    // Verify structure
    assert_eq!(root_cell.value().label, "root");
    assert_eq!(root_cell.value().children.len(), 2);

    let left_bond = &root_cell.value().children[0];
    assert_eq!(left_bond.value().unwrap().label, "left");
    assert_eq!(left_bond.value().unwrap().children.len(), 1);

    let leaf_bond = &left_bond.value().unwrap().children[0];
    assert_eq!(leaf_bond.value().unwrap().label, "leaf");

    // Total nodes
    assert_eq!(solvent.len(), 4);
}

#[test]
fn collect_all_keys() {
    let mut solvent = Solvent::new();

    let leaf = TreeNode {
        label: "leaf".to_string(),
        children: vec![],
    };
    let leaf_cell = solvent.add(leaf);

    let root = TreeNode {
        label: "root".to_string(),
        children: vec![Bond::from_cell(Arc::clone(&leaf_cell))],
    };
    let root_cell = solvent.add(root);

    // Collect all referenced keys using BondVisitor
    struct KeyCollector {
        keys: Vec<Key>,
    }

    impl BondVisitor for KeyCollector {
        fn visit_bond(&mut self, key: &Key) {
            self.keys.push(*key);
        }
    }

    let mut collector = KeyCollector { keys: vec![] };
    root_cell.value().visit_bonds(&mut collector);

    // Should have collected the leaf's key
    assert_eq!(collector.keys.len(), 1);
    assert_eq!(collector.keys[0], leaf_cell.key());
}
