mod conformance_support;

use conformance_support::{ProcedureWheel, ReactionBench, export_imported, import_fixture};
use polyepoxide_core::{DynBond, ExportFormat, ExportProfile, ImportFormat, ImportMode, Oxide};

const MPV_BENCH_CID: &str = "bafyr4ies7seh7nu3tjwkpiiykjq4ariz2tfzu52znxduzxmew7dbz3rdeu";
const REFLUX_LOOP_CID: &str = "bafyr4icciadmwbcoptd5vqda7zzi6dvymsekrglbqjzzlxuvrybqymkyo4";
const REACTION_BENCH_SCHEMA_CID: &str =
    "bafyr4ihste65lvydigb5qwuv4ljcpn32crdlpbny52vqawc6jjq5lrmgye";
const MPV_DYN_BOND_CID: &str = "bafyr4ibicikw7bq3fl4heclavecpa2obhmei5lofke73hovswnn4fxq65q";

#[test]
fn direct_yaml_fixture_imports_and_reexports() {
    let imported = import_fixture::<ReactionBench>(
        "import-export/mpv-reduction-bench.yaml",
        "schema/reaction-bench.schema.yamlld",
        ImportFormat::Yaml,
        ImportMode::Lenient,
    );

    assert_eq!(imported.root_cid.to_string(), MPV_BENCH_CID);
    assert_eq!(imported.value.compute_cid(), imported.root_cid);

    let exported = export_imported(&imported, ExportFormat::Yaml, ExportProfile::Direct);
    assert!(exported.contains("station: MPV bench 3"));
    assert!(exported.contains("id: urn:px-occ:"));
}

#[test]
fn full_jsonld_fixture_imports_and_exports_full_view() {
    let imported = import_fixture::<ReactionBench>(
        "import-export/mpv-reduction-bench.full.jsonld",
        "schema/reaction-bench.schema.yamlld",
        ImportFormat::JsonLd,
        ImportMode::Lenient,
    );

    assert_eq!(imported.root_cid.to_string(), MPV_BENCH_CID);

    let exported = export_imported(&imported, ExportFormat::JsonLd, ExportProfile::Full);
    assert!(exported.contains("\"$value\""));
    assert!(exported.contains("\"@context\""));
}

#[test]
fn canonical_ligation_fixture_imports_and_exports() {
    let imported = import_fixture::<ProcedureWheel>(
        "import-export/reflux-loop.canonical.yaml",
        "schema/procedure-wheel.schema.yamlld",
        ImportFormat::Yaml,
        ImportMode::Canonical,
    );

    assert_eq!(imported.root_cid.to_string(), REFLUX_LOOP_CID);

    let exported = export_imported(&imported, ExportFormat::Yaml, ExportProfile::Canonical);
    assert!(exported.contains("$link"));
    assert!(exported.contains("$ligation"));
    assert!(exported.contains("$value"));
}

#[test]
fn dynamic_bond_fixture_imports_and_reexports() {
    let imported = import_fixture::<DynBond>(
        "import-export/mpv-reduction-bench.dyn-bond.yamlld",
        "schema/dyn-bond.schema.yamlld",
        ImportFormat::YamlLd,
        ImportMode::Lenient,
    );

    assert_eq!(imported.value.schema_cid().to_string(), REACTION_BENCH_SCHEMA_CID);
    assert_eq!(imported.value.cid().to_string(), MPV_BENCH_CID);
    assert_eq!(imported.root_cid.to_string(), MPV_DYN_BOND_CID);
    assert_eq!(imported.value.compute_cid(), imported.root_cid);

    let exported = export_imported(&imported, ExportFormat::YamlLd, ExportProfile::Direct);
    assert!(exported.contains("@context"));
    assert!(exported.contains("schema:"));
    assert!(exported.contains("bond:"));
}
