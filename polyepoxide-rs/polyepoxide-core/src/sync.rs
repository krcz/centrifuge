//! Sync operations for pulling/pushing data between stores.
//!
//! These operations transfer values with all their transitive dependencies
//! between AsyncStore implementations. The algorithm interleaves traversal
//! with transfer to avoid double-fetching: each node is fetched once from
//! source, checked against dest, stored if missing, then traversed for bonds.

use cid::Cid;
use std::sync::Arc;

use crate::traverse::collect_bonds;
use crate::{AsyncStore, Cell, Solvent, Structure};

/// Error during sync operations.
#[derive(Debug, thiserror::Error)]
pub enum SyncError<S, D> {
    #[error("node not found: {0}")]
    NotFound(Cid),
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
/// This maintains the invariant that if a CID exists in dest, all its
/// dependencies are already present. This allows using `dest.has()` to
/// skip already-synced subgraphs without separate visited tracking.
///
/// # Arguments
/// * `source` - The store to pull from
/// * `dest` - The store to pull into
/// * `value_cid` - CID of the root value to sync
/// * `schema_cid` - CID of the root value's schema
///
/// # Returns
/// The set of CIDs that were transferred
pub async fn pull<S, D>(
    source: &S,
    dest: &D,
    value_cid: Cid,
    schema_cid: Cid,
) -> Result<Vec<Cid>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    let mut transferred = Vec::new();
    let mut schemas = Solvent::new();

    pull_recursive(
        source,
        dest,
        value_cid,
        schema_cid,
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
    value_cid: Cid,
    schema_cid: Cid,
    schemas: &mut Solvent,
    transferred: &mut Vec<Cid>,
) -> Result<(), SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    // If dest already has this CID, all dependencies are present (invariant)
    if dest.async_has(&value_cid).await.map_err(SyncError::Dest)? {
        return Ok(());
    }

    // Ensure schema is available
    let schema_cell = ensure_schema(source, dest, schema_cid, schemas, transferred).await?;

    // Fetch value from source
    let value_bytes = source
        .async_get(&value_cid)
        .await
        .map_err(SyncError::Source)?
        .ok_or(SyncError::NotFound(value_cid))?;

    // Parse to discover bonds (use serde_ipld_dagcbor for DAG-CBOR)
    let value: ipld_core::ipld::Ipld = serde_ipld_dagcbor::from_slice(&value_bytes)
        .map_err(|e| SyncError::Format(format!("value parse error: {}", e)))?;

    // First, recursively pull all bond dependencies (children before parent)
    let mut bonds = Vec::new();
    collect_bonds(&value, schema_cell.value(), schemas, &mut bonds);
    for (bond_cid, bond_schema_cid) in bonds {
        Box::pin(pull_recursive(
            source,
            dest,
            bond_cid,
            bond_schema_cid,
            schemas,
            transferred,
        ))
        .await?;
    }

    // Now store this value (all dependencies are already in dest)
    dest.async_put(&value_cid, &value_bytes)
        .await
        .map_err(SyncError::Dest)?;
    transferred.push(value_cid);

    Ok(())
}

/// Ensure a schema is available at dest, fetching from source if needed.
/// Returns a Cell containing the schema for traversal.
async fn ensure_schema<S, D>(
    source: &S,
    dest: &D,
    cid: Cid,
    schemas: &mut Solvent,
    transferred: &mut Vec<Cid>,
) -> Result<Arc<Cell<Structure>>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    // Check if already in solvent
    if let Some(cell) = schemas.get::<Structure>(&cid) {
        return Ok(cell);
    }

    // Check if dest has it
    let dest_has = dest.async_has(&cid).await.map_err(SyncError::Dest)?;

    // Fetch from source
    let bytes = source
        .async_get(&cid)
        .await
        .map_err(SyncError::Source)?
        .ok_or(SyncError::NotFound(cid))?;

    // Store in dest if missing
    if !dest_has {
        dest.async_put(&cid, &bytes).await.map_err(SyncError::Dest)?;
        transferred.push(cid);
    }

    let schema: Structure = serde_ipld_dagcbor::from_slice(&bytes)
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
    transferred: &mut Vec<Cid>,
) -> Result<(), SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    match schema {
        Structure::Sequence(inner) | Structure::Bond(inner) => {
            let cid = inner.cid();
            if schemas.get::<Structure>(&cid).is_none() {
                Box::pin(ensure_schema(source, dest, cid, schemas, transferred)).await?;
            }
        }
        Structure::Tuple(elems) => {
            for elem in elems {
                let cid = elem.cid();
                if schemas.get::<Structure>(&cid).is_none() {
                    Box::pin(ensure_schema(source, dest, cid, schemas, transferred)).await?;
                }
            }
        }
        Structure::Record(fields) | Structure::Tagged(fields) => {
            for (_, field) in fields {
                let cid = field.cid();
                if schemas.get::<Structure>(&cid).is_none() {
                    Box::pin(ensure_schema(source, dest, cid, schemas, transferred)).await?;
                }
            }
        }
        Structure::Map { key: k, value: v } | Structure::OrderedMap { key: k, value: v } => {
            let kk = k.cid();
            let vk = v.cid();
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

/// Push a value and all its dependencies from source to destination.
///
/// This is semantically the same as `pull`, just from the perspective of
/// the data owner pushing to a remote store.
pub async fn push<S, D>(
    source: &S,
    dest: &D,
    value_cid: Cid,
    schema_cid: Cid,
) -> Result<Vec<Cid>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    pull(source, dest, value_cid, schema_cid).await
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
        let (value_cid, schema_cid) = solvent.persist_cell(&cell, &source).unwrap();

        let transferred = pull(&source, &dest, value_cid, schema_cid)
            .await
            .unwrap();

        assert!(!transferred.is_empty());
        assert!(dest.has(&value_cid).unwrap());
        assert!(dest.has(&schema_cid).unwrap());
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
        let author_cid = author_cell.cid();

        // Create chapter with bond to author
        let chapter = Chapter {
            title: "Introduction".into(),
            page_count: 25,
            author: Bond::from_cell(Arc::clone(&author_cell)),
        };
        let chapter_cell = solvent.add(chapter);
        let (chapter_cid, schema_cid) = solvent.persist_cell(&chapter_cell, &source).unwrap();

        let transferred = pull(&source, &dest, chapter_cid, schema_cid)
            .await
            .unwrap();

        // Should have transferred chapter and author
        assert!(transferred.contains(&chapter_cid));
        assert!(transferred.contains(&author_cid));
        assert!(dest.has(&chapter_cid).unwrap());
        assert!(dest.has(&author_cid).unwrap());
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
        let (book_cid, schema_cid) = solvent.persist_cell(&book_cell, &source).unwrap();

        let transferred = pull(&source, &dest, book_cid, schema_cid)
            .await
            .unwrap();

        // Should have transferred everything: book, 2 chapters, 2 authors
        assert!(transferred.contains(&book_cid));
        assert!(transferred.contains(&chapter1_cell.cid()));
        assert!(transferred.contains(&chapter2_cell.cid()));
        assert!(transferred.contains(&author1_cell.cid()));
        assert!(transferred.contains(&author2_cell.cid()));

        // Verify all are in dest
        assert!(dest.has(&book_cid).unwrap());
        assert!(dest.has(&chapter1_cell.cid()).unwrap());
        assert!(dest.has(&chapter2_cell.cid()).unwrap());
        assert!(dest.has(&author1_cell.cid()).unwrap());
        assert!(dest.has(&author2_cell.cid()).unwrap());
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
        let (book_cid, schema_cid) = solvent.persist_cell(&book_cell, &source).unwrap();

        let transferred = pull(&source, &dest, book_cid, schema_cid)
            .await
            .unwrap();

        // Shared author should only be transferred once
        let author_count = transferred
            .iter()
            .filter(|k| **k == author_cell.cid())
            .count();
        assert_eq!(author_count, 1);

        // All values should be in dest
        assert!(dest.has(&book_cid).unwrap());
        assert!(dest.has(&chapter1_cell.cid()).unwrap());
        assert!(dest.has(&chapter2_cell.cid()).unwrap());
        assert!(dest.has(&author_cell.cid()).unwrap());
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
        let (value_cid, schema_cid) = solvent.persist_cell(&cell, &source).unwrap();

        // Pre-populate dest with the same data
        solvent.persist_cell(&cell, &dest).unwrap();

        let transferred = pull(&source, &dest, value_cid, schema_cid)
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
        let (chapter_cid, schema_cid) = solvent.persist_cell(&chapter_cell, &source).unwrap();

        let transferred = push(&source, &dest, chapter_cid, schema_cid)
            .await
            .unwrap();

        assert!(!transferred.is_empty());
        assert!(dest.has(&chapter_cid).unwrap());
        assert!(dest.has(&author_cell.cid()).unwrap());
    }
}
