//! Operations a runtime delegates to the runner
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
//! - `handle_binding_d1` returns "D1 binding '<name>' not implemented"
//! - `handle_binding_images` returns "Images binding '<name>' not implemented"
//! - `handle_binding_worker` returns "Worker binding '<name>' not implemented"
//! - `handle_binding_durable_object` and `durable_object_id` return
//!   "Durable Object binding '<name>' not implemented"
//! - `handle_websocket_connect` returns "WebSocket not available"
//! - `handle_log` prints to stderr
//!
//! Runners only need to override the methods they want to implement.

mod d1;
mod durable_object;
mod images;
mod sql;

pub use d1::D1Error;
pub use d1::D1ErrorKind;
pub use d1::D1Meta;
pub use d1::D1Op;
pub use d1::D1Result;
pub use d1::D1Rows;
pub use d1::D1Statement;
pub use d1::D1StatementResult;
pub use durable_object::ObjectIdSource;
pub use durable_object::ObjectResponse;
pub use durable_object::ObjectSocket;
pub use durable_object::ObjectSocketOp;
pub use durable_object::ObjectStorageOp;
pub use durable_object::ObjectStorageResult;
pub use images::ImageFormat;
pub use images::ImageOutput;
pub use images::ImageTransform;
pub use images::ImagesOp;
pub use images::ImagesResult;
pub use images::ResizeFit;
pub use sql::DatabaseOp;
pub use sql::DatabaseResult;
pub use sql::SqlParam;
pub use sql::SqlPrimitive;

use crate::{HttpRequest, HttpResponse, LogLevel, WebSocketConnection};
use std::collections::HashMap;
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

    /// D1 operation (SQLite)
    BindingD1 {
        /// Binding name (e.g., "DB")
        binding: String,
        /// The operation to perform
        op: D1Op,
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

    /// A request to a Durable Object
    BindingDurableObject {
        /// Binding name (e.g., "COUNTER")
        binding: String,
        /// The id of the object, as durable_object_id made it
        id: String,
        request: HttpRequest,
    },

    /// Storage of the Durable Object the runtime serves
    DurableObjectStorage(ObjectStorageOp),

    /// WebSockets of the Durable Object the runtime serves
    DurableObjectSocket(ObjectSocketOp),

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

    /// D1 operation result
    D1(D1Result),

    /// Image operation result
    Images(ImagesResult),

    /// Durable Object storage result
    ObjectStorage(ObjectStorageResult),

    /// Acknowledgement for fire-and-forget operations (Log, etc.)
    Ack,

    /// WebSocket connection established (contains bidirectional channels)
    WebSocket(Result<WebSocketConnection, String>),

    /// A response that hands over a socket: the socket of the client goes
    /// on the sender once the caller upgrades (BindingDurableObject)
    Upgrade(
        HttpResponse,
        tokio::sync::oneshot::Sender<WebSocketConnection>,
    ),
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

    /// Handle a D1 operation
    ///
    /// Default: returns an `Unavailable` error
    fn handle_binding_d1(&self, binding: &str, _op: D1Op) -> OpFuture<'_, D1Result> {
        let message = format!("D1 binding '{}' not implemented", binding);

        Box::pin(async move {
            D1Result::Error(D1Error {
                kind: D1ErrorKind::Unavailable,
                message,
                statement: None,
            })
        })
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

    /// Send a request to a Durable Object of the namespace of `binding`
    ///
    /// Default: returns error "not implemented"
    fn handle_binding_durable_object(
        &self,
        binding: &str,
        _id: &str,
        _request: HttpRequest,
    ) -> OpFuture<'_, Result<ObjectResponse, String>> {
        let err = format!("Durable Object binding '{}' not implemented", binding);
        Box::pin(async move { Err(err) })
    }

    /// Make the id of a Durable Object of the namespace of `binding`. The
    /// guest calls this synchronously, so it must not block.
    ///
    /// Default: returns error "not implemented"
    fn durable_object_id(&self, binding: &str, _source: ObjectIdSource) -> Result<String, String> {
        Err(format!(
            "Durable Object binding '{}' not implemented",
            binding
        ))
    }

    /// Run an operation on the storage of the Durable Object these
    /// operations serve
    ///
    /// Default: returns error "not implemented"
    fn handle_durable_object_storage(
        &self,
        _op: ObjectStorageOp,
    ) -> OpFuture<'_, ObjectStorageResult> {
        Box::pin(async {
            ObjectStorageResult::Error("Durable Object storage not implemented".into())
        })
    }

    /// Run an operation on the WebSockets of the Durable Object these
    /// operations serve. It must not block.
    ///
    /// Default: drops the op, which closes a socket it carries
    fn handle_durable_object_socket(&self, _op: ObjectSocketOp) {}

    /// The hibernatable sockets of the Durable Object these operations
    /// serve. The guest calls this synchronously, so it must not block.
    ///
    /// Default: none
    fn durable_object_sockets(&self) -> Vec<ObjectSocket> {
        Vec::new()
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
                Operation::BindingD1 { binding, op } => {
                    OperationResult::D1(self.handle_binding_d1(&binding, op).await)
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

                Operation::BindingDurableObject {
                    binding,
                    id,
                    request,
                } => match self
                    .handle_binding_durable_object(&binding, &id, request)
                    .await
                {
                    Ok(ObjectResponse {
                        response,
                        socket: Some(socket),
                    }) => OperationResult::Upgrade(response, socket),
                    Ok(answer) => OperationResult::Http(Ok(answer.response)),
                    Err(error) => OperationResult::Http(Err(error)),
                },
                Operation::DurableObjectStorage(op) => {
                    OperationResult::ObjectStorage(self.handle_durable_object_storage(op).await)
                }
                Operation::DurableObjectSocket(op) => {
                    self.handle_durable_object_socket(op);
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
mod test_support {
    /// Deserialize then serialize again, to compare against the input JSON
    pub fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(json: &str) -> String {
        let value: T = serde_json::from_str(json).expect("deserialization failed");

        serde_json::to_string(&value).expect("serialization failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpMethod, RequestBody};

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

    #[tokio::test]
    async fn durable_object_binding_stubs_name_the_binding() {
        let op = Operation::BindingDurableObject {
            binding: "COUNTER".to_string(),
            id: "00".to_string(),
            request: HttpRequest {
                method: HttpMethod::Get,
                url: "http://object/".to_string(),
                headers: HashMap::new(),
                body: RequestBody::None,
            },
        };
        let message = "Durable Object binding 'COUNTER' not implemented";

        match DefaultOps.handle(op).await {
            OperationResult::Http(Err(err)) => assert_eq!(err, message),
            other => panic!("expected an Http error, got {:?}", other),
        }

        let id = DefaultOps.durable_object_id("COUNTER", ObjectIdSource::Unique);
        assert_eq!(id, Err(message.to_string()));
    }

    #[tokio::test]
    async fn d1_binding_stub_names_the_binding() {
        let op = Operation::BindingD1 {
            binding: "DB".to_string(),
            op: D1Op::Exec {
                sql: "SELECT 1".to_string(),
            },
        };

        match DefaultOps.handle(op).await {
            OperationResult::D1(D1Result::Error(err)) => {
                assert_eq!(err.kind, D1ErrorKind::Unavailable);
                assert_eq!(err.message, "D1 binding 'DB' not implemented");
            }
            other => panic!("expected a D1 error, got {:?}", other),
        }
    }
}
