//! Heiwa automation primitives.
//!
//! This crate owns local background task definitions, trigger events, queueing,
//! rate-limit checks, and durable execution evidence. It is intentionally local-
//! first: the user machine schedules and records work under `~/.heiwa/state`,
//! while provider/model dispatch remains owned by the Heiwa runtime/DREX layer.

pub mod executor;
pub mod scheduler;
pub mod storage;
pub mod types;

pub use executor::{AutomationExecutor, ExecutionOutcome, ExecutionRunner, QueueResult};
pub use scheduler::{AutomationScheduler, SchedulerEvent, SchedulerStatus};
pub use storage::AutomationStore;
pub use types::*;
