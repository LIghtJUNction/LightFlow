use super::{BatchResumeOptions, BatchRunOptions};
use crate::api::ApiService;
use crate::cli::{CliError, CliResult, validate_path_segment};
use crate::workflow::{WorkflowArtifact, WorkflowExecutionOptions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const RUNS_DIR: &str = ".lightflow/runs";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct BatchManifest {
    run_id: String,
    workflow_id: Option<String>,
    max_gpu_jobs: usize,
    max_cpu_jobs: usize,
    batch_size: usize,
    retries: u32,
    reserve_mem: Option<String>,
    reserve_vram: Option<String>,
    max_load: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct BatchJobDefinition {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    workflow_id: Option<String>,
    #[serde(default)]
    inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    disabled_nodes: Vec<String>,
    #[serde(default)]
    enabled_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BatchJobRecord {
    id: String,
    workflow_id: String,
    #[serde(default)]
    inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    disabled_nodes: Vec<String>,
    #[serde(default)]
    enabled_nodes: Vec<String>,
    status: BatchJobStatus,
    attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outputs: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artifacts: Vec<WorkflowArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    started_at_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    completed_at_ms: Option<u128>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BatchJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct BatchRunSummary {
    run_id: String,
    run_dir: String,
    total: usize,
    completed: usize,
    failed: usize,
    queued: usize,
    max_gpu_jobs: usize,
    max_cpu_jobs: usize,
    batch_size: usize,
    resource_policy: serde_json::Value,
}

#[derive(Debug, Clone)]
struct JobOutcome {
    index: usize,
    outputs: Option<serde_json::Map<String, serde_json::Value>>,
    artifacts: Vec<WorkflowArtifact>,
    error: Option<String>,
}

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

fn read_job_definitions(
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

fn read_jobs(run_dir: &Path) -> CliResult<Vec<BatchJobRecord>> {
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

fn write_jobs(run_dir: &Path, jobs: &[BatchJobRecord]) -> CliResult<()> {
    let mut file = fs::File::create(run_dir.join("jobs.jsonl"))?;
    for job in jobs {
        serde_json::to_writer(&mut file, job)?;
        writeln!(file)?;
    }
    Ok(())
}

fn write_json_pretty(path: &Path, value: &impl Serialize) -> CliResult<()> {
    let mut file = fs::File::create(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    writeln!(file)?;
    Ok(())
}

fn append_event(
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

fn summary_json(
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

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
