//! Result type of the invoke binding

/// What a call to a function of the host gives back
#[derive(Debug)]
pub enum InvokeResult {
    /// What the function returned, as JSON
    Value(serde_json::Value),
    /// The message the guest's promise rejects with
    Error(String),
}
