//! Proving timeline of a job, one record per worker task.

use std::collections::HashMap;

use crate::{JobPhase, WorkerId};

/// The tasks of a job with their timing.
#[derive(Debug, Default)]
pub struct JobTimeline {
    /// When each worker got its outstanding task, in Unix milliseconds.
    pub starts: HashMap<WorkerId, u64>,
    /// The tasks that ended.
    pub tasks: Vec<TaskRecord>,
}

/// One worker task.
#[derive(Debug, Clone)]
pub struct TaskRecord {
    /// The worker that ran the task.
    pub worker_id: WorkerId,
    /// The phase of the task.
    pub phase: JobPhase,
    /// When the coordinator sent the task, in Unix milliseconds.
    pub coordinator_start: u64,
    /// When the coordinator got the result, in Unix milliseconds.
    pub coordinator_end: u64,
    /// The encoded `TaskTiming` of the worker.
    pub timing: Vec<u8>,
}
