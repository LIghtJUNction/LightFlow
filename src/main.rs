fn main() {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to start async runtime: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = runtime.block_on(lightflow::cli::run_from_env()) {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}
