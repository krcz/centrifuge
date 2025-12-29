//! Integration tests demonstrating nested structures with bonds.

use polyepoxide_core::{oxide, Bond, BondVisitor, Cid, Oxide, Solvent, Structure};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A simple node with a value and optional reference to another node.
#[derive(Debug, Clone, Serialize, Deserialize, Oxide)]
struct LinkedNode {
    value: String,
    next: Option<Bond<LinkedNode>>,
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

    assert_eq!(left_next.cid(), right_next.cid());

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
    let target_cid = target_cell.cid();

    let source = LinkedNode {
        value: "source".to_string(),
        next: Some(Bond::from_cell(Arc::clone(&target_cell))),
    };

    // Serialize and deserialize
    let bytes = source.to_bytes();
    let recovered: LinkedNode = Oxide::from_bytes(&bytes).unwrap();

    // After deserialization, the bond should be unresolved but have the same CID
    let recovered_bond = recovered.next.as_ref().unwrap();
    assert!(!recovered_bond.is_resolved());
    assert_eq!(recovered_bond.cid(), target_cid);
}

/// A tree node with multiple children.
#[derive(Debug, Clone, Serialize, Deserialize, Oxide)]
struct TreeNode {
    label: String,
    children: Vec<Bond<TreeNode>>,
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

    // Collect all referenced CIDs using BondVisitor
    struct CidCollector {
        cids: Vec<Cid>,
    }

    impl BondVisitor for CidCollector {
        fn visit_bond(&mut self, cid: &Cid) {
            self.cids.push(*cid);
        }
    }

    let mut collector = CidCollector { cids: vec![] };
    root_cell.value().visit_bonds(&mut collector);

    // Should have collected the leaf's CID
    assert_eq!(collector.cids.len(), 1);
    assert_eq!(collector.cids[0], leaf_cell.cid());
}

// --- Derive macro feature tests ---

/// Test the #[oxide] attribute macro (syntax sugar for all derives)
#[oxide]
struct SimplePoint {
    x: f64,
    y: f64,
}

#[test]
fn oxide_attribute_macro() {
    let mut solvent = Solvent::new();
    let point = SimplePoint { x: 1.0, y: 2.0 };
    let cell = solvent.add(point);

    // Verify schema
    if let Structure::Record(fields) = SimplePoint::schema() {
        assert_eq!(fields.len(), 2);
        assert!(fields.contains_key("x"));
        assert!(fields.contains_key("y"));
    } else {
        panic!("Expected Record schema");
    }

    // Verify roundtrip
    let bytes = cell.value().to_bytes();
    let recovered: SimplePoint = Oxide::from_bytes(&bytes).unwrap();
    assert_eq!(recovered.x, 1.0);
    assert_eq!(recovered.y, 2.0);
}

/// Test that #[oxide] adds correct serde attributes for Option (array encoding)
#[oxide]
#[derive(PartialEq)]
struct WithOptionalField {
    name: String,
    count: Option<u32>,
}

#[test]
fn oxide_option_array_encoding() {
    // Test None case
    let v1 = WithOptionalField {
        name: "test".to_string(),
        count: None,
    };
    let bytes1 = v1.to_bytes();
    let recovered1: WithOptionalField = Oxide::from_bytes(&bytes1).unwrap();
    assert_eq!(recovered1, v1);

    // Test Some case
    let v2 = WithOptionalField {
        name: "test".to_string(),
        count: Some(42),
    };
    let bytes2 = v2.to_bytes();
    let recovered2: WithOptionalField = Oxide::from_bytes(&bytes2).unwrap();
    assert_eq!(recovered2, v2);
}

/// Test that #[oxide] adds correct serde attributes for Result (lowercase keys)
#[oxide]
#[derive(PartialEq)]
struct WithResultField {
    result: Result<i32, String>,
}

#[test]
fn oxide_result_lowercase_encoding() {
    // Test Ok case
    let v1 = WithResultField { result: Ok(42) };
    let bytes1 = v1.to_bytes();
    let recovered1: WithResultField = Oxide::from_bytes(&bytes1).unwrap();
    assert_eq!(recovered1, v1);

    // Test Err case
    let v2 = WithResultField {
        result: Err("error".to_string()),
    };
    let bytes2 = v2.to_bytes();
    let recovered2: WithResultField = Oxide::from_bytes(&bytes2).unwrap();
    assert_eq!(recovered2, v2);
}

/// Test C-style enum (all unit variants)
#[derive(Debug, Clone, Serialize, Deserialize, Oxide, PartialEq)]
enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn enum_unit_variants() {
    // Schema should be Enum
    if let Structure::Enum(variants) = Color::schema() {
        assert_eq!(variants, vec!["Red", "Green", "Blue"]);
    } else {
        panic!("Expected Enum schema");
    }

    // Roundtrip
    let color = Color::Green;
    let bytes = color.to_bytes();
    let recovered: Color = Oxide::from_bytes(&bytes).unwrap();
    assert_eq!(recovered, Color::Green);
}

/// Test tagged union enum (variants with data)
#[derive(Debug, Clone, Serialize, Deserialize, Oxide, PartialEq)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Point,
}

#[test]
fn enum_tagged_variants() {
    // Schema should be Tagged
    if let Structure::Tagged(variants) = Shape::schema() {
        assert_eq!(variants.len(), 3);
        assert!(variants.contains_key("Circle"));
        assert!(variants.contains_key("Rectangle"));
        assert!(variants.contains_key("Point"));
    } else {
        panic!("Expected Tagged schema");
    }

    // Roundtrip
    let shape = Shape::Rectangle {
        width: 10.0,
        height: 20.0,
    };
    let bytes = shape.to_bytes();
    let recovered: Shape = Oxide::from_bytes(&bytes).unwrap();
    assert_eq!(recovered, shape);
}

/// Test tuple struct
#[derive(Debug, Clone, Serialize, Deserialize, Oxide, PartialEq)]
struct Pair(String, i32);

#[test]
fn tuple_struct() {
    // Schema should be Tuple
    if let Structure::Tuple(elems) = Pair::schema() {
        assert_eq!(elems.len(), 2);
    } else {
        panic!("Expected Tuple schema");
    }

    // Roundtrip
    let pair = Pair("test".to_string(), 42);
    let bytes = pair.to_bytes();
    let recovered: Pair = Oxide::from_bytes(&bytes).unwrap();
    assert_eq!(recovered, pair);
}

/// Test unit struct
#[derive(Debug, Clone, Serialize, Deserialize, Oxide, PartialEq)]
struct Marker;

#[test]
fn unit_struct() {
    // Schema should be Unit
    assert!(matches!(Marker::schema(), Structure::Unit));

    // Roundtrip
    let bytes = Marker.to_bytes();
    let _recovered: Marker = Oxide::from_bytes(&bytes).unwrap();
}

/// Test #[oxide(rename)] attribute
#[derive(Debug, Clone, Serialize, Deserialize, Oxide)]
struct RenamedFields {
    #[oxide(rename = "firstName")]
    first_name: String,
    #[oxide(rename = "lastName")]
    last_name: String,
}

#[test]
fn oxide_rename_attribute() {
    if let Structure::Record(fields) = RenamedFields::schema() {
        assert!(fields.contains_key("firstName"));
        assert!(fields.contains_key("lastName"));
        assert!(!fields.contains_key("first_name"));
    } else {
        panic!("Expected Record schema");
    }
}

/// Test #[oxide(skip)] attribute
#[derive(Debug, Clone, Serialize, Deserialize, Oxide)]
#[allow(dead_code)]
struct WithSkipped {
    name: String,
    #[oxide(skip)]
    #[serde(skip)]
    cached_value: i32,
}

#[test]
fn oxide_skip_attribute() {
    if let Structure::Record(fields) = WithSkipped::schema() {
        assert!(fields.contains_key("name"));
        assert!(!fields.contains_key("cached_value"));
    } else {
        panic!("Expected Record schema");
    }
}

/// Test generic struct
#[derive(Debug, Clone, Serialize, Deserialize, Oxide)]
#[serde(bound = "T: Oxide")]
struct Wrapper<T: Oxide> {
    inner: T,
}

#[test]
fn generic_struct() {
    // Schema for Wrapper<String>
    let schema = <Wrapper<String>>::schema();
    if let Structure::Record(fields) = schema {
        assert!(fields.contains_key("inner"));
    } else {
        panic!("Expected Record schema");
    }

    // Roundtrip
    let wrapper = Wrapper {
        inner: "hello".to_string(),
    };
    let bytes = wrapper.to_bytes();
    let recovered: Wrapper<String> = Oxide::from_bytes(&bytes).unwrap();
    assert_eq!(recovered.inner, "hello");
}
