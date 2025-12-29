use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A 32-byte Blake3 hash that uniquely identifies an oxide.
///
/// Keys are serialized as CBOR byte strings (major type 2), not as arrays.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key([u8; 32]);

impl Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KeyVisitor;

        impl serde::de::Visitor<'_> for KeyVisitor {
            type Value = Key;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("32-byte key")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(v);
                    Ok(Key(arr))
                } else {
                    Err(E::invalid_length(v.len(), &"32 bytes"))
                }
            }
        }

        deserializer.deserialize_bytes(KeyVisitor)
    }
}

impl Key {
    /// Computes the key (hash) of the given data.
    pub fn from_data(data: &[u8]) -> Self {
        Key(*blake3::hash(data).as_bytes())
    }

    /// Creates a key from raw bytes (e.g., when deserializing from CBOR).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Key(bytes)
    }

    /// Returns the key as a byte slice.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key({})", self)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_deterministic() {
        let data = b"hello world";
        let k1 = Key::from_data(data);
        let k2 = Key::from_data(data);
        assert_eq!(k1, k2);
    }

    #[test]
    fn key_different_data() {
        let k1 = Key::from_data(b"hello");
        let k2 = Key::from_data(b"world");
        assert_ne!(k1, k2);
    }

    #[test]
    fn key_display() {
        let k = Key::from_data(b"test");
        let s = format!("{}", k);
        assert_eq!(s.len(), 64); // 32 bytes * 2 hex chars
    }
}
