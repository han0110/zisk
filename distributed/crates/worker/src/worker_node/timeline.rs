use std::collections::HashMap;

use prost::Message;
use zisk_coordinator_api::grpc::proto::{
    AsmExecution, ExecutorTime, ProofTiming, Span, TaskTiming,
};
use zisk_prover_backend::ZiskBackend;

use super::WorkerNodeGrpc;

impl<T: ZiskBackend + 'static> WorkerNodeGrpc<T> {
    /// Takes the proving timeline of the last task as an encoded `TaskTiming`.
    pub(super) fn task_timing(&self) -> Vec<u8> {
        let prover = self.worker.prover_arc();
        let records = prover.take_proof_records();
        let worker_end = chrono::Utc::now().timestamp_millis() as u64;
        let mut timing =
            TaskTiming { worker_start: self.worker_start, worker_end, ..Default::default() };
        // The spans are microseconds from the origin of the records. The start of each record in
        // Unix milliseconds rounds down, so each gives a lower bound of the origin.
        let Some(origin_us) = records.iter().map(|(_, r)| r.start_ms * 1000 - r.start_us).max()
        else {
            return timing.encode_to_vec();
        };

        if records.iter().any(|(_, r)| r.proof_type == proofman::RECORD_KIND_EXECUTE) {
            let time = prover.get_execution_info().map(|(_, time)| time).unwrap_or_default();
            timing.executor_time = Some(ExecutorTime {
                total_duration: time.total_duration,
                execution_duration: time.execution_duration,
                count_and_plan_duration: time.count_and_plan_duration,
                count_and_plan_mo_duration: time.count_and_plan_mo_duration,
                asm: time.asm_execution_duration.map(|a| AsmExecution { time: a.time, mhz: a.mhz }),
            });
        }

        let mut indices: HashMap<String, u32> = HashMap::new();
        let mut spans = |spans: Vec<proofman::Span>| -> Vec<Span> {
            spans
                .into_iter()
                .map(|span| Span {
                    name: *indices.entry(span.name).or_insert_with_key(|name| {
                        timing.names.push(name.clone());
                        timing.names.len() as u32 - 1
                    }),
                    start_us: origin_us + span.start_us,
                    end_us: origin_us + span.end_us,
                    depth: span.depth,
                })
                .collect()
        };
        timing.proof_timings = records
            .into_iter()
            .map(|(air_name, record)| ProofTiming {
                id: record.id as u32,
                proof_type: record.proof_type as u32,
                airgroup_id: record.airgroup_id as u32,
                air_name,
                start: record.start_ms,
                end: record.end_ms,
                breakdown_ms: Default::default(),
                spans: spans(record.spans),
            })
            .collect();
        timing.encode_to_vec()
    }
}
