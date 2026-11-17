// Espresso con birra
#[path = "mod.rs"]
mod cli;
mod option;

use clap::Parser;

#[tokio::main]
async fn main() -> conbira::Result<()> {
    cli::run(option::CliOptions::parse()).await
}
