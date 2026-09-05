pub mod metrics;
pub mod settings;

pub use settings::TelemetrySettings;

use tracing_subscriber::fmt::layer;
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "otel")]
use opentelemetry::KeyValue;
#[cfg(feature = "otel")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "otel")]
use opentelemetry_sdk::{
    Resource,
    trace::{self, Sampler},
};

/// Initialize tracing with JSON formatting and optional OTLP exporting
pub fn init_telemetry(settings: &TelemetrySettings) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&settings.log_level));

    let fmt_layer = layer().json();

    let subscriber = Registry::default().with(env_filter).with(fmt_layer);

    #[cfg(feature = "otel")]
    {
        if let Some(endpoint) = &settings.otlp_endpoint {
            let resource = Resource::new(vec![KeyValue::new(
                "service.name",
                settings.service_name.clone(),
            )]);

            let exporter = match opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
                .build()
            {
                Ok(exporter) => exporter,
                Err(err) => {
                    eprintln!("Failed to initialize OTLP exporter: {err}");
                    subscriber.init();
                    return;
                }
            };

            #[allow(deprecated)]
            let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
                .with_config(
                    trace::Config::default()
                        .with_sampler(Sampler::AlwaysOn)
                        .with_resource(resource),
                )
                .build();

            use opentelemetry::trace::TracerProvider as _;
            let tracer = tracer_provider.tracer("telemetry");

            let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

            subscriber.with(telemetry_layer).init();
            return;
        }
    }

    subscriber.init();
}

/// Creates a tracing span for a request, suitable for #[tracing::instrument] parent context or manual usage
#[inline]
pub fn span_for_request(request_id: &str) -> tracing::Span {
    tracing::info_span!("request", request_id = request_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_init_no_panic() {
        let settings = TelemetrySettings {
            service_name: "test-service".into(),
            log_level: "info".into(),
            otlp_endpoint: None,
        };

        // This will verify that initializing telemetry does not panic.
        init_telemetry(&settings);
    }
}
