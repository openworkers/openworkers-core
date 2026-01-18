use crate::{HttpRequest, HttpResponse, ResponseSender};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::oneshot;

// ============================================================================
// Fetch Event (HTTP request handling)
// ============================================================================

/// Fetch event initialization data
#[derive(Debug)]
pub struct FetchInit {
    pub req: HttpRequest,
    pub res_tx: ResponseSender,
}

impl FetchInit {
    pub fn new(req: HttpRequest, res_tx: ResponseSender) -> Self {
        Self { req, res_tx }
    }
}

#[cfg(feature = "deno")]
impl deno_core::Resource for FetchInit {
    fn close(self: std::rc::Rc<Self>) {
        // Resource is being closed, nothing to clean up
    }
}

// ============================================================================
// Task Event (unified task execution model)
// ============================================================================

/// Source of task trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskSource {
    /// Triggered by a cron schedule
    Schedule {
        /// Unix timestamp (ms) when the task was scheduled to run
        time: u64,
    },

    /// Triggered by another task (chaining)
    Chained {
        /// ID of the parent task
        parent_task_id: String,
        /// ID of the worker that created this task
        parent_worker_id: String,
        /// Name of the parent worker (optional, for debug/logs)
        parent_worker_name: Option<String>,
    },

    /// Triggered from a fetch handler
    Worker {
        /// ID of the worker that created this task
        worker_id: String,
        /// Worker name (optional)
        worker_name: Option<String>,
    },

    /// Triggered manually (API, CLI, Dashboard)
    Invoke {
        /// Origin of the invocation ("cli", "dashboard", "api", etc.)
        origin: Option<String>,
    },
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Success or failure
    pub success: bool,
    /// JSON data returned (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    /// Error message if failed (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TaskResult {
    /// Create a successful result with optional data
    pub fn ok(data: impl Into<Option<JsonValue>>) -> Self {
        Self {
            success: true,
            data: data.into(),
            error: None,
        }
    }

    /// Create a failed result with an error message
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }

    /// Create a successful result with no data
    pub fn success() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
        }
    }
}

impl Default for TaskResult {
    fn default() -> Self {
        Self::success()
    }
}

/// Task event initialization data
#[derive(Debug)]
pub struct TaskInit {
    /// Unique identifier for this execution
    pub task_id: String,
    /// Optional JSON payload
    pub payload: Option<JsonValue>,
    /// Source of the trigger
    pub source: Option<TaskSource>,
    /// Attempt number (starts at 1)
    pub attempt: u32,
    /// Channel to send the result back
    pub res_tx: oneshot::Sender<TaskResult>,
}

impl TaskInit {
    /// Create a new TaskInit with all fields
    pub fn new(
        task_id: String,
        payload: Option<JsonValue>,
        source: Option<TaskSource>,
        attempt: u32,
        res_tx: oneshot::Sender<TaskResult>,
    ) -> Self {
        Self {
            task_id,
            payload,
            source,
            attempt,
            res_tx,
        }
    }

    /// Create a simple TaskInit with just an ID (for testing/CLI)
    pub fn simple(task_id: String, res_tx: oneshot::Sender<TaskResult>) -> Self {
        Self {
            task_id,
            payload: None,
            source: Some(TaskSource::Invoke { origin: None }),
            attempt: 1,
            res_tx,
        }
    }
}

#[cfg(feature = "deno")]
impl deno_core::Resource for TaskInit {
    fn close(self: std::rc::Rc<Self>) {
        // Resource is being closed, nothing to clean up
    }
}

// ============================================================================
// Event Enum (main entry point)
// ============================================================================

/// Event type discriminator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Fetch,
    Task,
}

/// Event to be executed by a Worker
pub enum Event {
    /// HTTP request handler (fetch event)
    Fetch(Option<FetchInit>),
    /// Generic task (unified model for scheduled, chained, invoked)
    Task(Option<TaskInit>),
}

impl Event {
    pub fn event_type(&self) -> EventType {
        match self {
            Event::Fetch(_) => EventType::Fetch,
            Event::Task(_) => EventType::Task,
        }
    }

    /// Create a fetch event
    pub fn fetch(req: HttpRequest) -> (Self, oneshot::Receiver<HttpResponse>) {
        let (tx, rx) = oneshot::channel();
        (Event::Fetch(Some(FetchInit::new(req, tx))), rx)
    }

    /// Create a task event with full control
    pub fn task(
        task_id: String,
        payload: Option<JsonValue>,
        source: Option<TaskSource>,
        attempt: u32,
    ) -> (Self, oneshot::Receiver<TaskResult>) {
        let (tx, rx) = oneshot::channel();
        (
            Event::Task(Some(TaskInit::new(task_id, payload, source, attempt, tx))),
            rx,
        )
    }

    /// Create a task event from a schedule (cron)
    pub fn from_schedule(task_id: String, time: u64) -> (Self, oneshot::Receiver<TaskResult>) {
        Self::task(task_id, None, Some(TaskSource::Schedule { time }), 1)
    }

    /// Create a task event for manual invocation (CLI, API, etc.)
    pub fn invoke(
        task_id: String,
        payload: Option<JsonValue>,
        origin: Option<String>,
    ) -> (Self, oneshot::Receiver<TaskResult>) {
        Self::task(task_id, payload, Some(TaskSource::Invoke { origin }), 1)
    }
}
