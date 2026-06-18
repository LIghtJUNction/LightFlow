mod support;

use serde_json::Value;
use std::fs;
use support::*;

#[test]
fn batch_run_persists_jobs_and_resume_finishes_pending_work()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_project_specs(&root)?;
    let jobs_path = root.join("jobs.jsonl");
    fs::write(
        &jobs_path,
        r#"{"id":"one","inputs":{"in":"alpha"}}
{"id":"two","inputs":{"in":"beta"}}
"#,
    )?;

    let run = lfw(
        &root,
        [
            "batch",
            "run",
            jobs_path.to_str().unwrap(),
            "--workflow",
            "lightflow.child",
            "--run-id",
            "batch-test",
            "--max-gpu-jobs",
            "2",
            "--max-cpu-jobs",
            "auto",
            "--batch-size",
            "auto",
            "--reserve-mem",
            "1GB",
        ],
    )?;
    assert_eq!(run["run_id"], "batch-test");
    assert_eq!(run["total"], 2);
    assert_eq!(run["completed"], 2);
    assert_eq!(run["failed"], 0);
    assert_eq!(run["max_gpu_jobs"], 2);
    assert_eq!(run["resource_policy"]["reserve_mem"], "1GB");

    let run_dir = root.join(".lightflow/runs/batch-test");
    assert!(run_dir.join("manifest.json").exists());
    assert!(run_dir.join("input.jsonl").exists());
    assert!(run_dir.join("events.jsonl").exists());

    let mut jobs = read_jsonl(&run_dir.join("jobs.jsonl"))?;
    assert_eq!(jobs[0]["status"], "completed");
    assert_eq!(jobs[0]["outputs"]["out"], "alpha");
    assert_eq!(jobs[1]["status"], "completed");
    assert_eq!(jobs[1]["outputs"]["out"], "beta");

    jobs[1]["status"] = Value::String("queued".to_owned());
    jobs[1]["outputs"] = Value::Null;
    write_jsonl(&run_dir.join("jobs.jsonl"), &jobs)?;

    let resumed = lfw(
        &root,
        ["batch", "resume", "batch-test", "--max-gpu-jobs", "1"],
    )?;
    assert_eq!(resumed["completed"], 2);
    assert_eq!(resumed["failed"], 0);
    assert_eq!(resumed["max_gpu_jobs"], 1);

    let resumed_jobs = read_jsonl(&run_dir.join("jobs.jsonl"))?;
    assert_eq!(resumed_jobs[1]["status"], "completed");
    assert_eq!(resumed_jobs[1]["outputs"]["out"], "beta");
    let events = fs::read_to_string(run_dir.join("events.jsonl"))?;
    assert!(events.contains("batch_started"));
    assert!(events.contains("batch_resumed"));
    assert!(events.contains("job_completed"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn read_jsonl(path: &std::path::Path) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

fn write_jsonl(path: &std::path::Path, values: &[Value]) -> Result<(), Box<dyn std::error::Error>> {
    let mut output = String::new();
    for value in values {
        output.push_str(&serde_json::to_string(value)?);
        output.push('\n');
    }
    fs::write(path, output)?;
    Ok(())
}
