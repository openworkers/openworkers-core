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
//! - `handle_binding_images` returns "Images binding '<name>' not implemented"
//! - `handle_binding_worker` returns "Worker binding '<name>' not implemented"
//! - `handle_websocket_connect` returns "WebSocket not available"
//! - `handle_log` prints to stderr
//!
//! Runners only need to override the methods they want to implement.

mod images;
mod sql;

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
}
