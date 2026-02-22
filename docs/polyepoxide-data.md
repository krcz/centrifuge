# Polyepoxide Data Structures

## Why Standardize Structures

Polyepoxide can store arbitrarily large graphs, but applications still need reusable collection and indexing primitives with deterministic semantics. Standardized structures ensure:

- deterministic content-addressed shape across implementations,
- cross-language interoperability without ad-hoc interpretation,
- reusable building blocks for large collections and secondary indices.

This document is a compatibility specification. New structures should follow the same section layout.

## Standardized Structures

Current standardized structures:

1. Patricia Radix Trie

Additional structures may be standardized in future revisions.

## Patricia Radix Trie

### Motivation

Use this structure when collections are too large for a single node and require deterministic key-based lookup/indexing. Typical use cases:

- indexing objects by `Cid`,
- secondary indexes over application keys,
- prefix/range discovery over ordered keys.

### Model

The trie stores values as bonds and is itself oxide-serializable.

```text
type RadixNode<V> {
  segment: ByteString
  value: Option<Bond<V>>
  children: Sequence<Bond<RadixNode<V>>>
}

type RadixTrie<V> {
  root: RadixNode<V>
}
```

`segment` is the compressed path fragment from parent to node.

### Invariants

1. `root.segment` MUST be empty.
2. Non-root `segment` MUST be non-empty.
3. If a full key ends at a node path, `value` MUST be present; otherwise it MUST be absent.
4. Children MUST be sorted by first-byte ascending over child `segment`.
5. Sibling children MUST have distinct first bytes.
6. The trie MUST be path-compressed (Patricia): branching occurs at the first differing byte.

### Algorithms

Insert:

1. Encode key to a byte key.
2. Walk child by first byte.
3. If no child matches, add a leaf with the remaining suffix.
4. If a child exists, split at longest common prefix when needed.
5. Reinsert/split children in canonical sorted order.
6. Reinserting an existing key replaces its value.

Lookup:

1. Encode key to a byte key.
2. Walk child edges by first byte.
3. Require full segment-prefix match at each step.
4. Succeed only when all key bytes are consumed and `value` is present.

Range:

1. Encode bounds to byte keys.
2. Return entries where `start <= byte_key < end`.
3. Return order MUST be lexicographic over byte keys.

### Normative Details

#### Key Encoding Profiles

All trie semantics are defined over lexicographic byte keys. For any profile used in range/index operations:

```text
a < b  (logical order)  =>  encode(a) < encode(b)  (lexicographic byte order)
```

Supported profile mappings:

1. `bytes` / `ByteString`: `encode(k) = k`
2. `string`: UTF-8 bytes
3. `u8/u16/u32/u64`: fixed-width big-endian bytes
4. `i8/i16/i32/i64`: fixed-width big-endian with highest bit flipped
5. `Cid`: binary CID bytes

Notes:

- Byte key ordering is comparable only within the same key profile.
- If an index mixes logical key domains, the application MUST define a stable global key namespace/encoding policy.

#### Oxide/Bond Integration

1. `RadixNode<V>` and `RadixTrie<V>` MUST be oxide values.
2. Child edges MUST be `Bond<RadixNode<V>>`.
3. Stored payloads MUST be `Bond<V>`.
4. Standard solvent dissolve/resolve behavior MUST apply.
5. Persisting trie roots MUST persist transitive dependencies via bond traversal.

The trie forms a merklized Patricia DAG in the generic Polyepoxide sense.

### Compatibility Checklist

Two implementations are compatible for this structure if they agree on:

1. key profile encoding,
2. Patricia split and child-order rules,
3. invariants above,
4. range interval semantics (`[start, end)` over byte keys),
5. oxide/bond persistence semantics.
