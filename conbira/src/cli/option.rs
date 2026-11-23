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
        /// Assign existing roles by comma-separated codes (defaults to user).
        #[arg(long, value_delimiter = ',', default_value = "user")]
        role: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    mod contract {
        use super::*;

        const LOCAL_USER_ARGS_WITHOUT_PASSWORD: [&str; 6] = [
            "conbira-cli",
            "user-create-local",
            "--nickname",
            "name",
            "--email",
            "user@example.com",
        ];

        #[test]
        fn local_user_command_rejects_a_missing_password() {
            let result = CliOptions::try_parse_from(LOCAL_USER_ARGS_WITHOUT_PASSWORD);
            // 비밀번호가 빠진 명령은 파싱 단계에서 거부한다.
            assert!(result.is_err());
        }

        #[test]
        fn local_user_command_preserves_inputs_and_defaults_to_user_role() {
            let options = CliOptions::try_parse_from(
                LOCAL_USER_ARGS_WITHOUT_PASSWORD
                    .into_iter()
                    .chain(["--password", "secret"]),
            )
            .unwrap();
            // 사용자 입력은 유지되고 역할을 생략하면 user를 사용한다.
            assert!(
                matches!(options.command, Some(Command::UserCreateLocal { nickname, email, password, role }) if nickname == "name" && email == "user@example.com" && password == "secret" && role == ["user"])
            );
        }

        #[test]
        fn local_user_command_accepts_comma_separated_roles() {
            let options =
                CliOptions::try_parse_from(LOCAL_USER_ARGS_WITHOUT_PASSWORD.into_iter().chain([
                    "--password",
                    "secret",
                    "--role",
                    "user,admin",
                ]))
                .unwrap();
            // 쉼표로 구분한 두 역할을 순서대로 읽는다.
            assert!(
                matches!(options.command, Some(Command::UserCreateLocal { role, .. }) if role == ["user", "admin"])
            );
        }
    }
}
