#![allow(dead_code)]

use polyepoxide_core::{
    Bond, Cid, ErasedBond, ExportFormat, ExportOptions, ExportProfile, ImportFormat, ImportMode,
    ImportOptions, MemoryStore, Oxide, Solvent, Store, Structure, export, import, key_from_cid,
    load_schema_recursive, resolve_reflexive_with_store,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, polyepoxide_core::Oxide)]
#[oxide(crate = polyepoxide_core)]
pub struct ReagentBottle {
    pub label: String,
    pub formula: String,
    pub lot: String,
    pub purity_pct: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, polyepoxide_core::Oxide)]
#[oxide(crate = polyepoxide_core)]
pub struct ReactionBench {
    pub station: String,
    pub substrate: Bond<ReagentBottle>,
    pub reagent: Bond<ReagentBottle>,
    pub solvent: Bond<ReagentBottle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, polyepoxide_core::Oxide)]
#[oxide(crate = polyepoxide_core)]
pub struct ProcedureStep {
    pub step_no: u8,
    pub instruction: String,
    pub next: Bond<ProcedureStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, polyepoxide_core::Oxide)]
#[oxide(crate = polyepoxide_core)]
pub struct ProcedureWheel {
    pub campaign: String,
    pub entry: Bond<ProcedureStep>,
}

pub struct ImportedFixture<T: Oxide> {
    pub value: T,
    pub root_cid: Cid,
    pub schema_cid: Cid,
    pub store: MemoryStore,
    pub schemas: Solvent,
}

pub struct SchemaFixture {
    pub root_cid: Cid,
    pub schemas: Solvent,
}

pub fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("conformance-fixtures")
        .join(relative)
}

pub fn read_fixture(relative: &str) -> String {
    std::fs::read_to_string(fixture_path(relative)).unwrap()
}

pub fn import_schema_fixture(relative: &str) -> SchemaFixture {
    let bootstrap = Solvent::new();
    let bootstrap_schema_cid = bootstrap.add_bond(&Structure::schema()).cid();
    let store = MemoryStore::new();
    let root_cid = import(
        &read_fixture(relative),
        ImportFormat::YamlLd,
        bootstrap_schema_cid,
        &store,
        &bootstrap,
        &ImportOptions {
            mode: ImportMode::Canonical,
        },
    )
    .unwrap();
    let schemas = Solvent::new();
    let _ = load_schema_recursive(&store, &schemas, root_cid).unwrap();
    SchemaFixture { root_cid, schemas }
}

fn decode_root<T: Oxide>(store: &MemoryStore, root_cid: Cid) -> T {
    let (cid, _) = resolve_reflexive_with_store(store, root_cid, &[])
        .unwrap()
        .expect("root bond should resolve");
    let bytes = store.get(&key_from_cid(&cid)).unwrap().unwrap();
    T::from_bytes(&bytes).unwrap()
}

pub fn import_fixture<T: Oxide>(
    relative: &str,
    schema_relative: &str,
    format: ImportFormat,
    mode: ImportMode,
) -> ImportedFixture<T> {
    let store = MemoryStore::new();
    let schema = import_schema_fixture(schema_relative);
    let root_cid = import(
        &read_fixture(relative),
        format,
        schema.root_cid,
        &store,
        &schema.schemas,
        &ImportOptions { mode },
    )
    .unwrap();
    let value = decode_root::<T>(&store, root_cid);
    ImportedFixture {
        value,
        root_cid,
        schema_cid: schema.root_cid,
        store,
        schemas: schema.schemas,
    }
}

pub fn export_schema_fixture<T: Oxide>(
    format: ExportFormat,
    profile: ExportProfile,
) -> (String, Cid, Cid) {
    let solvent = Solvent::new();
    let root = match solvent.add_bond(&T::schema()) {
        Bond::Link(cell) => cell,
        Bond::Ligation(ligation) => match *ligation {
            polyepoxide_core::Ligation::Ligase(args) => match args.first().unwrap() {
                ErasedBond::Link(cell) => cell
                    .clone()
                    .into_any_arc()
                    .downcast::<polyepoxide_core::Cell<Structure>>()
                    .ok()
                    .unwrap(),
                _ => panic!("expected schema root cell"),
            },
            _ => panic!("expected root ligase"),
        },
        Bond::Unresolved(_) => panic!("schema root must not be unresolved"),
    };
    let store = MemoryStore::new();
    let (_, schema_cid) = solvent.persist_cell(&root, &store).unwrap();
    let root_cid = T::schema().cid();
    let schemas = Solvent::new();
    let _ = load_schema_recursive(&store, &schemas, schema_cid).unwrap();
    let text = export(
        &store,
        &schemas,
        root_cid,
        schema_cid,
        format,
        &ExportOptions {
            profile,
            pretty: true,
            unwrap_top_level_occurrence: false,
            exclude_top_level_fields: Vec::new(),
        },
    )
    .unwrap();
    (text, root_cid, schema_cid)
}

pub fn export_imported<T: Oxide>(
    imported: &ImportedFixture<T>,
    format: ExportFormat,
    profile: ExportProfile,
) -> String {
    export(
        &imported.store,
        &imported.schemas,
        imported.root_cid,
        imported.schema_cid,
        format,
        &ExportOptions {
            profile,
            pretty: true,
            unwrap_top_level_occurrence: false,
            exclude_top_level_fields: Vec::new(),
        },
    )
    .unwrap()
}

pub fn persist_export<T: Oxide>(
    value: &T,
    format: ExportFormat,
    profile: ExportProfile,
) -> (String, polyepoxide_core::Cid, polyepoxide_core::Cid) {
    let solvent = Solvent::new();
    let cell = solvent.add(value.clone());
    let store = MemoryStore::new();
    let (value_cid, schema_cid) = solvent.persist_cell(&cell, &store).unwrap();
    let schemas = Solvent::new();
    let _ = load_schema_recursive(&store, &schemas, schema_cid).unwrap();
    let text = export(
        &store,
        &schemas,
        value_cid,
        schema_cid,
        format,
        &ExportOptions {
            profile,
            pretty: true,
            unwrap_top_level_occurrence: false,
            exclude_top_level_fields: Vec::new(),
        },
    )
    .unwrap();
    (text, value_cid, schema_cid)
}

pub fn decode_bottle(store: &MemoryStore, bond: &Bond<ReagentBottle>) -> ReagentBottle {
    let cid = bond.cid();
    let bytes = store.get(&key_from_cid(&cid)).unwrap().unwrap();
    ReagentBottle::from_bytes(&bytes).unwrap()
}

pub fn resolve_bench_into_solvent(
    solvent: &Solvent,
    bench: &ReactionBench,
    store: &MemoryStore,
) -> ReactionBench {
    let substrate = solvent.add(decode_bottle(store, &bench.substrate));
    let reagent = solvent.add(decode_bottle(store, &bench.reagent));
    let solvent_bottle = solvent.add(decode_bottle(store, &bench.solvent));

    ReactionBench {
        station: bench.station.clone(),
        substrate: Bond::from_cell(substrate),
        reagent: Bond::from_cell(reagent),
        solvent: Bond::from_cell(solvent_bottle),
    }
}

pub fn bench_components_resolved(bench: &ReactionBench) -> bool {
    bench.substrate.value().is_some()
        && bench.reagent.value().is_some()
        && bench.solvent.value().is_some()
}
