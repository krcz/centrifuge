//! Polyepoxide TUI explorer tool.

mod app;
mod export;
mod store;
mod tree;
mod ui;

use std::path::PathBuf;
use std::str::FromStr;

use cid::Cid;
use clap::{Parser, Subcommand};

use app::App;
use export::{export, ExportFormat, ExportOptions};
use store::AnyStore;

#[derive(Parser)]
#[command(name = "polyepoxide-tool")]
#[command(about = "TUI explorer for polyepoxide graph structures")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Explore a graph in the TUI
    Explore {
        /// CID of the root value
        #[arg(long)]
        cid: String,

        /// CID of the root value's schema
        #[arg(long)]
        schema: String,

        /// Store type: fjall or rocks
        #[arg(long, default_value = "fjall")]
        store: String,

        /// Path to the store
        #[arg(long)]
        path: PathBuf,
    },

    /// Export a value to JSON or YAML
    Export {
        /// CID of the root value
        #[arg(long)]
        cid: String,

        /// CID of the root value's schema
        #[arg(long)]
        schema: String,

        /// Store type: fjall or rocks
        #[arg(long, default_value = "fjall")]
        store: String,

        /// Path to the store
        #[arg(long)]
        path: PathBuf,

        /// Output format: json or yaml
        #[arg(long, default_value = "json")]
        format: String,

        /// Maximum depth to expand bonds (0 = only $ref)
        #[arg(long, default_value = "2")]
        depth: usize,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Explore {
            cid,
            schema,
            store,
            path,
        } => {
            let root_cid = Cid::from_str(&cid)?;
            let schema_cid = Cid::from_str(&schema)?;
            let store = open_store(&store, &path)?;

            let mut app = App::new(store, root_cid, schema_cid)?;
            app.run()?;
        }
        Command::Export {
            cid,
            schema,
            store,
            path,
            format,
            depth,
            output,
        } => {
            let root_cid = Cid::from_str(&cid)?;
            let schema_cid = Cid::from_str(&schema)?;
            let store = open_store(&store, &path)?;

            let format = match format.to_lowercase().as_str() {
                "json" => ExportFormat::Json,
                "yaml" | "yml" => ExportFormat::Yaml,
                _ => return Err(format!("unknown format: {}", format).into()),
            };

            let options = ExportOptions {
                depth,
                pretty: true,
            };

            // Build a solvent with the schema
            let mut schemas = polyepoxide_core::Solvent::new();
            load_schema_recursive(&store, &mut schemas, schema_cid)?;

            let content = export(&store, &schemas, root_cid, schema_cid, format, &options)?;

            match output {
                Some(path) => std::fs::write(path, content)?,
                None => print!("{}", content),
            }
        }
    }

    Ok(())
}

fn open_store(store_type: &str, path: &PathBuf) -> Result<AnyStore, Box<dyn std::error::Error>> {
    match store_type.to_lowercase().as_str() {
        "fjall" => Ok(AnyStore::open_fjall(path)?),
        "rocks" | "rocksdb" => Ok(AnyStore::open_rocks(path)?),
        _ => Err(format!("unknown store type: {}", store_type).into()),
    }
}

fn load_schema_recursive(
    store: &AnyStore,
    schemas: &mut polyepoxide_core::Solvent,
    cid: Cid,
) -> Result<(), Box<dyn std::error::Error>> {
    use polyepoxide_core::{Store, Structure};

    if schemas.get::<Structure>(&cid).is_some() {
        return Ok(());
    }

    let bytes = store
        .get(&cid)?
        .ok_or_else(|| format!("schema not found: {}", cid))?;

    let schema: Structure = serde_ipld_dagcbor::from_slice(&bytes)?;

    // Recursively load nested schemas
    match &schema {
        Structure::Sequence(inner) | Structure::Bond(inner) => {
            load_schema_recursive(store, schemas, inner.cid())?;
        }
        Structure::Tuple(elems) => {
            for elem in elems {
                load_schema_recursive(store, schemas, elem.cid())?;
            }
        }
        Structure::Record(fields) | Structure::Tagged(fields) => {
            for (_, field) in fields {
                load_schema_recursive(store, schemas, field.cid())?;
            }
        }
        Structure::Map { key: k, value: v } | Structure::OrderedMap { key: k, value: v } => {
            load_schema_recursive(store, schemas, k.cid())?;
            load_schema_recursive(store, schemas, v.cid())?;
        }
        _ => {}
    }

    schemas.add(schema);
    Ok(())
}
