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

/// An operation on the storage of the Durable Object the runtime serves.
/// Values are bytes the runtime serialized; the host stores them as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectStorageOp {
    /// The values of `keys`, in the same order
    Get { keys: Vec<String> },
    /// Entries in key order, between `start` (included) and `end` (excluded)
    List {
        start: Option<String>,
        /// Starts after this key, instead of at `start`
        start_after: Option<String>,
        end: Option<String>,
        prefix: Option<String>,
        reverse: bool,
        limit: Option<u64>,
    },
    /// One atomic write: everything first if `delete_all`, then the deletes,
    /// then the puts
    Write {
        puts: Vec<(String, Vec<u8>)>,
        deletes: Vec<String>,
        delete_all: bool,
    },
}

/// The result of an `ObjectStorageOp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectStorageResult {
    /// For `Get`: one value per key, `None` for a missing key
    Values(Vec<Option<Vec<u8>>>),
    /// For `List`
    Entries(Vec<(String, Vec<u8>)>),
    /// For `Write`: how many of `deletes` existed
    Written {
        deleted: u64,
    },
    Error(String),
}
