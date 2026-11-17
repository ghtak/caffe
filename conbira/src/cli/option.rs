use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub(crate) struct CliOptions {
    #[command(flatten)]
    pub common: conbira::option::Options,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Generate a self-signed certificate for the given DNS names or IP
    /// addresses.
    #[arg(long, num_args = 1.., conflicts_with = "generate_ed25519")]
    pub generate_tls: Option<Vec<String>>,

    /// Generate an Ed25519 key pair.
    #[arg(long)]
    pub generate_ed25519: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Run SQL and registered code migrations.
    Migration {
        /// Directories containing SQL migrations.
        #[arg(required = true, num_args = 1..)]
        directories: Vec<PathBuf>,
    },
}
