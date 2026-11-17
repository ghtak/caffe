use clap::Parser;

#[derive(Debug, Parser)]
pub(crate) struct CliOptions {
    #[command(flatten)]
    pub common: conbira::option::Options,

    /// Generate a self-signed certificate for the given DNS names or IP addresses.
    #[arg(long, num_args = 1.., conflicts_with = "generate_ed25519")]
    pub generate_tls: Option<Vec<String>>,

    /// Generate an Ed25519 key pair.
    #[arg(long)]
    pub generate_ed25519: bool,
}
