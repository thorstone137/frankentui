# Coverage Gap Report

Generated: 2026-02-06
Tool: `cargo llvm-cov --workspace --all-targets --all-features --summary-only --json --output-path /tmp/ftui_coverage_post.json`
Overall line coverage: **209,765 / 234,487 (89.46%)**

## Executive Summary

- Full workspace coverage run completed successfully (no fallback to older baseline).
- All gated crates meet or exceed their configured thresholds.
- Only two files in gated crates remain below crate target: Doom modules in `ftui-extras`.
- The previous `program.rs` critical gap is closed: `crates/ftui-runtime/src/program.rs` is now **81.14%**.

## Per-Crate Target Compliance

| Crate | Target | Actual (lines) | Delta | Status |
|-------|--------|----------------|-------|--------|
| ftui-render | >= 80% | 13,834 / 14,509 (95.35%) | +15.35 | PASS |
| ftui-core | >= 80% | 12,312 / 12,698 (96.96%) | +16.96 | PASS |
| ftui-style | >= 80% | 3,974 / 4,348 (91.40%) | +11.40 | PASS |
| ftui-text | >= 80% | 8,113 / 8,374 (96.88%) | +16.88 | PASS |
| ftui-layout | >= 75% | 4,262 / 4,408 (96.69%) | +21.69 | PASS |
| ftui-runtime | >= 75% | 24,434 / 26,491 (92.24%) | +17.24 | PASS |
| ftui-widgets | >= 70% | 33,420 / 35,630 (93.80%) | +23.80 | PASS |
| ftui-extras | >= 60% | 52,377 / 58,000 (90.31%) | +30.31 | PASS |

## Priority Gap Map (Post-Run)

### Priority 0: Below-Target Files in Gated Crates

These are the only files currently below their crate target:

| File | Coverage | Crate Target | Gap |
|------|----------|--------------|-----|
| `crates/ftui-extras/src/doom/palette.rs` | 46 / 79 (58.23%) | 60% | -1.77 |
| `crates/ftui-extras/src/doom/wad.rs` | 218 / 371 (58.76%) | 60% | -1.24 |

Recommended tests:
- Doom palette table/lookup invariants, blend/clamp edge cases.
- WAD parse error paths, malformed lump indices, boundary offsets.

### Priority 1: Near-Floor Hotspots (<85%) in Gated Crates

| File | Coverage | Notes |
|------|----------|-------|
| `crates/ftui-runtime/src/undo/command.rs` | 515 / 668 (77.10%) | Lowest in runtime crate |
| `crates/ftui-runtime/src/program.rs` | 2,830 / 3,488 (81.14%) | Huge improvement; still worth deepening branch coverage |
| `crates/ftui-style/src/table_theme.rs` | 1,530 / 1,874 (81.64%) | Large surface with many behavior combinations |
| `crates/ftui-widgets/src/keyboard_drag.rs` | 477 / 580 (82.24%) | Input edge sequencing |
| `crates/ftui-widgets/src/modal/animation.rs` | 493 / 595 (82.86%) | Timing/transition paths |
| `crates/ftui-widgets/src/modal/dialog.rs` | 523 / 618 (84.63%) | Interaction branches |

### Priority 2: Non-Gated but High-Impact Runtime Surfaces

These do not gate CI thresholds but represent user-visible behavior:

| File | Coverage |
|------|----------|
| `crates/ftui-demo-showcase/src/screens/quake.rs` | 0 / 548 (0.00%) |
| `crates/ftui-demo-showcase/src/screens/markdown_live_editor.rs` | 0 / 574 (0.00%) |
| `crates/ftui-harness/src/main.rs` | 305 / 1,411 (21.62%) |
| `crates/ftui-demo-showcase/src/screens/shakespeare.rs` | 583 / 1,351 (43.15%) |
| `crates/ftui-demo-showcase/src/screens/visual_effects.rs` | 1,520 / 3,301 (46.05%) |

## Notes

- This report supersedes the stale 2026-02-02 baseline and its prior gap priorities.
- For full per-file breakdowns across target crates, see `docs/testing/coverage-report.md`.
- For CI threshold policy and rationale, see `docs/testing/coverage-matrix.md`.
