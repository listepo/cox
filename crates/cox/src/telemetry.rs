//! Process-wide structured logging and optional OTLP export. This belongs at
//! the binary boundary: core emits `tracing` spans but never opens files or
//! sockets, while this module owns exporter lifecycle and shutdown flushing.

use std::path::Path;

use cox_protocol::Config;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Keeps asynchronous file logging and OTLP providers alive until shutdown.
pub struct TelemetryGuard {
    _file: WorkerGuard,
    #[cfg(feature = "otel")]
    tracer: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
    #[cfg(feature = "otel")]
    logger: Option<opentelemetry_sdk::logs::SdkLoggerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        {
            if let Some(provider) = self.logger.take() {
                let _ = provider.shutdown();
            }
            if let Some(provider) = self.tracer.take() {
                let _ = provider.shutdown();
            }
        }
    }
}

/// Installs local JSON logging and, when configured, OTLP trace/log layers.
pub fn init(config: &Config, home: &Path) -> anyhow::Result<TelemetryGuard> {
    let log_dir = home.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let appender = tracing_appender::rolling::daily(log_dir, "cox.log");
    let (writer, file_guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_new(&config.core.log_level)?;
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(true)
        .with_ansi(false)
        .with_writer(writer)
        .with_filter(filter.clone());

    if !config.telemetry.otel {
        tracing_subscriber::registry().with(file_layer).try_init()?;
        return Ok(TelemetryGuard {
            _file: file_guard,
            #[cfg(feature = "otel")]
            tracer: None,
            #[cfg(feature = "otel")]
            logger: None,
        });
    }

    #[cfg(not(feature = "otel"))]
    return Err(anyhow::anyhow!(
        "telemetry.otel=true requires a cox build with the `otel` feature"
    ));

    #[cfg(feature = "otel")]
    {
        init_otel(config, file_layer, file_guard)
    }
}

#[cfg(feature = "otel")]
fn init_otel<S>(
    config: &Config,
    file_layer: S,
    file_guard: WorkerGuard,
) -> anyhow::Result<TelemetryGuard>
where
    S: tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig as _;

    let mut span_builder = opentelemetry_otlp::SpanExporter::builder().with_http();
    let mut log_builder = opentelemetry_otlp::LogExporter::builder().with_http();
    if !config.telemetry.endpoint.trim().is_empty() {
        span_builder =
            span_builder.with_endpoint(signal_endpoint(&config.telemetry.endpoint, "v1/traces"));
        log_builder =
            log_builder.with_endpoint(signal_endpoint(&config.telemetry.endpoint, "v1/logs"));
    }
    let resource = resource();
    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(span_builder.build()?)
        .with_resource(resource.clone())
        .build();
    let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_batch_exporter(log_builder.build()?)
        .with_resource(resource)
        .build();
    let trace_layer = tracing_opentelemetry::layer().with_tracer(tracer_provider.tracer("cox"));
    let log_filter = EnvFilter::try_new(format!(
        "{},hyper=off,h2=off,reqwest=off,opentelemetry=off",
        config.core.log_level
    ))?;
    let log_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider)
            .with_filter(log_filter);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(trace_layer)
        .with(log_layer)
        .try_init()?;
    Ok(TelemetryGuard {
        _file: file_guard,
        tracer: Some(tracer_provider),
        logger: Some(logger_provider),
    })
}

#[cfg(feature = "otel")]
fn resource() -> opentelemetry_sdk::Resource {
    // A code-set service name overrides detectors. Apply the fallback first,
    // then resource attributes, then the explicit OTEL_SERVICE_NAME override.
    let builder = opentelemetry_sdk::Resource::builder()
        .with_service_name("cox")
        .with_detector(Box::new(
            opentelemetry_sdk::resource::EnvResourceDetector::new(),
        ));
    match std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|s| !s.is_empty())
    {
        Some(name) => builder.with_service_name(name).build(),
        None => builder.build(),
    }
}

#[cfg(feature = "otel")]
fn signal_endpoint(base: &str, signal: &str) -> String {
    format!("{}/{signal}", base.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "otel")]
    use std::io::{Read as _, Write as _};

    #[cfg(feature = "otel")]
    #[test]
    fn telemetry_resource_service_name_precedence() {
        for (attributes, service, expected) in [
            ("deployment.environment=test", "", "cox"),
            (
                "service.name=from-attributes,deployment.environment=test",
                "",
                "from-attributes",
            ),
            (
                "service.name=from-attributes,deployment.environment=test",
                "from-service",
                "from-service",
            ),
        ] {
            let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
                .args([
                    "--exact",
                    "telemetry::tests::telemetry_resource_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("OTEL_RESOURCE_ATTRIBUTES", attributes)
                .env("OTEL_SERVICE_NAME", service)
                .env("COX_TEST_SERVICE_NAME", expected)
                .output()
                .expect("resource child");
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[cfg(feature = "otel")]
    #[test]
    #[ignore = "isolated resource environment; run by precedence test"]
    fn telemetry_resource_child() {
        let resource = super::resource();
        assert_eq!(
            resource
                .get(&opentelemetry::Key::new("service.name"))
                .map(|v| v.to_string()),
            Some(std::env::var("COX_TEST_SERVICE_NAME").expect("expected name")),
        );
        assert_eq!(
            resource
                .get(&opentelemetry::Key::new("deployment.environment"))
                .map(|v| v.to_string()),
            Some("test".into()),
        );
    }

    #[cfg(feature = "otel")]
    #[test]
    fn telemetry_signal_endpoints_are_otlp_http_paths() {
        assert_eq!(
            super::signal_endpoint("http://localhost:4318/", "v1/traces"),
            "http://localhost:4318/v1/traces"
        );
        assert_eq!(
            super::signal_endpoint("https://ingest.example/otel", "v1/logs"),
            "https://ingest.example/otel/v1/logs"
        );
    }

    #[cfg(feature = "otel")]
    #[test]
    fn telemetry_otlp_collector_receives_span_and_log() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("collector bind");
        let address = listener.local_addr().expect("collector address");
        let collector = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for stream in listener.incoming().take(2) {
                let mut stream = stream.expect("collector accept");
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .expect("read timeout");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let read = stream.read(&mut chunk).expect("collector read");
                    request.extend_from_slice(&chunk[..read]);
                    let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                        continue;
                    };
                    let headers = String::from_utf8_lossy(&request[..header_end + 4]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + length {
                        assert!(length > 0, "OTLP payload is non-empty");
                        paths.push(
                            headers
                                .lines()
                                .next()
                                .and_then(|line| line.split_whitespace().nth(1))
                                .unwrap_or_default()
                                .to_string(),
                        );
                        break;
                    }
                }
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .expect("collector response");
            }
            paths
        });

        let home = tempfile::tempdir().expect("telemetry home");
        let mut config = cox_protocol::Config::default();
        config.telemetry.otel = true;
        config.telemetry.endpoint = format!("http://{address}");
        let guard = super::init(&config, home.path()).expect("telemetry init");
        let span = tracing::info_span!("telemetry_test_span", session.id = "session-1");
        let _entered = span.enter();
        tracing::info!(event.name = "telemetry_test_log", "test log");
        drop(_entered);
        drop(span);
        drop(guard);

        let mut paths = collector.join().expect("collector thread");
        paths.sort();
        assert_eq!(paths, ["/v1/logs", "/v1/traces"]);
    }
}
