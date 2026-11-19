use std::path::Path;
use std::sync::Arc;

use conbira::app::auth::user_create_local as create_local;
use conbira::app::db::Database;
use conbira::config::Config;
use conbira::error::ResultExt as _;
use conbira::{Error, Result};
use espr::validate::Validate as _;

use crate::{migration, option};

pub(crate) async fn run(options: option::CliOptions) -> Result<()> {
    let option::CliOptions {
        common,
        command,
        generate_tls: tls_names,
        generate_ed25519: ed25519_requested,
    } = options;
    if command.is_some() && (tls_names.is_some() || ed25519_requested) {
        return Err(Error::new(
            conbira::Code::InvalidConfiguration,
            "Commands cannot be combined with key generation",
        )
        .at(conbira::here!()));
    }
    if let Some(option::Command::UserCreateLocal {
        nickname,
        email,
        password,
        role,
    }) = &command
    {
        let config = Config::load(&common.config, common.env_file.as_deref())?;
        return user_create_local(&config, nickname, email, password, role).await;
    }
    if let Some(option::Command::Migration { directories }) = command {
        let config = Config::load(&common.config, common.env_file.as_deref())?;
        return migration::run(&config, &directories, code_migrations()).await;
    }
    match (tls_names, ed25519_requested) {
        (Some(names), _) => {
            let config = Config::load(&common.config, common.env_file.as_deref())?;
            generate_tls(&config, &names)
        }
        (None, true) => {
            let config = Config::load(&common.config, common.env_file.as_deref())?;
            generate_ed25519(&config)
        }
        (None, false) => {
            for example in [
                "Examples:",
                "  conbira-cli migration dir01 dir02     Run migrations",
                "  conbira-cli user-create-local --nickname NAME --email EMAIL --password PASSWORD [--role user,admin]",
                "  conbira-cli --generate-tls localhost  Generate a TLS certificate and private key",
                "  conbira-cli --generate-ed25519        Generate an Ed25519 key pair for JWT signing",
            ] {
                println!("{example}");
            }
            Ok(())
        }
    }
}

async fn user_create_local(
    config: &Config,
    nickname: &str,
    email: &str,
    password: &str,
    role: &[String],
) -> Result<()> {
    let request = create_local::Input {
        nickname: nickname.to_owned(),
        email: email.to_owned(),
        password: password.to_owned(),
    };
    request.validate()?;
    let db = Database::new(&config.database)
        .await
        .map_err_internal("Failed to initialize database", conbira::here!())?;
    let password_hasher = Arc::new(
        espr::hash::argon2::Hasher::new(&config.argon2)
            .map_err_internal("Failed to initialize password hasher", conbira::here!())?,
    );
    let password_service = conbira::app::auth::service::password_hash::PasswordHashService::new(
        password_hasher,
        &config.auth.password_hash,
    )?;
    let mut command: create_local::usecase::Command = request.into();
    command.role_codes = role.to_vec();
    command.status = conbira::app::auth::entity::UserStatus::Active;
    let user = create_local::usecase::execute(&db, &password_service, command).await?;
    let output = serde_json::to_string(&user)
        .map_err_internal("Failed to serialize created user", conbira::here!())?;
    println!("{output}");
    Ok(())
}

/// Register code migration implementations here; their `order()` determines
/// execution order.
fn code_migrations() -> Vec<Box<dyn migration::Migration>> {
    vec![Box::new(migration::sample_0002::InsertSamples)]
}

fn generate_tls(config: &Config, names: &[String]) -> Result<()> {
    let tls = config.http.tls.as_ref().ok_or_else(|| {
        Error::new(
            conbira::Code::InvalidConfiguration,
            "HTTP TLS configuration is required",
        )
        .at(conbira::here!())
    })?;
    create_parent_directories(&[&tls.cert_path, &tls.key_path])?;
    espr::tls::generate(tls, names)
        .map_err_internal("Failed to generate TLS certificate", conbira::here!())?;
    Ok(())
}

fn generate_ed25519(config: &Config) -> Result<()> {
    let keys = &config.jwt.ed25519;
    create_parent_directories(&[&keys.key_path, &keys.public_key_path])?;
    espr::jwt::ed25519::generate(keys)
        .map_err_internal("Failed to generate Ed25519 keys", conbira::here!())?;
    Ok(())
}

fn create_parent_directories(paths: &[&Path]) -> Result<()> {
    for path in paths {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err_internal("Failed to create parent directory", conbira::here!())?;
        }
    }
    Ok(())
}
