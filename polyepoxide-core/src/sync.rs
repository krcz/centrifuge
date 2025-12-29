//! Sync operations for pulling/pushing data between stores.
//!
//! These operations transfer values with all their transitive dependencies
//! between AsyncStore implementations. The algorithm interleaves traversal
//! with transfer to avoid double-fetching: each node is fetched once from
//! source, checked against dest, stored if missing, then traversed for bonds.

use std::sync::Arc;

use crate::{AsyncStore, Cell, Key, Solvent, Structure};

/// Error during sync operations.
#[derive(Debug, thiserror::Error)]
pub enum SyncError<S, D> {
    #[error("node not found: {0}")]
    NotFound(Key),
    #[error("invalid format: {0}")]
    Format(String),
    #[error("source store error: {0}")]
    Source(S),
    #[error("destination store error: {0}")]
    Dest(D),
}

/// Pull a value and all its dependencies from source to destination.
///
/// Uses dependency-first order: children are stored before parents.
/// This maintains the invariant that if a key exists in dest, all its
/// dependencies are already present. This allows using `dest.has()` to
/// skip already-synced subgraphs without separate visited tracking.
///
/// # Arguments
/// * `source` - The store to pull from
/// * `dest` - The store to pull into
/// * `value_key` - Key of the root value to sync
/// * `schema_key` - Key of the root value's schema
///
/// # Returns
/// The set of keys that were transferred
pub async fn pull<S, D>(
    source: &S,
    dest: &D,
    value_key: Key,
    schema_key: Key,
) -> Result<Vec<Key>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    let mut transferred = Vec::new();
    let mut schemas = Solvent::new();

    pull_recursive(
        source,
        dest,
        value_key,
        schema_key,
        &mut schemas,
        &mut transferred,
    )
    .await?;

    Ok(transferred)
}

/// Recursive helper for pull - processes dependencies before storing current value.
async fn pull_recursive<S, D>(
    source: &S,
    dest: &D,
    value_key: Key,
    schema_key: Key,
    schemas: &mut Solvent,
    transferred: &mut Vec<Key>,
) -> Result<(), SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    // If dest already has this key, all dependencies are present (invariant)
    if dest.async_has(&value_key).await.map_err(SyncError::Dest)? {
        return Ok(());
    }

    // Ensure schema is available
    let schema_cell = ensure_schema(source, dest, schema_key, schemas, transferred).await?;

    // Fetch value from source
    let value_bytes = source
        .async_get(&value_key)
        .await
        .map_err(SyncError::Source)?
        .ok_or(SyncError::NotFound(value_key))?;

    // Parse to discover bonds
    let value: ciborium::Value = ciborium::from_reader(&value_bytes[..])
        .map_err(|e| SyncError::Format(format!("value parse error: {}", e)))?;

    // First, recursively pull all bond dependencies (children before parent)
    let mut bonds = Vec::new();
    collect_bonds(&value, schema_cell.value(), schemas, &mut bonds);
    for (bond_key, bond_schema_key) in bonds {
        Box::pin(pull_recursive(
            source,
            dest,
            bond_key,
            bond_schema_key,
            schemas,
            transferred,
        ))
        .await?;
    }

    // Now store this value (all dependencies are already in dest)
    dest.async_put(&value_key, &value_bytes)
        .await
        .map_err(SyncError::Dest)?;
    transferred.push(value_key);

    Ok(())
}

/// Ensure a schema is available at dest, fetching from source if needed.
/// Returns a Cell containing the schema for traversal.
async fn ensure_schema<S, D>(
    source: &S,
    dest: &D,
    key: Key,
    schemas: &mut Solvent,
    transferred: &mut Vec<Key>,
) -> Result<Arc<Cell<Structure>>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    // Check if already in solvent
    if let Some(cell) = schemas.get::<Structure>(&key) {
        return Ok(cell);
    }

    // Check if dest has it
    let dest_has = dest.async_has(&key).await.map_err(SyncError::Dest)?;

    // Fetch from source
    let bytes = source
        .async_get(&key)
        .await
        .map_err(SyncError::Source)?
        .ok_or(SyncError::NotFound(key))?;

    // Store in dest if missing
    if !dest_has {
        dest.async_put(&key, &bytes).await.map_err(SyncError::Dest)?;
        transferred.push(key);
    }

    let schema: Structure = ciborium::from_reader(&bytes[..])
        .map_err(|e| SyncError::Format(format!("schema parse error: {}", e)))?;

    // Recursively ensure nested schema bonds are transferred
    ensure_nested_schemas(source, dest, &schema, schemas, transferred).await?;

    // Add to solvent (this also resolves internal bonds)
    Ok(schemas.add(schema))
}

/// Recursively ensure all schema bonds are transferred.
async fn ensure_nested_schemas<S, D>(
    source: &S,
    dest: &D,
    schema: &Structure,
    schemas: &mut Solvent,
    transferred: &mut Vec<Key>,
) -> Result<(), SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    match schema {
        Structure::Sequence(inner) | Structure::Bond(inner) => {
            let key = inner.key();
            if schemas.get::<Structure>(&key).is_none() {
                Box::pin(ensure_schema(source, dest, key, schemas, transferred)).await?;
            }
        }
        Structure::Tuple(elems) => {
            for elem in elems {
                let key = elem.key();
                if schemas.get::<Structure>(&key).is_none() {
                    Box::pin(ensure_schema(source, dest, key, schemas, transferred)).await?;
                }
            }
        }
        Structure::Record(fields) | Structure::Tagged(fields) => {
            for (_, field) in fields {
                let key = field.key();
                if schemas.get::<Structure>(&key).is_none() {
                    Box::pin(ensure_schema(source, dest, key, schemas, transferred)).await?;
                }
            }
        }
        Structure::Map { key: k, value: v } | Structure::OrderedMap { key: k, value: v } => {
            let kk = k.key();
            let vk = v.key();
            if schemas.get::<Structure>(&kk).is_none() {
                Box::pin(ensure_schema(source, dest, kk, schemas, transferred)).await?;
            }
            if schemas.get::<Structure>(&vk).is_none() {
                Box::pin(ensure_schema(source, dest, vk, schemas, transferred)).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Extract bond targets from a value given its schema.
/// Appends (value_key, schema_key) pairs to `bonds`.
/// Silently skips malformed data - we only care about finding bonds.
fn collect_bonds(
    value: &ciborium::Value,
    schema: &Structure,
    schemas: &Solvent,
    bonds: &mut Vec<(Key, Key)>,
) {
    match schema {
        Structure::Bond(inner_schema) => {
            if let ciborium::Value::Bytes(bytes) = value {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(bytes);
                    let target_key = Key::from_bytes(arr);
                    bonds.push((target_key, inner_schema.key()));
                }
            }
        }
        Structure::Record(fields) => {
            if let Some(map) = value.as_map() {
                use std::collections::HashMap;
                let field_values: HashMap<&str, &ciborium::Value> = map
                    .iter()
                    .filter_map(|(k, v)| k.as_text().map(|name| (name, v)))
                    .collect();
                for (name, field_schema_bond) in fields {
                    if let Some(fv) = field_values.get(name.as_str()) {
                        if let Some(s) = field_schema_bond.value() {
                            collect_bonds(fv, s, schemas, bonds);
                        }
                    }
                }
            }
        }
        Structure::Sequence(inner) => {
            if let Some(arr) = value.as_array() {
                if let Some(inner_schema) = inner.value() {
                    for elem in arr {
                        collect_bonds(elem, inner_schema, schemas, bonds);
                    }
                }
            }
        }
        Structure::Tuple(elems) => {
            if let Some(arr) = value.as_array() {
                for (elem_schema_bond, elem_val) in elems.iter().zip(arr.iter()) {
                    if let Some(s) = elem_schema_bond.value() {
                        collect_bonds(elem_val, s, schemas, bonds);
                    }
                }
            }
        }
        Structure::Tagged(variants) => {
            if let Some(map) = value.as_map() {
                if map.len() == 1 {
                    if let Some(name) = map[0].0.as_text() {
                        if let Some(variant_schema_bond) = variants.get(name) {
                            if let Some(s) = variant_schema_bond.value() {
                                collect_bonds(&map[0].1, s, schemas, bonds);
                            }
                        }
                    }
                }
            }
        }
        Structure::Map { key: k, value: v } | Structure::OrderedMap { key: k, value: v } => {
            if let Some(map) = value.as_map() {
                if let (Some(ks), Some(vs)) = (k.value(), v.value()) {
                    for (mk, mv) in map {
                        collect_bonds(mk, ks, schemas, bonds);
                        collect_bonds(mv, vs, schemas, bonds);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Push a value and all its dependencies from source to destination.
///
/// This is semantically the same as `pull`, just from the perspective of
/// the data owner pushing to a remote store.
pub async fn push<S, D>(
    source: &S,
    dest: &D,
    value_key: Key,
    schema_key: Key,
) -> Result<Vec<Key>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    pull(source, dest, value_key, schema_key).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bond, MemoryStore, Oxide, Solvent, Store};
    use std::sync::Arc;

    // Complex test structures using derive macro with crate path override
    // Use #[oxide(crate = crate)] to make derive work inside the crate

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Oxide)]
    #[oxide(crate = crate)]
    struct Author {
        name: String,
        bio: String,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Oxide)]
    #[oxide(crate = crate)]
    struct Chapter {
        title: String,
        page_count: u32,
        author: Bond<Author>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Oxide)]
    #[oxide(crate = crate)]
    struct Book {
        title: String,
        year: u32,
        chapters: Vec<Bond<Chapter>>,
    }

    #[tokio::test]
    async fn pull_simple_record() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let mut solvent = Solvent::new();

        let author = Author {
            name: "Jane Doe".into(),
            bio: "A prolific writer".into(),
        };
        let cell = solvent.add(author);
        let (value_key, schema_key) = solvent.persist_cell(&cell, &source).unwrap();

        let transferred = pull(&source, &dest, value_key, schema_key)
            .await
            .unwrap();

        assert!(!transferred.is_empty());
        assert!(dest.has(&value_key).unwrap());
        assert!(dest.has(&schema_key).unwrap());
    }

    #[tokio::test]
    async fn pull_with_single_bond() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let mut solvent = Solvent::new();

        // Create author
        let author = Author {
            name: "John Smith".into(),
            bio: "Expert in Rust".into(),
        };
        let author_cell = solvent.add(author);
        let author_key = author_cell.key();

        // Create chapter with bond to author
        let chapter = Chapter {
            title: "Introduction".into(),
            page_count: 25,
            author: Bond::from_cell(Arc::clone(&author_cell)),
        };
        let chapter_cell = solvent.add(chapter);
        let (chapter_key, schema_key) = solvent.persist_cell(&chapter_cell, &source).unwrap();

        let transferred = pull(&source, &dest, chapter_key, schema_key)
            .await
            .unwrap();

        // Should have transferred chapter and author
        assert!(transferred.contains(&chapter_key));
        assert!(transferred.contains(&author_key));
        assert!(dest.has(&chapter_key).unwrap());
        assert!(dest.has(&author_key).unwrap());
    }

    #[tokio::test]
    async fn pull_with_nested_bonds() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let mut solvent = Solvent::new();

        // Create authors
        let author1 = Author {
            name: "Alice".into(),
            bio: "Chapter 1 author".into(),
        };
        let author1_cell = solvent.add(author1);

        let author2 = Author {
            name: "Bob".into(),
            bio: "Chapter 2 author".into(),
        };
        let author2_cell = solvent.add(author2);

        // Create chapters with bonds to authors
        let chapter1 = Chapter {
            title: "Getting Started".into(),
            page_count: 30,
            author: Bond::from_cell(Arc::clone(&author1_cell)),
        };
        let chapter1_cell = solvent.add(chapter1);

        let chapter2 = Chapter {
            title: "Advanced Topics".into(),
            page_count: 45,
            author: Bond::from_cell(Arc::clone(&author2_cell)),
        };
        let chapter2_cell = solvent.add(chapter2);

        // Create book with bonds to chapters
        let book = Book {
            title: "The Rust Book".into(),
            year: 2024,
            chapters: vec![
                Bond::from_cell(Arc::clone(&chapter1_cell)),
                Bond::from_cell(Arc::clone(&chapter2_cell)),
            ],
        };
        let book_cell = solvent.add(book);
        let (book_key, schema_key) = solvent.persist_cell(&book_cell, &source).unwrap();

        let transferred = pull(&source, &dest, book_key, schema_key)
            .await
            .unwrap();

        // Should have transferred everything: book, 2 chapters, 2 authors
        assert!(transferred.contains(&book_key));
        assert!(transferred.contains(&chapter1_cell.key()));
        assert!(transferred.contains(&chapter2_cell.key()));
        assert!(transferred.contains(&author1_cell.key()));
        assert!(transferred.contains(&author2_cell.key()));

        // Verify all are in dest
        assert!(dest.has(&book_key).unwrap());
        assert!(dest.has(&chapter1_cell.key()).unwrap());
        assert!(dest.has(&chapter2_cell.key()).unwrap());
        assert!(dest.has(&author1_cell.key()).unwrap());
        assert!(dest.has(&author2_cell.key()).unwrap());
    }

    #[tokio::test]
    async fn pull_with_shared_bonds() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let mut solvent = Solvent::new();

        // Create a shared author referenced by multiple chapters
        let shared_author = Author {
            name: "Shared Author".into(),
            bio: "Writes everything".into(),
        };
        let author_cell = solvent.add(shared_author);

        // Create two chapters both referencing the same author
        let chapter1 = Chapter {
            title: "Part One".into(),
            page_count: 50,
            author: Bond::from_cell(Arc::clone(&author_cell)),
        };
        let chapter1_cell = solvent.add(chapter1);

        let chapter2 = Chapter {
            title: "Part Two".into(),
            page_count: 60,
            author: Bond::from_cell(Arc::clone(&author_cell)),
        };
        let chapter2_cell = solvent.add(chapter2);

        let book = Book {
            title: "Shared Author Book".into(),
            year: 2024,
            chapters: vec![
                Bond::from_cell(Arc::clone(&chapter1_cell)),
                Bond::from_cell(Arc::clone(&chapter2_cell)),
            ],
        };
        let book_cell = solvent.add(book);
        let (book_key, schema_key) = solvent.persist_cell(&book_cell, &source).unwrap();

        let transferred = pull(&source, &dest, book_key, schema_key)
            .await
            .unwrap();

        // Shared author should only be transferred once
        let author_count = transferred
            .iter()
            .filter(|k| **k == author_cell.key())
            .count();
        assert_eq!(author_count, 1);

        // All values should be in dest
        assert!(dest.has(&book_key).unwrap());
        assert!(dest.has(&chapter1_cell.key()).unwrap());
        assert!(dest.has(&chapter2_cell.key()).unwrap());
        assert!(dest.has(&author_cell.key()).unwrap());
    }

    #[tokio::test]
    async fn pull_incremental() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let mut solvent = Solvent::new();

        let author = Author {
            name: "Already Synced".into(),
            bio: "Pre-existing".into(),
        };
        let cell = solvent.add(author);
        let (value_key, schema_key) = solvent.persist_cell(&cell, &source).unwrap();

        // Pre-populate dest with the same data
        solvent.persist_cell(&cell, &dest).unwrap();

        let transferred = pull(&source, &dest, value_key, schema_key)
            .await
            .unwrap();

        // Nothing should be transferred since dest already has everything
        assert!(transferred.is_empty());
    }

    #[tokio::test]
    async fn push_with_bonds() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let mut solvent = Solvent::new();

        let author = Author {
            name: "Push Author".into(),
            bio: "Testing push".into(),
        };
        let author_cell = solvent.add(author);

        let chapter = Chapter {
            title: "Push Chapter".into(),
            page_count: 10,
            author: Bond::from_cell(Arc::clone(&author_cell)),
        };
        let chapter_cell = solvent.add(chapter);
        let (chapter_key, schema_key) = solvent.persist_cell(&chapter_cell, &source).unwrap();

        let transferred = push(&source, &dest, chapter_key, schema_key)
            .await
            .unwrap();

        assert!(!transferred.is_empty());
        assert!(dest.has(&chapter_key).unwrap());
        assert!(dest.has(&author_cell.key()).unwrap());
    }
}
