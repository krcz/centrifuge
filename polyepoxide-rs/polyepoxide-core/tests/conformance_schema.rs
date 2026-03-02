mod conformance_support;

use conformance_support::{ProcedureWheel, ReactionBench, export_schema_fixture, read_fixture};
use polyepoxide_core::{DynBond, ExportFormat, ExportProfile, Oxide, Structure};

const STRUCTURE_SCHEMA_ROOT_CID: &str =
    "bagaybqabdyqaeegsrwbr3zyxq2rv7iztnos5xawfxaf6gheugc3gaegai7tdykq";
const STRUCTURE_SCHEMA_SCHEMA_CID: &str =
    "bagaybqabdyqaeegsrwbr3zyxq2rv7iztnos5xawfxaf6gheugc3gaegai7tdykq";
const REACTION_BENCH_SCHEMA_ROOT_CID: &str =
    "bafyr4ihste65lvydigb5qwuv4ljcpn32crdlpbny52vqawc6jjq5lrmgye";
const PROCEDURE_WHEEL_SCHEMA_ROOT_CID: &str =
    "bafyr4iexlq4cqpmipq4xktahosewtrhucfzvrnh3lq5mjh2tku4wkxdruq";
const DYN_BOND_SCHEMA_ROOT_CID: &str =
    "bafyr4igxca66cm65tqyuhhictd43o2l3omrfg4zepnf33s7kg7p6spbqge";

fn assert_schema_fixture<T: Oxide>(path: &str, expected_root_cid: &str, expected_schema_cid: &str) {
    let (exported, root_cid, schema_cid) =
        export_schema_fixture::<T>(ExportFormat::YamlLd, ExportProfile::Canonical);

    assert_eq!(exported, read_fixture(path));
    assert_eq!(root_cid.to_string(), expected_root_cid);
    assert_eq!(schema_cid.to_string(), expected_schema_cid);
}

#[test]
fn structure_schema_fixture_matches_export_then_cid() {
    // `Structure::schema()` is self-describing, so the root bond CID and the schema CID
    // coincide even though the test checks them in two distinct roles.
    assert_schema_fixture::<Structure>(
        "schema/structure-schema.yamlld",
        STRUCTURE_SCHEMA_ROOT_CID,
        STRUCTURE_SCHEMA_SCHEMA_CID,
    );
}

#[test]
fn reaction_bench_schema_fixture_matches_export_then_cid() {
    assert_schema_fixture::<ReactionBench>(
        "schema/reaction-bench.schema.yamlld",
        REACTION_BENCH_SCHEMA_ROOT_CID,
        STRUCTURE_SCHEMA_SCHEMA_CID,
    );
}

#[test]
fn procedure_wheel_schema_fixture_matches_export_then_cid() {
    assert_schema_fixture::<ProcedureWheel>(
        "schema/procedure-wheel.schema.yamlld",
        PROCEDURE_WHEEL_SCHEMA_ROOT_CID,
        STRUCTURE_SCHEMA_SCHEMA_CID,
    );
}

#[test]
fn dyn_bond_schema_fixture_matches_export_then_cid() {
    assert_schema_fixture::<DynBond>(
        "schema/dyn-bond.schema.yamlld",
        DYN_BOND_SCHEMA_ROOT_CID,
        STRUCTURE_SCHEMA_SCHEMA_CID,
    );
}
