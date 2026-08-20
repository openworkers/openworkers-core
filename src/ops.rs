//! Operations for runner-controlled operations
//!
//! Enum-based approach for extensibility. All runtime to runner communication
//! goes through OperationsHandler.
//!
//! This allows the runner to:
//! - Track stats (request counts, bytes transferred, latency)
//! - Enforce permissions (which URLs can be fetched)
//! - Enforce quotas (max requests per minute, etc.)
//! - Inject auth for bindings (ASSETS, R2, KV, etc.)
//! - Collect logs from workers
//!
//! ## Default behavior
//!
//! All handler methods have sensible defaults (stubs):
//! - `handle_fetch` returns "Fetch not available"
//! - `handle_binding_fetch` returns "Binding '<name>' not configured"
//! - `handle_binding_storage` returns "Storage binding '<name>' not implemented"
//! - `handle_binding_kv` returns "KV binding '<name>' not implemented"
//! - `handle_binding_database` returns "Database binding '<name>' not implemented"
//! - `handle_binding_images` returns "Images binding '<name>' not implemented"
//! - `handle_binding_worker` returns "Worker binding '<name>' not implemented"
//! - `handle_websocket_connect` returns "WebSocket not available"
//! - `handle_log` prints to stderr
//!
//! Runners only need to override the methods they want to implement.

use crate::{HttpRequest, HttpResponse, LogLevel, WebSocketConnection};
use base64::Engine;
use base64::alphabet;
use base64::engine::DecodePaddingMode;
use base64::engine::GeneralPurpose;
use base64::engine::GeneralPurposeConfig;
use serde::de;
use serde::ser::SerializeMap;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Storage operation types for get/put/head/list/delete/fetch
#[derive(Debug, Clone)]
pub enum StorageOp {
    /// Get an object by key (returns body as bytes)
    Get { key: String },
    /// Fetch an object by key (returns full HTTP response with headers)
    Fetch { key: String },
    /// Put an object (key + body)
    Put { key: String, body: Vec<u8> },
    /// Head (metadata only) for an object
    Head { key: String },
    /// List objects with optional prefix
    List {
        prefix: Option<String>,
        limit: Option<u32>,
    },
    /// Delete an object
    Delete { key: String },
}

/// Result from a storage operation
#[derive(Debug)]
pub enum StorageResult {
    /// Object body (for get) or empty (for put/delete success)
    Body(Option<Vec<u8>>),
    /// Full HTTP response (for fetch)
    Response(HttpResponse),
    /// Metadata from head operation
    Head { size: u64, etag: Option<String> },
    /// List of keys
    List { keys: Vec<String>, truncated: bool },
    /// Error message
    Error(String),
}

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

/// KV operation types for get/put/delete/list
#[derive(Debug, Clone)]
pub enum KvOp {
    /// Get a value by key
    Get { key: String },
    /// Put a value (key + JSON value, optional TTL in seconds)
    Put {
        key: String,
        value: serde_json::Value,
        expires_in: Option<u64>,
    },
    /// Delete a key
    Delete { key: String },
    /// List keys with optional prefix and limit
    List {
        prefix: Option<String>,
        limit: Option<u32>,
    },
}

/// Result from a KV operation
#[derive(Debug)]
pub enum KvResult {
    /// Value (for get) - None if key doesn't exist
    Value(Option<serde_json::Value>),
    /// List of keys (for list)
    Keys(Vec<String>),
    /// Success (for put/delete)
    Ok,
    /// Error message
    Error(String),
}

/// How a resized image fits its target box (CSS `object-fit` semantics)
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResizeFit {
    /// Scale to fit inside the box, keeping the aspect ratio
    Contain,
    /// Scale to fill the box, keeping the aspect ratio, cropping the overflow
    Cover,
    /// Shrink and crop to the box exactly, centered until gravity is added
    Crop,
    /// Scale to fit inside the box, padding the rest, transparent until
    /// background is added
    Pad,
    /// Like Contain, but never scale up
    ScaleDown,
}

/// Transform applied to an image
///
/// Non-exhaustive: the vocabulary catches up with Cloudflare one transform at
/// a time, in minor versions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ImageTransform {
    /// Resize to the given box (a missing side is derived from the other)
    Resize {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
        fit: ResizeFit,
    },
    /// Trim pixels off each edge, the only crop a guest can express: it never
    /// learns the image dimensions, so an absolute rectangle is out of reach
    Trim {
        #[serde(default)]
        top: u32,
        #[serde(default)]
        right: u32,
        #[serde(default)]
        bottom: u32,
        #[serde(default)]
        left: u32,
    },
    /// Rotate clockwise
    Rotate { degrees: u16 },
}

/// Encoding of a transformed image
///
/// Deserializes from the bare token or from the MIME type a Cloudflare worker
/// passes; serialization is always the bare token. Non-exhaustive: formats are
/// added in minor versions.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ImageFormat {
    /// PNG, lossless
    #[serde(alias = "image/png")]
    Png,
    /// JPEG, lossy
    #[serde(alias = "image/jpeg")]
    Jpeg,
    /// WebP, lossy
    #[serde(alias = "image/webp")]
    Webp,
    /// AVIF, lossy
    #[serde(alias = "image/avif")]
    Avif,
}

impl ImageFormat {
    /// MIME type an encoded image of this format is served with
    pub fn content_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Avif => "image/avif",
        }
    }
}

/// Output encoding requested for a transform
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ImageOutput {
    /// Target format
    pub format: ImageFormat,
    /// Encoder quality in 1..=100, ignored for Png
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,
}

/// Image operation types
///
/// Core only transports the descriptor. Validating it (degrees a multiple of
/// 90, quality within range, input actually decodable) is the handler's job.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImagesOp {
    /// Decode the input, apply the transforms in order, encode as `output`
    Transform {
        input: Vec<u8>,
        transforms: Vec<ImageTransform>,
        output: ImageOutput,
    },
}

/// Result from an image operation
///
/// The image is encoded in the format the caller asked for, so the MIME type
/// comes from [`ImageFormat::content_type`] rather than from the handler.
#[derive(Debug)]
pub enum ImagesResult {
    /// Encoded image
    Image { data: Vec<u8> },
    /// Error message
    Error(String),
}

/// Operations that runtimes delegate to the runner
#[derive(Debug)]
pub enum Operation {
    /// Direct HTTP fetch (pass-through, no auth modification)
    Fetch(HttpRequest),

    /// Fetch via a binding (ASSETS, R2, etc.) - runner injects auth
    BindingFetch {
        /// Binding name (e.g., "ASSETS", "MY_BUCKET")
        binding: String,
        /// The request (runner will add auth headers based on binding config)
        request: HttpRequest,
    },

    /// Storage operation (get/put/head/list/delete)
    BindingStorage {
        /// Binding name (e.g., "MY_STORAGE")
        binding: String,
        /// The operation to perform
        op: StorageOp,
    },

    /// KV operation (get/put/delete)
    BindingKv {
        /// Binding name (e.g., "MY_KV")
        binding: String,
        /// The operation to perform
        op: KvOp,
    },

    /// Database operation (SQL query)
    BindingDatabase {
        /// Binding name (e.g., "MY_DB")
        binding: String,
        /// The operation to perform
        op: DatabaseOp,
    },

    /// Image operation (transform)
    BindingImages {
        /// Binding name (e.g., "IMAGES")
        binding: String,
        /// The operation to perform
        op: ImagesOp,
    },

    /// Worker binding (worker-to-worker calls)
    BindingWorker {
        /// Binding name (e.g., "MY_WORKER")
        binding: String,
        /// The request to send to the target worker
        request: HttpRequest,
    },

    /// Log message (fire-and-forget)
    Log { level: LogLevel, message: String },

    /// Open an outgoing WebSocket connection
    WebSocketConnect {
        /// WebSocket URL (ws:// or wss://)
        url: String,
        /// Additional headers for the handshake
        headers: HashMap<String, String>,
    },
}

/// Results for operations
#[derive(Debug)]
pub enum OperationResult {
    /// HTTP response (used by Fetch and BindingFetch)
    Http(Result<HttpResponse, String>),

    /// Storage operation result
    Storage(StorageResult),

    /// KV operation result
    Kv(KvResult),

    /// Database operation result
    Database(DatabaseResult),

    /// Image operation result
    Images(ImagesResult),

    /// Acknowledgement for fire-and-forget operations (Log, etc.)
    Ack,

    /// WebSocket connection established (contains bidirectional channels)
    WebSocket(Result<WebSocketConnection, String>),
}

/// Future type alias for async operation results
pub type OpFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Handler for operations - implemented by the runner
///
/// All methods have default implementations (stubs) so runners only need
/// to override the operations they want to support.
///
/// # Example
///
/// ```ignore
/// struct MyRunner;
///
/// impl OperationsHandler for MyRunner {
///     // Only implement fetch, use defaults for the rest
///     fn handle_fetch(&self, request: HttpRequest) -> OpFuture<'_, Result<HttpResponse, String>> {
///         Box::pin(async move {
///             // Your fetch implementation here
///         })
///     }
/// }
/// ```
pub trait OperationsHandler: Send + Sync {
    /// Downcast to concrete type (for warm isolate reuse).
    ///
    /// Default implementation uses `Any`. Override if your handler needs
    /// to be downcasted for per-request state updates.
    fn as_any(&self) -> &dyn std::any::Any {
        // Default: no downcasting support (returns a dummy type)
        &()
    }

    /// Handle a fetch request
    ///
    /// Default: returns error "Fetch not available"
    fn handle_fetch(&self, _request: HttpRequest) -> OpFuture<'_, Result<HttpResponse, String>> {
        Box::pin(async { Err("Fetch not available".into()) })
    }

    /// Handle a binding fetch request (ASSETS, R2, KV, etc.)
    ///
    /// Default: returns error with binding name
    fn handle_binding_fetch(
        &self,
        binding: &str,
        _request: HttpRequest,
    ) -> OpFuture<'_, Result<HttpResponse, String>> {
        let err = format!("Binding '{}' not configured", binding);
        Box::pin(async move { Err(err) })
    }

    /// Handle a storage operation (get/put/head/list/delete)
    ///
    /// Default: returns error "not implemented"
    fn handle_binding_storage(&self, binding: &str, _op: StorageOp) -> OpFuture<'_, StorageResult> {
        let err = format!("Storage binding '{}' not implemented", binding);
        Box::pin(async move { StorageResult::Error(err) })
    }

    /// Handle a KV operation (get/put/delete)
    ///
    /// Default: returns error "not implemented"
    fn handle_binding_kv(&self, binding: &str, _op: KvOp) -> OpFuture<'_, KvResult> {
        let err = format!("KV binding '{}' not implemented", binding);
        Box::pin(async move { KvResult::Error(err) })
    }

    /// Handle a database operation (SQL query)
    ///
    /// Default: returns error "not implemented"
    fn handle_binding_database(
        &self,
        binding: &str,
        _op: DatabaseOp,
    ) -> OpFuture<'_, DatabaseResult> {
        let err = format!("Database binding '{}' not implemented", binding);
        Box::pin(async move { DatabaseResult::Error(err) })
    }

    /// Handle an image operation (transform)
    ///
    /// Default: returns error "not implemented"
    fn handle_binding_images(&self, binding: &str, _op: ImagesOp) -> OpFuture<'_, ImagesResult> {
        let err = format!("Images binding '{}' not implemented", binding);
        Box::pin(async move { ImagesResult::Error(err) })
    }

    /// Handle a worker binding (worker-to-worker call)
    ///
    /// Default: returns error "not implemented"
    fn handle_binding_worker(
        &self,
        binding: &str,
        _request: HttpRequest,
    ) -> OpFuture<'_, Result<HttpResponse, String>> {
        let err = format!("Worker binding '{}' not implemented", binding);
        Box::pin(async move { Err(err) })
    }

    /// Handle an outgoing WebSocket connection
    ///
    /// Default: returns error "WebSocket not available"
    fn handle_websocket_connect(
        &self,
        _url: &str,
        _headers: HashMap<String, String>,
    ) -> OpFuture<'_, Result<WebSocketConnection, String>> {
        Box::pin(async { Err("WebSocket not available".into()) })
    }

    /// Handle a log message
    ///
    /// Default: prints to stderr
    fn handle_log(&self, level: LogLevel, message: String) {
        eprintln!("[{:?}] {}", level, message);
    }

    /// Dispatch an operation to the appropriate handler
    ///
    /// This method has a default implementation that dispatches to the
    /// individual handler methods. Override only if you need custom dispatch logic.
    fn handle(&self, op: Operation) -> OpFuture<'_, OperationResult> {
        Box::pin(async move {
            match op {
                Operation::Fetch(request) => {
                    OperationResult::Http(self.handle_fetch(request).await)
                }
                Operation::BindingFetch { binding, request } => {
                    OperationResult::Http(self.handle_binding_fetch(&binding, request).await)
                }
                Operation::BindingStorage { binding, op } => {
                    OperationResult::Storage(self.handle_binding_storage(&binding, op).await)
                }
                Operation::BindingKv { binding, op } => {
                    OperationResult::Kv(self.handle_binding_kv(&binding, op).await)
                }
                Operation::BindingDatabase { binding, op } => {
                    OperationResult::Database(self.handle_binding_database(&binding, op).await)
                }
                Operation::BindingImages { binding, op } => {
                    OperationResult::Images(self.handle_binding_images(&binding, op).await)
                }
                Operation::BindingWorker { binding, request } => {
                    OperationResult::Http(self.handle_binding_worker(&binding, request).await)
                }
                Operation::Log { level, message } => {
                    self.handle_log(level, message);
                    OperationResult::Ack
                }

                Operation::WebSocketConnect { url, headers } => {
                    OperationResult::WebSocket(self.handle_websocket_connect(&url, headers).await)
                }
            }
        })
    }
}

/// Arc wrapper for OperationsHandler trait object
pub type OperationsHandle = Arc<dyn OperationsHandler>;

/// Default stub implementation using all default trait methods
///
/// Use this for testing or when no runner operations are needed.
/// All operations use the default stubs:
/// - Fetch: returns error
/// - BindingFetch: returns error
/// - Log: prints to stderr
pub struct DefaultOps;

impl OperationsHandler for DefaultOps {}

// Keep DirectOperations as an alias for backwards compatibility
#[doc(hidden)]
pub type DirectOperations = DefaultOps;

#[cfg(test)]
mod tests {
    use super::*;

    /// Deserialize then serialize again, to compare against the input JSON
    fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(json: &str) -> String {
        let value: T = serde_json::from_str(json).expect("deserialization failed");

        serde_json::to_string(&value).expect("serialization failed")
    }

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

    #[test]
    fn images_op_descriptor_round_trips() {
        let json = concat!(
            r#"{"transform":{"input":[1,2,3],"transforms":["#,
            r#"{"resize":{"width":800,"height":600,"fit":"scale-down"}},"#,
            r#"{"trim":{"top":10,"right":0,"bottom":0,"left":20}},"#,
            r#"{"rotate":{"degrees":90}}"#,
            r#"],"output":{"format":"webp","quality":80}}}"#
        );

        assert_eq!(round_trip::<ImagesOp>(json), json);
    }

    #[test]
    fn image_transform_accepts_a_single_resize_side() {
        let json = r#"{"resize":{"width":800,"fit":"cover"}}"#;

        let transform: ImageTransform = serde_json::from_str(json).unwrap();

        match &transform {
            ImageTransform::Resize { width, height, fit } => {
                assert_eq!(*width, Some(800));
                assert!(height.is_none());
                assert!(matches!(fit, ResizeFit::Cover));
            }
            other => panic!("expected Resize, got {:?}", other),
        }

        assert_eq!(serde_json::to_string(&transform).unwrap(), json);
    }

    #[test]
    fn image_format_accepts_a_mime_type() {
        let json = r#"{"format":"image/webp"}"#;

        let output: ImageOutput = serde_json::from_str(json).unwrap();

        assert!(matches!(output.format, ImageFormat::Webp));
        assert_eq!(
            serde_json::to_string(&output).unwrap(),
            r#"{"format":"webp"}"#
        );
    }

    #[test]
    fn image_formats_have_content_types() {
        assert_eq!(ImageFormat::Png.content_type(), "image/png");
        assert_eq!(ImageFormat::Jpeg.content_type(), "image/jpeg");
        assert_eq!(ImageFormat::Webp.content_type(), "image/webp");
        assert_eq!(ImageFormat::Avif.content_type(), "image/avif");
    }

    #[tokio::test]
    async fn images_binding_stub_names_the_binding() {
        let op = Operation::BindingImages {
            binding: "IMAGES".to_string(),
            op: ImagesOp::Transform {
                input: vec![1, 2, 3],
                transforms: vec![ImageTransform::Rotate { degrees: 90 }],
                output: ImageOutput {
                    format: ImageFormat::Png,
                    quality: None,
                },
            },
        };

        match DefaultOps.handle(op).await {
            OperationResult::Images(ImagesResult::Error(err)) => {
                assert_eq!(err, "Images binding 'IMAGES' not implemented");
            }
            other => panic!("expected an Images error, got {:?}", other),
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
