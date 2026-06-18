fn main() -> lightflow::runner::RunnerResult<()> {
    lightflow::runner::run_workflow_from_env(lightflow_b::define())
}
