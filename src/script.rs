use std::collections::HashMap;

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
}

/// Script with code, environment variables, and bindings
#[derive(Debug, Clone)]
pub struct Script {
    pub code: String,
    pub env: Option<HashMap<String, String>>,
    /// Bindings available to the worker (names + types only, no credentials)
    pub bindings: Vec<BindingInfo>,
}

impl Script {
    /// Create a new script with code only
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            env: None,
            bindings: Vec::new(),
        }
    }

    /// Create a new script with code and environment variables
    pub fn with_env(code: impl Into<String>, env: HashMap<String, String>) -> Self {
        Self {
            code: code.into(),
            env: Some(env),
            bindings: Vec::new(),
        }
    }

    /// Create a new script with code, environment variables, and bindings
    pub fn with_bindings(
        code: impl Into<String>,
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
