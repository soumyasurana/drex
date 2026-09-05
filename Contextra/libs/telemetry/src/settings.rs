use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySettings {
    pub service_name: String,
    pub log_level: String,
    pub otlp_endpoint: Option<String>,
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        Self {
            service_name: "unknown-service".into(),
            log_level: "info".into(),
            otlp_endpoint: None,
        }
    }
}
