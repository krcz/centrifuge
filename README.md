# Rapid Prototyping Idea Centrifuge

Centrifuge is a monorepo for rapidly prototyping a few personal projects. The rise of Large Language Models and coding agents allowed quick and low-cost creation of large amounts of code. As action creates information [^1], it can be useful for iterating on the design, even if the code itself is soon replaced or removed entirely.

## Polyepoxide

Polyepoxide is a blockchain-inspired synchronization database built on Merkle DAGs — the same content-addressed tree structure underlying blockchains, but applied to personal data synchronization rather than distributed consensus. It provides version control semantics for structured application data, with built-in support for offline editing, conflict resolution, and partial synchronization across devices.

### Merkle DAG with Structured Data

Data is organized as a directed acyclic graph where each node is identified by the Blake3 hash of its CBOR-serialized content. This enables deduplication, trustless verification, and natural caching — identical values yield identical hashes regardless of when or where they were created.

Unlike self-describing formats, Polyepoxide stores schemas separately in the DAG. Schemas and data are traversed in parallel (a "zipped" approach), keeping serialized data compact while preserving full type information. Schema types map directly to programming language constructs: sized integers (u8 through u64, i8 through i64), records, enums, tagged unions, and typed references (bonds) that enable lazy loading across DAG boundaries.

**Prototype status:** Core data model implemented in Rust. Schema types, CBOR serialization, and Cell/Bond/Solvent abstractions complete.

**Prior art:**
- *[IPLD](https://ipld.io/)*: Both use content-addressed DAGs. IPLD accepts a wide range of formats and uses unbounded integers. Polyepoxide focuses on tight programming language integration with bit-sized types and direct Rust/TypeScript mappings.
- *[Git objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)*: Git uses content addressing for blobs/trees/commits. Polyepoxide extends this to arbitrary structured data with schema validation.
- *[Ceramic](https://ceramic.network/)*/*[ComposeDB](https://composedb.js.org/)*: GraphQL schemas over IPLD with DID auth and blockchain anchoring. More opinionated than IPLD; requires consensus Polyepoxide avoids.
- *[Datomic](https://www.datomic.com/)*: Schema-as-data pattern storing attribute definitions as immutable datoms. Enables time-travel queries across schema versions. Datomic relies on a centralized Transactor for serialization; Polyepoxide is peer-to-peer, allowing updates via merge proposals.

### Synchronization Protocol

The sync protocol runs over libp2p, enabling any device to act as both client and server. Peers transfer data via symmetric push/pull operations — request nodes by hash, check existence, or upload batches. When syncing a node, all transitive dependencies are transferred first, maintaining the invariant that if a key exists locally, all its referenced nodes are present.

The protocol is designed for selective synchronization. A mobile client could sync only metadata and thumbnails for a photo gallery, fetching full-resolution images on demand. Sync preferences are per-device; peers don't track each other's filter configurations.

**Prototype status:** Basic protocol implemented in Rust. Protocol `/polyepoxide/sync/0.1.0` with Get/Has/Put operations. RemoteStore abstraction wraps peers as async stores. Selective sync configuration not yet implemented.

**Prior art:**
- *[GraphSync](https://ipld.io/specs/transport/graphsync/)*: IPFS protocol for syncing DAG subgraphs. Polyepoxide's approach is similar, with simpler request-response semantics.
- *[IPFS Bitswap](https://docs.ipfs.tech/concepts/bitswap/)*: Block-level exchange protocol. Bitswap optimizes for general file distribution; Polyepoxide is schema-aware and transfers structured data with dependencies.
- *[rsync](https://rsync.samba.org/)*: Delta-based file sync. Polyepoxide uses content addressing for deduplication rather than delta computation.
- *[CAR Mirror](https://github.com/fission-codes/car-mirror-spec)*: "rsync for DAGs" — client sends Bloom filter of known blocks, server streams missing blocks. Reduces round trips to O(1). Polyepoxide could adopt similar Bloom filtering for its sync layer.
- *[Negentropy](https://github.com/hoytech/negentropy)*: Range-based set reconciliation with O(d log n) communication. Powers Nostr relay sync at 10M+ elements. Applicable to Polyepoxide's hash-based node comparison.
- *[Hypercore](https://hypercore-protocol.org/)*/*[DAT](https://dat-ecosystem.org/)*: Append-only logs with signed Merkle trees enabling sparse replication. Similar "dependencies first" guarantees.
- *[OrbitDB](https://orbitdb.org/)*: Merkle-CRDTs on IPFS where operations link to causal predecessors. Polyepoxide's commits serve a similar causal ordering role, but OrbitDB uses automatic CRDT merging while Polyepoxide chooses explicit conflict handling.

### State and Computation

Schemas can contain validators — WASM code that constrains what values are valid for a type. This mimics refinement types: beyond structural constraints (field types, enum variants), schemas can enforce semantic properties like "positive integer", "valid email", or "blockchain with verified transitions". Validators run locally when creating or modifying data; invalid values are rejected before entering the DAG. This enables smart contract-like behavior without a blockchain.

Computation nodes (thunks) represent suspended computations whose results are deterministically derived from their inputs. Use cases include indices, derived views, schema migrations, and aggregations. The hash of a computation can be known before execution, enabling references to not-yet-computed results and caching across clients.

**Prototype status:** Not yet implemented.

**Prior art:**
- *[EUTXO (Cardano)](https://docs.cardano.org/learn/eutxo-explainer/)*: DAG nodes with validator scripts. Polyepoxide adapts this model for sync databases rather than blockchain consensus.
- *[Nix derivations](https://nixos.org/manual/nix/stable/language/derivations.html)*: Content-addressed build artifacts from deterministic computations. Polyepoxide's thunks serve a similar role for data transformations.
- *[IPLD ADLs](https://ipld.io/docs/advanced-data-layouts/)*: Advanced Data Layouts enable custom block transformations but suffer from a "signaling problem" — no standard way to indicate which ADL interprets a block. Polyepoxide's typed references (bonds) make interpretation explicit.
- *[Salsa](https://github.com/salsa-rs/salsa)*/*[Adapton](https://github.com/adapton/adapton.rust)*: Incremental computation frameworks that automatically re-run only necessary parts when inputs change. These are in-memory libraries; Polyepoxide bakes incremental computation into the database layer, treating derived views as first-class content-addressed data.
- *[CosmWasm](https://cosmwasm.com/)*: Battle-tested WASM validators with deterministic compilation (`rust-optimizer` for reproducible WASM), custom serde eliminating non-deterministic floating-point, and gas metering. Proves WASM validation works without consensus; verification is hash-based.
- *[IPVM](https://github.com/ipvm-wg/spec)*: Computation as content-addressed data — request CID maps to response CID. Enables network-wide memoization; directly relevant to thunks.
- *[Unison](https://www.unison-lang.org/)*: Functions identified by hashes of syntax trees; names are metadata. Cached results, no diamond dependencies.
- *[F*](https://www.fstar-lang.org/)*: Dependent types with refinement predicates, extracts to WASM. Powers verified TLS implementations. Academic foundation for validator expressiveness.
- *[Differential Dataflow](https://github.com/TimelyDataflow/differential-dataflow)*: Propagates deltas through dataflow graphs with time-proportional-to-change complexity. Relevant for derived views and indices.

### Version Control and Conflict Resolution

State history is tracked through content-addressed commits. When the same data is edited on multiple devices offline, divergent states are detected upon sync. Independent changes merge automatically — edits to different fields or different records combine without intervention. Only true conflicts (concurrent edits to the same field) are surfaced explicitly; the system never silently discards changes.

The conflict philosophy favors explicit user resolution over automatic semantic merging. CRDTs can produce semantically nonsensical results for structured data (calendars, task lists, documents with cross-references). Per-field merge strategies can be configured: last-writer-wins for simple fields, CRDT-based merging for long-form text, manual resolution for semantically significant changes.

**Prototype status:** Not yet implemented.

**Prior art:**
- *[Git](https://git-scm.com/)*/*[Mercurial](https://www.mercurial-scm.org/)*: Snapshot-based VCS. Polyepoxide extends this to structured application data, not just files.
- *[DARCS](http://darcs.net/)*/*[Pijul](https://pijul.org/)*: Patch-based VCS with commutative patches. Polyepoxide focuses on tree diffing rather than patch algebra.
- *[CouchDB](https://couchdb.apache.org/)*: Document database with multi-master replication and conflict detection. Similar philosophy of exposing conflicts, but CouchDB treats whole documents as the conflict unit. Polyepoxide isolates conflicts to specific fields, enabling schema-aware resolution.
- *[CRDTs (Automerge)](https://automerge.org/)*: Automatic conflict-free merging. CRDTs guarantee convergence but not semantic correctness — a counter can go negative, concurrent text insertions can interleave characters ("fboaor" instead of "foobar"). Polyepoxide rejects pure CRDTs for structured data, surfacing conflicts for human judgment.
- *[Noms](https://github.com/attic-labs/noms)*/*[Dolt](https://www.dolthub.com/)*: Noms pioneered prolly trees (content-addressed B-trees with rolling-hash boundaries) for structured Merkle DAGs. Dolt, its successor, implements MySQL-compatible SQL with cell-level three-way merge and conflicts stored in system tables.
- *[Irmin](https://github.com/mirage/irmin)*: OCaml library with programmable merge functions per content type. Enables CRDT-like behavior when desired while preserving explicit conflicts otherwise.

### Interfaces for Interoperability

A GraphQL interface enables querying the DAG from external applications, leveraging existing tooling for selective field fetching across graph structures.

MCP (Model Context Protocol) provides first-class access for LLM agents: read nodes, search by content, propose changes, and review history. Agent-initiated changes are written to a separate branch state rather than the main DAG, allowing users to review and accept modifications while maintaining human oversight.

**Prototype status:** Not yet implemented.

**Prior art:**
- *[GraphQL](https://graphql.org/)*: Native fit for DAG data. Standard ecosystem for typed, selective queries.
- *[MCP](https://modelcontextprotocol.io/)*: Anthropic's protocol for LLM tool access. Enables structured agent interaction with the database.

### Built-in Cryptography

Subtree-level encryption enables storing sensitive data on public infrastructure (e.g., Filecoin) without exposing contents. Encryption operates at natural boundaries — per document, per collection, or per sensitivity level — rather than per-node, reducing key management overhead.

A key hierarchy (master → domain → collection) allows fine-grained sharing. Revoking access protects new data; the system accepts that past data may have been copied rather than attempting expensive re-encryption.

Future zero-knowledge capabilities are considered, including membership proofs, transition validity verification, and selective disclosure of record fields.

**Prototype status:** Not yet implemented.

**Prior art:**
- *[IPFS](https://ipfs.tech/)*/*[Filecoin](https://filecoin.io/)*: Content-addressed storage without built-in encryption. Polyepoxide adds an encryption layer.
- *[Tahoe-LAFS](https://tahoe-lafs.org/)*: Encrypted distributed storage with capability-based access. Similar encryption-at-rest model.
- *[Aleo](https://aleo.org/)*: ZK-private state transitions. Polyepoxide's ZK plans are less ambitious but conceptually aligned.
- *[Peergos](https://peergos.org/)*/*[Cryptree](https://peergos.org/posts/cryptree)*: Subtree encryption where directory keys derive child keys. "Secret links" keep decryption keys client-side. Peergos explicitly rejects convergent encryption to prevent confirmation attacks; Polyepoxide likely uses convergent encryption within a user's dataset for deduplication, accepting this trade-off for personal data.
- *[WNFS](https://github.com/wnfs-wg/rs-wnfs)*: Extends cryptrees with skip ratchets for efficient temporal key derivation. Combines encryption with CRDTs for offline-first collaboration.
- *[UCAN](https://ucan.xyz/)*: JWT-extended tokens with embedded capabilities and proof chains, verified via DID public keys. Rights can only be attenuated on delegation, never amplified; revocation uses content-addressed revocation lists. Likely the right authorization model for Polyepoxide.

[^1]: The idea was [put into these words by Brian Armstrong](https://www.youtube.com/shorts/ysXKy9MCQy4), who learned it from Paul Graham.
