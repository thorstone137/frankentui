# Coverage Report (llvm-cov)

- Generated: 2026-02-06
- Command: `cargo llvm-cov --workspace --all-targets --all-features --summary-only --json --output-path /tmp/ftui_coverage_post.json`
- Overall (lines): **209,765 / 234,487 (89.46%)**
- Notes: Full workspace coverage run completed successfully; JSON report saved at `/tmp/ftui_coverage_post.json`.

## Coverage Summary (Lines)
Target policy: overall >= 70%; per-crate targets per `coverage-matrix.md`.

| Crate | Covered / Total | % | Target | Delta | Status |
| --- | ---: | ---: | ---: | ---: | :---: |
| `ftui` | 43/44 | 97.73% | n/a | n/a | n/a |
| `ftui-core` | 12,312/12,698 | 96.96% | 80% | +16.96 | PASS |
| `ftui-demo-showcase` | 48,075/59,099 | 81.35% | n/a | n/a | n/a |
| `ftui-extras` | 52,377/58,000 | 90.31% | 60% | +30.31 | PASS |
| `ftui-harness` | 5,756/7,235 | 79.56% | n/a | n/a | n/a |
| `ftui-i18n` | 598/635 | 94.17% | n/a | n/a | n/a |
| `ftui-layout` | 4,262/4,408 | 96.69% | 75% | +21.69 | PASS |
| `ftui-pty` | 2,567/3,016 | 85.11% | n/a | n/a | n/a |
| `ftui-render` | 13,834/14,509 | 95.35% | 80% | +15.35 | PASS |
| `ftui-runtime` | 24,434/26,491 | 92.24% | 75% | +17.24 | PASS |
| `ftui-style` | 3,974/4,348 | 91.40% | 80% | +11.40 | PASS |
| `ftui-text` | 8,113/8,374 | 96.88% | 80% | +16.88 | PASS |
| `ftui-widgets` | 33,420/35,630 | 93.80% | 70% | +23.80 | PASS |

## Lowest-Covered Files (Top 5 per Target Crate)
### `ftui-core`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-core/src/terminal_session.rs` | 511/557 | 91.74% |
| `/data/projects/frankentui/crates/ftui-core/src/caps_probe.rs` | 1,299/1,391 | 93.39% |
| `/data/projects/frankentui/crates/ftui-core/src/animation/presets.rs` | 341/363 | 93.94% |
| `/data/projects/frankentui/crates/ftui-core/src/lib.rs` | 338/359 | 94.15% |
| `/data/projects/frankentui/crates/ftui-core/src/animation/spring.rs` | 316/334 | 94.61% |

### `ftui-extras`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-extras/src/doom/palette.rs` | 46/79 | 58.23% |
| `/data/projects/frankentui/crates/ftui-extras/src/doom/wad.rs` | 218/371 | 58.76% |
| `/data/projects/frankentui/crates/ftui-extras/src/doom/framebuffer.rs` | 71/108 | 65.74% |
| `/data/projects/frankentui/crates/ftui-extras/src/doom/map.rs` | 311/473 | 65.75% |
| `/data/projects/frankentui/crates/ftui-extras/src/doom/tables.rs` | 46/62 | 74.19% |

### `ftui-layout`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-layout/src/debug.rs` | 711/778 | 91.39% |
| `/data/projects/frankentui/crates/ftui-layout/src/cache.rs` | 613/636 | 96.38% |
| `/data/projects/frankentui/crates/ftui-layout/src/lib.rs` | 1,424/1,475 | 96.54% |
| `/data/projects/frankentui/crates/ftui-layout/src/grid.rs` | 485/489 | 99.18% |
| `/data/projects/frankentui/crates/ftui-layout/src/responsive.rs` | 176/177 | 99.44% |

### `ftui-render`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-render/src/terminal_model.rs` | 798/915 | 87.21% |
| `/data/projects/frankentui/crates/ftui-render/src/diff.rs` | 1,865/2,025 | 92.10% |
| `/data/projects/frankentui/crates/ftui-render/src/diff_strategy.rs` | 539/579 | 93.09% |
| `/data/projects/frankentui/crates/ftui-render/src/lib.rs` | 209/224 | 93.30% |
| `/data/projects/frankentui/crates/ftui-render/src/frame.rs` | 618/662 | 93.35% |

### `ftui-runtime`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-runtime/src/undo/command.rs` | 515/668 | 77.10% |
| `/data/projects/frankentui/crates/ftui-runtime/src/program.rs` | 2,830/3,488 | 81.14% |
| `/data/projects/frankentui/crates/ftui-runtime/src/telemetry.rs` | 813/960 | 84.69% |
| `/data/projects/frankentui/crates/ftui-runtime/src/terminal_writer.rs` | 2,300/2,632 | 87.39% |
| `/data/projects/frankentui/crates/ftui-runtime/src/resize_coalescer.rs` | 1,495/1,688 | 88.57% |

### `ftui-style`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-style/src/table_theme.rs` | 1,530/1,874 | 81.64% |
| `/data/projects/frankentui/crates/ftui-style/src/color.rs` | 736/756 | 97.35% |
| `/data/projects/frankentui/crates/ftui-style/src/theme.rs` | 712/721 | 98.75% |
| `/data/projects/frankentui/crates/ftui-style/src/stylesheet.rs` | 414/415 | 99.76% |
| `/data/projects/frankentui/crates/ftui-style/src/lib.rs` | 66/66 | 100.00% |

### `ftui-text`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-text/src/markup.rs` | 460/521 | 88.29% |
| `/data/projects/frankentui/crates/ftui-text/src/search.rs` | 291/307 | 94.79% |
| `/data/projects/frankentui/crates/ftui-text/src/text.rs` | 1,061/1,116 | 95.07% |
| `/data/projects/frankentui/crates/ftui-text/src/width_cache.rs` | 1,167/1,215 | 96.05% |
| `/data/projects/frankentui/crates/ftui-text/src/normalization.rs` | 165/171 | 96.49% |

### `ftui-widgets`
| File | Covered / Total | % |
| --- | ---: | ---: |
| `/data/projects/frankentui/crates/ftui-widgets/src/keyboard_drag.rs` | 477/580 | 82.24% |
| `/data/projects/frankentui/crates/ftui-widgets/src/modal/animation.rs` | 493/595 | 82.86% |
| `/data/projects/frankentui/crates/ftui-widgets/src/modal/dialog.rs` | 523/618 | 84.63% |
| `/data/projects/frankentui/crates/ftui-widgets/src/textarea.rs` | 948/1,104 | 85.87% |
| `/data/projects/frankentui/crates/ftui-widgets/src/stateful.rs` | 399/464 | 85.99% |

## Follow-ups
- `ftui-extras` lowest-covered modules are currently in the Doom renderer (`crates/ftui-extras/src/doom/*`).
- `ftui-style/src/table_theme.rs` (81.64%) and `ftui-runtime/src/undo/command.rs` (77.10%) are the next-largest unit-test gaps among gated crates.
