use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ENV_PREFIX: &str = "DREX";
const ENV_SEPARATOR: &str = "__";
const DEFAULT_CONFIG_DIR: &str = "crates/drex-config/configs";

/// The main application configuration.
///
/// Configuration is loaded from multiple sources in order of precedence (lowest to highest):
/// 1. `crates/config/default.toml` — checked into git, no secrets
/// 2. `crates/config/{DREX_ENV}.toml` — optional per-environment overrides
/// 3. `.env` — gitignored, loaded via `dotenvy`
/// 4. Real environment variables prefixed with `DREX__`, using double underscores for nesting
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub app_name: String,
    pub environment: String,
    pub log_level: String,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub ollama: OllamaConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedisConfig {
    pub url: String,
}

/// Ollama backend configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL for the Ollama server.
    pub base_url: String,
    /// Default model to use.
    pub default_model: String,
    /// Request timeout in seconds.
    pub timeout_seconds: u64,
}

impl OllamaConfig {
    /// Get the full generate API URL.
    pub fn generate_url(&self) -> String {
        format!("{}/api/generate", self.base_url.trim_end_matches('/'))
    }

    /// Get the chat API URL.
    pub fn chat_url(&self) -> String {
        format!("{}/api/chat", self.base_url.trim_end_matches('/'))
    }
}

/// Errors that can occur during configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigErrorKind {
    #[error("configuration file error: {0}")]
    FileError(#[from] ConfigError),

    #[error("configuration validation failed: {0}")]
    Validation(String),

    #[error("environment variable collision: {0}")]
    EnvCollision(String),
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_name: "drex".to_string(),
            environment: "development".to_string(),
            log_level: "info".to_string(),
            database: DatabaseConfig {
                url: "postgres://postgres:postgres@localhost:5432/drex".to_string(),
                max_connections: 10,
            },
            redis: RedisConfig {
                url: "redis://localhost:6379".to_string(),
            },
            ollama: OllamaConfig {
                base_url: "http://localhost:11434".to_string(),
                default_model: "gemma3:4b".to_string(),
                timeout_seconds: 120,
            },
        }
    }
}

impl AppConfig {
    /// Load configuration from all sources.
    ///
    /// Precedence (lowest to highest):
    /// 1. `crates/drex-config/configs/default.toml`
    /// 2. `crates/drex-config/configs/{DREX_ENV}.toml` (optional)
    /// 3. `.env` file (via dotenvy)
    /// 4. Environment variables with `DREX__` prefix
    pub fn load() -> Result<Self, ConfigErrorKind> {
        // Load .env file if present
        dotenvy::dotenv().ok();

        // Check for case-variant env key collisions
        Self::check_env_key_collisions()?;

        // Determine environment
        let env = std::env::var("DREX_ENV").unwrap_or_else(|_| "development".to_string());

        let mut builder = Config::builder()
            // Lowest precedence: default.toml
            .add_source(File::with_name(&format!("{}/default.toml", DEFAULT_CONFIG_DIR)).required(true));

        // Optional per-environment config (e.g., production.toml)
        let env_config_path = format!("{}/{}.toml", DEFAULT_CONFIG_DIR, env);
        builder = builder.add_source(File::with_name(&env_config_path).required(false));

        // Highest precedence: environment variables with DREX__ prefix
        builder = builder.add_source(
            Environment::with_prefix(ENV_PREFIX)
                .separator(ENV_SEPARATOR)
                .try_parsing(true),
        );

        let config = builder.build()?;
        let app_config: AppConfig = config.try_deserialize()?;

        // Validate the configuration
        app_config.validate()?;

        Ok(app_config)
    }

    /// Load configuration with a custom config directory (for testing).
    #[cfg(test)]
    pub fn load_from_dir(config_dir: &str) -> Result<Self, ConfigErrorKind> {
        dotenvy::dotenv().ok();
        Self::check_env_key_collisions()?;

        let env = std::env::var("DREX_ENV").unwrap_or_else(|_| "development".to_string());

        let mut builder = Config::builder()
            .add_source(File::with_name(&format!("{}/default.toml", config_dir)).required(true));

        let env_config_path = format!("{}/{}.toml", config_dir, env);
        builder = builder.add_source(File::with_name(&env_config_path).required(false));

        builder = builder.add_source(
            Environment::with_prefix(ENV_PREFIX)
                .separator(ENV_SEPARATOR)
                .try_parsing(true),
        );

        let config = builder.build()?;
        let app_config: AppConfig = config.try_deserialize()?;
        app_config.validate()?;

        Ok(app_config)
    }

    /// Validate the configuration values.
    fn validate(&self) -> Result<(), ConfigErrorKind> {
        if self.app_name.is_empty() {
            return Err(ConfigErrorKind::Validation(String::from(
                "app_name cannot be empty",
            )));
        }

        if self.database.url.is_empty() {
            return Err(ConfigErrorKind::Validation(String::from(
                "database.url cannot be empty",
            )));
        }

        if self.redis.url.is_empty() {
            return Err(ConfigErrorKind::Validation(String::from(
                "redis.url cannot be empty",
            )));
        }

        if self.database.max_connections == 0 {
            return Err(ConfigErrorKind::Validation(String::from(
                "database.max_connections must be greater than 0",
            )));
        }

        if self.ollama.base_url.is_empty() {
            return Err(ConfigErrorKind::Validation(String::from(
                "ollama.base_url cannot be empty",
            )));
        }

        if self.ollama.default_model.is_empty() {
            return Err(ConfigErrorKind::Validation(String::from(
                "ollama.default_model cannot be empty",
            )));
        }

        if self.ollama.timeout_seconds == 0 {
            return Err(ConfigErrorKind::Validation(String::from(
                "ollama.timeout_seconds must be greater than 0",
            )));
        }

        Ok(())
    }

    /// Check for ambiguous environment variable collisions.
    ///
    /// `config-rs`'s `Environment` source lowercases the prefix, the separator-split
    /// key, and every candidate env var before comparing. Two case-variant spellings
    /// of the same logical key both match and collapse to the same internal key,
    /// with iteration order deciding which value survives.
    fn check_env_key_collisions() -> Result<(), ConfigErrorKind> {
        let prefix_lower = format!(
            "{}{}",
            ENV_PREFIX.to_lowercase(),
            ENV_SEPARATOR.to_lowercase()
        );
        let separator_lower = ENV_SEPARATOR.to_lowercase();

        // normalized_key -> list of raw env var names that map to it
        let mut seen: HashMap<String, Vec<String>> = HashMap::new();

        for (raw_key, _) in std::env::vars() {
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
            return Err(ConfigErrorKind::EnvCollision(collisions.join("; ")));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_config(dir: &TempDir, name: &str, content: &str) {
        let path = dir.path().join(format!("{}.toml", name));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_load_default_config() {
        // Clear any DREX__ env vars to ensure isolation
        temp_env::with_vars::<String, String, _, ()>(
            [],
            || {
                let temp_dir = TempDir::new().unwrap();
                let default_toml = r#"
app_name = "drex-test"
environment = "development"
log_level = "debug"

[database]
url = "postgres://test@localhost/drex"
max_connections = 5

[redis]
url = "redis://localhost:6379"

[ollama]
base_url = "http://localhost:11434"
default_model = "gemma3:4b"
timeout_seconds = 120
"#;
                create_test_config(&temp_dir, "default", default_toml);

                let config = AppConfig::load_from_dir(temp_dir.path().to_str().unwrap()).unwrap();

                assert_eq!(config.app_name, "drex-test");
                assert_eq!(config.environment, "development");
                assert_eq!(config.log_level, "debug");
                assert_eq!(config.database.url, "postgres://test@localhost/drex");
                assert_eq!(config.database.max_connections, 5);
                assert_eq!(config.redis.url, "redis://localhost:6379");
                assert_eq!(config.ollama.base_url, "http://localhost:11434");
                assert_eq!(config.ollama.default_model, "gemma3:4b");
                assert_eq!(config.ollama.timeout_seconds, 120);
            },
        );
    }

    #[test]
    fn test_validation_rejects_empty_app_name() {
        let mut config = AppConfig::default();
        config.app_name = String::new();

        let err = config.validate().unwrap_err();
        assert!(matches!(err, ConfigErrorKind::Validation(_)));
        assert!(err.to_string().contains("app_name"));
    }

    #[test]
    fn test_validation_rejects_empty_database_url() {
        let mut config = AppConfig::default();
        config.database.url = String::new();

        let err = config.validate().unwrap_err();
        assert!(matches!(err, ConfigErrorKind::Validation(_)));
        assert!(err.to_string().contains("database.url"));
    }

    #[test]
    fn test_validation_rejects_zero_max_connections() {
        let mut config = AppConfig::default();
        config.database.max_connections = 0;

        let err = config.validate().unwrap_err();
        assert!(matches!(err, ConfigErrorKind::Validation(_)));
        assert!(err.to_string().contains("max_connections"));
    }

    #[test]
    fn test_detects_env_var_collision() {
        temp_env::with_vars(
            [
                ("DREX__DATABASE__URL", Some("postgres://test")),
                ("drex__database__url", Some("postgres://other")),
            ],
            || {
                let result = AppConfig::check_env_key_collisions();
                assert!(result.is_err());
                assert!(result.unwrap_err().to_string().contains("database.url"));
            },
        );
    }

    #[test]
    fn test_env_var_override() {
        let temp_dir = TempDir::new().unwrap();
        let default_toml = r#"
app_name = "drex-default"
environment = "development"
log_level = "info"

[database]
url = "postgres://default@localhost/drex"
max_connections = 10

[redis]
url = "redis://localhost:6379"

[ollama]
base_url = "http://localhost:11434"
default_model = "gemma3:4b"
timeout_seconds = 120
"#;
        create_test_config(&temp_dir, "default", default_toml);

        temp_env::with_var("DREX__DATABASE__URL", Some("postgres://override@localhost"), || {
            let config = AppConfig::load_from_dir(temp_dir.path().to_str().unwrap()).unwrap();
            assert_eq!(config.database.url, "postgres://override@localhost");
            assert_eq!(config.app_name, "drex-default"); // Unchanged
        });
    }

    #[test]
    fn test_env_var_parses_nested_value() {
        let temp_dir = TempDir::new().unwrap();
        let default_toml = r#"
app_name = "drex"
environment = "development"
log_level = "info"

[database]
url = "postgres://localhost/drex"
max_connections = 10

[redis]
url = "redis://localhost:6379"

[ollama]
base_url = "http://localhost:11434"
default_model = "gemma3:4b"
timeout_seconds = 120
"#;
        create_test_config(&temp_dir, "default", default_toml);

        temp_env::with_var("DREX__DATABASE__MAX_CONNECTIONS", Some("25"), || {
            let config = AppConfig::load_from_dir(temp_dir.path().to_str().unwrap()).unwrap();
            assert_eq!(config.database.max_connections, 25);
        });
    }
}
