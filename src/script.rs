use std::collections::HashMap;

/// Worker code - JavaScript source, WebAssembly binary, or pre-compiled snapshot
#[derive(Debug, Clone)]
pub enum WorkerCode {
    /// JavaScript/TypeScript source code (for V8, Deno, QuickJS, etc.)
    JavaScript(String),
    /// WebAssembly binary (for Wasmtime runtime)
    #[cfg(feature = "wasm")]
    WebAssembly(Vec<u8>),
    /// Pre-compiled snapshot (platform-specific, determined by the runtime)
    Snapshot(Vec<u8>),
}

impl WorkerCode {
    /// Create JavaScript code
    pub fn js(code: impl Into<String>) -> Self {
        Self::JavaScript(code.into())
    }

    /// Create WebAssembly code from bytes
    #[cfg(feature = "wasm")]
    pub fn wasm(bytes: Vec<u8>) -> Self {
        Self::WebAssembly(bytes)
    }

    /// Create Snapshot from bytes
    pub fn snapshot(bytes: Vec<u8>) -> Self {
        Self::Snapshot(bytes)
    }

    /// Check if this is JavaScript
    pub fn is_js(&self) -> bool {
        matches!(self, Self::JavaScript(_))
    }

    /// Check if this is WebAssembly
    #[cfg(feature = "wasm")]
    pub fn is_wasm(&self) -> bool {
        matches!(self, Self::WebAssembly(_))
    }

    /// Check if this is a Snapshot
    pub fn is_snapshot(&self) -> bool {
        matches!(self, Self::Snapshot(_))
    }

    /// Get JavaScript source if this is JS code
    pub fn as_js(&self) -> Option<&str> {
        match self {
            Self::JavaScript(s) => Some(s),
            _ => None,
        }
    }

    /// Get WebAssembly bytes if this is WASM code
    #[cfg(feature = "wasm")]
    pub fn as_wasm(&self) -> Option<&[u8]> {
        match self {
            Self::WebAssembly(b) => Some(b),
            _ => None,
        }
    }

    /// Get Snapshot bytes if this is a Snapshot
    pub fn as_snapshot(&self) -> Option<&[u8]> {
        match self {
            Self::Snapshot(b) => Some(b),
            _ => None,
        }
    }
}

// Convenience: String -> JavaScript
impl From<String> for WorkerCode {
    fn from(s: String) -> Self {
        Self::JavaScript(s)
    }
}

impl From<&str> for WorkerCode {
    fn from(s: &str) -> Self {
        Self::JavaScript(s.to_string())
    }
}

// Convenience: Vec<u8> -> WebAssembly
#[cfg(feature = "wasm")]
impl From<Vec<u8>> for WorkerCode {
    fn from(b: Vec<u8>) -> Self {
        Self::WebAssembly(b)
    }
}

/// Type of binding (resource type)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingType {
    /// Static assets (S3/R2 read-only)
    Assets,
    /// Object storage (S3/R2 read-write)
    Storage,
    /// Key-value store
    Kv,
    /// Database (PostgreSQL via postgate)
    Database,
    /// Worker-to-worker binding
    Worker,
}

/// Binding info passed to the runtime (name + type, no credentials)
#[derive(Debug, Clone)]
pub struct BindingInfo {
    /// Binding name as it appears in JS (e.g., "ASSETS", "MY_BUCKET")
    pub name: String,
    /// Type of binding
    pub binding_type: BindingType,
}

impl BindingInfo {
    pub fn new(name: impl Into<String>, binding_type: BindingType) -> Self {
        Self {
            name: name.into(),
            binding_type,
        }
    }

    pub fn assets(name: impl Into<String>) -> Self {
        Self::new(name, BindingType::Assets)
    }

    pub fn storage(name: impl Into<String>) -> Self {
        Self::new(name, BindingType::Storage)
    }

    pub fn kv(name: impl Into<String>) -> Self {
        Self::new(name, BindingType::Kv)
    }

    pub fn database(name: impl Into<String>) -> Self {
        Self::new(name, BindingType::Database)
    }

    pub fn worker(name: impl Into<String>) -> Self {
        Self::new(name, BindingType::Worker)
    }
}

/// Script with code, environment variables, and bindings
#[derive(Debug, Clone)]
pub struct Script {
    pub code: WorkerCode,
    pub env: Option<HashMap<String, String>>,
    /// Bindings available to the worker (names + types only, no credentials)
    pub bindings: Vec<BindingInfo>,
}

impl Script {
    /// Create a new script with code only
    pub fn new(code: impl Into<WorkerCode>) -> Self {
        Self {
            code: code.into(),
            env: None,
            bindings: Vec::new(),
        }
    }

    /// Create a new script with code and environment variables
    pub fn with_env(code: impl Into<WorkerCode>, env: HashMap<String, String>) -> Self {
        Self {
            code: code.into(),
            env: Some(env),
            bindings: Vec::new(),
        }
    }

    /// Create a new script with code, environment variables, and bindings
    pub fn with_bindings(
        code: impl Into<WorkerCode>,
        env: Option<HashMap<String, String>>,
        bindings: Vec<BindingInfo>,
    ) -> Self {
        Self {
            code: code.into(),
            env,
            bindings,
        }
    }

    /// Add a binding to the script
    pub fn add_binding(mut self, binding: BindingInfo) -> Self {
        self.bindings.push(binding);
        self
    }
}
