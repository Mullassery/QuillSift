//! `scrapeowl` command-line entry point.

#![forbid(unsafe_code)]

mod cli;
mod commands;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Fetch(args) => commands::fetch(args).await,
        Command::Extract(args) => commands::extract(args).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
