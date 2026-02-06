# Unit Test Coverage Matrix

This document encodes the project's expectations for unit test coverage by crate and module.
It prevents "test later" drift, keeps kernel invariants continuously verified, and makes CI
decisions explicit.

See Bead: bd-2nu8.17.

## How to Use
- When adding a new module, add it here.
- When adding a new public API, add explicit unit tests here.
- CI enforces these thresholds via the coverage gate.

## Coverage Targets (v1)
- Overall workspace: >= 70% (CI gate)
- ftui-render: >= 80% (kernel)
- ftui-core: >= 80% (terminal/session + input)
- ftui-style: >= 80%
- ftui-text: >= 80%
- ftui-layout: >= 75%
- ftui-runtime: >= 75%
- ftui-widgets: >= 70%
- ftui-extras: >= 60% (feature-gated)

Non-gated crates (report-only, no threshold): ftui, ftui-harness, ftui-demo-showcase, ftui-pty, ftui-simd.

Note: Integration-heavy PTY tests are enforced separately; do not "unit test" around reality.

## Last Measured: 2026-02-06 (cargo llvm-cov; full workspace)
- Command: `cargo llvm-cov --workspace --all-targets --all-features --summary-only --json --output-path /tmp/ftui_coverage_post.json`
- Overall (lines): **89.46%**
- Full breakdown: see `coverage-report.md`

| Crate | Target | Actual | Status |
|-------|--------|--------|--------|
| ftui-render | >= 80% | 95.35% | PASS |
| ftui-core | >= 80% | 96.96% | PASS |
| ftui-style | >= 80% | 91.40% | PASS |
| ftui-text | >= 80% | 96.88% | PASS |
| ftui-layout | >= 75% | 96.69% | PASS |
| ftui-runtime | >= 75% | 92.24% | PASS |
| ftui-widgets | >= 70% | 93.80% | PASS |
| ftui-extras | >= 60% | 90.31% | PASS |

## ftui-render (>= 80%)
Kernel correctness lives here.

### Cell / CellContent / CellAttrs
- [x] CellContent creation from char vs grapheme-id
- [x] Width semantics (ASCII, wide, combining, emoji)
- [x] Continuation-cell sentinel semantics for wide glyphs
- [x] PackedRgba: construction + Porter-Duff alpha blending
- [x] CellAttrs: bitflags operations + merge/override
- [x] 16-byte Cell layout invariants (size/alignment) + bits_eq correctness

### Buffer
- [x] Create/resize buffer with dimensions
- [x] get/set bounds checking + deterministic defaults
- [x] Clear semantics (full vs region)
- [x] Scissor stack push/pop semantics (intersection monotonicity)
- [x] Opacity stack push/pop semantics (product in [0,1])
- [x] Wide glyph placement + continuation cells
- [x] Iteration order and row-major storage assumptions

### Diff
- [x] Empty diff (no changes)
- [x] Single cell change
- [x] Row changes
- [x] Run grouping behavior
- [x] Scratch buffer reuse (no unbounded allocations)

### Presenter
- [x] Cursor tracking correctness
- [x] Style tracking correctness
- [x] Link tracking correctness (OSC 8 open/close)
- [x] Single-write-per-frame behavior
- [x] Synchronized output behavior where supported (fallback correctness)

### Other Modules
- ansi.rs
- budget.rs
- counting_writer.rs
- drawing.rs
- frame.rs
- grapheme_pool.rs
- headless.rs (test infrastructure)
- link_registry.rs
- sanitize.rs
- terminal_model.rs (test infrastructure)

## ftui-core (>= 80%)

### Event types
- [x] Canonical key/mouse/resize/paste/focus event types are stable

### InputParser
- [x] Bounded CSI/OSC/DCS parsing (DoS limits)
- [x] Bracketed paste decoding + max size
- [x] Mouse SGR decoding
- [x] Focus/resize event decoding

### TerminalCapabilities
- [x] Env heuristic detection (TERM/COLORTERM)
- [x] Mux flags (tmux/screen/zellij) correctness

### TerminalSession lifecycle
- [x] RAII enter/exit discipline
- [ ] Panic cleanup paths are idempotent — partial coverage via PTY tests; needs dedicated unit test

### Other Modules
- animation.rs
- cursor.rs
- event_coalescer.rs
- geometry.rs
- inline_mode.rs
- logging.rs
- mux_passthrough.rs

## ftui-style (>= 80%)
- [x] Style defaults + builder ergonomics (style.rs)
- [x] Deterministic style merge (explicit masks) (style.rs)
- [x] Color downgrade (truecolor -> 256 -> 16 -> mono) (color.rs)
- [x] Theme presets + semantic slots (theme.rs)
- [x] StyleSheet registry + named style composition (stylesheet.rs)

## ftui-text (>= 80%)
- [x] Segment system correctness (segment.rs)
- [x] Width measurement correctness + LRU cache behavior (width_cache.rs)
- [x] Grapheme segmentation helpers for wrap/truncate correctness (wrap.rs)
- [x] Wrap/truncate semantics for ZWJ/emoji/combining (wrap.rs + unicode corpus)
- [x] Markup parser correctness (feature-gated) (markup.rs)

## ftui-layout (>= 75%)
- [x] Rect operations (intersection/contains) (geometry.rs in ftui-core)
- [x] Flex constraint solving + gaps (lib.rs)
- [x] Grid placement + spanning + named areas (grid.rs)
- [x] Min/max sizing invariants (lib.rs)

## ftui-runtime (>= 75%)
- [x] Deterministic scheduling (update/view loop) (simulator.rs + program.rs)
- [x] Cmd sequencing + cancellation (program.rs via ProgramSimulator + PTY)
- [x] Subscription polling correctness (subscription.rs)

## ftui-widgets (>= 70%)
- [x] Harness-essential widgets have snapshot tests (renderable_snapshots.rs)
- [x] Widgets: key unit tests (render + layout invariants) (frame_integration.rs + per-module)
- Latest per-file coverage details: see `coverage-report.md`

## ftui-extras (>= 60%)
- [x] Feature-gated modules include correctness tests (measured with `--all-features`)
- Latest per-file coverage details: see `coverage-report.md`
