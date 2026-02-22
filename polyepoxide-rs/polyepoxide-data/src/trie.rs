use polyepoxide_core::oxide;
use polyepoxide_core::{Bond, ByteString, Oxide};

use crate::OrderedKey;

/// A single Patricia radix node.
///
/// `segment` stores the compressed path segment from parent to this node.
/// Children are sorted by the first byte of their segment.
#[oxide]
#[serde(bound = "T: Oxide")]
pub struct RadixNode<T: Oxide> {
    pub segment: ByteString,
    pub value: Option<Bond<T>>,
    pub children: Vec<Bond<RadixNode<T>>>,
}

impl<T: Oxide> RadixNode<T> {
    fn root() -> Self {
        Self {
            segment: ByteString::new(Vec::new()),
            value: None,
            children: Vec::new(),
        }
    }

    fn leaf(segment: Vec<u8>, value: Bond<T>) -> Self {
        Self {
            segment: ByteString::new(segment),
            value: Some(value),
            children: Vec::new(),
        }
    }

    fn child_first_byte(child: &Bond<RadixNode<T>>) -> u8 {
        child
            .value()
            .and_then(|node| node.segment.as_bytes().first().copied())
            .expect("radix trie mutation requires resolved children with non-empty segments")
    }

    fn sorted_children(mut children: Vec<Bond<RadixNode<T>>>) -> Vec<Bond<RadixNode<T>>> {
        children.sort_by_key(Self::child_first_byte);
        children
    }

    fn insert_relative(&mut self, key: &[u8], value: Bond<T>) {
        if key.is_empty() {
            self.value = Some(value);
            return;
        }

        let first = key[0];
        let pos = self
            .children
            .binary_search_by_key(&first, Self::child_first_byte);

        match pos {
            Err(insert_at) => {
                self.children
                    .insert(insert_at, Bond::new(RadixNode::leaf(key.to_vec(), value)));
            }
            Ok(index) => {
                let mut child = self.children[index]
                    .value()
                    .cloned()
                    .expect("radix trie mutation requires resolved child nodes");

                let child_segment = child.segment.as_bytes().to_vec();
                let common = common_prefix_len(&child_segment, key);

                if common == child_segment.len() {
                    if common == key.len() {
                        child.value = Some(value);
                    } else {
                        child.insert_relative(&key[common..], value);
                    }
                    self.children[index] = Bond::new(child);
                    return;
                }

                let mut existing_child = child;
                existing_child.segment = ByteString::new(child_segment[common..].to_vec());
                let existing_bond = Bond::new(existing_child);

                let mut split = RadixNode {
                    segment: ByteString::new(child_segment[..common].to_vec()),
                    value: None,
                    children: Vec::new(),
                };

                if common == key.len() {
                    split.value = Some(value);
                    split.children.push(existing_bond);
                } else {
                    let new_leaf = Bond::new(RadixNode::leaf(key[common..].to_vec(), value));
                    split.children = Self::sorted_children(vec![existing_bond, new_leaf]);
                }

                self.children[index] = Bond::new(split);
            }
        }
    }

    fn get_relative(&self, key: &[u8]) -> Option<Bond<T>> {
        if key.is_empty() {
            return self.value.clone();
        }

        let first = key[0];
        let child = self.children.iter().find_map(|child| {
            let node = child.value()?;
            let head = node.segment.as_bytes().first().copied()?;
            if head == first { Some(node) } else { None }
        })?;
        let segment = child.segment.as_bytes();
        if !key.starts_with(segment) {
            return None;
        }

        child.get_relative(&key[segment.len()..])
    }

    fn collect_entries(&self, prefix: &mut Vec<u8>, out: &mut Vec<(ByteString, Bond<T>)>) {
        let original_len = prefix.len();
        prefix.extend_from_slice(self.segment.as_bytes());

        if let Some(value) = &self.value {
            out.push((ByteString::new(prefix.clone()), value.clone()));
        }

        for child in &self.children {
            if let Some(node) = child.value() {
                node.collect_entries(prefix, out);
            }
        }

        prefix.truncate(original_len);
    }
}

/// A deterministic, byte-ordered Patricia radix trie.
///
/// Shape is canonical for a given key-value set as long as keys are encoded with
/// a deterministic, order-preserving `OrderedKey` implementation.
#[oxide]
#[serde(bound = "T: Oxide")]
pub struct RadixTrie<T: Oxide> {
    pub root: RadixNode<T>,
}

impl<T: Oxide> RadixTrie<T> {
    pub fn new() -> Self {
        Self {
            root: RadixNode::root(),
        }
    }

    pub fn insert<K: OrderedKey>(&mut self, key: K, value: Bond<T>) {
        let encoded = key.encode_key();
        self.root.insert_relative(&encoded, value);
    }

    pub fn insert_value<K: OrderedKey>(&mut self, key: K, value: T) {
        self.insert(key, Bond::new(value));
    }

    pub fn get<K: OrderedKey>(&self, key: K) -> Option<Bond<T>> {
        let encoded = key.encode_key();
        self.root.get_relative(&encoded)
    }

    pub fn entries(&self) -> Vec<(ByteString, Bond<T>)> {
        let mut out = Vec::new();
        let mut prefix = Vec::new();
        self.root.collect_entries(&mut prefix, &mut out);
        out
    }

    /// Returns entries in the half-open interval `[start, end)`.
    pub fn range<K: OrderedKey>(&self, start: &K, end: &K) -> Vec<(ByteString, Bond<T>)> {
        let lower = start.encode_key();
        let upper = end.encode_key();
        if lower >= upper {
            return Vec::new();
        }

        self.entries()
            .into_iter()
            .filter(|(key, _)| {
                let bytes = key.as_bytes();
                bytes >= lower.as_slice() && bytes < upper.as_slice()
            })
            .collect()
    }
}

impl<T: Oxide> Default for RadixTrie<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::RadixTrie;
    use polyepoxide_core::{Bond, MemoryStore, Oxide, Solvent, Store, key_from_cid};

    #[test]
    fn insertion_order_invariant_shape() {
        let pairs = [
            ("ab", "v1"),
            ("abc", "v2"),
            ("abd", "v3"),
            ("b", "v4"),
            ("ba", "v5"),
            ("car", "v6"),
        ];

        let mut left = RadixTrie::<String>::new();
        for (k, v) in pairs {
            left.insert(k, Bond::new(v.to_string()));
        }

        let mut right = RadixTrie::<String>::new();
        for (k, v) in pairs.into_iter().rev() {
            right.insert(k, Bond::new(v.to_string()));
        }

        assert_eq!(left.compute_cid(), right.compute_cid());
    }

    #[test]
    fn get_roundtrip_for_mixed_prefixes() {
        let mut trie = RadixTrie::<String>::new();
        trie.insert("a", Bond::new("A".to_string()));
        trie.insert("ab", Bond::new("AB".to_string()));
        trie.insert("abc", Bond::new("ABC".to_string()));
        trie.insert("b", Bond::new("B".to_string()));

        assert_eq!(
            trie.get("a").and_then(|b| b.value().cloned()),
            Some("A".to_string())
        );
        assert_eq!(
            trie.get("ab").and_then(|b| b.value().cloned()),
            Some("AB".to_string())
        );
        assert_eq!(
            trie.get("abc").and_then(|b| b.value().cloned()),
            Some("ABC".to_string())
        );
        assert_eq!(
            trie.get("b").and_then(|b| b.value().cloned()),
            Some("B".to_string())
        );
        assert!(trie.get("ac").is_none());
    }

    #[test]
    fn range_query_uses_lexicographic_order() {
        let mut trie = RadixTrie::<String>::new();
        trie.insert("a", Bond::new("A".to_string()));
        trie.insert("ab", Bond::new("AB".to_string()));
        trie.insert("b", Bond::new("B".to_string()));
        trie.insert("ba", Bond::new("BA".to_string()));
        trie.insert("c", Bond::new("C".to_string()));

        let got: Vec<_> = trie
            .range(&"ab", &"c")
            .into_iter()
            .map(|(k, _)| String::from_utf8(k.0).unwrap())
            .collect();

        assert_eq!(
            got,
            vec!["ab".to_string(), "b".to_string(), "ba".to_string()]
        );
    }

    #[test]
    fn trie_is_store_persistable() {
        let solvent = Solvent::new();
        let store = MemoryStore::new();

        let mut trie = RadixTrie::<String>::new();
        trie.insert("alpha", Bond::new("A".to_string()));
        trie.insert("beta", Bond::new("B".to_string()));

        let cell = solvent.add(trie);
        let (value_cid, schema_cid) = solvent.persist_cell(&cell, &store).unwrap();

        let value_key = key_from_cid(&value_cid);
        let schema_key = key_from_cid(&schema_cid);

        assert!(store.get(&value_key).unwrap().is_some());
        assert!(store.get(&schema_key).unwrap().is_some());
    }
}
