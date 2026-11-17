use std::path::Path;

use config::{Config, Environment, File, FileFormat, Source};
use serde::de::DeserializeOwned;

use crate::{Error, Result};

pub fn load<T>(filename: impl AsRef<Path>, env_prefix: Option<&str>) -> Result<T>
where
    T: DeserializeOwned,
{
    load_source(File::from(filename.as_ref()).required(true), env_prefix)
}

pub fn load_str<T>(contents: &str, format: FileFormat, env_prefix: Option<&str>) -> Result<T>
where
    T: DeserializeOwned,
{
    load_source(File::from_str(contents, format), env_prefix)
}

fn load_source<T>(
    source: impl Source + Send + Sync + 'static,
    env_prefix: Option<&str>,
) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut builder = Config::builder().add_source(source);
    if let Some(prefix) = env_prefix {
        if prefix.is_empty() {
            return Err(Error::InvalidParameter(
                "environment prefix must not be empty".into(),
            ));
        }
        builder = builder.add_source(
            Environment::with_prefix(prefix)
                .prefix_separator("_")
                .separator("__"),
        );
    }
    let settings = builder.build()?;

    Ok(settings.try_deserialize()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod sample {
        use serde::Deserialize;

        #[derive(Debug, Deserialize)]
        pub struct Config {
            pub id: i32,
            pub name: String,
        }
    }

    #[derive(Debug, serde::Deserialize)]
    struct TestConfig {
        sample: sample::Config,
    }

    #[test]
    fn loads_test_config_from_toml() {
        let path = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("tests/fixtures/config.toml");
        let config: TestConfig = load(path, None).unwrap();

        assert_eq!(config.sample.id, 42);
        assert_eq!(config.sample.name, "example");
    }

    #[test]
    fn loads_test_config_from_string() {
        let config: TestConfig = load_str(
            include_str!("../tests/fixtures/config.toml"),
            FileFormat::Toml,
            None,
        )
        .unwrap();

        assert_eq!(config.sample.id, 42);
        assert_eq!(config.sample.name, "example");
    }

    #[test]
    fn rejects_empty_environment_prefix() {
        let result = load::<TestConfig>("unused.toml", Some(""));

        assert!(matches!(result, Err(Error::InvalidParameter(_))));
    }

    #[test]
    fn requires_config_file() {
        let path = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("tests/fixtures/missing.toml");
        let result = load::<TestConfig>(path, None);

        assert!(matches!(result, Err(Error::ConfigError(_))));
    }

    #[test]
    fn environment_overrides_toml() {
        const PREFIX: &str = "ESPR_TEST_CONFIG_OVERRIDE";
        const MARKER: &str = "ESPR_CONFIG_OVERRIDE_CHILD";

        if std::env::var_os(MARKER).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "config::tests::environment_overrides_toml"])
                .env(MARKER, "1")
                .env("ESPR_TEST_CONFIG_OVERRIDE_SAMPLE__NAME", "overridden")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "stdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let config: TestConfig = load_str(
            include_str!("../tests/fixtures/config.toml"),
            FileFormat::Toml,
            Some(PREFIX),
        )
        .unwrap();

        assert_eq!(config.sample.id, 42);
        assert_eq!(config.sample.name, "overridden");
    }
}
