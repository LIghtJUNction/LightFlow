use super::records::{
    BatchJobDefinition, BatchJobRecord, BatchJobStatus, BatchManifest, BatchRunSummary,
};
use crate::cli::{CliError, CliResult};
use serde::Serialize;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const RUNS_DIR: &str = ".lightflow/runs";

pub(super) fn read_job_definitions(
    path: &Path,
    default_workflow_id: Option<&str>,
) -> CliResult<Vec<BatchJobRecord>> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut jobs = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let definition: BatchJobDefinition = serde_json::from_str(&line)?;
        let workflow_id = definition
            .workflow_id
            .or_else(|| default_workflow_id.map(str::to_owned))
            .ok_or_else(|| {
                CliError::Usage(format!(
                    "job {} is missing workflow_id and no --workflow was provided",
                    index + 1
                ))
            })?;
        jobs.push(BatchJobRecord {
            id: definition
                .id
                .unwrap_or_else(|| format!("job-{}", index + 1)),
            workflow_id,
            inputs: definition.inputs,
            disabled_nodes: definition.disabled_nodes,
            enabled_nodes: definition.enabled_nodes,
            status: BatchJobStatus::Queued,
            attempts: 0,
            outputs: None,
            artifacts: Vec::new(),
            error: None,
            started_at_ms: None,
            completed_at_ms: None,
        });
    }
    Ok(jobs)
}

pub(super) fn read_jobs(run_dir: &Path) -> CliResult<Vec<BatchJobRecord>> {
    let file = fs::File::open(run_dir.join("jobs.jsonl"))?;
    let reader = BufReader::new(file);
    let mut jobs = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            jobs.push(serde_json::from_str(&line)?);
        }
    }
    Ok(jobs)
}

pub(super) fn write_jobs(run_dir: &Path, jobs: &[BatchJobRecord]) -> CliResult<()> {
    let mut file = fs::File::create(run_dir.join("jobs.jsonl"))?;
    for job in jobs {
        serde_json::to_writer(&mut file, job)?;
        writeln!(file)?;
    }
    Ok(())
}

pub(super) fn write_json_pretty(path: &Path, value: &impl Serialize) -> CliResult<()> {
    let mut file = fs::File::create(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    writeln!(file)?;
    Ok(())
}

pub(super) fn append_event(
    run_dir: &Path,
    event: &str,
    job_id: Option<&str>,
    data: Option<serde_json::Value>,
) -> CliResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(run_dir.join("events.jsonl"))?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "ts_ms": now_ms(),
            "event": event,
            "job_id": job_id,
            "data": data.unwrap_or(serde_json::Value::Null),
        }),
    )?;
    writeln!(file)?;
    Ok(())
}

pub(super) fn summary_json(
    run_dir: &Path,
    manifest: &BatchManifest,
    jobs: &[BatchJobRecord],
) -> serde_json::Value {
    let completed = jobs
        .iter()
        .filter(|job| job.status == BatchJobStatus::Completed)
        .count();
    let failed = jobs
        .iter()
        .filter(|job| job.status == BatchJobStatus::Failed)
        .count();
    let queued = jobs
        .iter()
        .filter(|job| job.status == BatchJobStatus::Queued)
        .count();
    serde_json::to_value(BatchRunSummary {
        run_id: manifest.run_id.clone(),
        run_dir: run_dir.display().to_string(),
        total: jobs.len(),
        completed,
        failed,
        queued,
        max_gpu_jobs: manifest.max_gpu_jobs,
        max_cpu_jobs: manifest.max_cpu_jobs,
        batch_size: manifest.batch_size,
        resource_policy: json!({
            "gpu_execution_concurrency": manifest.max_gpu_jobs,
            "cpu_preprocess_concurrency": manifest.max_cpu_jobs,
            "batch_size": manifest.batch_size,
            "reserve_mem": manifest.reserve_mem,
            "reserve_vram": manifest.reserve_vram,
            "max_load": manifest.max_load,
            "note": "This scheduler limits workflow execution concurrency and records resource intent; runtime-specific GPU batching and memory probes plug in behind the same run state.",
        }),
    })
    .expect("batch summary serializes")
}

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
