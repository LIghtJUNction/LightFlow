use super::{BatchResumeOptions, BatchRunOptions};
mod records;
mod storage;

use crate::api::ApiService;
use crate::cli::{CliError, CliResult, validate_path_segment};
use crate::workflow::WorkflowExecutionOptions;
use records::{BatchJobRecord, BatchJobStatus, BatchManifest, JobOutcome};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::thread;
use storage::{
    RUNS_DIR, append_event, now_ms, read_job_definitions, read_jobs, summary_json, write_jobs,
    write_json_pretty,
};

pub(super) fn run_batch(
    service: &ApiService,
    options: &BatchRunOptions,
) -> CliResult<serde_json::Value> {
    let run_id = options
        .run_id
        .clone()
        .unwrap_or_else(|| format!("run-{}", now_ms()));
    validate_path_segment(&run_id, "run id")?;
    let run_dir = service.repo_root().join(RUNS_DIR).join(&run_id);
    if run_dir.exists() {
        return Err(CliError::Usage(format!(
            "batch run already exists: {run_id}"
        )));
    }
    fs::create_dir_all(&run_dir)?;

    let manifest = BatchManifest {
        run_id: run_id.clone(),
        workflow_id: options.workflow_id.clone(),
        max_gpu_jobs: options.max_gpu_jobs,
        max_cpu_jobs: options.max_cpu_jobs,
        batch_size: options.batch_size,
        retries: options.retries,
        reserve_mem: options.reserve_mem.clone(),
        reserve_vram: options.reserve_vram.clone(),
        max_load: options.max_load.clone(),
    };
    write_json_pretty(&run_dir.join("manifest.json"), &manifest)?;
    fs::copy(&options.jobs_path, run_dir.join("input.jsonl"))?;

    let mut jobs = read_job_definitions(&options.jobs_path, options.workflow_id.as_deref())?;
    write_jobs(&run_dir, &jobs)?;
    append_event(&run_dir, "batch_started", None, None)?;
    execute_pending_jobs(service, &run_dir, &manifest, &mut jobs)?;
    append_event(&run_dir, "batch_finished", None, None)?;

    Ok(summary_json(&run_dir, &manifest, &jobs))
}

pub(super) fn resume_batch(
    service: &ApiService,
    options: &BatchResumeOptions,
) -> CliResult<serde_json::Value> {
    validate_path_segment(&options.run_id, "run id")?;
    let run_dir = service.repo_root().join(RUNS_DIR).join(&options.run_id);
    let mut manifest: BatchManifest =
        serde_json::from_slice(&fs::read(run_dir.join("manifest.json"))?)?;
    if let Some(max_gpu_jobs) = options.max_gpu_jobs {
        manifest.max_gpu_jobs = max_gpu_jobs;
    }
    let mut jobs = read_jobs(&run_dir)?;
    append_event(&run_dir, "batch_resumed", None, None)?;
    execute_pending_jobs(service, &run_dir, &manifest, &mut jobs)?;
    append_event(&run_dir, "batch_finished", None, None)?;
    Ok(summary_json(&run_dir, &manifest, &jobs))
}

fn execute_pending_jobs(
    service: &ApiService,
    run_dir: &Path,
    manifest: &BatchManifest,
    jobs: &mut [BatchJobRecord],
) -> CliResult<()> {
    loop {
        let indexes = jobs
            .iter()
            .enumerate()
            .filter(|(_, job)| should_run(job, manifest.retries))
            .take(manifest.max_gpu_jobs)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if indexes.is_empty() {
            return Ok(());
        }

        for index in &indexes {
            let job = &mut jobs[*index];
            job.status = BatchJobStatus::Running;
            job.attempts += 1;
            job.started_at_ms = Some(now_ms());
            job.completed_at_ms = None;
            job.error = None;
            append_event(run_dir, "job_started", Some(&job.id), None)?;
        }
        write_jobs(run_dir, jobs)?;

        let mut handles = Vec::new();
        for index in indexes {
            let service = service.clone();
            let job = jobs[index].clone();
            handles.push(thread::spawn(move || execute_one_job(service, index, job)));
        }

        for handle in handles {
            let outcome = handle
                .join()
                .map_err(|_| CliError::Usage("batch worker panicked".to_owned()))?;
            let job = &mut jobs[outcome.index];
            match outcome.error {
                Some(error) => {
                    job.status = BatchJobStatus::Failed;
                    job.error = Some(error.clone());
                    job.outputs = None;
                    job.artifacts.clear();
                    job.completed_at_ms = Some(now_ms());
                    append_event(
                        run_dir,
                        "job_failed",
                        Some(&job.id),
                        Some(json!({ "error": error })),
                    )?;
                }
                None => {
                    job.status = BatchJobStatus::Completed;
                    job.outputs = outcome.outputs;
                    job.artifacts = outcome.artifacts;
                    job.error = None;
                    job.completed_at_ms = Some(now_ms());
                    append_event(run_dir, "job_completed", Some(&job.id), None)?;
                }
            }
        }
        write_jobs(run_dir, jobs)?;
    }
}

fn execute_one_job(service: ApiService, index: usize, job: BatchJobRecord) -> JobOutcome {
    let options = WorkflowExecutionOptions {
        inputs: job.inputs,
        disabled_nodes: job.disabled_nodes,
        enabled_nodes: job.enabled_nodes,
        patch: None,
    };
    match service.execute_workflow(&job.workflow_id, options) {
        Ok(execution) => JobOutcome {
            index,
            outputs: Some(execution.outputs),
            artifacts: execution.artifacts,
            error: None,
        },
        Err(error) => JobOutcome {
            index,
            outputs: None,
            artifacts: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

fn should_run(job: &BatchJobRecord, retries: u32) -> bool {
    match job.status {
        BatchJobStatus::Queued => true,
        BatchJobStatus::Failed => job.attempts <= retries,
        BatchJobStatus::Running => true,
        BatchJobStatus::Completed => false,
    }
}
