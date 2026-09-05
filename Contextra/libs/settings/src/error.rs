use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("failed to load configuration: {0}")]
    Load(#[from] config::ConfigError),

    #[error("ambiguous environment variable casing detected: {0}")]
    AmbiguousEnvVars(String),
}
