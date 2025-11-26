use crate::{HttpRequest, HttpResponse};
use tokio::sync::oneshot;

/// Fetch event initialization data
#[derive(Debug)]
pub struct FetchInit {
    pub req: HttpRequest,
    pub res_tx: oneshot::Sender<HttpResponse>,
}

impl FetchInit {
    pub fn new(req: HttpRequest, res_tx: oneshot::Sender<HttpResponse>) -> Self {
        Self { req, res_tx }
    }
}

#[cfg(feature = "deno")]
impl deno_core::Resource for FetchInit {
    fn close(self: std::rc::Rc<Self>) {
        // Resource is being closed, nothing to clean up
    }
}

/// Scheduled event initialization data
#[derive(Debug)]
pub struct ScheduledInit {
    pub time: u64,
    pub res_tx: oneshot::Sender<()>,
}

impl ScheduledInit {
    /// Create ScheduledInit (res_tx first, time second for compatibility)
    pub fn new(res_tx: oneshot::Sender<()>, time: u64) -> Self {
        Self { time, res_tx }
    }
}

#[cfg(feature = "deno")]
impl deno_core::Resource for ScheduledInit {
    fn close(self: std::rc::Rc<Self>) {
        // Resource is being closed, nothing to clean up
    }
}

/// Task type discriminator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    Fetch,
    Scheduled,
}

/// Task to be executed by a Worker
pub enum Task {
    Fetch(Option<FetchInit>),
    Scheduled(Option<ScheduledInit>),
}

impl Task {
    pub fn task_type(&self) -> TaskType {
        match self {
            Task::Fetch(_) => TaskType::Fetch,
            Task::Scheduled(_) => TaskType::Scheduled,
        }
    }

    /// Create a fetch task
    pub fn fetch(req: HttpRequest) -> (Self, oneshot::Receiver<HttpResponse>) {
        let (tx, rx) = oneshot::channel();
        (Task::Fetch(Some(FetchInit::new(req, tx))), rx)
    }

    /// Create a scheduled task
    pub fn scheduled(time: u64) -> (Self, oneshot::Receiver<()>) {
        let (tx, rx) = oneshot::channel();
        (Task::Scheduled(Some(ScheduledInit::new(tx, time))), rx)
    }
}
