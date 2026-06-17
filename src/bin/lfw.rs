#[tokio::main]
async fn main() {
    if let Err(error) = lightflow::cli::run_from_env().await {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}
