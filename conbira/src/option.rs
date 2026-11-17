use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct Options {
    /// Path to the configuration file (relative to the current directory).
    #[arg(long, default_value = "config.toml")]
    pub config: PathBuf,

    /// Load environment variables from this file (defaults to .env if present).
    #[arg(long)]
    pub env_file: Option<PathBuf>,
}
