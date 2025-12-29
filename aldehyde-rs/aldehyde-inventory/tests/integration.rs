//! Integration tests for Aldehyde inventory system

use aldehyde_inventory::{
    Event, EventKind, EventLog, ExifData, ExifTag, ExifValue, Inventory, Item, Photo,
    PhotoRegistry, Placement, PlacementMap,
};
use polyepoxide_core::{Bond, ByteString, Oxide, Solvent};
use std::sync::Arc;

#[test]
fn create_item() {
    let mut solvent = Solvent::new();

    let item = Item {
        id: "item-001".to_string(),
        name: "Blue Widget".to_string(),
        description: Some("A small blue widget".to_string()),
    };

    let cell = solvent.add(item);
    assert_eq!(cell.value().name, "Blue Widget");
    assert_eq!(cell.value().id, "item-001");
    assert!(cell.value().description.is_some());
}

#[test]
fn create_photo_with_content() {
    let mut solvent = Solvent::new();

    let content = ByteString::new(vec![0x89, 0x50, 0x4E, 0x47]); // PNG magic bytes
    let content_cell = solvent.add(content);

    let photo = Photo {
        filename: "widget.png".to_string(),
        mime_type: "image/png".to_string(),
        width: Some(640),
        height: Some(480),
        exif: None,
        thumbnails: Vec::new(),
        content: Bond::from_cell(content_cell),
    };

    let photo_cell = solvent.add(photo);
    assert_eq!(photo_cell.value().filename, "widget.png");
    assert_eq!(photo_cell.value().width, Some(640));
    assert!(photo_cell.value().content.is_resolved());
}

#[test]
fn create_photo_with_exif() {
    let mut solvent = Solvent::new();

    // EXIF tag IDs
    const TAG_MAKE: u16 = 0x010F;
    const TAG_MODEL: u16 = 0x0110;
    const TAG_EXPOSURE_TIME: u16 = 0x829A;

    let exif = ExifData {
        tags: vec![
            ExifTag {
                id: TAG_MAKE,
                values: vec![ExifValue::Ascii("Canon".to_string())],
            },
            ExifTag {
                id: TAG_MODEL,
                values: vec![ExifValue::Ascii("EOS 5D".to_string())],
            },
            ExifTag {
                id: TAG_EXPOSURE_TIME,
                values: vec![ExifValue::Rational { num: 1, denom: 125 }],
            },
        ],
        camera_make: Some("Canon".to_string()),
        camera_model: Some("EOS 5D".to_string()),
        date_taken: Some("2024-01-15T10:30:00Z".to_string()),
        gps_latitude: Some(37.7749),
        gps_longitude: Some(-122.4194),
    };
    let exif_cell = solvent.add(exif);

    let content = ByteString::new(vec![0xFF, 0xD8, 0xFF]); // JPEG magic
    let content_cell = solvent.add(content);

    let photo = Photo {
        filename: "photo.jpg".to_string(),
        mime_type: "image/jpeg".to_string(),
        width: Some(4000),
        height: Some(3000),
        exif: Some(Bond::from_cell(exif_cell)),
        thumbnails: Vec::new(),
        content: Bond::from_cell(content_cell),
    };

    let photo_cell = solvent.add(photo);
    let exif_bond = photo_cell.value().exif.as_ref().unwrap();
    assert!(exif_bond.is_resolved());
    assert_eq!(
        exif_bond.value().unwrap().camera_make,
        Some("Canon".to_string())
    );
    assert_eq!(exif_bond.value().unwrap().tags.len(), 3);
}

#[test]
fn placement_hierarchy() {
    let mut solvent = Solvent::new();

    // Room -> Shelf -> Box hierarchy
    let room = Item {
        id: "room-001".to_string(),
        name: "Storage Room".to_string(),
        description: None,
    };
    let shelf = Item {
        id: "shelf-001".to_string(),
        name: "Metal Shelf".to_string(),
        description: None,
    };
    let box_item = Item {
        id: "box-001".to_string(),
        name: "Cardboard Box".to_string(),
        description: Some("Contains electronics".to_string()),
    };

    let _room_cell = solvent.add(room);
    let _shelf_cell = solvent.add(shelf);
    let _box_cell = solvent.add(box_item);

    // Shelf is inside Room
    let shelf_placement = Placement {
        item_id: "shelf-001".to_string(),
        location_id: Some("room-001".to_string()),
    };
    // Box is on Shelf
    let box_placement = Placement {
        item_id: "box-001".to_string(),
        location_id: Some("shelf-001".to_string()),
    };
    // Room is top-level
    let room_placement = Placement {
        item_id: "room-001".to_string(),
        location_id: None,
    };

    let p1 = solvent.add(room_placement);
    let p2 = solvent.add(shelf_placement);
    let p3 = solvent.add(box_placement);

    let map = PlacementMap {
        placements: vec![
            Bond::from_cell(p1),
            Bond::from_cell(p2),
            Bond::from_cell(p3),
        ],
    };

    let map_cell = solvent.add(map);
    assert_eq!(map_cell.value().placements.len(), 3);
}

#[test]
fn event_logging() {
    let mut solvent = Solvent::new();

    let create_event = Event {
        item_id: "item-001".to_string(),
        kind: EventKind::ItemCreated,
        timestamp: 1704067200000, // 2024-01-01 00:00:00 UTC
        target_id: None,
        note: Some("Initial creation".to_string()),
    };

    let scan_event = Event {
        item_id: "item-001".to_string(),
        kind: EventKind::ItemScanned,
        timestamp: 1704153600000, // 2024-01-02 00:00:00 UTC
        target_id: None,
        note: None,
    };

    let place_event = Event {
        item_id: "item-001".to_string(),
        kind: EventKind::ItemPlaced,
        timestamp: 1704240000000, // 2024-01-03 00:00:00 UTC
        target_id: Some("shelf-001".to_string()),
        note: None,
    };

    let e1 = solvent.add(create_event);
    let e2 = solvent.add(scan_event);
    let e3 = solvent.add(place_event);

    let log = EventLog {
        events: vec![
            Bond::from_cell(e1),
            Bond::from_cell(e2),
            Bond::from_cell(e3),
        ],
    };

    let log_cell = solvent.add(log);
    assert_eq!(log_cell.value().events.len(), 3);

    // Verify event kinds
    let events: Vec<_> = log_cell
        .value()
        .events
        .iter()
        .map(|b| b.value().unwrap().kind.clone())
        .collect();
    assert_eq!(events[0], EventKind::ItemCreated);
    assert_eq!(events[1], EventKind::ItemScanned);
    assert_eq!(events[2], EventKind::ItemPlaced);
}

#[test]
fn full_inventory() {
    let mut solvent = Solvent::new();

    // Create items
    let item1 = Item {
        id: "item-001".to_string(),
        name: "Laptop".to_string(),
        description: Some("Work laptop".to_string()),
    };
    let item2 = Item {
        id: "item-002".to_string(),
        name: "Desk".to_string(),
        description: None,
    };

    let item1_cell = solvent.add(item1);
    let item2_cell = solvent.add(item2);

    // Create placements
    let p1 = Placement {
        item_id: "item-001".to_string(),
        location_id: Some("item-002".to_string()), // Laptop on desk
    };
    let p2 = Placement {
        item_id: "item-002".to_string(),
        location_id: None, // Desk is top-level
    };

    let p1_cell = solvent.add(p1);
    let p2_cell = solvent.add(p2);

    let placements = PlacementMap {
        placements: vec![Bond::from_cell(p1_cell), Bond::from_cell(p2_cell)],
    };
    let placements_cell = solvent.add(placements);

    // Create photo registry (empty)
    let photo_registry = PhotoRegistry {
        attachments: vec![],
    };
    let photos_cell = solvent.add(photo_registry);

    // Create event log
    let event = Event {
        item_id: "item-001".to_string(),
        kind: EventKind::ItemCreated,
        timestamp: 1704067200000,
        target_id: None,
        note: None,
    };
    let event_cell = solvent.add(event);

    let event_log = EventLog {
        events: vec![Bond::from_cell(event_cell)],
    };
    let events_cell = solvent.add(event_log);

    // Create root inventory
    let inventory = Inventory {
        items: vec![
            Bond::from_cell(Arc::clone(&item1_cell)),
            Bond::from_cell(Arc::clone(&item2_cell)),
        ],
        placements: Bond::from_cell(placements_cell),
        photos: Bond::from_cell(photos_cell),
        events: Bond::from_cell(events_cell),
    };

    let inventory_cell = solvent.add(inventory);

    // Verify structure
    assert_eq!(inventory_cell.value().items.len(), 2);
    assert!(inventory_cell.value().placements.is_resolved());
    assert!(inventory_cell.value().photos.is_resolved());
    assert!(inventory_cell.value().events.is_resolved());

    // Content-addressed: same item should have same CID
    let item1_dup = Item {
        id: "item-001".to_string(),
        name: "Laptop".to_string(),
        description: Some("Work laptop".to_string()),
    };
    let item1_dup_cell = solvent.add(item1_dup);
    assert_eq!(item1_cell.cid(), item1_dup_cell.cid());
}

#[test]
fn serialization_roundtrip() {
    let item = Item {
        id: "test-item".to_string(),
        name: "Test Item".to_string(),
        description: Some("A test".to_string()),
    };

    let bytes = item.to_bytes();
    let restored: Item = Item::from_bytes(&bytes).unwrap();

    assert_eq!(restored.id, item.id);
    assert_eq!(restored.name, item.name);
    assert_eq!(restored.description, item.description);
}

#[test]
fn schema_generation() {
    use polyepoxide_core::Structure;

    let item_schema = Item::schema();
    match &item_schema {
        Structure::Record(fields) => {
            assert!(fields.contains_key("id"));
            assert!(fields.contains_key("name"));
            assert!(fields.contains_key("description"));
        }
        _ => panic!("Expected Record structure for Item"),
    }

    let event_kind_schema = EventKind::schema();
    match &event_kind_schema {
        Structure::Enum(variants) => {
            assert!(variants.contains(&"ItemCreated".to_string()));
            assert!(variants.contains(&"ItemScanned".to_string()));
        }
        _ => panic!("Expected Enum structure for EventKind"),
    }
}
