# POLYEPOXIDE-CLI

## Dependencies

- `13-POLYEPOXIDE-EXPORT`
- `13-POLYEPOXIDE-FJALL`
- `13-POLYEPOXIDE-ROCKSDB`

## Intent

Define the portable command-line surface for inspecting Polyepoxide stores, resolving graph roots, exploring value/schema graphs, and exporting graph content.

Polyepoxide graphs are content-addressed and schema-guided, so ordinary database tooling cannot show enough context to debug application roots, bond targets, schema shape, or reflexive traversal. The CLI provides the operator-facing inspection surface for local persistent stores. It should make it possible to open a store, identify a root by bookmark or explicit CIDs, browse the graph interactively, and render export documents without requiring application-specific oxide types to be compiled into the tool.

This spec describes the language-agnostic CLI behavior. Language-specific specs should describe concrete binary names, terminal UI libraries, host-language dispatch types, exact key bindings, and filesystem/error APIs.

## Command Surface

The tool exposes two required command families:

- `explore`: interactive terminal graph explorer
- `export`: batch export of a root graph to a document format

Portable command grammar:

```text
<tool> explore --path <path> [--store <store>] (--bookmark <name> | --cid <cid> --schema <schema_cid>)

<tool> export  --path <path> [--store <store>] (--bookmark <name> | --cid <cid> --schema <schema_cid>)
               [--format <format>] [--profile <profile>] [--output <path>]
```

`--path <path>` is required for persistent local backends. `--store <store>` selects the backend and defaults to `fjall`.

Supported store values:

- `fjall`
- `rocks`
- `rocksdb`

`rocks` and `rocksdb` are aliases for the RocksDB-backed store.

Root selection is required and must use exactly one of:

- `--bookmark <name>`
- `--cid <cid> --schema <schema_cid>`

`--cid` and `--schema` are a pair. Supplying only one is invalid. Supplying either explicit CID flag together with `--bookmark` is invalid.

The export command accepts:

- `--format jsonld`
- `--format json-ld`
- `--format json`
- `--format yaml`
- `--format yml`
- `--format yamlld`
- `--format yaml-ld`

The default export format is `jsonld`. `json`, `jsonld`, and `json-ld` all select JSON-LD output. `yml` and `yaml` select YAML output. `yamlld` and `yaml-ld` select YAML-LD output.

The export command accepts:

- `--profile canonical`
- `--profile full`
- `--profile direct`

The default export profile is `full`.

`--output <path>` writes export output to a file. When omitted, export writes the document to standard output.

## Root Resolution

A command resolves its root to `(value_cid, schema_cid)` before doing graph work.

Bookmark resolution loads a named bookmark from the selected store. The logical bookmark value is `DynamicBond`, so a bookmark carries both:

- `bond.cid()`: the value root CID
- `schema.cid()`: the schema root CID

Explicit root resolution parses the two CIDs supplied by `--cid` and `--schema`.

Bookmark roots are preferred for application workflows because they keep value identity and schema context together. Explicit CIDs are useful for low-level debugging, reproducible fixtures, and stores without bookmark metadata.

Portable root-resolution interface:

```text
type RootSelector:
  Bookmark(name: string)
  Explicit(value_cid: CID, schema_cid: CID)

function resolve_root(store: Store, selector: RootSelector) -> Result<(CID, CID)>
```

## Store Selection

The CLI must open one of the standard persistent stores through a common store interface. Backend choice is a runtime option and must not change graph semantics, export semantics, bookmark behavior, or root-resolution behavior.

Portable store-opening interface:

```text
enum StoreBackend:
  fjall
  rocksdb

type StoreOptions:
  backend: StoreBackend
  path: path

function open_store(options: StoreOptions) -> Result<Store>
```

The returned store must satisfy the normal `12-POLYEPOXIDE-STORE` contract inherited through the backend specs, including multihash-keyed block lookup and `name -> DynamicBond` bookmark behavior.

## Explore Command

`explore` starts an interactive terminal UI at the resolved root. It displays the root value as a schema-guided tree and lets the user inspect nested values, bond targets, and schema context.

The explorer should show:

- the current focused root CID
- breadcrumb or equivalent focus history
- the selected node's schema-derived type hint
- scalar values where compact enough to display inline
- collection sizes for sequences, tuples, maps, and records
- bond targets as navigable references
- enough schema context to understand erased or dynamic positions

The explorer must decode persisted bytes through `Structure` rather than through application-specific host-language oxide types. It should use store-backed traversal equivalent to `13-POLYEPOXIDE-EXPORT` so that ordinary links and reflexive ligation edges follow the same scope rules as export.

Required explorer actions:

- move selection through the tree
- expand and collapse tree nodes
- zoom into a selected bond target, making it the focused root
- zoom out to a previous focused root
- zoom to the schema of the selected node
- export the current focused root or selected bond target
- quit without modifying store content

Exact key bindings and terminal layout are language-specific.

When the explorer encounters missing blocks, malformed bytes, unresolved ligation, or schema/value shape mismatches, it should report the problem at the current interaction boundary instead of silently displaying incorrect data. Implementations may fail command startup for errors required to build the initial root view.

## Export Command

`export` renders the resolved root as YAML, JSON-LD, or YAML-LD using the selected export profile. The command must call the export semantics from `13-POLYEPOXIDE-EXPORT`; CLI-specific defaults and file handling must not alter import/export meaning.

Portable export flow:

```text
function run_export(command: ExportCommand) -> Result<void>:
  store = open_store(command.store_options)
  (root_cid, schema_cid) = resolve_root(store, command.root_selector)
  schemas = Solvent.new()
  load_schema_recursive(store, schemas, schema_cid)
  document = export(store, schemas, root_cid, schema_cid, command.format, command.options)
  write document to command.output_path or stdout
```

The batch export command exports the command root. Interactive explorer export may export either the current focused root or the selected navigable bond target, as long as it supplies the matching schema CID and uses the same export semantics.

## Standard Interfaces

Language implementations should expose behavior equivalent to the following pseudocode. Names may follow host-language conventions.

```text
enum Command:
  Explore(options: ExploreCommand)
  Export(options: ExportCommand)

type CommonCommandOptions:
  store: StoreOptions
  root: RootSelector

type ExploreCommand:
  common: CommonCommandOptions

type ExportCommand:
  common: CommonCommandOptions
  format: ExportFormat
  profile: ExportProfile
  output_path: optional<path>

enum ExportFormat:
  JSON_LD
  YAML
  YAML_LD

enum ExportProfile:
  canonical
  full
  direct
```

Interactive explorer implementations should use an internal tree model equivalent to:

```text
type ExplorerNode:
  id: string
  label: string
  value_cid: optional<CID>
  schema_cid: CID
  value_scope: list<CID>
  schema_scope: list<CID>
  type_hint: string
  children: list<ExplorerNode>

type Focus:
  value_cid: CID
  schema_cid: CID
  value_scope: list<CID>
  schema_scope: list<CID>

interface Explorer:
  focus() -> Focus
  select_next() -> void
  select_previous() -> void
  toggle_selected() -> void
  zoom_in_selected() -> Result<bool>
  zoom_out() -> Result<bool>
  zoom_to_selected_schema() -> Result<bool>
  export_selected(format: ExportFormat, profile: ExportProfile) -> Result<string>
```

The tree representation is not a storage format. It is a presentation model derived from store bytes and schema traversal.

## Compatibility Checks

The CLI should be checked against the following behavior:

- opening a Fjall store through `--store fjall --path <path>`
- opening a RocksDB store through both `--store rocks --path <path>` and `--store rocksdb --path <path>`
- resolving a root from a bookmark containing a `DynamicBond`
- resolving a root from explicit `--cid` and `--schema` values
- rejecting missing root selectors
- rejecting mixed bookmark and explicit-CID root selectors
- rejecting an explicit root with only one of `--cid` or `--schema`
- exporting JSON-LD, YAML, and YAML-LD
- exporting with `canonical`, `full`, and `direct` profiles
- writing export output to stdout when `--output` is omitted
- writing export output to a file when `--output` is supplied
- loading the schema graph before export
- preserving export semantics from `13-POLYEPOXIDE-EXPORT`
- navigating, expanding, zooming into a bond, zooming out, and zooming to schema in the explorer

## Out of Scope

- importing documents from the CLI
- mutating graph content
- bookmark management commands
- long-running synchronization or network replication commands
- backend implementation internals
- exact terminal UI layout and key bindings
- host-language command parser, terminal, and filesystem libraries
