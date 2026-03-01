//! Polyepoxide TUI explorer tool.

mod app;
mod store;
mod tree;
mod ui;

use std::path::PathBuf;
use std::str::FromStr;

use cid::Cid;
use clap::{Parser, Subcommand};
use polyepoxide_core::{ExportFormat, ExportOptions, ExportProfile, Solvent, export, load_schema_recursive};

use app::App;
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

    /// Export a value to YAML, JSON-LD, or YAML-LD
    Export {
        /// CID of the root bond or value
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

        /// Output format: jsonld, yaml, or yamlld
        #[arg(long, default_value = "jsonld")]
        format: String,

        /// Export profile: canonical, full, or direct
        #[arg(long, default_value = "full")]
        profile: String,

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
            profile,
            output,
        } => {
            let root_cid = Cid::from_str(&cid)?;
            let schema_cid = Cid::from_str(&schema)?;
            let store = open_store(&store, &path)?;

            let format = match format.to_lowercase().as_str() {
                "jsonld" | "json-ld" | "json" => ExportFormat::JsonLd,
                "yaml" | "yml" => ExportFormat::Yaml,
                "yamlld" | "yaml-ld" => ExportFormat::YamlLd,
                _ => return Err(format!("unknown format: {}", format).into()),
            };

            let profile = match profile.to_lowercase().as_str() {
                "canonical" => ExportProfile::Canonical,
                "full" => ExportProfile::Full,
                "direct" => ExportProfile::Direct,
                _ => return Err(format!("unknown profile: {}", profile).into()),
            };

            let options = ExportOptions {
                profile,
                pretty: true,
            };

            let schemas = Solvent::new();
            let _ = load_schema_recursive(&store, &schemas, schema_cid)?;

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
