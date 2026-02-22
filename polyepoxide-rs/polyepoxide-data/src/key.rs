use polyepoxide_core::{ByteString, Cid};

/// Encodes keys to byte strings with lexicographic ordering semantics.
///
/// Implementations must preserve ordering: `a < b` in key space implies
/// `encode(a) < encode(b)` in lexicographic byte order.
pub trait OrderedKey {
    fn encode_key(&self) -> Vec<u8>;
}

impl OrderedKey for ByteString {
    fn encode_key(&self) -> Vec<u8> {
        self.0.clone()
    }
}

impl OrderedKey for Vec<u8> {
    fn encode_key(&self) -> Vec<u8> {
        self.clone()
    }
}

impl OrderedKey for &[u8] {
    fn encode_key(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl OrderedKey for String {
    fn encode_key(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl OrderedKey for &str {
    fn encode_key(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl OrderedKey for Cid {
    fn encode_key(&self) -> Vec<u8> {
        self.to_bytes()
    }
}

impl OrderedKey for &Cid {
    fn encode_key(&self) -> Vec<u8> {
        self.to_bytes()
    }
}

macro_rules! impl_unsigned_ordered_key {
    ($t:ty) => {
        impl OrderedKey for $t {
            fn encode_key(&self) -> Vec<u8> {
                self.to_be_bytes().to_vec()
            }
        }
    };
}

macro_rules! impl_signed_ordered_key {
    ($t:ty, $u:ty) => {
        impl OrderedKey for $t {
            fn encode_key(&self) -> Vec<u8> {
                let bits = <$u>::from_be_bytes(self.to_be_bytes());
                let normalized = bits ^ (1 << (<$u>::BITS - 1));
                normalized.to_be_bytes().to_vec()
            }
        }
    };
}

impl_unsigned_ordered_key!(u8);
impl_unsigned_ordered_key!(u16);
impl_unsigned_ordered_key!(u32);
impl_unsigned_ordered_key!(u64);

impl_signed_ordered_key!(i8, u8);
impl_signed_ordered_key!(i16, u16);
impl_signed_ordered_key!(i32, u32);
impl_signed_ordered_key!(i64, u64);

#[cfg(test)]
mod tests {
    use super::OrderedKey;
    use polyepoxide_core::compute_cid;

    #[test]
    fn unsigned_encoding_preserves_order() {
        let values = [0u32, 1, 2, 255, 256, 65_535, 65_536, u32::MAX];

        for pair in values.windows(2) {
            let a = pair[0].encode_key();
            let b = pair[1].encode_key();
            assert!(a < b);
        }
    }

    #[test]
    fn signed_encoding_preserves_order() {
        let values = [i32::MIN, -1024, -1, 0, 1, 1024, i32::MAX];

        for pair in values.windows(2) {
            let a = pair[0].encode_key();
            let b = pair[1].encode_key();
            assert!(a < b);
        }
    }

    #[test]
    fn cid_encoding_matches_binary_cid_bytes() {
        let cid = compute_cid(b"polyepoxide-data");
        assert_eq!(cid.encode_key(), cid.to_bytes());
        assert_eq!((&cid).encode_key(), cid.to_bytes());
    }
}
