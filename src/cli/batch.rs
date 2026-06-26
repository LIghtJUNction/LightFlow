use super::{CliError, CliResult};
use crate::api::ApiService;
use std::path::PathBuf;
use std::thread;

mod runner;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum BatchCommand {
    Run(BatchRunOptions),
    Resume(BatchResumeOptions),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct BatchRunOptions {
    pub(super) jobs_path: PathBuf,
    pub(super) workflow_id: Option<String>,
    pub(super) run_id: Option<String>,
    pub(super) max_gpu_jobs: usize,
    pub(super) max_cpu_jobs: usize,
    pub(super) batch_size: usize,
    pub(super) retries: u32,
    pub(super) reserve_mem: Option<String>,
    pub(super) reserve_vram: Option<String>,
    pub(super) max_load: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct BatchResumeOptions {
    pub(super) run_id: String,
    pub(super) max_gpu_jobs: Option<usize>,
}

pub(super) fn parse_batch_options(args: &[String]) -> CliResult<BatchCommand> {
    let action = match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") | None => {
            return Err(CliError::Usage(batch_usage()));
        }
        Some(action) => action,
    };
    match action {
        "run" => parse_batch_run_options(args),
        "resume" => parse_batch_resume_options(args),
        _ => Err(CliError::Usage(format!(
            "batch action must be run|resume\n{}",
            batch_usage()
        ))),
    }
}

fn batch_usage() -> String {
    [
        "usage:",
        "  lfw batch run <jobs.jsonl> [--workflow <workflow_id>] [--run-id <id>] [--max-gpu-jobs <n|auto>] [--max-cpu-jobs <n|auto>] [--batch-size <n|auto>] [--retries <n>] [--reserve-mem <size>] [--reserve-vram <size>] [--max-load <n>]",
        "  lfw batch resume <run_id> [--max-gpu-jobs <n|auto>]",
        "",
        "Runs or resumes JSONL workflow job queues with local run records under .lightflow/runs/.",
        "Each JSONL job can provide id, workflow_id, inputs, disabled_nodes, and enabled_nodes.",
        "Use --workflow as the default workflow id when jobs omit workflow_id.",
        "Use --max-gpu-jobs, --max-cpu-jobs, --batch-size, --reserve-mem, --reserve-vram, and --max-load to document the local resource policy.",
    ]
    .join("\n")
}

pub(super) fn execute_batch(
    service: &ApiService,
    options: &BatchCommand,
) -> CliResult<serde_json::Value> {
    match options {
        BatchCommand::Run(options) => runner::run_batch(service, options),
        BatchCommand::Resume(options) => runner::resume_batch(service, options),
    }
}

fn parse_batch_run_options(args: &[String]) -> CliResult<BatchCommand> {
    if args
        .get(1)
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(batch_usage()));
    }
    let Some(jobs_path) = required_batch_positional_arg(args, 1).map(PathBuf::from) else {
        return Err(CliError::Usage(batch_usage()));
    };
    let mut workflow_id = None;
    let mut run_id = None;
    let mut max_gpu_jobs = 1;
    let mut max_cpu_jobs = default_cpu_jobs();
    let mut batch_size = 1;
    let mut retries = 0;
    let mut reserve_mem = None;
    let mut reserve_vram = None;
    let mut max_load = None;
    let mut index = 2;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--workflow" => workflow_id = Some(required_batch_flag_value(args, index)?.to_owned()),
            "--run-id" => run_id = Some(required_batch_flag_value(args, index)?.to_owned()),
            "--max-gpu-jobs" => {
                max_gpu_jobs = parse_auto_usize(required_batch_flag_value(args, index)?, 1, flag)?
            }
            "--max-cpu-jobs" => {
                max_cpu_jobs = parse_auto_usize(
                    required_batch_flag_value(args, index)?,
                    default_cpu_jobs(),
                    flag,
                )?
            }
            "--batch-size" => {
                batch_size = parse_auto_usize(required_batch_flag_value(args, index)?, 1, flag)?
            }
            "--retries" => retries = parse_u32(required_batch_flag_value(args, index)?, flag)?,
            "--reserve-mem" => {
                reserve_mem = Some(required_batch_flag_value(args, index)?.to_owned())
            }
            "--reserve-vram" => {
                reserve_vram = Some(required_batch_flag_value(args, index)?.to_owned())
            }
            "--max-load" => max_load = Some(required_batch_flag_value(args, index)?.to_owned()),
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for batch run: {flag}"
                )));
            }
        }
        index += 2;
    }
    Ok(BatchCommand::Run(BatchRunOptions {
        jobs_path,
        workflow_id,
        run_id,
        max_gpu_jobs,
        max_cpu_jobs,
        batch_size,
        retries,
        reserve_mem,
        reserve_vram,
        max_load,
    }))
}

fn parse_batch_resume_options(args: &[String]) -> CliResult<BatchCommand> {
    if args
        .get(1)
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(batch_usage()));
    }
    let Some(run_id) = required_batch_positional_arg(args, 1).map(str::to_owned) else {
        return Err(CliError::Usage(batch_usage()));
    };
    let mut max_gpu_jobs = None;
    let mut index = 2;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--max-gpu-jobs" => {
                max_gpu_jobs = Some(parse_auto_usize(
                    required_batch_flag_value(args, index)?,
                    1,
                    flag,
                )?)
            }
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for batch resume: {flag}"
                )));
            }
        }
        index += 2;
    }
    Ok(BatchCommand::Resume(BatchResumeOptions {
        run_id,
        max_gpu_jobs,
    }))
}

fn required_batch_flag_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(batch_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(batch_usage()));
    }
    Ok(value)
}

fn required_batch_positional_arg(args: &[String], index: usize) -> Option<&str> {
    let value = args.get(index).map(String::as_str)?;
    if value.starts_with('-') || value == "|" {
        return None;
    }
    Some(value)
}

fn parse_auto_usize(value: &str, auto: usize, flag: &str) -> CliResult<usize> {
    if value == "auto" {
        return Ok(auto.max(1));
    }
    let parsed = value
        .parse::<usize>()
        .map_err(|_| CliError::Usage(format!("{flag} must be a positive integer or auto")))?;
    if parsed == 0 {
        return Err(CliError::Usage(format!(
            "{flag} must be a positive integer or auto"
        )));
    }
    Ok(parsed)
}

fn parse_u32(value: &str, flag: &str) -> CliResult<u32> {
    value
        .parse::<u32>()
        .map_err(|_| CliError::Usage(format!("{flag} must be a non-negative integer")))
}

fn default_cpu_jobs() -> usize {
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        .saturating_div(2)
        .clamp(1, 8)
}
