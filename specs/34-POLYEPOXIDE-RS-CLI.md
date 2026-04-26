# POLYEPOXIDE-RS-CLI

## Dependencies

- `14-POLYEPOXIDE-CLI`
- `33-POLYEPOXIDE-RS-EXPORT`
- `33-POLYEPOXIDE-RS-FJALL`
- `33-POLYEPOXIDE-RS-ROCKSDB`

## Intent

Describe how the language-agnostic CLI contract from `14-POLYEPOXIDE-CLI` is implemented in Rust by the `polyepoxide-tool` crate.

This spec focuses on the Rust crate structure, command parsing, runtime store dispatch, TUI state model, store-backed tree traversal, and the Rust-specific implementation choices that matter for compatibility. It does not restate the portable CLI contract except where the Rust representation adds important detail.

## Crate Surface

The Rust CLI lives in `polyepoxide-rs/polyepoxide-tool`.

The crate package is named `polyepoxide-tool`. Its installed binary target is:

```toml
[[bin]]
name = "px"
path = "src/main.rs"
```

The clap command metadata currently uses `polyepoxide-tool` as the command name shown in generated help. The binary invoked through Cargo or installation is `px`.

The crate depends on:

- `polyepoxide-core` with the `derive` feature
- `polyepoxide-fjall`
- `polyepoxide-rocks`
- `cid`, `ipld-core`, and `serde_ipld_dagcbor`
- `clap` with `derive`
- `ratatui`, `crossterm`, and `tui-tree-widget`
- `thiserror`
- `unicode-segmentation`

## Modules

- `main.rs`: clap entry point, command definitions, root resolution, store opening, batch export.
- `store.rs`: runtime-dispatched `AnyStore`.
- `app.rs`: TUI application state, terminal setup, event loop, key handling, interactive export.
- `tree.rs`: store-backed tree model, schema-guided node construction, focus navigation.
- `ui.rs`: ratatui rendering.

## Command Parsing

Rust uses `clap` derive types for command parsing:

```rust
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Explore { root: RootArgs, store: String, path: PathBuf },
    Export {
        root: RootArgs,
        store: String,
        path: PathBuf,
        format: String,
        profile: String,
        output: Option<PathBuf>,
    },
}

#[derive(Args, Clone, Debug)]
struct RootArgs {
    bookmark: Option<String>,
    cid: Option<String>,
    schema: Option<String>,
}
```

The accepted command surface is:

```text
px explore --path <path> [--store <fjall|rocks|rocksdb>]
           (--bookmark <name> | --cid <cid> --schema <schema_cid>)

px export  --path <path> [--store <fjall|rocks|rocksdb>]
           (--bookmark <name> | --cid <cid> --schema <schema_cid>)
           [--format <jsonld|json-ld|json|yaml|yml|yamlld|yaml-ld>]
           [--profile <canonical|full|direct>]
           [-o <path> | --output <path>]
```

Defaults:

- `--store` defaults to `fjall`.
- `--format` defaults to `jsonld`.
- `--profile` defaults to `full`.
- omitted `--output` writes export output to stdout.

`RootArgs` uses clap validation to reject mixed bookmark and explicit-CID root selectors and to require `--cid` and `--schema` as a pair. `resolve_root` still validates that one complete root selector was supplied.

## Root Resolution

Rust implements root resolution as:

```rust
fn resolve_root<S: Store>(
    store: &S,
    root: &RootArgs,
) -> Result<(Cid, Cid), Box<dyn std::error::Error>>;
```

When `--bookmark <name>` is supplied, `resolve_root` calls `Store::get_bookmark(name)`. The decoded bookmark is a `DynBond`, so the command root is:

```rust
(bookmark.cid(), bookmark.schema_cid())
```

When explicit CIDs are supplied, both strings are parsed with `Cid::from_str`.

The function returns an error when a bookmark is missing or when no complete root selector is available.

## Store Dispatch

`store.rs` defines the runtime dispatch layer:

```rust
#[derive(Debug, Error)]
pub enum AnyStoreError {
    Fjall(#[from] polyepoxide_fjall::FjallError),
    Rocks(#[from] polyepoxide_rocks::RocksError),
}

pub enum AnyStore {
    Fjall(FjallStore),
    Rocks(RocksStore),
}
```

Constructors:

```rust
impl AnyStore {
    pub fn open_fjall(path: impl AsRef<Path>) -> Result<Self, AnyStoreError>;
    pub fn open_rocks(path: impl AsRef<Path>) -> Result<Self, AnyStoreError>;
}
```

`open_store(store_type, path)` in `main.rs` maps:

- `fjall` to `AnyStore::open_fjall(path)`
- `rocks` and `rocksdb` to `AnyStore::open_rocks(path)`

`AnyStore` implements `polyepoxide_core::Store` by delegating all block and bookmark byte methods to the selected backend:

```rust
impl Store for AnyStore {
    type Error = AnyStoreError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    fn has(&self, key: &[u8]) -> Result<bool, Self::Error>;
    fn get_bookmark_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, Self::Error>;
    fn put_bookmark_bytes(&self, name: &str, value: &[u8]) -> Result<(), Self::Error>;
}
```

Typed bookmark behavior comes from the default `Store` trait methods described in `32-POLYEPOXIDE-RS-STORE`.

## Batch Export

The `export` subcommand:

1. opens the selected store
2. resolves `(root_cid, schema_cid)`
3. parses `format` into `ExportFormat`
4. parses `profile` into `ExportProfile`
5. builds `ExportOptions`
6. creates a schema `Solvent`
7. calls `load_schema_recursive(&store, &schemas, schema_cid)`
8. calls `export(&store, &schemas, root_cid, schema_cid, format, &options)`
9. writes to `--output` or stdout

Format parsing maps:

- `jsonld`, `json-ld`, and `json` to `ExportFormat::JsonLd`
- `yaml` and `yml` to `ExportFormat::Yaml`
- `yamlld` and `yaml-ld` to `ExportFormat::YamlLd`

Profile parsing maps:

- `canonical` to `ExportProfile::Canonical`
- `full` to `ExportProfile::Full`
- `direct` to `ExportProfile::Direct`

Batch export constructs:

```rust
let options = ExportOptions {
    profile,
    pretty: true,
    unwrap_top_level_occurrence: false,
    exclude_top_level_fields: Vec::new(),
};
```

## TUI Application

`app.rs` defines:

```rust
pub struct App {
    pub tree: TreeModel,
    pub tree_state: TreeState<NodeId>,
    pub should_quit: bool,
    pub last_error: Option<String>,
    pub export_path: Option<PathBuf>,
}
```

`App::new(store, root_cid, schema_cid)` creates a `TreeModel`, selects the first root node, and opens it in `TreeState`.

`App::run` enables raw mode, enters the alternate screen, creates a `ratatui::Terminal<CrosstermBackend<Stdout>>`, runs the event loop, then disables raw mode and leaves the alternate screen.

The event loop redraws the UI and handles key press events from `crossterm`.

Current key behavior:

| Key | Behavior |
| --- | --- |
| `q`, Esc | quit |
| Up, `k` | move selection up |
| Down, `j` | move selection down |
| Left, `h` | tree-widget left action |
| Right, `l` | tree-widget right action |
| Enter | toggle selected node expansion |
| `z` | zoom into selected node |
| `s` | zoom to selected node's schema |
| `b`, Backspace | zoom out |
| `e` | export selected/current target as JSON-LD |
| `y` | export selected/current target as YAML |
| `L` | export selected/current target as YAML-LD |

`handle_key` clears `last_error` before handling each key. Navigation errors from zoom and interactive export are stored in `last_error`.

Interactive exports use `ExportOptions::default()`. If the selected node has a `cid`, the TUI exports that CID with the node's schema CID. Otherwise it exports the current focused root. Output is written to:

```text
export_<first-12-cid-chars>.<jsonld|yaml|yamlld>
```

On successful write, `export_path` stores the generated filename.

## Tree Model

`tree.rs` defines the store-backed exploration model:

```rust
pub struct TreeModel {
    pub nodes: HashMap<NodeId, NodeData>,
    pub roots: Vec<NodeId>,
    pub breadcrumbs: Vec<Breadcrumb>,
    store: AnyStore,
    schemas: Solvent,
    root_cid: Cid,
    root_schema_cid: Cid,
    root_context: Vec<Cid>,
    root_schema_scope: Vec<Cid>,
}
```

`TreeModel::new(store, root_cid, root_schema_cid)` creates a schema solvent, loads the root schema with `load_schema_recursive`, and builds the initial tree.

The model is rebuilt whenever the focused root changes. Rebuilding clears `nodes` and `roots`, creates a `StoreCursor` from `(root_cid, root_schema_cid)`, decodes the root IPLD value, builds a root `NodeId`, and recursively populates child nodes.

### NodeId

`NodeId` is a stable string wrapper used as the `tui-tree-widget` item identifier:

```rust
pub struct NodeId(String);
```

Root IDs have the form:

```text
root:<cid>
```

Child IDs append the child key to the parent ID:

```text
<parent_id>:<field-or-index>
```

### NodeData

Each tree node stores traversal and display metadata:

```rust
pub struct NodeData {
    pub cid: Option<Cid>,
    pub schema_cid: Cid,
    pub schema_scope: Vec<Cid>,
    pub type_hint: String,
    pub display: String,
    pub children: Vec<NodeId>,
    pub context: Vec<Cid>,
    pub cursor_state: CursorState,
}
```

`cid` is present for IPLD links. For `Structure::Bond`, reflexive CIDs are resolved with `resolve_reflexive_with_store` against the node context when possible; otherwise the stored link CID is kept.

`cursor_state` lets later zoom and schema actions reconstruct a `StoreCursor` for the selected node.

### Schema-Guided Children

`collect_children` interprets generic `Ipld` values through `Structure`:

| `Structure` | Child behavior |
| --- | --- |
| `Record` | children for present fields in schema order |
| `Option` | list entries, direct `some` value, or no children for null |
| `Sequence` | one child per list element |
| `Tuple` | one child per tuple element |
| `Tagged` | child for the single variant payload when recognized |
| `Map` | one child per map value, keyed by map key text |
| `OrderedMap` | separate `[i].key` and `[i].value` children |
| `Bond` | follows the link with `StoreCursor::follow_bond` and inlines target children under the bond node |
| primitives and `Enum` | no children |

Child schema states are created with `StoreCursor::child_schema_cursor`. Bond targets are followed with `StoreCursor::follow_bond`.

The current implementation builds children eagerly for the current focused tree. Missing blocks, malformed bytes, or traversal errors during build can fail startup or zoom/rebuild operations.

### Navigation

Tree focus navigation is implemented by:

```rust
pub fn zoom_in(&mut self, node_id: &NodeId) -> Result<bool, Box<dyn std::error::Error>>;
pub fn zoom_out(&mut self) -> Result<bool, Box<dyn std::error::Error>>;
pub fn zoom_to_schema(&mut self, node_id: &NodeId) -> Result<bool, Box<dyn std::error::Error>>;
```

`zoom_in` requires the selected node to have a CID. It pushes the current root state to `breadcrumbs`, reconstructs a `StoreCursor` from the node's `CursorState`, follows the bond when the node schema is `Structure::Bond`, updates `root_cid`, `root_schema_cid`, value scope, and schema scope, then rebuilds the tree.

`zoom_out` pops the latest `Breadcrumb`, restores root CID, schema CID, value scope, and schema scope, then rebuilds the tree.

`zoom_to_schema` pushes a breadcrumb, sets `root_cid` to the selected node's `schema_cid`, sets `root_schema_cid` to `Structure::schema().cid()`, clears both scopes, and rebuilds the tree. This works because `Structure` is self-describing.

`breadcrumb_path()` renders the current focus stack as short CID labels joined by ` > `.

### Display Formatting

`format_node_display(label, ipld, schema)` builds one-line labels:

- `Structure::Bond` links render as `label: Bond<T>` followed by a short target CID.
- `Structure::Cid` links render as `label: Cid = <short-cid>`.
- strings are truncated at 30 grapheme clusters using `unicode-segmentation`
- integers, floats, and booleans render as scalar values
- bytes render as byte counts
- lists render item counts
- maps render only the label and type hint
- null renders as `= null`

`schema_to_type_hint` maps `Structure` to compact hints such as `String`, `Seq<T>`, `Record{a, b}`, `Tagged{ok|err}`, `Enum{a|b}`, and `Bond<T>`. When an inner schema bond is unavailable as a resolved value, the hint uses `?`.

CID shortening uses the first 12 grapheme clusters and appends `...` when truncated.

## UI Rendering

`ui.rs` renders three vertical regions with ratatui:

- header: title with breadcrumb path and selected node details
- tree: `tui_tree_widget::Tree` built from `TreeModel::tree_items()`
- help bar: visible key hints

Selected-node details include the type hint, CID when present, and value context scope when non-empty. `last_error` and `export_path` are tracked by `App` but are not currently rendered by `ui.rs`.

The tree widget uses:

- dark gray bold highlight style
- a right-pointing triangle symbol for closed nodes
- a down-pointing triangle symbol for open nodes
- no-children symbol of two spaces

## Rust Design Choices

- The CLI uses `Box<dyn std::error::Error>` at the command boundary to keep the prototype command code small.
- `AnyStore` is used instead of making the TUI generic over backend type, because backend choice is a runtime CLI option.
- Schema traversal is delegated to `StoreCursor` and `load_schema_recursive` from `polyepoxide-core`, preserving the same semantics as Rust export.
- The TUI tree is a presentation cache over persisted store bytes and schema traversal state; it is not a persisted graph representation.
- The current tree build is eager within the focused root rather than demand-loading individual expanded nodes.
- Exact key bindings and ratatui layout are Rust implementation details, while the portable action set belongs to `14-POLYEPOXIDE-CLI`.
- The tool currently exposes inspection and export workflows only. Import, mutation, bookmark management, and sync commands are future work.

## Compatibility Checks

The Rust CLI should be covered by tests or manual checks equivalent to:

- `resolve_root` returns `(bookmark.cid(), bookmark.schema_cid())` for a `DynBond` bookmark
- `resolve_root` parses explicit value and schema CID strings
- clap rejects bookmark selectors mixed with explicit CID flags
- clap rejects only one of `--cid` or `--schema`
- `open_store` accepts `fjall`
- `open_store` accepts both `rocks` and `rocksdb`
- unknown store types return an error
- export format parsing accepts `jsonld`, `json-ld`, `json`, `yaml`, `yml`, `yamlld`, and `yaml-ld`
- export profile parsing accepts `canonical`, `full`, and `direct`
- batch export loads schemas recursively before calling `export`
- batch export writes stdout when `--output` is absent and writes a file when it is present
- `TreeModel::new` loads the root schema and creates a selected/open root
- tree building produces children for records, sequences, tuples, maps, ordered maps, tagged payloads, options, and bonds
- zoom in, zoom out, and zoom to schema update root state and rebuild the tree
- TUI export writes the selected CID when selected node has one and falls back to the current root otherwise

## Out of Scope

- portable CLI semantics already defined by `14-POLYEPOXIDE-CLI`
- import command implementation
- graph mutation workflows
- bookmark management commands
- synchronization commands
- backend storage internals
- replacing ratatui/crossterm/tui-tree-widget with another terminal stack
