use serde::{Deserialize, Serialize};

/// Reason why a worker execution failed
///
/// This is returned as `Err(TerminationReason)` from `Worker::exec()`.
/// A successful execution returns `Ok(())`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminationReason {
    // === Resource limits ===
    /// Worker exceeded CPU time limit
    CpuTimeLimit,

    /// Worker exceeded wall-clock time limit
    WallClockTimeout,

    /// Worker exceeded memory limit (heap or ArrayBuffer)
    MemoryLimit,

    // === JS errors (userland) ===
    /// Worker threw an uncaught exception (syntax error, throw, etc.)
    Exception(String),

    // === Runtime errors (Rust-side) ===
    /// Worker failed to initialize
    InitializationError(String),

    /// Worker was terminated by external signal (e.g., shutdown)
    Terminated,

    /// Worker was aborted via abort() call
    Aborted,

    /// Unexpected error
    Other(String),
}

impl TerminationReason {
    /// Returns true if this represents a resource limit violation
    pub fn is_limit_exceeded(&self) -> bool {
        matches!(
            self,
            Self::CpuTimeLimit | Self::WallClockTimeout | Self::MemoryLimit
        )
    }

    /// Returns true if this is a JS userland error
    pub fn is_js_error(&self) -> bool {
        matches!(self, Self::Exception(_))
    }

    /// Returns true if this is a runtime (Rust-side) error
    pub fn is_runtime_error(&self) -> bool {
        matches!(
            self,
            Self::InitializationError(_) | Self::Terminated | Self::Aborted | Self::Other(_)
        )
    }

    /// Get a human-readable description
    pub fn description(&self) -> &str {
        match self {
            Self::CpuTimeLimit => "Worker exceeded CPU time limit",
            Self::WallClockTimeout => "Worker exceeded wall-clock time limit",
            Self::MemoryLimit => "Worker exceeded memory limit",
            Self::Exception(msg) => msg,
            Self::InitializationError(msg) => msg,
            Self::Terminated => "Worker was terminated",
            Self::Aborted => "Worker was aborted",
            Self::Other(msg) => msg,
        }
    }

    /// Get an appropriate HTTP status code for this termination reason
    pub fn http_status(&self) -> u16 {
        match self {
            Self::CpuTimeLimit | Self::MemoryLimit => 429, // Too Many Requests
            Self::WallClockTimeout => 504,                 // Gateway Timeout
            Self::Exception(_) | Self::InitializationError(_) | Self::Other(_) => 500, // Internal Server Error
            Self::Terminated | Self::Aborted => 503, // Service Unavailable
        }
    }
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl std::error::Error for TerminationReason {}
