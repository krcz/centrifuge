//! Polyepoxide TUI explorer tool.

mod app;
mod store;
mod tree;
mod ui;

use std::path::PathBuf;
use std::str::FromStr;

use cid::Cid;
use clap::{Args, Parser, Subcommand};
use polyepoxide_core::{
    export, load_schema_recursive, ExportFormat, ExportOptions, ExportProfile, Solvent, Store,
};

use app::App;
use store::AnyStore;

#[derive(Parser)]
#[command(name = "polyepoxide-tool")]
#[command(about = "TUI explorer for polyepoxide graph structures")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Clone, Debug)]
struct RootArgs {
    /// Bookmark name for the root value
    #[arg(long, conflicts_with_all = ["cid", "schema"])]
    bookmark: Option<String>,

    /// CID of the root value
    #[arg(long, requires = "schema")]
    cid: Option<String>,

    /// CID of the root value's schema
    #[arg(long, requires = "cid")]
    schema: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Explore a graph in the TUI
    Explore {
        #[command(flatten)]
        root: RootArgs,

        /// Store type: fjall or rocks
        #[arg(long, default_value = "fjall")]
        store: String,

        /// Path to the store
        #[arg(long)]
        path: PathBuf,
    },

    /// Export a value to YAML, JSON-LD, or YAML-LD
    Export {
        #[command(flatten)]
        root: RootArgs,

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
        Command::Explore { root, store, path } => {
            let store = open_store(&store, &path)?;
            let (root_cid, schema_cid) = resolve_root(&store, &root)?;

            let mut app = App::new(store, root_cid, schema_cid)?;
            app.run()?;
        }
        Command::Export {
            root,
            store,
            path,
            format,
            profile,
            output,
        } => {
            let store = open_store(&store, &path)?;
            let (root_cid, schema_cid) = resolve_root(&store, &root)?;

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
                unwrap_top_level_occurrence: false,
                exclude_top_level_fields: Vec::new(),
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

fn resolve_root<S: Store>(
    store: &S,
    root: &RootArgs,
) -> Result<(Cid, Cid), Box<dyn std::error::Error>> {
    if let Some(bookmark) = &root.bookmark {
        let bookmark = store
            .get_bookmark(bookmark)?
            .ok_or_else(|| format!("bookmark not found: {}", bookmark))?;
        return Ok((bookmark.cid(), bookmark.schema_cid()));
    }

    match (&root.cid, &root.schema) {
        (Some(cid), Some(schema)) => Ok((Cid::from_str(cid)?, Cid::from_str(schema)?)),
        _ => Err("provide either --bookmark or both --cid and --schema".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::{Bond, DynBond, MemoryStore};

    #[test]
    fn resolve_root_from_bookmark() {
        let store = MemoryStore::new();
        let bookmark = DynBond::from_typed(Bond::new("hello".to_string()));
        store.put_bookmark("root", &bookmark).unwrap();

        let root = RootArgs {
            bookmark: Some("root".to_string()),
            cid: None,
            schema: None,
        };

        let resolved = resolve_root(&store, &root).unwrap();
        assert_eq!(resolved, (bookmark.cid(), bookmark.schema_cid()));
    }

    #[test]
    fn resolve_root_from_cid_and_schema() {
        let store = MemoryStore::new();
        let bookmark = DynBond::from_typed(Bond::new("hello".to_string()));
        let root = RootArgs {
            bookmark: None,
            cid: Some(bookmark.cid().to_string()),
            schema: Some(bookmark.schema_cid().to_string()),
        };

        let resolved = resolve_root(&store, &root).unwrap();
        assert_eq!(resolved, (bookmark.cid(), bookmark.schema_cid()));
    }
}
