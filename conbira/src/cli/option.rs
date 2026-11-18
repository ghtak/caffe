use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
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

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Run SQL and registered code migrations.
    Migration {
        /// Directories containing SQL migrations.
        #[arg(required = true, num_args = 1..)]
        directories: Vec<PathBuf>,
    },
    /// Create a local user.
    UserCreateLocal {
        #[arg(long)]
        nickname: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_user_command_requires_all_inputs() {
        let args = [
            "conbira-cli",
            "user-create-local",
            "--nickname",
            "name",
            "--email",
            "user@example.com",
        ];
        // 비밀번호가 빠진 명령은 파싱 단계에서 거부한다.
        assert!(CliOptions::try_parse_from(args).is_err());
        let options =
            CliOptions::try_parse_from(args.into_iter().chain(["--password", "secret"])).unwrap();
        assert!(
            matches!(options.command, Some(Command::UserCreateLocal { nickname, email, password }) if nickname == "name" && email == "user@example.com" && password == "secret")
        );
    }
}
