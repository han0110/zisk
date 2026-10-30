use chrono::Utc;
use zisk_cluster_api::execute_task_response::ResultData;
use zisk_cluster_api::ExecuteTaskResponse;
use zisk_cluster_common::{JobId, JobPhase, TaskRecord, WorkerId};

use crate::Coordinator;

impl Coordinator {
    /// Keeps the timing of a task result with the times the coordinator sent the task and got
    /// the result. A failed task and a task of no proving phase keep nothing.
    pub(crate) async fn record_task(&self, worker_id: &WorkerId, response: &ExecuteTaskResponse) {
        let phase = match response.result_data {
            Some(ResultData::Challenges(_)) => JobPhase::Contributions,
            Some(ResultData::Proofs(_)) => JobPhase::Prove,
            Some(ResultData::FinalProof(_) | ResultData::PartialAggProofs(_)) => JobPhase::Recurse,
            _ => return,
        };
        let job_id = JobId::from(response.job_id.clone());
        let Some(job) = self.jobs.read().await.get(&job_id).cloned() else {
            return;
        };
        let mut job = job.write().await;
        if !response.success || job.state().is_resolved() || !job.workers.contains(worker_id) {
            return;
        }
        // A task of the contributions or the prove phase starts with its phase.
        let coordinator_start = match job.timeline.starts.remove(worker_id) {
            Some(start) => start,
            None => job.phase_start_time(&phase).map_or(0, |t| t.timestamp_millis() as u64),
        };
        job.timeline.tasks.push(TaskRecord {
            worker_id: worker_id.clone(),
            phase,
            coordinator_start,
            coordinator_end: Utc::now().timestamp_millis() as u64,
            timing: response.timing.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use zisk_cluster_api::execute_task_response::ResultData;
    use zisk_cluster_api::ExecuteTaskResponse;
    use zisk_cluster_common::{JobPhase, JobState, WorkerId};

    use crate::coordinator::exec_stats_from_job;
    use crate::coordinator::tests::setup_coordinator_with_job;

    #[tokio::test]
    async fn test_records_task_timing() {
        let (coordinator, workers, job_id) =
            setup_coordinator_with_job(2, JobPhase::Contributions, |_| {}).await;
        let (w0, w1, outsider) = (&workers[0].0, &workers[1].0, &WorkerId::from("w9".to_string()));
        let entry = coordinator.jobs.read().await.get(&job_id).cloned().unwrap();
        entry.write().await.change_state(JobState::Running(JobPhase::Prove));
        entry.write().await.timeline.starts.insert(w1.clone(), 7);

        let cases = [
            (w0, true, ResultData::Challenges(Default::default()), Some(JobPhase::Contributions)),
            (w0, true, ResultData::Proofs(Default::default()), Some(JobPhase::Prove)),
            (w1, true, ResultData::FinalProof(Default::default()), Some(JobPhase::Recurse)),
            (w0, false, ResultData::Proofs(Default::default()), None),
            (w0, true, ResultData::Execution(Default::default()), None),
            (outsider, true, ResultData::Proofs(Default::default()), None),
        ];
        for (index, (worker_id, success, result_data, _)) in cases.iter().enumerate() {
            let response = ExecuteTaskResponse {
                job_id: job_id.as_string(),
                worker_id: worker_id.as_string(),
                success: *success,
                result_data: Some(result_data.clone()),
                timing: vec![index as u8],
                ..Default::default()
            };
            coordinator.record_task(worker_id, &response).await;
        }

        let job = entry.read().await;
        let start =
            |phase: &JobPhase| job.phase_start_time(phase).unwrap().timestamp_millis() as u64;
        let expected: Vec<_> = (cases.iter().enumerate())
            .filter_map(|(index, (worker_id, _, _, phase))| {
                let phase = phase.clone()?;
                let coordinator_start = if *worker_id == w1 { 7 } else { start(&phase) };
                Some(((*worker_id).clone(), phase, coordinator_start, vec![index as u8]))
            })
            .collect();
        let stats = exec_stats_from_job(&job);
        let recorded: Vec<_> = (stats.tasks.iter())
            .map(|t| (t.worker_id.clone(), t.phase.clone(), t.coordinator_start, t.timing.clone()))
            .collect();
        assert_eq!(recorded, expected);
        assert!(stats.tasks.iter().all(|t| t.coordinator_start <= t.coordinator_end));
        assert_eq!(stats.proof_start, job.phase_start_time(&JobPhase::Contributions));
    }
}
