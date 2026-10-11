//! SQL parameter and result types for the database binding

use base64::Engine;
use base64::alphabet;
use base64::engine::DecodePaddingMode;
use base64::engine::GeneralPurpose;
use base64::engine::GeneralPurposeConfig;
use serde::de;
use serde::ser::SerializeMap;
use std::fmt;

/// JSON key tagging a base64 byte string (see [`SqlPrimitive::Bytes`])
const BYTES_TAG: &str = "$bytes";

/// Base64 engine for [`SqlPrimitive::Bytes`]: standard alphabet, padded on
/// encode, padded or unpadded accepted on decode (JS `btoa` pads, Rust may not).
const BYTES_BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// SQL primitive value - used directly or within arrays
///
/// Serializes as the bare JSON value (null, bool, number, string) except for
/// [`SqlPrimitive::Bytes`], which JSON cannot express: it is tagged as
/// `{"$bytes": "<base64>"}` so a byte string is never confused with an array
/// of integers.
#[derive(Debug, Clone)]
pub enum SqlPrimitive {
    /// NULL value
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value (i64)
    Int(i64),
    /// Floating point value (f64)
    Float(f64),
    /// String value
    String(String),
    /// Binary value (BYTEA, BLOB) - JSON form is `{"$bytes": "<base64>"}`
    Bytes(Vec<u8>),
}

impl serde::Serialize for SqlPrimitive {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            SqlPrimitive::Null => serializer.serialize_unit(),
            SqlPrimitive::Bool(value) => serializer.serialize_bool(*value),
            SqlPrimitive::Int(value) => serializer.serialize_i64(*value),
            SqlPrimitive::Float(value) => serializer.serialize_f64(*value),
            SqlPrimitive::String(value) => serializer.serialize_str(value),
            SqlPrimitive::Bytes(value) => {
                let mut map = serializer.serialize_map(Some(1))?;

                map.serialize_entry(BYTES_TAG, &BYTES_BASE64.encode(value))?;
                map.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for SqlPrimitive {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SqlPrimitiveVisitor)
    }
}

/// Visitor accepting any JSON scalar, plus `{"$bytes": "<base64>"}`
struct SqlPrimitiveVisitor;

impl<'de> de::Visitor<'de> for SqlPrimitiveVisitor {
    type Value = SqlPrimitive;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "null, a boolean, a number, a string or {{\"{}\": \"<base64>\"}}",
            BYTES_TAG
        )
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::Null)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::Int(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        match i64::try_from(value) {
            Ok(value) => Ok(SqlPrimitive::Int(value)),
            Err(_) => Err(de::Error::custom(format!(
                "integer {} overflows i64",
                value
            ))),
        }
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::Float(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::String(value.to_string()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::String(value))
    }

    /// Native byte strings: serde_v8 hands an `ArrayBuffer` or a typed array
    /// to `deserialize_any` this way
    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::Bytes(value.to_vec()))
    }

    fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(SqlPrimitive::Bytes(value))
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let key: String = match map.next_key()? {
            Some(key) => key,
            None => {
                return Err(de::Error::custom(format!(
                    "expected an object with a single '{}' key, got an empty object",
                    BYTES_TAG
                )));
            }
        };

        if key != BYTES_TAG {
            return Err(de::Error::custom(format!(
                "unknown key '{}', expected '{}'",
                key, BYTES_TAG
            )));
        }

        let encoded: String = map.next_value()?;

        let bytes = BYTES_BASE64.decode(&encoded).map_err(|err| {
            de::Error::custom(format!("invalid base64 in '{}': {}", BYTES_TAG, err))
        })?;

        if map.next_key::<String>()?.is_some() {
            return Err(de::Error::custom(format!(
                "expected an object with a single '{}' key",
                BYTES_TAG
            )));
        }

        Ok(SqlPrimitive::Bytes(bytes))
    }
}

/// SQL parameter value - primitives or arrays of primitives
///
/// Deserializes transparently from JS values (as `#[serde(untagged)]` would):
/// a JSON array is always an [`SqlParam::Array`], anything else is a
/// [`SqlPrimitive`]. PostgreSQL supports arrays of primitives (e.g.,
/// `WHERE id = ANY($1::int[])`).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum SqlParam {
    /// Array of primitives (for ANY() queries, etc.)
    Array(Vec<SqlPrimitive>),
    /// Single primitive value
    Primitive(SqlPrimitive),
}

impl<'de> serde::Deserialize<'de> for SqlParam {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SqlParamVisitor)
    }
}

/// Visitor routing sequences to [`SqlParam::Array`] and everything else to
/// [`SqlPrimitive`]; an untagged enum would report base64 errors as the
/// useless "did not match any variant".
struct SqlParamVisitor;

impl<'de> de::Visitor<'de> for SqlParamVisitor {
    type Value = SqlParam;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a SQL primitive or an array of SQL primitives")
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));

        while let Some(value) = seq.next_element()? {
            values.push(value);
        }

        Ok(SqlParam::Array(values))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(SqlParam::Primitive(SqlPrimitive::Null))
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_bool(value)
            .map(SqlParam::Primitive)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_i64(value)
            .map(SqlParam::Primitive)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_u64(value)
            .map(SqlParam::Primitive)
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_f64(value)
            .map(SqlParam::Primitive)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_str(value)
            .map(SqlParam::Primitive)
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_string(value)
            .map(SqlParam::Primitive)
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_bytes(value)
            .map(SqlParam::Primitive)
    }

    fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        SqlPrimitiveVisitor
            .visit_byte_buf(value)
            .map(SqlParam::Primitive)
    }

    fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        SqlPrimitiveVisitor.visit_map(map).map(SqlParam::Primitive)
    }
}

/// Database operation types for SQL queries
#[derive(Debug, Clone)]
pub enum DatabaseOp {
    /// Execute a SQL query
    Query {
        /// SQL statement
        sql: String,
        /// Query parameters
        params: Vec<SqlParam>,
    },
}

/// Result from a database operation
///
/// A handler returns either shape: `Rows` is the JSON path, `Table` is the
/// typed path, which lets [`SqlPrimitive::Bytes`] reach the runtime without a
/// JSON re-parse. Core never converts one into the other; each runtime decides
/// which shapes it accepts.
#[derive(Debug)]
pub enum DatabaseResult {
    /// Rows as JSON array
    Rows(String),
    /// Typed rows: column names plus one value per column, per row
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<SqlPrimitive>>,
    },
    /// Error message
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::round_trip;

    #[test]
    fn sql_primitive_scalars_round_trip() {
        assert_eq!(round_trip::<SqlPrimitive>("null"), "null");
        assert_eq!(round_trip::<SqlPrimitive>("true"), "true");
        assert_eq!(round_trip::<SqlPrimitive>("42"), "42");
        assert_eq!(round_trip::<SqlPrimitive>("-42"), "-42");
        assert_eq!(round_trip::<SqlPrimitive>("1.5"), "1.5");
        assert_eq!(round_trip::<SqlPrimitive>("\"widget\""), "\"widget\"");
    }

    #[test]
    fn sql_primitive_bytes_round_trip() {
        let json = r#"{"$bytes":"AQI="}"#;

        let value: SqlPrimitive = serde_json::from_str(json).unwrap();

        match &value {
            SqlPrimitive::Bytes(bytes) => assert_eq!(bytes, &[1, 2]),
            other => panic!("expected Bytes, got {:?}", other),
        }

        assert_eq!(serde_json::to_string(&value).unwrap(), json);
    }

    #[test]
    fn sql_primitive_bytes_accepts_unpadded_base64() {
        let value: SqlPrimitive = serde_json::from_str(r#"{"$bytes":"AQI"}"#).unwrap();

        match value {
            SqlPrimitive::Bytes(bytes) => assert_eq!(bytes, vec![1, 2]),
            other => panic!("expected Bytes, got {:?}", other),
        }
    }

    #[test]
    fn sql_primitive_bytes_empty_round_trip() {
        let json = r#"{"$bytes":""}"#;

        let value: SqlPrimitive = serde_json::from_str(json).unwrap();

        match &value {
            SqlPrimitive::Bytes(bytes) => assert!(bytes.is_empty()),
            other => panic!("expected Bytes, got {:?}", other),
        }

        assert_eq!(serde_json::to_string(&value).unwrap(), json);
    }

    #[test]
    fn sql_primitive_rejects_invalid_base64() {
        let err = serde_json::from_str::<SqlPrimitive>(r#"{"$bytes":"!!!!"}"#).unwrap_err();

        assert!(
            err.to_string().contains("invalid base64 in '$bytes'"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn sql_primitive_rejects_unknown_tag() {
        let err = serde_json::from_str::<SqlPrimitive>(r#"{"$blob":"AQI="}"#).unwrap_err();

        assert!(
            err.to_string().contains("unknown key '$blob'"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn sql_primitive_rejects_extra_keys() {
        let err =
            serde_json::from_str::<SqlPrimitive>(r#"{"$bytes":"AQI=","extra":1}"#).unwrap_err();

        assert!(
            err.to_string().contains("single '$bytes' key"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn sql_primitive_rejects_empty_object() {
        let err = serde_json::from_str::<SqlPrimitive>("{}").unwrap_err();

        assert!(
            err.to_string().contains("empty object"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn sql_primitive_rejects_array() {
        let err = serde_json::from_str::<SqlPrimitive>("[1,2,3]").unwrap_err();

        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn sql_param_integer_array_never_becomes_bytes() {
        let value: SqlParam = serde_json::from_str("[1,2,3]").unwrap();

        match value {
            SqlParam::Array(items) => {
                assert_eq!(items.len(), 3);
                assert!(
                    items
                        .iter()
                        .all(|item| matches!(item, SqlPrimitive::Int(_)))
                );
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn sql_param_object_becomes_bytes() {
        let value: SqlParam = serde_json::from_str(r#"{"$bytes":"AQID"}"#).unwrap();

        match value {
            SqlParam::Primitive(SqlPrimitive::Bytes(bytes)) => assert_eq!(bytes, vec![1, 2, 3]),
            other => panic!("expected Bytes primitive, got {:?}", other),
        }
    }

    #[test]
    fn sql_param_array_may_hold_bytes() {
        let json = r#"[{"$bytes":"AQID"},"widget",null]"#;

        let value: SqlParam = serde_json::from_str(json).unwrap();

        match &value {
            SqlParam::Array(items) => assert_eq!(items.len(), 3),
            other => panic!("expected Array, got {:?}", other),
        }

        assert_eq!(serde_json::to_string(&value).unwrap(), json);
    }

    #[test]
    fn sql_param_list_round_trips() {
        let json = r#"[null,true,42,1.5,"widget",[1,2],{"$bytes":"AQI="}]"#;

        assert_eq!(round_trip::<Vec<SqlParam>>(json), json);
    }

    /// Deserializes through `visit_byte_buf`, as serde_v8 does for a typed array
    struct ByteBuf(Vec<u8>);

    impl<'de> serde::Deserializer<'de> for ByteBuf {
        type Error = de::value::Error;

        fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            visitor.visit_byte_buf(self.0)
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct enum identifier ignored_any
        }
    }

    #[test]
    fn sql_primitive_accepts_native_bytes() {
        let value =
            <SqlPrimitive as serde::Deserialize>::deserialize(ByteBuf(vec![0, 255])).unwrap();

        match value {
            SqlPrimitive::Bytes(bytes) => assert_eq!(bytes, vec![0, 255]),
            other => panic!("expected Bytes, got {:?}", other),
        }
    }

    #[test]
    fn sql_param_accepts_native_bytes() {
        let value = <SqlParam as serde::Deserialize>::deserialize(ByteBuf(vec![1])).unwrap();

        match value {
            SqlParam::Primitive(SqlPrimitive::Bytes(bytes)) => assert_eq!(bytes, vec![1]),
            other => panic!("expected Bytes primitive, got {:?}", other),
        }
    }

    #[test]
    fn sql_param_reports_invalid_base64() {
        let err = serde_json::from_str::<SqlParam>(r#"{"$bytes":"!!!!"}"#).unwrap_err();

        assert!(
            err.to_string().contains("invalid base64 in '$bytes'"),
            "unexpected error: {}",
            err
        );
    }
}
