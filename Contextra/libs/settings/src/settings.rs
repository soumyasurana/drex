use crate::error::SettingsError;
use config::{Config, Environment, File};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;

const ENV_PREFIX: &str = "CONTEXTRA";
const ENV_SEPARATOR: &str = "__";

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub env: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RedisSettings {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct VectorStoreSettings {
    pub url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ProvidersSettings {
    pub provider: Option<String>,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TelemetrySettings {
    pub enabled: bool,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Settings {
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub vector_store: VectorStoreSettings,
    pub providers: ProvidersSettings,
    pub telemetry: TelemetrySettings,
}

impl Settings {
    pub fn load() -> Result<Self, SettingsError> {
        dotenvy::dotenv().ok();

        Self::check_env_key_collisions()?;

        let run_mode = env::var("CONTEXTRA_ENV").unwrap_or_else(|_| "development".into());

        let mut builder = Config::builder()
            .add_source(File::with_name("configs/default.toml").required(false))
            .add_source(File::with_name(&format!("configs/{}.toml", run_mode)).required(false))
            .add_source(File::with_name("configs/local.toml").required(false))
            .add_source(Environment::with_prefix(ENV_PREFIX).separator(ENV_SEPARATOR));

        if let Ok(redis_url) = env::var("REDIS_URL")
            && env::var("CONTEXTRA__REDIS__URL").is_err()
        {
            builder = builder.set_override("redis.url", redis_url)?;
        }
        if let Ok(database_url) = env::var("DATABASE_URL")
            && env::var("CONTEXTRA__DATABASE__URL").is_err()
        {
            builder = builder.set_override("database.url", database_url)?;
        }

        if let Ok(qdrant_url) = env::var("QDRANT_URL")
            && env::var("CONTEXTRA__VECTOR_STORE__URL").is_err()
        {
            builder = builder.set_override("vector_store.url", qdrant_url)?;
        }

        if let Ok(server_host) = env::var("SERVER_HOST")
            && env::var("CONTEXTRA__SERVER__HOST").is_err()
        {
            builder = builder.set_override("server.host", server_host)?;
        }

        if let Ok(server_port) = env::var("SERVER_PORT")
            && env::var("CONTEXTRA__SERVER__PORT").is_err()
        {
            builder = builder.set_override("server.port", server_port)?;
        }

        let config = builder.build()?;
        Ok(config.try_deserialize()?)
    }

    /*
    `config-rs`'s `Environment` source lowercases the prefix, the separator-split
    key, and every candidate env var before comparing/inserting. That means two
    case-variant spellings of the same logical key (e.g. `CONTEXTRA__SERVER__PORT`
    and `Contextra__Server__Port`) both match and collapse to the same internal
    key, with `std::env::vars()` iteration order (unspecified by std) deciding
    which value survives. We scan the real environment ourselves first and fail
    loudly on any such collision instead of silently picking one.
    Note: `Environment::with_prefix(..).separator(ENV_SEPARATOR)` also uses
    `ENV_SEPARATOR` as the prefix separator by default (config-rs's
    `prefix_separator` falls back to `separator` when unset), so the real
    prefix boundary is `{ENV_PREFIX}{ENV_SEPARATOR}`, not `{ENV_PREFIX}_`.
    */
    fn check_env_key_collisions() -> Result<(), SettingsError> {
        let prefix_lower = format!(
            "{}{}",
            ENV_PREFIX.to_lowercase(),
            ENV_SEPARATOR.to_lowercase()
        );
        let separator_lower = ENV_SEPARATOR.to_lowercase();

        // normalized_key -> list of raw env var names that map to it
        let mut seen: HashMap<String, Vec<String>> = HashMap::new();

        for (raw_key, _) in env::vars() {
            let lower = raw_key.to_lowercase();
            if let Some(stripped) = lower.strip_prefix(&prefix_lower) {
                let normalized = stripped.replace(&separator_lower, ".");
                seen.entry(normalized).or_default().push(raw_key);
            }
        }

        let collisions: Vec<String> = seen
            .into_iter()
            .filter(|(_, raw_keys)| raw_keys.len() > 1)
            .map(|(normalized, raw_keys)| {
                format!(
                    "'{}' set via ambiguous variants: {}",
                    normalized,
                    raw_keys.join(", ")
                )
            })
            .collect();

        if !collisions.is_empty() {
            return Err(SettingsError::AmbiguousEnvVars(collisions.join("; ")));
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_from_temp_toml_and_env() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::new()?;
        let toml_content = r#"
            [server]
            host = "127.0.0.1"
            port = 8080
            workers = 4
            env = "test"
            log_level = "info"

            [database]
            url = "postgres://user:pass@localhost/db"
            max_connections = 10

            [redis]
            url = "redis://localhost/"

            [vector_store]
            url = "http://localhost:8000"

            [providers]
            provider = "openai"
            openai_api_key = "sk-..."

            [telemetry]
            enabled = false
        "#;
        write!(temp_file, "{}", toml_content)?;

        let path = temp_file
            .path()
            .to_str()
            .ok_or("temporary file path is not valid UTF-8")?;

        // set_override exercises Settings deserialization/merging, but does NOT
        // exercise the Environment source's string-parsing or prefix/separator
        // logic — see test_env_source_parses_real_vars below for that coverage.
        let builder = Config::builder()
            .add_source(File::with_name(path).format(config::FileFormat::Toml))
            .set_override("server.port", 9090)?
            .set_override("database.max_connections", 20)?
            .set_override("providers.anthropic_api_key", "ant-...")?;

        let config: Settings = builder.build()?.try_deserialize()?;

        assert_eq!(config.server.port, 9090);
        assert_eq!(config.database.max_connections, 20);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(
            config.providers.anthropic_api_key,
            Some("ant-...".to_string())
        );
        assert_eq!(config.providers.provider, Some("openai".to_string()));
        assert_eq!(config.providers.gemini_api_key, None);

        Ok(())
    }

    #[test]
    fn test_env_source_parses_real_vars() -> Result<(), Box<dyn std::error::Error>> {
        temp_env::with_vars(
            [
                ("CONTEXTRA__SERVER__PORT", Some("9090")),
                ("CONTEXTRA__DATABASE__MAX_CONNECTIONS", Some("20")),
            ],
            || -> Result<(), Box<dyn std::error::Error>> {
                let builder = Config::builder()
                    .add_source(config::File::from_str(
                        r#"
                            [server]
                            host = "127.0.0.1"
                            port = 8080
                            workers = 4
                            env = "test"
                            log_level = "info"

                            [database]
                            url = "postgres://user:pass@localhost/db"
                            max_connections = 10

                            [redis]
                            url = "redis://localhost/"

                            [vector_store]
                            url = "http://localhost:8000"

                            [providers]

                            [telemetry]
                            enabled = false
                        "#,
                        config::FileFormat::Toml,
                    ))
                    .add_source(
                        Environment::with_prefix(ENV_PREFIX)
                            .separator(ENV_SEPARATOR)
                            .try_parsing(true),
                    );

                let config: Settings = builder.build()?.try_deserialize()?;

                assert_eq!(config.server.port, 9090);
                assert_eq!(config.database.max_connections, 20);
                assert_eq!(config.server.host, "127.0.0.1");

                Ok(())
            },
        )?;

        Ok(())
    }

    #[test]
    fn test_detects_case_variant_env_collision() {
        temp_env::with_vars(
            [
                ("CONTEXTRA__SERVER__PORT", Some("9090")),
                ("Contextra__Server__Port", Some("9091")),
            ],
            || {
                let result = Settings::check_env_key_collisions();
                assert!(
                    result.is_err(),
                    "expected an error due to ambiguous env var collision"
                );
                if let Err(e) = result {
                    assert!(e.to_string().contains("server.port"));
                }
            },
        );
    }

    #[test]
    fn test_no_false_positive_on_distinct_keys() {
        temp_env::with_vars(
            [
                ("CONTEXTRA__SERVER__PORT", Some("9090")),
                ("CONTEXTRA__DATABASE__MAX_CONNECTIONS", Some("20")),
            ],
            || {
                assert!(Settings::check_env_key_collisions().is_ok());
            },
        );
    }
}
