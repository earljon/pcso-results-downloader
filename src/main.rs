mod browser;
mod cli;
mod dates;
mod error;
mod pipeline;
mod s3;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let default_filter = "info,chromiumoxide=error";
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter)),
        )
        .with_target(false)
        .init();

    let args = cli::Args::parse();
    if let Err(e) = pipeline::run(args).await {
        tracing::error!("{}", e);
        std::process::exit(1);
    }
}
