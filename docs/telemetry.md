# Telemetry Integration Guide

FrankenTUI provides optional OpenTelemetry integration for exporting tracing
spans to an OTLP collector. This enables auditability, debugging, and
observability without impacting default performance.

> **Off by default.** Telemetry is never enabled unless you explicitly set
> environment variables and compile with the `telemetry` feature flag.

---

## Quick Start

### 1. Enable the Feature

Add the `telemetry` feature to your Cargo dependency:

```toml
[dependencies]
ftui-runtime = { version = "0.1", features = ["telemetry"] }
```

### 2. Configure Environment

Set the OTLP endpoint:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4318"
export OTEL_SERVICE_NAME="my-app"
```

### 3. Initialize in Your App

```rust
use ftui_runtime::TelemetryConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse config from environment and install subscriber
    let _guard = TelemetryConfig::from_env().install()?;

    // Your FrankenTUI app...

    Ok(())
    // Guard dropped here, flushes pending spans
}
```

---

## Environment Variables

FrankenTUI supports the standard OpenTelemetry environment variables:

### Core Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `OTEL_SDK_DISABLED` | `false` | Set to `true` to disable telemetry entirely |
| `OTEL_SERVICE_NAME` | SDK default | Service name for resource identification |
| `OTEL_TRACES_EXPORTER` | unset | Set to `otlp` to enable export |

### Endpoint Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset | Base OTLP endpoint URL |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | unset | Per-signal override for traces |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `http/protobuf` | `grpc` or `http/protobuf` |
| `OTEL_EXPORTER_OTLP_HEADERS` | unset | `key=value,key2=value2` for auth |

### FrankenTUI Extensions

| Variable | Default | Description |
|----------|---------|-------------|
| `FTUI_OTEL_HTTP_ENDPOINT` | unset | Convenience override for HTTP endpoint |
| `OTEL_TRACE_ID` | unset | 32 hex chars to attach to parent trace |
| `OTEL_PARENT_SPAN_ID` | unset | 16 hex chars for parent span |
| `FTUI_TELEMETRY_VERBOSE` | `false` | Enable verbose field emission |

---

## Integration Strategies

### Strategy 1: Automatic (Simple Apps)

For applications without an existing tracing subscriber:

```rust
use ftui_runtime::TelemetryConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = TelemetryConfig::from_env().install()?;

    // Guard must be held until shutdown
    run_app()?;

    Ok(())
}
```

**Note:** `install()` will fail with `TelemetryError::SubscriberAlreadySet` if
your application already has a global tracing subscriber.

### Strategy 2: Layer Integration (Complex Apps)

For applications that manage their own tracing subscriber:

```rust
use ftui_runtime::TelemetryConfig;
use tracing_subscriber::{layer::SubscriberExt, Registry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TelemetryConfig::from_env();

    if config.is_enabled() {
        let (otel_layer, _provider) = config.build_layer()?;

        let subscriber = Registry::default()
            .with(otel_layer)
            .with(my_logging_layer());

        tracing::subscriber::set_global_default(subscriber)?;
    }

    run_app()?;
    Ok(())
}
```

---

## Attaching to Parent Traces

To attach FrankenTUI spans to an existing distributed trace (e.g., from a
parent process or orchestrator):

```bash
# Set both trace ID and parent span ID
export OTEL_TRACE_ID="0123456789abcdef0123456789abcdef"
export OTEL_PARENT_SPAN_ID="0123456789abcdef"
export OTEL_EXPORTER_OTLP_ENDPOINT="http://collector:4318"
```

**Validation rules:**
- `OTEL_TRACE_ID` must be exactly 32 lowercase hex characters
- `OTEL_PARENT_SPAN_ID` must be exactly 16 lowercase hex characters
- All-zeros values are invalid (per OTEL spec)

If either value is missing or invalid, FrankenTUI creates a new root trace
(fail-open behavior).

---

## Event Schema

FrankenTUI emits spans following the schema in `docs/spec/telemetry-events.md`.

### Runtime Phase Spans

```
ftui.program.init       # Model initialization
ftui.program.update     # Single update cycle
ftui.program.view       # View rendering
```

### Render Pipeline Spans

```
ftui.render.frame       # Complete frame cycle
ftui.render.diff        # Buffer diff computation
ftui.render.present     # ANSI emission
```

### Decision Events

```
ftui.decision.degradation   # Degradation level change
ftui.decision.fallback      # Capability fallback
ftui.decision.resize        # Resize handling
```

---

## Performance Impact

### When Disabled (Default)

- **Zero runtime overhead**: Feature not compiled in
- **No dependencies**: OTEL crates not included in binary

### When Feature Enabled but Env Vars Unset

- **Minimal overhead**: Single boolean check on startup
- **No exporter**: No network or memory overhead

### When Enabled and Active

- **Batch processing**: Spans are batched, not sent synchronously
- **Background thread**: Export happens off the main loop
- **Typical overhead**: < 1% CPU, < 2MB additional memory

---

## Redaction Policy

FrankenTUI follows a conservative redaction policy:

### Never Emitted

- User input content (key presses, text)
- File paths
- Environment variables (except OTEL_* and FTUI_*)
- Memory addresses

### Verbose Mode Only

Enable with `FTUI_TELEMETRY_VERBOSE=true`:

- Full widget type names
- Message enum variants
- Capability details

### Always Emitted

- Counts (widget count, change count)
- Durations (in microseconds)
- Dimensions (width, height)
- Enum variants (screen mode, degradation level)

---

## Debugging

### Check if Telemetry is Active

```rust
let config = TelemetryConfig::from_env();
let ledger = config.evidence_ledger();

println!("Enabled: {}", ledger.enabled);
println!("Reason: {:?}", ledger.enabled_reason);
println!("Endpoint: {:?}", ledger.endpoint_source);
```

### Common Issues

**"TelemetryError::SubscriberAlreadySet"**

Your application already has a global tracing subscriber. Use `build_layer()`
instead of `install()`.

**"No spans appearing in collector"**

1. Check `OTEL_EXPORTER_OTLP_ENDPOINT` is set
2. Verify the collector is running and accessible
3. Check for firewall rules blocking the port

**"Invalid trace ID ignored"**

Trace IDs must be 32 lowercase hex characters. Check your orchestrator
is passing valid IDs.

---

## References

- [OpenTelemetry Rust SDK](https://docs.rs/opentelemetry)
- [OTLP Specification](https://opentelemetry.io/docs/specs/otlp/)
- `docs/spec/telemetry.md` - Env var contract
- `docs/spec/telemetry-events.md` - Event schema
