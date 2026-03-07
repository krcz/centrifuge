mod conformance_support;

use conformance_support::{ReactionBench, import_fixture, resolve_bench_into_solvent};
use polyepoxide_core::{
    AsyncStore, ExportFormat, ExportOptions, ExportProfile, ImportFormat, ImportMode, MemoryStore,
    Solvent, export, load_schema_recursive, pull, push,
};

const FISCHER_BENCH_CID: &str = "bafyr4ia6clb2lcd7i5pn3p7ohg7ja24n7udptiouvjf5cemznmm7kt6gcq";

#[tokio::test]
async fn imported_fixture_pulls_between_stores() {
    let imported = import_fixture::<ReactionBench>(
        "sync/fischer-esterification-bench.yaml",
        "schema/reaction-bench.schema.yamlld",
        ImportFormat::Yaml,
        ImportMode::Lenient,
    );

    let solvent = Solvent::new();
    let resolved = resolve_bench_into_solvent(&solvent, &imported.value, &imported.store);
    let cell = solvent.add(resolved);
    let source = MemoryStore::new();
    let (value_cid, schema_cid) = solvent.persist_cell(&cell, &source).unwrap();
    let dest = MemoryStore::new();
    let transferred = pull(&source, &dest, value_cid, schema_cid).await.unwrap();

    assert!(transferred.contains(&value_cid));
    assert!(
        dest.async_has(&polyepoxide_core::key_from_cid(&value_cid))
            .await
            .unwrap()
    );

    let schemas = Solvent::new();
    let _ = load_schema_recursive(&dest, &schemas, schema_cid).unwrap();
    let exported = export(
        &dest,
        &schemas,
        value_cid,
        schema_cid,
        ExportFormat::Yaml,
        &ExportOptions {
            profile: ExportProfile::Direct,
            pretty: true,
            unwrap_top_level_occurrence: false,
            exclude_top_level_fields: Vec::new(),
        },
    )
    .unwrap();

    assert!(exported.contains("Fischer esterification hood"));
}

#[tokio::test]
async fn imported_fixture_pushes_between_stores() {
    let imported = import_fixture::<ReactionBench>(
        "sync/fischer-esterification-bench.yaml",
        "schema/reaction-bench.schema.yamlld",
        ImportFormat::Yaml,
        ImportMode::Lenient,
    );

    let solvent = Solvent::new();
    let resolved = resolve_bench_into_solvent(&solvent, &imported.value, &imported.store);
    let cell = solvent.add(resolved);
    let source = MemoryStore::new();
    let (value_cid, schema_cid) = solvent.persist_cell(&cell, &source).unwrap();
    assert_eq!(value_cid.to_string(), FISCHER_BENCH_CID);

    let dest = MemoryStore::new();
    let transferred = push(&source, &dest, value_cid, schema_cid).await.unwrap();

    assert!(transferred.contains(&value_cid));
    assert!(
        dest.async_has(&polyepoxide_core::key_from_cid(&value_cid))
            .await
            .unwrap()
    );
}
