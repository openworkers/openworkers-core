use crate::{LogEvent, RuntimeLimits, Script, Task, TerminationReason};
use std::future::Future;

/// Type alias for log event sender
pub type LogSender = std::sync::mpsc::Sender<LogEvent>;

/// Common trait for all JavaScript runtime workers
///
/// Note: Futures are not required to be `Send` because JS runtimes
/// typically have thread-local contexts that cannot be shared across threads.
pub trait Worker: Sized {
    /// Create a new worker with the given script and options
    fn new(
        script: Script,
        log_tx: Option<LogSender>,
        limits: Option<RuntimeLimits>,
    ) -> impl Future<Output = Result<Self, TerminationReason>>;

    /// Execute a task
    ///
    /// Returns:
    /// - `Ok(())` if the JS handler executed successfully
    /// - `Err(TerminationReason)` if something went wrong
    fn exec(&mut self, task: Task) -> impl Future<Output = Result<(), TerminationReason>>;

    /// Abort the worker execution
    ///
    /// This should stop any running JS execution as soon as possible.
    /// After calling abort(), the worker should not be used again.
    fn abort(&mut self);
}
