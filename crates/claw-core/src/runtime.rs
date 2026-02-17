//! Runtime environment abstraction for claw-rust.
//!
//! Ports OpenClaw's `RuntimeEnv` type, wrapping the `tracing` ecosystem
//! to provide structured logging with a simple interface.

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the global tracing subscriber with fmt + env-filter layers.
///
/// Call this once at application startup before any logging occurs.
/// The filter respects the `RUST_LOG` environment variable, defaulting
/// to `info` level if unset.
///
/// # Examples
///
/// ```no_run
/// claw_core::runtime::init_tracing();
/// ```
pub fn init_tracing() {
    init_tracing_with_default("info");
}

/// Initialize tracing with a custom default filter level.
///
/// The `RUST_LOG` environment variable takes precedence over `default_level`.
pub fn init_tracing_with_default(default_level: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .init();
}

/// Runtime environment providing logging and process lifecycle methods.
///
/// Wraps `tracing` macros behind a simple interface matching OpenClaw's
/// `RuntimeEnv` type. Supports structured fields for context like
/// `channel_id`, `session_key`, etc.
///
/// # Examples
///
/// ```
/// use claw_core::runtime::RuntimeEnv;
///
/// let env = RuntimeEnv::new("my-service");
/// env.log("Server started on port 8080");
/// env.warn("Connection pool running low");
/// env.error("Failed to connect to database");
/// ```
#[derive(Debug, Clone)]
pub struct RuntimeEnv {
    /// Service or component name attached to every log entry.
    service: String,
}

impl RuntimeEnv {
    /// Create a new `RuntimeEnv` for the given service name.
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    /// Return the service name.
    pub fn service(&self) -> &str {
        &self.service
    }

    /// Log a message at `trace` level.
    pub fn trace(&self, message: &str) {
        tracing::trace!(service = %self.service, "{message}");
    }

    /// Log a message at `debug` level.
    pub fn debug(&self, message: &str) {
        tracing::debug!(service = %self.service, "{message}");
    }

    /// Log a message at `info` level (the default "log" method).
    pub fn log(&self, message: &str) {
        tracing::info!(service = %self.service, "{message}");
    }

    /// Log a message at `warn` level.
    pub fn warn(&self, message: &str) {
        tracing::warn!(service = %self.service, "{message}");
    }

    /// Log a message at `error` level.
    pub fn error(&self, message: &str) {
        tracing::error!(service = %self.service, "{message}");
    }

    /// Log a message at `info` level with structured key-value fields.
    ///
    /// Fields are emitted as tracing structured data alongside the message.
    ///
    /// # Examples
    ///
    /// ```
    /// use claw_core::runtime::RuntimeEnv;
    ///
    /// let env = RuntimeEnv::new("gateway");
    /// env.log_with_fields("request received", &[
    ///     ("channel_id", "telegram-123"),
    ///     ("session_key", "sk_abc"),
    /// ]);
    /// ```
    pub fn log_with_fields(&self, message: &str, fields: &[(&str, &str)]) {
        let span = tracing::info_span!("structured", service = %self.service);
        let _guard = span.enter();

        // Build a comma-separated fields string for the log message,
        // while also recording each field as a tracing event field
        // through the span context.
        let fields_str: String = fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");

        tracing::info!("{message} [{fields_str}]");
    }

    /// Log a message at `error` level with structured key-value fields.
    pub fn error_with_fields(&self, message: &str, fields: &[(&str, &str)]) {
        let span = tracing::error_span!("structured", service = %self.service);
        let _guard = span.enter();

        let fields_str: String = fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");

        tracing::error!("{message} [{fields_str}]");
    }

    /// Terminate the process with the given exit code.
    ///
    /// This calls `std::process::exit` and does **not** return.
    pub fn exit(&self, code: i32) -> ! {
        tracing::info!(service = %self.service, code, "process exiting");
        std::process::exit(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::fmt::MakeWriter;
    use std::sync::{Arc, Mutex};

    /// A writer that captures output into a shared buffer for test assertions.
    #[derive(Clone)]
    struct CaptureWriter {
        buf: Arc<Mutex<Vec<u8>>>,
    }

    impl CaptureWriter {
        fn new() -> Self {
            Self {
                buf: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn contents(&self) -> String {
            let buf = self.buf.lock().expect("lock poisoned");
            String::from_utf8_lossy(&buf).to_string()
        }
    }

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buf.lock().expect("lock poisoned").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Install a test subscriber that captures output, run the closure, then
    /// return captured output. Uses `tracing::subscriber::with_default` so it
    /// does not conflict with the global subscriber.
    fn capture_logs<F: FnOnce()>(f: F) -> String {
        let writer = CaptureWriter::new();
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new("trace"))
            .with(fmt::layer().with_writer(writer.clone()).with_ansi(false));

        tracing::subscriber::with_default(subscriber, f);
        writer.contents()
    }

    #[test]
    fn new_sets_service_name() {
        let env = RuntimeEnv::new("test-svc");
        assert_eq!(env.service(), "test-svc");
    }

    #[test]
    fn clone_preserves_service() {
        let env = RuntimeEnv::new("cloneable");
        let cloned = env.clone();
        assert_eq!(cloned.service(), "cloneable");
    }

    #[test]
    fn log_emits_info_with_service() {
        let output = capture_logs(|| {
            let env = RuntimeEnv::new("gateway");
            env.log("hello world");
        });
        assert!(output.contains("hello world"), "missing message in: {output}");
        assert!(output.contains("gateway"), "missing service in: {output}");
        assert!(output.contains("INFO"), "missing INFO level in: {output}");
    }

    #[test]
    fn error_emits_error_level() {
        let output = capture_logs(|| {
            let env = RuntimeEnv::new("db");
            env.error("connection failed");
        });
        assert!(output.contains("connection failed"), "missing message in: {output}");
        assert!(output.contains("ERROR"), "missing ERROR level in: {output}");
    }

    #[test]
    fn warn_emits_warn_level() {
        let output = capture_logs(|| {
            let env = RuntimeEnv::new("pool");
            env.warn("running low");
        });
        assert!(output.contains("running low"), "missing message in: {output}");
        assert!(output.contains("WARN"), "missing WARN level in: {output}");
    }

    #[test]
    fn debug_emits_debug_level() {
        let output = capture_logs(|| {
            let env = RuntimeEnv::new("router");
            env.debug("route matched");
        });
        assert!(output.contains("route matched"), "missing message in: {output}");
        assert!(output.contains("DEBUG"), "missing DEBUG level in: {output}");
    }

    #[test]
    fn trace_emits_trace_level() {
        let output = capture_logs(|| {
            let env = RuntimeEnv::new("parser");
            env.trace("parsing token");
        });
        assert!(output.contains("parsing token"), "missing message in: {output}");
        assert!(output.contains("TRACE"), "missing TRACE level in: {output}");
    }

    #[test]
    fn log_with_fields_includes_structured_data() {
        let output = capture_logs(|| {
            let env = RuntimeEnv::new("handler");
            env.log_with_fields("request", &[
                ("channel_id", "tg-42"),
                ("session_key", "sk_xyz"),
            ]);
        });
        assert!(output.contains("channel_id=tg-42"), "missing field in: {output}");
        assert!(output.contains("session_key=sk_xyz"), "missing field in: {output}");
    }

    #[test]
    fn error_with_fields_includes_structured_data() {
        let output = capture_logs(|| {
            let env = RuntimeEnv::new("dispatch");
            env.error_with_fields("dispatch failed", &[("error_code", "E501")]);
        });
        assert!(output.contains("dispatch failed"), "missing message in: {output}");
        assert!(output.contains("error_code=E501"), "missing field in: {output}");
        assert!(output.contains("ERROR"), "missing ERROR level in: {output}");
    }
}
