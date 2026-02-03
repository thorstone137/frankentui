#![forbid(unsafe_code)]

//! Optional OpenTelemetry Telemetry Integration
//!
//! This module provides a non-invasive integration strategy for exporting
//! tracing spans to an OpenTelemetry collector. It follows the principle of
//! never clobbering existing tracing subscribers.
//!
//! # Integration Strategies
//!
//! ## Strategy 1: Automatic Installation (Recommended for Simple Apps)
//!
//! Use `TelemetryConfig::from_env().install()` when your application doesn't
//! have its own tracing subscriber. This will install a subscriber if none
//! exists.
//!
//! ```ignore
//! let _guard = TelemetryConfig::from_env().install()?;
//! // Runtime loop...
//! // Guard dropped on exit, flushes spans
//! ```
//!
//! ## Strategy 2: Layer Integration (For Apps with Existing Subscribers)
//!
//! Use `TelemetryConfig::from_env().build_layer()` when your application
//! already manages a tracing subscriber. Attach the layer to your subscriber.
//!
//! ```ignore
//! let (otel_layer, provider) = TelemetryConfig::from_env().build_layer()?;
//! let subscriber = Registry::default().with(otel_layer).with(my_other_layer);
//! subscriber.try_init()?; // Fails if a global subscriber already exists.
//! // Keep provider alive until shutdown.
//! ```
//!
//! # Env Var Contract
//!
//! See `docs/spec/telemetry.md` for the full env var specification. Key vars:
//!
//! - `OTEL_SDK_DISABLED=true` - Disable telemetry entirely
//! - `OTEL_EXPORTER_OTLP_ENDPOINT` - OTLP collector endpoint
//! - `OTEL_SERVICE_NAME` - Service name for resource
//! - `OTEL_TRACE_ID` / `OTEL_PARENT_SPAN_ID` - Explicit parent context
//!
//! # Invariants
//!
//! - Telemetry is never enabled without explicit env vars
//! - When disabled, overhead is a single boolean check
//! - Invalid trace IDs fail-open (create new root trace)
//! - `install()` fails if a global subscriber already exists

use std::env;
use std::fmt;

/// Telemetry configuration parsed from environment variables.
///
/// This struct captures the configuration for OpenTelemetry integration,
/// following the env var contract defined in `docs/spec/telemetry.md`.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled based on env vars.
    pub enabled: bool,
    /// Reason why telemetry is enabled/disabled (for evidence ledger).
    pub enabled_reason: EnabledReason,
    /// OTLP endpoint URL.
    pub endpoint: Option<String>,
    /// Source of the endpoint (for evidence ledger).
    pub endpoint_source: EndpointSource,
    /// OTLP protocol (grpc or http/protobuf).
    pub protocol: Protocol,
    /// Service name for the resource.
    pub service_name: Option<String>,
    /// Extra resource attributes.
    pub resource_attributes: Vec<(String, String)>,
    /// Optional explicit trace ID.
    pub trace_id: Option<TraceId>,
    /// Optional explicit parent span ID.
    pub parent_span_id: Option<SpanId>,
    /// Source of trace context (for evidence ledger).
    pub trace_context_source: TraceContextSource,
    /// OTLP headers for auth.
    pub headers: Vec<(String, String)>,
}

/// Reason why telemetry is enabled or disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnabledReason {
    /// Disabled by OTEL_SDK_DISABLED=true.
    SdkDisabled,
    /// Disabled by OTEL_TRACES_EXPORTER=none.
    ExporterNone,
    /// Enabled by explicit OTEL_TRACES_EXPORTER=otlp.
    ExplicitOtlp,
    /// Enabled by OTEL_EXPORTER_OTLP_ENDPOINT.
    EndpointSet,
    /// Enabled by FTUI_OTEL_HTTP_ENDPOINT.
    FtuiEndpointSet,
    /// Disabled by default (no trigger env vars set).
    DefaultDisabled,
}

/// Source of the OTLP endpoint configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSource {
    /// From OTEL_EXPORTER_OTLP_TRACES_ENDPOINT.
    TracesEndpoint,
    /// From FTUI_OTEL_HTTP_ENDPOINT.
    FtuiOverride,
    /// From OTEL_EXPORTER_OTLP_ENDPOINT.
    BaseEndpoint,
    /// Using protocol default (localhost:4318 for HTTP, 4317 for gRPC).
    ProtocolDefault,
    /// Not set (telemetry disabled).
    None,
}

/// OTLP transport protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Protocol {
    /// gRPC transport (localhost:4317 default).
    Grpc,
    /// HTTP with protobuf encoding (localhost:4318 default).
    #[default]
    HttpProtobuf,
}

impl Protocol {
    fn default_endpoint(self) -> &'static str {
        match self {
            Self::Grpc => "http://localhost:4317",
            Self::HttpProtobuf => "http://localhost:4318",
        }
    }
}

/// Source of trace context (explicit IDs vs new root).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceContextSource {
    /// Using explicit trace/span IDs from env vars.
    Explicit,
    /// Creating a new root trace.
    New,
    /// Telemetry disabled, no context.
    Disabled,
}

/// Validated 128-bit trace ID (32 hex chars).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// Parse a 32-char lowercase hex string into a trace ID.
    pub fn parse(s: &str) -> Option<Self> {
        if s.len() != 32
            || !s
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return None;
        }
        let mut bytes = [0u8; 16];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex_str = std::str::from_utf8(chunk).ok()?;
            bytes[i] = u8::from_str_radix(hex_str, 16).ok()?;
        }
        // All zeros is invalid per OTEL spec
        if bytes.iter().all(|&b| b == 0) {
            return None;
        }
        Some(Self(bytes))
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Validated 64-bit span ID (16 hex chars).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanId([u8; 8]);

impl SpanId {
    /// Parse a 16-char lowercase hex string into a span ID.
    pub fn parse(s: &str) -> Option<Self> {
        if s.len() != 16
            || !s
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return None;
        }
        let mut bytes = [0u8; 8];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex_str = std::str::from_utf8(chunk).ok()?;
            bytes[i] = u8::from_str_radix(hex_str, 16).ok()?;
        }
        // All zeros is invalid per OTEL spec
        if bytes.iter().all(|&b| b == 0) {
            return None;
        }
        Some(Self(bytes))
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

/// Errors that can occur during telemetry setup.
#[derive(Debug)]
pub enum TelemetryError {
    /// A global tracing subscriber is already installed.
    SubscriberAlreadySet,
    /// Exporter initialization failed.
    ExporterInit(String),
    /// Provider setup failed.
    ProviderSetup(String),
}

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubscriberAlreadySet => write!(
                f,
                "A global tracing subscriber is already set. Use build_layer() instead."
            ),
            Self::ExporterInit(msg) => write!(f, "Failed to initialize OTLP exporter: {msg}"),
            Self::ProviderSetup(msg) => write!(f, "Failed to set up tracer provider: {msg}"),
        }
    }
}

impl std::error::Error for TelemetryError {}

/// RAII guard for telemetry shutdown.
///
/// When dropped, ensures pending spans are flushed to the collector.
#[must_use = "TelemetryGuard must be held until shutdown to ensure spans are flushed"]
pub struct TelemetryGuard {
    /// Provider is kept alive to ensure spans are flushed on drop.
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
    /// Marker to prevent Send/Sync (guard must be dropped on same thread).
    _marker: std::marker::PhantomData<*const ()>,
}

impl TelemetryGuard {
    fn new(provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>) -> Self {
        Self {
            provider,
            _marker: std::marker::PhantomData,
        }
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            let _ = provider.shutdown();
        }
        // Log for visibility.
        tracing::debug!("TelemetryGuard dropped, flushing spans");
    }
}

impl TelemetryConfig {
    /// Parse telemetry configuration from environment variables.
    ///
    /// This follows the env var contract defined in `docs/spec/telemetry.md`.
    /// The parsing is deterministic and order-independent.
    pub fn from_env() -> Self {
        // Step 1: Check for SDK disabled
        if env::var("OTEL_SDK_DISABLED")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return Self::disabled(EnabledReason::SdkDisabled);
        }

        // Step 2: Check for exporter=none
        if env::var("OTEL_TRACES_EXPORTER")
            .map(|v| v.eq_ignore_ascii_case("none"))
            .unwrap_or(false)
        {
            return Self::disabled(EnabledReason::ExporterNone);
        }

        // Step 3: Determine if telemetry should be enabled
        let explicit_otlp = env::var("OTEL_TRACES_EXPORTER")
            .map(|v| v.eq_ignore_ascii_case("otlp"))
            .unwrap_or(false);
        let has_otel_endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok();
        let has_ftui_endpoint = env::var("FTUI_OTEL_HTTP_ENDPOINT").is_ok();

        let (enabled, enabled_reason) = if explicit_otlp {
            (true, EnabledReason::ExplicitOtlp)
        } else if has_ftui_endpoint {
            (true, EnabledReason::FtuiEndpointSet)
        } else if has_otel_endpoint {
            (true, EnabledReason::EndpointSet)
        } else {
            (false, EnabledReason::DefaultDisabled)
        };

        if !enabled {
            return Self::disabled(enabled_reason);
        }

        // Step 4: Parse protocol
        let protocol = env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
            .or_else(|_| env::var("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL"))
            .map(|v| {
                if v.eq_ignore_ascii_case("grpc") {
                    Protocol::Grpc
                } else {
                    Protocol::HttpProtobuf
                }
            })
            .unwrap_or_default();

        // Step 5: Resolve endpoint
        let (endpoint, endpoint_source) =
            if let Ok(ep) = env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT") {
                (Some(ep), EndpointSource::TracesEndpoint)
            } else if let Ok(ep) = env::var("FTUI_OTEL_HTTP_ENDPOINT") {
                (Some(ep), EndpointSource::FtuiOverride)
            } else if let Ok(ep) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
                (Some(ep), EndpointSource::BaseEndpoint)
            } else {
                (
                    Some(protocol.default_endpoint().to_string()),
                    EndpointSource::ProtocolDefault,
                )
            };

        // Step 6: Parse trace context
        let trace_id = env::var("OTEL_TRACE_ID")
            .ok()
            .and_then(|s| TraceId::parse(&s));
        let parent_span_id = env::var("OTEL_PARENT_SPAN_ID")
            .ok()
            .and_then(|s| SpanId::parse(&s));

        let trace_context_source = if trace_id.is_some() && parent_span_id.is_some() {
            TraceContextSource::Explicit
        } else {
            TraceContextSource::New
        };

        // Step 7: Parse other settings
        let service_name = env::var("OTEL_SERVICE_NAME").ok();
        let resource_attributes = Self::parse_kv_list(&env::var("OTEL_RESOURCE_ATTRIBUTES").ok());
        let headers = Self::parse_kv_list(&env::var("OTEL_EXPORTER_OTLP_HEADERS").ok());

        Self {
            enabled,
            enabled_reason,
            endpoint,
            endpoint_source,
            protocol,
            service_name,
            resource_attributes,
            trace_id,
            parent_span_id,
            trace_context_source,
            headers,
        }
    }

    /// Create a disabled config.
    fn disabled(reason: EnabledReason) -> Self {
        Self {
            enabled: false,
            enabled_reason: reason,
            endpoint: None,
            endpoint_source: EndpointSource::None,
            protocol: Protocol::default(),
            service_name: None,
            resource_attributes: vec![],
            trace_id: None,
            parent_span_id: None,
            trace_context_source: TraceContextSource::Disabled,
            headers: vec![],
        }
    }

    /// Parse a comma-separated key=value list.
    fn parse_kv_list(s: &Option<String>) -> Vec<(String, String)> {
        s.as_ref()
            .map(|s| {
                s.split(',')
                    .filter_map(|kv| {
                        let mut parts = kv.splitn(2, '=');
                        let key = parts.next()?.trim();
                        let value = parts.next()?.trim();
                        if key.is_empty() {
                            None
                        } else {
                            Some((key.to_string(), value.to_string()))
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if telemetry is enabled.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Install a tracing subscriber with OpenTelemetry export.
    ///
    /// This will fail with `TelemetryError::SubscriberAlreadySet` if a global
    /// subscriber is already installed. Use `build_layer()` instead in that case.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A global subscriber is already set
    /// - The OTLP exporter fails to initialize
    ///
    /// # Returns
    ///
    /// Returns a `TelemetryGuard` that must be held until shutdown. Dropping
    /// the guard will flush pending spans.
    #[cfg(feature = "telemetry")]
    pub fn install(self) -> Result<TelemetryGuard, TelemetryError> {
        if !self.enabled {
            tracing::debug!(reason = ?self.enabled_reason, "Telemetry disabled");
            return Ok(TelemetryGuard::new(None));
        }

        let endpoint = self.endpoint.clone();
        let protocol = self.protocol;
        let trace_context = self.trace_context_source;
        let (otel_layer, provider) = self.build_layer()?;

        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let subscriber = tracing_subscriber::registry().with(otel_layer);
        if subscriber.try_init().is_err() {
            let _ = provider.shutdown();
            return Err(TelemetryError::SubscriberAlreadySet);
        }

        tracing::info!(
            endpoint = ?endpoint,
            protocol = ?protocol,
            trace_context = ?trace_context,
            "Telemetry installed"
        );

        Ok(TelemetryGuard::new(Some(provider)))
    }

    /// Build an OpenTelemetry layer for manual subscriber integration.
    ///
    /// Use this when your application already manages a tracing subscriber.
    /// Attach the returned layer to your subscriber registry.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (otel_layer, _provider) = config.build_layer()?;
    /// let subscriber = Registry::default()
    ///     .with(otel_layer)
    ///     .with(my_other_layers);
    /// subscriber.try_init()?;
    /// ```
    ///
    /// # Note
    ///
    /// The returned provider must be kept alive until shutdown to ensure
    /// spans are properly exported.
    #[cfg(feature = "telemetry")]
    pub fn build_layer(
        self,
    ) -> Result<
        (
            tracing_opentelemetry::OpenTelemetryLayer<
                tracing_subscriber::Registry,
                opentelemetry_sdk::trace::Tracer,
            >,
            opentelemetry_sdk::trace::SdkTracerProvider,
        ),
        TelemetryError,
    > {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::trace::SdkTracerProvider;

        if !self.enabled {
            return Err(TelemetryError::ExporterInit(
                "Telemetry is disabled".to_string(),
            ));
        }

        // Build the exporter
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(self.endpoint.as_deref().unwrap_or("http://localhost:4318"))
            .build()
            .map_err(|e: opentelemetry_otlp::ExporterBuildError| {
                TelemetryError::ExporterInit(e.to_string())
            })?;

        // Build the provider with batch processor for production
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .build();

        // Get a tracer
        let tracer = provider.tracer("ftui-runtime");

        // Build the layer
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);

        Ok((layer, provider))
    }

    /// Get an evidence ledger entry for debugging/auditing.
    ///
    /// Returns a structured summary of the configuration decisions.
    pub fn evidence_ledger(&self) -> EvidenceLedger {
        EvidenceLedger {
            enabled: self.enabled,
            enabled_reason: self.enabled_reason,
            endpoint_source: self.endpoint_source,
            protocol: self.protocol,
            trace_context_source: self.trace_context_source,
            service_name: self.service_name.clone(),
        }
    }
}

/// Evidence ledger for telemetry configuration decisions.
///
/// This provides a structured record of why telemetry was configured
/// the way it was, useful for debugging and auditing.
#[derive(Debug, Clone)]
pub struct EvidenceLedger {
    pub enabled: bool,
    pub enabled_reason: EnabledReason,
    pub endpoint_source: EndpointSource,
    pub protocol: Protocol,
    pub trace_context_source: TraceContextSource,
    pub service_name: Option<String>,
}

/// Current schema version for telemetry events.
///
/// See `docs/spec/telemetry-events.md` for version compatibility rules.
pub const SCHEMA_VERSION: &str = "1.0.0";

/// Evidence for a runtime decision, suitable for audit trails.
///
/// This struct captures the reasoning behind non-trivial decisions
/// made by the runtime, enabling post-hoc analysis and debugging.
#[derive(Debug, Clone)]
pub struct DecisionEvidence {
    /// Rule or heuristic that triggered the decision.
    pub rule: String,
    /// Summary of inputs (redacted as per policy).
    pub inputs_summary: String,
    /// Chosen action.
    pub action: String,
    /// Confidence (0.0-1.0) if probabilistic.
    pub confidence: Option<f32>,
    /// Alternative actions considered.
    pub alternatives: Vec<String>,
    /// Brief explanation for humans.
    pub explanation: String,
}

impl DecisionEvidence {
    /// Create a simple decision with rule and action.
    pub fn simple(rule: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            inputs_summary: String::new(),
            action: action.into(),
            confidence: None,
            alternatives: vec![],
            explanation: String::new(),
        }
    }

    /// Add an explanation to the evidence.
    #[must_use]
    pub fn with_explanation(mut self, explanation: impl Into<String>) -> Self {
        self.explanation = explanation.into();
        self
    }

    /// Add confidence score.
    #[must_use]
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    /// Add alternative actions that were considered.
    #[must_use]
    pub fn with_alternatives(mut self, alternatives: Vec<String>) -> Self {
        self.alternatives = alternatives;
        self
    }
}

/// Redaction utilities for telemetry.
///
/// These functions implement the redaction policy defined in
/// `docs/spec/telemetry-events.md`.
pub mod redact {
    use std::path::Path;

    /// Redact a file path (never emit full paths).
    #[inline]
    pub fn path(_path: &Path) -> &'static str {
        "[redacted:path]"
    }

    /// Redact user input content (never emit).
    #[inline]
    pub fn content(_content: &str) -> &'static str {
        "[redacted:content]"
    }

    /// Redact a memory address.
    #[inline]
    pub fn address<T>(_ptr: *const T) -> &'static str {
        "[redacted:address]"
    }

    /// Summarize a count without exposing content.
    #[inline]
    pub fn count<T>(items: &[T]) -> String {
        format!("{} items", items.len())
    }

    /// Summarize byte size.
    #[inline]
    pub fn bytes(size: usize) -> String {
        if size < 1024 {
            format!("{size} B")
        } else if size < 1024 * 1024 {
            format!("{:.1} KB", size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
        }
    }

    /// Check if verbose telemetry is enabled.
    #[inline]
    pub fn is_verbose() -> bool {
        std::env::var("FTUI_TELEMETRY_VERBOSE")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }

    /// Emit a field only if verbose mode is enabled.
    #[inline]
    pub fn if_verbose<T: Default>(value: T) -> T {
        if is_verbose() { value } else { T::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id_parse_valid() {
        let id = TraceId::parse("0123456789abcdef0123456789abcdef");
        assert!(id.is_some());
    }

    #[test]
    fn test_trace_id_parse_invalid_length() {
        assert!(TraceId::parse("0123456789abcdef").is_none());
        assert!(TraceId::parse("0123456789abcdef0123456789abcdef00").is_none());
    }

    #[test]
    fn test_trace_id_parse_invalid_uppercase() {
        assert!(TraceId::parse("0123456789ABCDEF0123456789abcdef").is_none());
    }

    #[test]
    fn test_trace_id_parse_all_zeros() {
        assert!(TraceId::parse("00000000000000000000000000000000").is_none());
    }

    #[test]
    fn test_span_id_parse_valid() {
        let id = SpanId::parse("0123456789abcdef");
        assert!(id.is_some());
    }

    #[test]
    fn test_span_id_parse_invalid() {
        assert!(SpanId::parse("012345").is_none());
        assert!(SpanId::parse("0123456789ABCDEF").is_none());
        assert!(SpanId::parse("0000000000000000").is_none());
    }

    #[test]
    fn test_parse_kv_list() {
        let result = TelemetryConfig::parse_kv_list(&Some("a=b,c=d".to_string()));
        assert_eq!(
            result,
            vec![
                ("a".to_string(), "b".to_string()),
                ("c".to_string(), "d".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_kv_list_with_spaces() {
        let result = TelemetryConfig::parse_kv_list(&Some(" key = value , k2=v2 ".to_string()));
        assert_eq!(
            result,
            vec![
                ("key".to_string(), "value".to_string()),
                ("k2".to_string(), "v2".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_kv_list_empty() {
        let result = TelemetryConfig::parse_kv_list(&None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_protocol_default_endpoint() {
        assert_eq!(Protocol::Grpc.default_endpoint(), "http://localhost:4317");
        assert_eq!(
            Protocol::HttpProtobuf.default_endpoint(),
            "http://localhost:4318"
        );
    }

    #[test]
    fn test_disabled_config() {
        let config = TelemetryConfig::disabled(EnabledReason::SdkDisabled);
        assert!(!config.is_enabled());
        assert_eq!(config.enabled_reason, EnabledReason::SdkDisabled);
        assert_eq!(config.trace_context_source, TraceContextSource::Disabled);
    }

    #[test]
    fn test_schema_version() {
        assert_eq!(SCHEMA_VERSION, "1.0.0");
    }

    #[test]
    fn test_decision_evidence_simple() {
        let evidence = DecisionEvidence::simple("degradation_rule", "reduce_to_skeleton");
        assert_eq!(evidence.rule, "degradation_rule");
        assert_eq!(evidence.action, "reduce_to_skeleton");
        assert!(evidence.confidence.is_none());
        assert!(evidence.alternatives.is_empty());
    }

    #[test]
    fn test_decision_evidence_builder() {
        let evidence = DecisionEvidence::simple("fallback_rule", "use_ascii")
            .with_explanation("Terminal does not support Unicode")
            .with_confidence(0.95)
            .with_alternatives(vec!["use_emoji".to_string(), "skip_render".to_string()]);

        assert_eq!(evidence.explanation, "Terminal does not support Unicode");
        assert_eq!(evidence.confidence, Some(0.95));
        assert_eq!(evidence.alternatives.len(), 2);
    }

    #[test]
    fn test_decision_evidence_confidence_clamped() {
        let evidence = DecisionEvidence::simple("test", "test").with_confidence(1.5);
        assert_eq!(evidence.confidence, Some(1.0));

        let evidence = DecisionEvidence::simple("test", "test").with_confidence(-0.5);
        assert_eq!(evidence.confidence, Some(0.0));
    }

    #[test]
    fn test_redact_path() {
        use std::path::Path;
        assert_eq!(
            redact::path(Path::new("/home/user/secret.txt")),
            "[redacted:path]"
        );
    }

    #[test]
    fn test_redact_content() {
        assert_eq!(redact::content("sensitive data"), "[redacted:content]");
    }

    #[test]
    fn test_redact_count() {
        let items = vec![1, 2, 3, 4, 5];
        assert_eq!(redact::count(&items), "5 items");
    }

    #[test]
    fn test_redact_bytes() {
        assert_eq!(redact::bytes(500), "500 B");
        assert_eq!(redact::bytes(2048), "2.0 KB");
        assert_eq!(redact::bytes(1024 * 1024 + 512 * 1024), "1.5 MB");
    }
}
