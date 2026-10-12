//! Durable Object ids

/// What the id of a Durable Object comes from. The runner makes the id, so
/// the guest never sees the key of the namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectIdSource {
    /// idFromName: one name always gives the same id
    Name(String),
    /// idFromString: an id that toString gave, which must be of the namespace
    Text(String),
    /// newUniqueId
    Unique,
}
