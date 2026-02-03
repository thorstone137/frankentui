# Continuous Reflow Migration Guide

> Migrating from `ResizeDebouncer` to `ResizeCoalescer` for regime-aware
> resize handling.

---

## 1. What Changed

The resize system has been replaced from a simple duration-based debouncer
to a regime-aware coalescer with statistical guarantees:

| Aspect | Old (ResizeDebouncer) | New (ResizeCoalescer) |
|--------|----------------------|----------------------|
| Strategy | Fixed-duration debounce | Regime-adaptive (Steady/Burst) |
| Behavior enum | `Immediate`, `Throttled`, `Placeholder` | `Immediate`, `Throttled` |
| Placeholder | Yes (visual resize indicator) | No (continuous reflow preferred) |
| Config | `Duration` (single knob) | `CoalescerConfig` (regime thresholds, deadlines) |
| Observability | None | Decision logs, telemetry hooks, JSONL export |
| Latency guarantee | None (debounce duration only) | Hard deadline (`hard_deadline_ms`) |
| Fairness | None | Input fairness guard prevents starvation |

### Removed Types

- `ResizeDebouncer` (internal struct, never public)
- `ResizeAction` (internal enum, never public)
- `ResizeBehavior::Placeholder` variant

### Added Types (public via `ftui_runtime`)

- `ResizeCoalescer` -- regime-aware coalescing engine
- `CoalescerConfig` -- configuration with all knobs
- `CoalesceAction` -- action returned by coalescer
- `Regime` -- `Steady` or `Burst`
- `DecisionLog` -- per-decision audit record
- `DecisionSummary` -- aggregate statistics
- `CoalescerStats` -- runtime stats snapshot
- `CycleTimePercentiles` -- p50/p95/p99 latency

---

## 2. ProgramConfig Migration

### Old API

```rust
let config = ProgramConfig::default()
    .with_resize_debounce(Duration::from_millis(50))
    .with_resize_behavior(ResizeBehavior::Placeholder);
```

### New API

```rust
use ftui_runtime::{CoalescerConfig, ResizeBehavior};

let config = ProgramConfig::default()
    .with_resize_coalescer(CoalescerConfig {
        steady_delay_ms: 16,     // ~60fps responsiveness
        burst_delay_ms: 40,      // aggressive coalescing during storms
        hard_deadline_ms: 100,   // absolute max latency
        ..Default::default()
    })
    .with_resize_behavior(ResizeBehavior::Throttled);
```

### Quick Migration (Legacy Mode)

If you need immediate resize behavior during migration:

```rust
let config = ProgramConfig::default()
    .with_legacy_resize(true);
// Equivalent to ResizeBehavior::Immediate
```

---

## 3. AppBuilder Migration

### Old API

```rust
App::new(model)
    .resize_debounce(Duration::from_millis(50))
    .resize_behavior(ResizeBehavior::Placeholder)
    .run()?;
```

### New API

```rust
App::new(model)
    .resize_coalescer(CoalescerConfig::default())
    .resize_behavior(ResizeBehavior::Throttled)
    .run()?;
```

### Legacy Mode

```rust
App::new(model)
    .legacy_resize(true)
    .run()?;
```

---

## 4. CoalescerConfig Reference

All fields with their defaults and guidance:

| Field | Default | Unit | Purpose |
|-------|---------|------|---------|
| `steady_delay_ms` | 16 | ms | Target responsiveness in normal operation (~60fps) |
| `burst_delay_ms` | 40 | ms | Coalesce window during resize storms |
| `hard_deadline_ms` | 100 | ms | Absolute max latency (never exceeded) |
| `burst_enter_rate` | 10.0 | events/s | Rate threshold to enter Burst regime |
| `burst_exit_rate` | 5.0 | events/s | Rate threshold to exit Burst regime (hysteresis) |
| `cooldown_frames` | 3 | frames | Hold Burst mode this many frames after rate drops |
| `rate_window_size` | 8 | events | Sliding window for rate estimation |
| `enable_logging` | false | bool | Enable JSONL decision logging |

### Tuning Guidance

**Low-latency apps** (editor, terminal multiplexer):
```rust
CoalescerConfig {
    steady_delay_ms: 8,      // faster response
    burst_delay_ms: 25,      // less aggressive coalescing
    hard_deadline_ms: 50,    // tighter deadline
    ..Default::default()
}
```

**Heavy-render apps** (data visualization, dashboards):
```rust
CoalescerConfig {
    steady_delay_ms: 32,     // trade latency for throughput
    burst_delay_ms: 80,      // more aggressive coalescing
    hard_deadline_ms: 150,   // wider deadline
    burst_enter_rate: 5.0,   // enter burst sooner
    ..Default::default()
}
```

---

## 5. Regime Behavior

The coalescer adapts between two regimes with hysteresis:

```
                  rate >= burst_enter_rate
    ┌─────────┐ ──────────────────────────> ┌─────────┐
    │  Steady  │                             │  Burst   │
    │ (quick)  │ <────────────────────────── │ (coalesce)│
    └─────────┘   rate < burst_exit_rate     └─────────┘
                  AND cooldown expired
```

**Steady mode**: Resize events apply after `steady_delay_ms`. Optimized
for single resizes and slow manual window dragging.

**Burst mode**: Resize events coalesce up to `burst_delay_ms`. Optimized
for resize storms (tiling WM, programmatic resizing, rapid drag).

**Hard deadline**: Regardless of regime, a render always occurs within
`hard_deadline_ms` of the first pending resize event.

---

## 6. Observability

### Telemetry Hooks

```rust
use ftui_runtime::resize_coalescer::TelemetryHooks;

let hooks = TelemetryHooks::new()
    .on_resize_applied(|log| {
        tracing::info!(
            width = log.applied_size.unwrap().0,
            height = log.applied_size.unwrap().1,
            coalesce_ms = log.coalesce_ms.unwrap_or(0.0),
            "resize applied"
        );
    })
    .on_regime_change(|from, to| {
        tracing::info!(?from, ?to, "regime transition");
    })
    .with_tracing(true);

let coalescer = ResizeCoalescer::new(config, (80, 24))
    .with_telemetry_hooks(hooks);
```

### Decision Logging (JSONL)

Enable with `CoalescerConfig { enable_logging: true, .. }`.

```rust
// Export decision log for analysis
let jsonl = coalescer.evidence_to_jsonl();
std::fs::write("resize_decisions.jsonl", jsonl)?;

// Determinism check
let checksum = coalescer.decision_checksum_hex();
assert_eq!(checksum, expected_checksum);
```

### SLA Monitoring

```rust
use ftui_runtime::{ResizeSlaMonitor, SlaConfig, make_sla_hooks};

let sla = ResizeSlaMonitor::new(SlaConfig::default());
let hooks = make_sla_hooks(&sla);
// Attach hooks to coalescer for automatic SLA tracking
```

---

## 7. Compatibility Checklist

### Required Changes

- [ ] Replace `ResizeBehavior::Placeholder` with `ResizeBehavior::Throttled`
- [ ] Replace `with_resize_debounce(duration)` with `with_resize_coalescer(config)`
- [ ] Replace `resize_debounce(duration)` (AppBuilder) with `resize_coalescer(config)`
- [ ] Remove any references to `ResizeAction` (was internal, now removed)

### Optional Enhancements

- [ ] Add telemetry hooks for resize monitoring
- [ ] Enable decision logging for debugging resize issues
- [ ] Set up SLA monitoring via `ResizeSlaMonitor`
- [ ] Tune `CoalescerConfig` for your application profile
- [ ] Export decision checksums for determinism verification in tests

### Testing

- [ ] Verify resize behavior in Steady mode (single resize, slow drag)
- [ ] Verify resize behavior in Burst mode (rapid resize storm)
- [ ] Verify hard deadline is respected (resize within `hard_deadline_ms`)
- [ ] Verify regime transitions with hysteresis (no oscillation)
- [ ] Check input fairness during resize storms (keyboard/mouse not starved)

---

## 8. Default Behavior

With no configuration, the defaults provide good behavior for most apps:

```rust
// These are equivalent:
ProgramConfig::default()
// same as:
ProgramConfig::default()
    .with_resize_coalescer(CoalescerConfig::default())
    .with_resize_behavior(ResizeBehavior::Throttled)
```

Default behavior:
- 16ms steady-state response (~60fps)
- 40ms coalescing during storms
- 100ms hard deadline (worst-case latency)
- Burst detection at 10 events/sec with 5 events/sec exit hysteresis
- Input fairness guard prevents keyboard/mouse starvation during resize

---

## 9. Mathematical Guarantees

The coalescer provides formal guarantees documented in
[resize-scheduler.md](resize-scheduler.md):

1. **Latest-wins**: The final resize in any burst is never dropped.
2. **Bounded latency**: Any pending resize applies within `hard_deadline_ms`.
3. **Regime monotonicity**: Event rate determines transitions deterministically.
4. **Determinism**: Identical event sequences produce identical decisions
   (verifiable via `decision_checksum()`).

---

## See Also

- [Resize Scheduler Spec](resize-scheduler.md) -- formal model and decision rules
- [Telemetry](../telemetry.md) -- observability infrastructure
- [Migration Map](../migration-map.md) -- broader codebase migration context
