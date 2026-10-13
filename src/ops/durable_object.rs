//! Durable Object ids, storage and WebSockets

use tokio::sync::oneshot;

use crate::{HttpResponse, WebSocketConnection, WebSocketId, WebSocketOutgoing, WebSocketUpgrade};

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
    /// The time of the alarm, in milliseconds since the Unix epoch
    GetAlarm,
    /// Sets the alarm to this time, or deletes it
    SetAlarm(Option<i64>),
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
    /// For `GetAlarm` and `SetAlarm`: the alarm after the op
    Alarm(Option<i64>),
    Error(String),
}

/// The answer of a Durable Object to a request. A 101 hands over a socket of
/// the object: the caller sends the socket of its client on `socket` once
/// it upgrades, or drops it.
#[derive(Debug)]
pub struct ObjectResponse {
    pub response: HttpResponse,
    pub socket: Option<oneshot::Sender<WebSocketConnection>>,
}

/// An operation on the WebSockets of the Durable Object the runtime serves.
/// The runner holds these sockets, so they outlive an instance of the object.
#[derive(Debug)]
pub enum ObjectSocketOp {
    /// The object takes `socket`, the server end of a pair it made. A
    /// hibernatable socket goes to the handlers of the class and does not
    /// keep the object in memory; another one goes to its event listeners.
    Accept {
        socket: WebSocketId,
        tags: Vec<String>,
        hibernatable: bool,
    },
    /// The response that carries `socket` is out: the socket of the client
    /// comes on `upgrade`
    Attach {
        socket: WebSocketId,
        upgrade: WebSocketUpgrade,
    },
    /// A frame to the client, or the close of the socket
    Send {
        socket: WebSocketId,
        message: WebSocketOutgoing,
    },
    /// The attachment of the socket, in the structured clone format of the
    /// runtime
    Attachment {
        socket: WebSocketId,
        value: Option<Vec<u8>>,
    },
}

/// A hibernatable socket of the object, as a new instance of it finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSocket {
    pub id: WebSocketId,
    pub tags: Vec<String>,
    pub attachment: Option<Vec<u8>>,
}
