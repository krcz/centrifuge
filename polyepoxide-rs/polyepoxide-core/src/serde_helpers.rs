//! Serde helpers for Polyepoxide-specific encodings.
//!
//! These modules provide custom serialization for types that need
//! non-default CBOR representations.

/// Serialize `Option<T>` as an array: `[]` for None, `[x]` for Some(x).
///
/// This encoding allows distinguishing `None` from `Some(null)` when T is nullable.
pub mod option_as_array {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        match value {
            None => serializer.collect_seq(std::iter::empty::<T>()),
            Some(v) => serializer.collect_seq(std::iter::once(v)),
        }
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let vec: Vec<T> = Vec::deserialize(deserializer)?;
        match vec.len() {
            0 => Ok(None),
            1 => Ok(vec.into_iter().next()),
            n => Err(serde::de::Error::invalid_length(
                n,
                &"0 or 1 elements for Option",
            )),
        }
    }
}

/// Serialize `Result<T, E>` with lowercase keys: `{"ok": x}` or `{"err": e}`.
pub mod result_lowercase {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T, E, S>(value: &Result<T, E>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        E: Serialize,
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(1))?;
        match value {
            Ok(v) => map.serialize_entry("ok", v)?,
            Err(e) => map.serialize_entry("err", e)?,
        }
        map.end()
    }

    pub fn deserialize<'de, T, E, D>(deserializer: D) -> Result<Result<T, E>, D::Error>
    where
        T: Deserialize<'de>,
        E: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};
        use std::marker::PhantomData;

        struct ResultVisitor<T, E>(PhantomData<(T, E)>);

        impl<'de, T, E> Visitor<'de> for ResultVisitor<T, E>
        where
            T: Deserialize<'de>,
            E: Deserialize<'de>,
        {
            type Value = Result<T, E>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map with 'ok' or 'err' key")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| serde::de::Error::missing_field("ok or err"))?;

                match key.as_str() {
                    "ok" => Ok(Ok(map.next_value()?)),
                    "err" => Ok(Err(map.next_value()?)),
                    other => Err(serde::de::Error::unknown_field(other, &["ok", "err"])),
                }
            }
        }

        deserializer.deserialize_map(ResultVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_dagcbor<T: serde::Serialize>(value: &T) -> Vec<u8> {
        serde_ipld_dagcbor::to_vec(value).unwrap()
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct WithOption {
        #[serde(with = "option_as_array")]
        value: Option<i32>,
    }

    #[test]
    fn option_none_as_empty_array() {
        let v = WithOption { value: None };
        let bytes = to_dagcbor(&v);
        let recovered: WithOption = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(recovered, v);
    }

    #[test]
    fn option_some_as_singleton_array() {
        let v = WithOption { value: Some(42) };
        let bytes = to_dagcbor(&v);
        let recovered: WithOption = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(recovered, v);
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct WithResult {
        #[serde(with = "result_lowercase")]
        value: Result<i32, String>,
    }

    #[test]
    fn result_ok_lowercase() {
        let v = WithResult { value: Ok(42) };
        let bytes = to_dagcbor(&v);
        let recovered: WithResult = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(recovered, v);
    }

    #[test]
    fn result_err_lowercase() {
        let v = WithResult {
            value: Err("oops".to_string()),
        };
        let bytes = to_dagcbor(&v);
        let recovered: WithResult = serde_ipld_dagcbor::from_slice(&bytes).unwrap();
        assert_eq!(recovered, v);
    }
}
