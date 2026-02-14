//! Sync operations for pulling/pushing data between stores.
//!
//! These operations transfer values with all their transitive dependencies
//! between AsyncStore implementations. The algorithm interleaves traversal
//! with transfer to avoid double-fetching: each node is fetched once from
//! source, checked against dest, stored if missing, then traversed for bonds.

use cid::Cid;
use ipld_core::ipld::Ipld;
use std::collections::HashSet;
use std::sync::Arc;

use crate::async_store::identity_overlay_async;
use crate::reflexive::parse_ligation_bytes;
use crate::{
    AsyncStore, Cell, Solvent, Structure, is_reflexive_cid, reflexive_to_data_cid, resolve_ligation,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraversalState {
    value_cid: Cid,
    schema_cid: Cid,
    scope: Vec<Cid>,
    schema_scope: Vec<Cid>,
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
    let source = identity_overlay_async(source);
    let dest = identity_overlay_async(dest);
    let mut transferred = Vec::new();
    let mut schemas = Solvent::new();
    let mut visited = HashSet::new();

    pull_recursive(
        &source,
        &dest,
        value_cid,
        schema_cid,
        &mut schemas,
        &mut transferred,
        &[],
        &[],
        &mut visited,
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
    scope: &[Cid],
    schema_scope: &[Cid],
    visited: &mut HashSet<TraversalState>,
) -> Result<(), SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    let state = TraversalState {
        value_cid,
        schema_cid,
        scope: scope.to_vec(),
        schema_scope: schema_scope.to_vec(),
    };
    if !visited.insert(state) {
        return Ok(());
    }

    // If dest already has this CID, all dependencies are present (invariant)
    if dest.async_has(&value_cid).await.map_err(SyncError::Dest)? {
        return Ok(());
    }

    // Ensure schema is available
    let Some((schema_cell, next_schema_scope)) =
        resolve_schema_cid(source, dest, schema_cid, schema_scope, schemas, transferred).await?
    else {
        return Ok(());
    };

    // Fetch value from source
    let value_bytes = source
        .async_get(&value_cid)
        .await
        .map_err(SyncError::Source)?
        .ok_or(SyncError::NotFound(value_cid))?;

    // Parse to discover bonds (use serde_ipld_dagcbor for DAG-CBOR)
    let value: Ipld = serde_ipld_dagcbor::from_slice(&value_bytes)
        .map_err(|e| SyncError::Format(format!("value parse error: {}", e)))?;

    // First, recursively pull all bond dependencies (children before parent)
    pull_dependencies(
        source,
        dest,
        &value,
        schema_cell.value(),
        schemas,
        transferred,
        scope,
        &next_schema_scope,
        visited,
    )
    .await?;

    // Now store this value (all dependencies are already in dest)
    dest.async_put(&value_cid, &value_bytes)
        .await
        .map_err(SyncError::Dest)?;
    transferred.push(value_cid);

    Ok(())
}

/// Traverses `value` according to `schema` and recursively pulls all bond
/// dependencies before the current node is stored.
///
/// The traversal is schema-guided and scope-aware for reflexive references.
/// Unknown/mismatched value shapes are skipped without failing the transfer.
async fn pull_dependencies<S, D>(
    source: &S,
    dest: &D,
    value: &Ipld,
    schema: &Structure,
    schemas: &mut Solvent,
    transferred: &mut Vec<Cid>,
    scope: &[Cid],
    schema_scope: &[Cid],
    visited: &mut HashSet<TraversalState>,
) -> Result<(), SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    match schema {
        Structure::Bond(inner_schema) => {
            if let Ipld::Link(target_cid) = value {
                let Some((resolved_cid, next_scope)) =
                    resolve_reflexive_edge(source, dest, *target_cid, scope, transferred).await?
                else {
                    return Ok(());
                };

                let Some((resolved_schema, next_schema_scope)) = resolve_schema_cid(
                    source,
                    dest,
                    inner_schema.cid(),
                    schema_scope,
                    schemas,
                    transferred,
                )
                .await?
                else {
                    return Ok(());
                };

                Box::pin(pull_recursive(
                    source,
                    dest,
                    resolved_cid,
                    resolved_schema.cid(),
                    schemas,
                    transferred,
                    &next_scope,
                    &next_schema_scope,
                    visited,
                ))
                .await?;
            }
        }
        Structure::Record(fields) => {
            if let Ipld::Map(map) = value {
                for (name, field_schema_bond) in fields {
                    if let Some(fv) = map.get(name) {
                        if let Some((field_schema, field_schema_scope)) = resolve_schema_cid(
                            source,
                            dest,
                            field_schema_bond.cid(),
                            schema_scope,
                            schemas,
                            transferred,
                        )
                        .await?
                        {
                            Box::pin(pull_dependencies(
                                source,
                                dest,
                                fv,
                                field_schema.value(),
                                schemas,
                                transferred,
                                scope,
                                &field_schema_scope,
                                visited,
                            ))
                            .await?;
                        }
                    }
                }
            }
        }
        Structure::Sequence(inner) => {
            if let Ipld::List(arr) = value {
                if let Some((inner_schema, inner_schema_scope)) = resolve_schema_cid(
                    source,
                    dest,
                    inner.cid(),
                    schema_scope,
                    schemas,
                    transferred,
                )
                .await?
                {
                    for elem in arr {
                        Box::pin(pull_dependencies(
                            source,
                            dest,
                            elem,
                            inner_schema.value(),
                            schemas,
                            transferred,
                            scope,
                            &inner_schema_scope,
                            visited,
                        ))
                        .await?;
                    }
                }
            }
        }
        Structure::Tuple(elems) => {
            if let Ipld::List(arr) = value {
                for (elem_schema_bond, elem_val) in elems.iter().zip(arr.iter()) {
                    if let Some((elem_schema, elem_schema_scope)) = resolve_schema_cid(
                        source,
                        dest,
                        elem_schema_bond.cid(),
                        schema_scope,
                        schemas,
                        transferred,
                    )
                    .await?
                    {
                        Box::pin(pull_dependencies(
                            source,
                            dest,
                            elem_val,
                            elem_schema.value(),
                            schemas,
                            transferred,
                            scope,
                            &elem_schema_scope,
                            visited,
                        ))
                        .await?;
                    }
                }
            }
        }
        Structure::Tagged(variants) => {
            if let Ipld::Map(map) = value {
                if map.len() == 1 {
                    if let Some((name, val)) = map.iter().next() {
                        if let Some(variant_schema_bond) = variants.get(name) {
                            if let Some((variant_schema, variant_schema_scope)) =
                                resolve_schema_cid(
                                    source,
                                    dest,
                                    variant_schema_bond.cid(),
                                    schema_scope,
                                    schemas,
                                    transferred,
                                )
                                .await?
                            {
                                Box::pin(pull_dependencies(
                                    source,
                                    dest,
                                    val,
                                    variant_schema.value(),
                                    schemas,
                                    transferred,
                                    scope,
                                    &variant_schema_scope,
                                    visited,
                                ))
                                .await?;
                            }
                        }
                    }
                }
            }
        }
        Structure::Map { value: v, .. } => {
            if let Ipld::Map(map) = value {
                if let Some((value_schema, value_schema_scope)) =
                    resolve_schema_cid(source, dest, v.cid(), schema_scope, schemas, transferred)
                        .await?
                {
                    for mv in map.values() {
                        Box::pin(pull_dependencies(
                            source,
                            dest,
                            mv,
                            value_schema.value(),
                            schemas,
                            transferred,
                            scope,
                            &value_schema_scope,
                            visited,
                        ))
                        .await?;
                    }
                }
            }
        }
        Structure::OrderedMap { key: k, value: v } => {
            let key_schema =
                resolve_schema_cid(source, dest, k.cid(), schema_scope, schemas, transferred)
                    .await?;
            let value_schema =
                resolve_schema_cid(source, dest, v.cid(), schema_scope, schemas, transferred)
                    .await?;

            if let (
                Some((key_schema, key_schema_scope)),
                Some((value_schema, value_schema_scope)),
            ) = (key_schema, value_schema)
            {
                if let Ipld::List(entries) = value {
                    for entry in entries {
                        if let Ipld::List(pair) = entry {
                            if pair.len() == 2 {
                                Box::pin(pull_dependencies(
                                    source,
                                    dest,
                                    &pair[0],
                                    key_schema.value(),
                                    schemas,
                                    transferred,
                                    scope,
                                    &key_schema_scope,
                                    visited,
                                ))
                                .await?;
                                Box::pin(pull_dependencies(
                                    source,
                                    dest,
                                    &pair[1],
                                    value_schema.value(),
                                    schemas,
                                    transferred,
                                    scope,
                                    &value_schema_scope,
                                    visited,
                                ))
                                .await?;
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    Ok(())
}

async fn resolve_schema_cid<S, D>(
    source: &S,
    dest: &D,
    cid: Cid,
    scope: &[Cid],
    schemas: &mut Solvent,
    transferred: &mut Vec<Cid>,
) -> Result<Option<(Arc<Cell<Structure>>, Vec<Cid>)>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    let mut resolved_cid = cid;
    let mut resolved_scope = scope.to_vec();

    while is_reflexive_cid(&resolved_cid) {
        let Some((next_cid, next_scope)) =
            resolve_reflexive_edge(source, dest, resolved_cid, &resolved_scope, transferred)
                .await?
        else {
            return Ok(None);
        };
        resolved_cid = next_cid;
        resolved_scope = next_scope;
    }

    let schema_cell = ensure_schema(source, dest, resolved_cid, schemas, transferred).await?;
    Ok(Some((schema_cell, resolved_scope)))
}

/// Resolves a bond edge that may be reflexive and returns the concrete target
/// CID with the next traversal scope.
///
/// Non-reflexive CIDs are returned unchanged. For non-identity reflexive CIDs,
/// the ligation payload is also transferred to `dest` if missing.
async fn resolve_reflexive_edge<S, D>(
    source: &S,
    dest: &D,
    cid: Cid,
    scope: &[Cid],
    transferred: &mut Vec<Cid>,
) -> Result<Option<(Cid, Vec<Cid>)>, SyncError<S::Error, D::Error>>
where
    S: AsyncStore,
    D: AsyncStore,
{
    if !is_reflexive_cid(&cid) {
        return Ok(Some((cid, scope.to_vec())));
    }

    let ligation = if cid.hash().code() == crate::MULTIHASH_IDENTITY {
        parse_ligation_bytes(cid.hash().digest())
    } else {
        let data_cid = reflexive_to_data_cid(&cid);
        let bytes = source
            .async_get(&data_cid)
            .await
            .map_err(SyncError::Source)?
            .ok_or(SyncError::NotFound(data_cid))?;

        if !dest.async_has(&data_cid).await.map_err(SyncError::Dest)? {
            dest.async_put(&data_cid, &bytes)
                .await
                .map_err(SyncError::Dest)?;
            transferred.push(data_cid);
        }

        parse_ligation_bytes(&bytes)
    };

    Ok(resolve_ligation(ligation, scope))
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
        dest.async_put(&cid, &bytes)
            .await
            .map_err(SyncError::Dest)?;
        transferred.push(cid);
    }

    let schema: Structure = serde_ipld_dagcbor::from_slice(&bytes)
        .map_err(|e| SyncError::Format(format!("schema parse error: {}", e)))?;

    // Add to solvent (this also resolves internal bonds)
    Ok(schemas.add(schema))
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
    use crate::{
        Bond, ErasedBond, Ligation, MemoryStore, Oxide, Solvent, Store, ligase_cid, slot_cid,
    };
    use ipld_core::ipld::Ipld;
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

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Oxide)]
    #[oxide(crate = crate)]
    struct Ring {
        name: String,
        next: Bond<Ring>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Oxide)]
    #[oxide(crate = crate)]
    struct Root {
        head: Bond<Ring>,
    }

    #[tokio::test]
    async fn pull_simple_record() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let solvent = Solvent::new();

        let author = Author {
            name: "Jane Doe".into(),
            bio: "A prolific writer".into(),
        };
        let cell = solvent.add(author);
        let (value_cid, schema_cid) = solvent.persist_cell(&cell, &source).unwrap();

        let transferred = pull(&source, &dest, value_cid, schema_cid).await.unwrap();

        assert!(!transferred.is_empty());
        assert!(dest.has(&value_cid).unwrap());
        assert!(dest.has(&schema_cid).unwrap());
    }

    #[tokio::test]
    async fn pull_with_single_bond() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let solvent = Solvent::new();

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

        let transferred = pull(&source, &dest, chapter_cid, schema_cid).await.unwrap();

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
        let solvent = Solvent::new();

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

        let transferred = pull(&source, &dest, book_cid, schema_cid).await.unwrap();

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
        let solvent = Solvent::new();

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

        let transferred = pull(&source, &dest, book_cid, schema_cid).await.unwrap();

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
        let solvent = Solvent::new();

        let author = Author {
            name: "Already Synced".into(),
            bio: "Pre-existing".into(),
        };
        let cell = solvent.add(author);
        let (value_cid, schema_cid) = solvent.persist_cell(&cell, &source).unwrap();

        // Pre-populate dest with the same data
        solvent.persist_cell(&cell, &dest).unwrap();

        let transferred = pull(&source, &dest, value_cid, schema_cid).await.unwrap();

        // Nothing should be transferred since dest already has everything
        assert!(transferred.is_empty());
    }

    #[tokio::test]
    async fn push_with_bonds() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let solvent = Solvent::new();

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

        let transferred = push(&source, &dest, chapter_cid, schema_cid).await.unwrap();

        assert!(!transferred.is_empty());
        assert!(dest.has(&chapter_cid).unwrap());
        assert!(dest.has(&author_cell.cid()).unwrap());
    }

    #[tokio::test]
    async fn pull_reflexive_ligase_with_slots() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let solvent = Solvent::new();

        let ring_a = Ring {
            name: "A".into(),
            next: Bond::from_cid(slot_cid(1)),
        };
        let ring_b = Ring {
            name: "B".into(),
            next: Bond::from_cid(slot_cid(0)),
        };
        let ring_a_cell = solvent.add(ring_a.clone());
        let ring_b_cell = solvent.add(ring_b.clone());

        source.put(&ring_a_cell.cid(), &ring_a.to_bytes()).unwrap();
        source.put(&ring_b_cell.cid(), &ring_b.to_bytes()).unwrap();

        let ligase_args = vec![
            ErasedBond::from_cid(ring_a_cell.cid()),
            ErasedBond::from_cid(ring_b_cell.cid()),
        ];
        let ligase_ref = ligase_cid(ligase_args.clone());
        let ligase_data_cid = reflexive_to_data_cid(&ligase_ref);
        source
            .put(&ligase_data_cid, &Ligation::Ligase(ligase_args).to_bytes())
            .unwrap();

        let root = Root {
            head: Bond::from_cid(ligase_ref),
        };
        let root_cell = solvent.add(root);
        let (root_cid, root_schema_cid) = solvent.persist_cell(&root_cell, &source).unwrap();

        let transferred = pull(&source, &dest, root_cid, root_schema_cid)
            .await
            .unwrap();

        assert!(transferred.contains(&root_cid));
        assert!(transferred.contains(&ligase_data_cid));
        assert!(transferred.contains(&ring_a_cell.cid()));
        assert!(transferred.contains(&ring_b_cell.cid()));
        assert!(!transferred.contains(&slot_cid(0)));
        assert!(!transferred.contains(&slot_cid(1)));

        assert!(dest.has(&root_cid).unwrap());
        assert!(dest.has(&ligase_data_cid).unwrap());
        assert!(dest.has(&ring_a_cell.cid()).unwrap());
        assert!(dest.has(&ring_b_cell.cid()).unwrap());
    }

    #[tokio::test]
    async fn pull_ordered_map_list_pairs_traverses_key_and_value() {
        let source = MemoryStore::new();
        let dest = MemoryStore::new();
        let solvent = Solvent::new();

        let key_author = Author {
            name: "Key Author".into(),
            bio: "used in ordered-map key".into(),
        };
        let value_author = Author {
            name: "Value Author".into(),
            bio: "used in ordered-map value".into(),
        };
        let key_author_cell = solvent.add(key_author);
        let value_author_cell = solvent.add(value_author);
        solvent.persist_cell(&key_author_cell, &source).unwrap();
        solvent.persist_cell(&value_author_cell, &source).unwrap();

        let ordered_schema = Structure::OrderedMap {
            key: Structure::bond(Author::schema()),
            value: Structure::bond(Author::schema()),
        };
        let schema_solvent = Solvent::new();
        let schema_cell = schema_solvent.add(ordered_schema);
        let (root_schema_cid, _structure_schema_cid) =
            schema_solvent.persist_cell(&schema_cell, &source).unwrap();

        let entry = Ipld::List(vec![
            Ipld::Link(key_author_cell.cid()),
            Ipld::Link(value_author_cell.cid()),
        ]);
        let root_ipld = Ipld::List(vec![entry]);
        let root_bytes = serde_ipld_dagcbor::to_vec(&root_ipld).unwrap();
        let root_cid = crate::compute_cid(&root_bytes);
        source.put(&root_cid, &root_bytes).unwrap();

        let transferred = pull(&source, &dest, root_cid, root_schema_cid)
            .await
            .unwrap();

        assert!(transferred.contains(&root_cid));
        assert!(transferred.contains(&key_author_cell.cid()));
        assert!(transferred.contains(&value_author_cell.cid()));
        assert!(dest.has(&key_author_cell.cid()).unwrap());
        assert!(dest.has(&value_author_cell.cid()).unwrap());
    }
}
