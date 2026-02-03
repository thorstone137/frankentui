# Telemetry Event Schema + Redaction Policy

This spec defines the event schema for FrankenTUI telemetry and the redaction
policy for sensitive data. It complements `telemetry.md` (env var contract).

---

## 1) Goals

- Define a stable, explicit event schema for OTEL spans/events.
- Establish conservative default redaction of sensitive data.
- Enable consumers to build dashboards and alerts.
- Provide clear semantics for user-supplied fields.

## 2) Non-Goals

- Performance-oriented micro-tracing (covered by separate profiling).
- Complete reconstruction of UI state from telemetry.
- Real-time streaming without batching.

---

## 3) Event Categories

### 3.1 Runtime Phase Events

High-level spans for the Elm/Bubbletea runtime loop.

| Span Name | Description | Fields |
|-----------|-------------|--------|
| `ftui.program.init` | Model initialization | `model_type`, `cmd_count` |
| `ftui.program.update` | Single update cycle | `msg_type`, `duration_us`, `cmd_type` |
| `ftui.program.view` | View rendering | `duration_us`, `widget_count` |
| `ftui.program.subscriptions` | Subscription management | `active_count`, `started`, `stopped` |

### 3.2 Render Pipeline Events

Spans for the render kernel (buffer, diff, presenter).

| Span Name | Description | Fields |
|-----------|-------------|--------|
| `ftui.render.frame` | Complete frame cycle | `width`, `height`, `duration_us` |
| `ftui.render.diff` | Buffer diff computation | `changes_count`, `rows_skipped`, `duration_us` |
| `ftui.render.present` | ANSI emission | `bytes_written`, `runs_count`, `duration_us` |
| `ftui.render.flush` | Output flush | `duration_us`, `sync_mode` |
| `ftui.reflow.apply` | Resize application outcome | `width`, `height`, `debounce_ms`, `latency_ms`, `rate_hz` |
| `ftui.reflow.placeholder` | Resize placeholder shown | `width`, `height`, `rate_hz` |

### 3.3 Decision Events

Point-in-time events for auditable decisions.

| Event Name | Description | Fields |
|------------|-------------|--------|
| `ftui.decision.degradation` | Degradation level change | `level`, `reason`, `budget_remaining` |
| `ftui.decision.fallback` | Capability fallback | `capability`, `fallback_to`, `reason` |
| `ftui.decision.resize` | Resize handling decision | `strategy`, `debounce_active`, `coalesced`, `same_size`, `width`, `height`, `rate_hz` |
| `ftui.decision.screen_mode` | Screen mode selection | `mode`, `ui_height`, `anchor` |

### 3.4 Input Events

Spans for input processing (redacted by default).

| Span Name | Description | Fields |
|-----------|-------------|--------|
| `ftui.input.event` | Input event processing | `event_type` (no content!) |
| `ftui.input.macro` | Macro playback | `macro_id`, `event_count` |

---

## 4) Field Schema

### 4.1 Common Fields (All Spans)

These fields are attached to every span:

```
service.name      string   - From OTEL_SERVICE_NAME or "ftui-runtime"
service.version   string   - FrankenTUI version
telemetry.sdk     string   - "ftui-telemetry"
host.arch         string   - Target architecture
process.pid       int      - Process ID
```

### 4.2 Duration Fields

All duration fields use microseconds (us) as the unit for precision:

```
duration_us       u64      - Elapsed time in microseconds
```

### 4.3 Decision Evidence Fields

Decision events include structured evidence:

```
decision.rule      string   - Rule/heuristic applied
decision.inputs    string   - JSON-serialized input state (redacted)
decision.action    string   - Chosen action
decision.confidence f32     - Confidence score (0.0-1.0) if applicable
```

---

## 5) Redaction Policy

### 5.1 Principles

1. **Conservative by default**: Err on the side of not emitting.
2. **No PII**: Never emit user input content, file paths, or secrets.
3. **Structural only**: Emit types and counts, not values.
4. **Opt-in detail**: Verbose fields require explicit configuration.

### 5.2 Never Emit (Hard Redaction)

The following MUST never appear in telemetry:

| Category | Examples |
|----------|----------|
| **User input content** | Key characters, text buffer contents, passwords |
| **File paths** | Log files, config paths, temp files |
| **Environment variables** | Beyond OTEL_* and FTUI_* prefixes |
| **Memory addresses** | Pointer values, buffer addresses |
| **Process arguments** | Command-line arguments |
| **User identifiers** | Usernames, home directories |

### 5.3 Conditionally Emit (Soft Redaction)

These are omitted by default but can be enabled via `FTUI_TELEMETRY_VERBOSE=true`:

| Category | When Enabled |
|----------|--------------|
| **Widget types** | Full widget type names |
| **Message types** | Model::Message enum variants |
| **Command types** | Cmd enum variants |
| **Capability details** | Full terminal capability report |

### 5.4 Always Emit (No Redaction)

These are considered safe for all environments:

| Category | Examples |
|----------|----------|
| **Counts** | Widget count, change count, event count |
| **Durations** | All timing measurements |
| **Dimensions** | Buffer width/height, UI height |
| **Enum variants** | Screen mode, degradation level |
| **Boolean flags** | Mouse enabled, sync available |

---

## 6) User-Supplied Field Handling

### 6.1 Custom Span Attributes

Applications may attach custom attributes via tracing:

```rust
tracing::info_span!("my_operation", custom.field = "value");
```

**Policy:**
- Prefix requirement: Custom fields MUST use a namespace prefix (e.g., `app.`, `custom.`)
- No automatic redaction: Application is responsible for not emitting sensitive data
- Pass-through: Custom fields are passed to the OTEL exporter unchanged

### 6.2 Custom Events

Applications may emit custom events:

```rust
tracing::info!(target: "app.audit", action = "user_action");
```

**Policy:**
- Filtered by target: Only targets matching `app.*` or `custom.*` are exported
- Rate limiting: Custom events are subject to the same batching as built-in events
- Documentation: Applications should document their custom event schemas

---

## 7) Schema Versioning

### 7.1 Version Field

All telemetry includes a schema version:

```
ftui.schema_version   string   - Semantic version (e.g., "1.0.0")
```

### 7.2 Compatibility Rules

- **Patch versions** (1.0.x): Additive only, no breaking changes
- **Minor versions** (1.x.0): New fields, deprecated fields still emitted
- **Major versions** (x.0.0): Breaking changes, old fields may be removed

### 7.3 Current Schema Version

**Version: 1.0.0** (Initial stable schema)

---

## 8) Invariants (Alien Artifact)

1. **Redaction completeness**: No user input content escapes to telemetry.
2. **Schema stability**: Breaking changes require major version bump.
3. **Duration precision**: All durations use microseconds.
4. **Deterministic field set**: Same operation produces same field names.
5. **Bounded cardinality**: Enum-typed fields have known cardinality.

### Failure Modes

| Scenario | Behavior |
|----------|----------|
| Serialization error | Log warning, omit event |
| Field value overflow | Saturate to max value |
| Unknown field type | Stringify with `.to_string()` |
| Custom field collision | Prefix with `app.` |

---

## 9) Evidence Ledger Fields

For decision events, include:

```rust
pub struct DecisionEvidence {
    /// Rule or heuristic that triggered the decision
    pub rule: String,
    /// Inputs to the decision (redacted as per policy)
    pub inputs_summary: String,
    /// Chosen action
    pub action: String,
    /// Confidence (0.0-1.0) if probabilistic
    pub confidence: Option<f32>,
    /// Alternative actions considered
    pub alternatives: Vec<String>,
    /// Brief explanation for humans
    pub explanation: String,
}
```

---

## 10) Implementation Notes

### 10.1 Span Attributes

Use `tracing::Span::record()` for dynamic fields:

```rust
let span = tracing::info_span!("ftui.render.frame", width = ?width, height = ?height);
let _guard = span.enter();
// ... render ...
span.record("duration_us", elapsed.as_micros() as u64);
```

### 10.2 Redaction Helper

Implement a redaction utility for consistent handling:

```rust
pub fn redact_path(path: &Path) -> &'static str {
    "[redacted:path]"
}

pub fn redact_content(content: &str) -> &'static str {
    "[redacted:content]"
}

pub fn summarize_count(items: &[T]) -> String {
    format!("{} items", items.len())
}
```

### 10.3 Verbose Mode

Check `FTUI_TELEMETRY_VERBOSE` for conditional fields:

```rust
fn is_verbose() -> bool {
    std::env::var("FTUI_TELEMETRY_VERBOSE")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
```

---

## 11) Tests

### Unit Tests

- Redaction functions return placeholder strings
- Schema version field is present
- Duration fields are u64 microseconds
- Custom field prefixing works correctly

### Property Tests

- No user input content appears in any telemetry output
- Field names are ASCII lowercase with dots
- All enum variants have known cardinality

### E2E Tests

- Capture OTEL export and verify schema compliance
- Verify redaction in verbose and non-verbose modes
- Check schema version in exported spans
