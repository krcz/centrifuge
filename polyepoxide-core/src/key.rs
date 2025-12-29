use serde::{Deserialize, Serialize};
use std::fmt;

/// A 32-byte Blake3 hash that uniquely identifies an oxide.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key([u8; 32]);

impl Key {
    /// Computes the key (hash) of the given data.
    pub fn from_data(data: &[u8]) -> Self {
        Key(*blake3::hash(data).as_bytes())
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
