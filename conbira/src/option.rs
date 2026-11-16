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

    /// Generate a self-signed certificate for the given DNS names or IP
    /// addresses.
    #[arg(long, num_args = 1.., conflicts_with = "generate_ed25519")]
    pub generate_tls: Option<Vec<String>>,

    /// Generate an Ed25519 key pair.
    #[arg(long)]
    pub generate_ed25519: bool,
}
