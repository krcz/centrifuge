mod conformance_support;

use conformance_support::{
    ReactionBench, bench_components_resolved, import_fixture, resolve_bench_into_solvent,
};
use polyepoxide_core::{
    ExportFormat, ExportOptions, ExportProfile, ImportFormat, ImportMode, MemoryStore, Solvent,
    export, load_schema_recursive,
};

const ALDOL_BENCH_CID: &str = "bafyr4ickhv7qhcexxf3ucy6z527bqzqlwd7jxgap4pwiom4kahw735d6cq";

#[test]
fn imported_fixture_can_be_loaded_into_solvent_and_exported() {
    let imported = import_fixture::<ReactionBench>(
        "solvent/aldol-bench.yaml",
        "schema/reaction-bench.schema.yamlld",
        ImportFormat::Yaml,
        ImportMode::Lenient,
    );

    let solvent = Solvent::new();
    let resolved = resolve_bench_into_solvent(&solvent, &imported.value, &imported.store);
    assert!(bench_components_resolved(&resolved));

    let cell = solvent.add(resolved.clone());
    let store = MemoryStore::new();
    let (value_cid, schema_cid) = solvent.persist_cell(&cell, &store).unwrap();
    assert_eq!(value_cid.to_string(), ALDOL_BENCH_CID);

    let schemas = Solvent::new();
    let _ = load_schema_recursive(&store, &schemas, schema_cid).unwrap();
    let exported = export(
        &store,
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

    assert!(exported.contains("station: Aldol bench 2"));
    assert!(exported.contains("THF"));
}
