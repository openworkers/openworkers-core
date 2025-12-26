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

    /// Log message (fire-and-forget)
    Log { level: LogLevel, message: String },
}

/// Results for operations
pub enum OperationResult {
    /// HTTP response (used by Fetch and BindingFetch)
    Http(Result<HttpResponse, String>),

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
