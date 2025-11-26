use std::collections::HashMap;

/// Script with code and environment variables
#[derive(Debug, Clone)]
pub struct Script {
    pub code: String,
    pub env: Option<HashMap<String, String>>,
}

impl Script {
    /// Create a new script with code only
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            env: None,
        }
    }

    /// Create a new script with code and environment variables
    pub fn with_env(code: impl Into<String>, env: HashMap<String, String>) -> Self {
        Self {
            code: code.into(),
            env: Some(env),
        }
    }
}
