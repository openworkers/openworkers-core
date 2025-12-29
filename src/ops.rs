//! Operations for runner-controlled operations
//!
//! Enum-based approach for extensibility. All runtime→runner communication
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
//! - `handle_fetch` → returns error "Fetch not available"
//! - `handle_binding_fetch` → returns error "Binding not configured"
//! - `handle_log` → prints to stderr
//!
//! Runners only need to override the methods they want to implement.

use crate::{HttpRequest, HttpResponse, LogLevel};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Storage operation types for get/put/head/list/delete
#[derive(Debug, Clone)]
pub enum StorageOp {
    /// Get an object by key
    Get { key: String },
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
    /// Metadata from head operation
    Head { size: u64, etag: Option<String> },
    /// List of keys
    List { keys: Vec<String>, truncated: bool },
    /// Error message
    Error(String),
}

/// Database operation types for SQL queries
#[derive(Debug, Clone)]
pub enum DatabaseOp {
    /// Execute a SQL query
    Query {
        /// SQL statement
        sql: String,
        /// Query parameters as JSON array
        params: Vec<String>,
    },
}

/// Result from a database operation
#[derive(Debug)]
pub enum DatabaseResult {
    /// Rows as JSON array
    Rows(String),
    /// Error message
    Error(String),
}

/// KV operation types for get/put/delete/list
#[derive(Debug, Clone)]
pub enum KvOp {
    /// Get a value by key
    Get { key: String },
    /// Put a value (key + value as string, optional TTL in seconds)
    Put {
        key: String,
        value: String,
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
    Value(Option<String>),
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

    /// Worker binding (worker-to-worker calls)
    BindingWorker {
        /// Binding name (e.g., "MY_WORKER")
        binding: String,
        /// The request to send to the target worker
        request: HttpRequest,
    },

    /// Log message (fire-and-forget)
    Log { level: LogLevel, message: String },
}

/// Results for operations
pub enum OperationResult {
    /// HTTP response (used by Fetch and BindingFetch)
    Http(Result<HttpResponse, String>),

    /// Storage operation result
    Storage(StorageResult),

    /// KV operation result
    Kv(KvResult),

    /// Database operation result
    Database(DatabaseResult),

    /// Acknowledgement for fire-and-forget operations (Log, etc.)
    Ack,
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
                Operation::BindingWorker { binding, request } => {
                    OperationResult::Http(self.handle_binding_worker(&binding, request).await)
                }
                Operation::Log { level, message } => {
                    self.handle_log(level, message);
                    OperationResult::Ack
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
