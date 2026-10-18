use clap::Parser;
use mythos_cli::{config::CliConfig, execute_command, Cli};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let mythos_dir = cli.mythos_dir.unwrap_or_else(CliConfig::default_mythos_dir);

    match execute_command(cli.command, &mythos_dir).await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}
