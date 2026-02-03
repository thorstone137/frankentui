# Fixes Summary - Session 2026-02-03 (Part 23)

## 59. Markdown Link Rendering
**File:** `crates/ftui-extras/src/markdown.rs`
**Issue:** `MarkdownRenderer` was parsing links but discarding the destination URL, meaning `[text](url)` was rendered with link styling but no actual link functionality (OSC 8).
**Fix:**
    - Updated `StyleContext` to include `Link(String)` variant.
    - Updated `RenderState` to track the current link URL in the style stack.
    - Updated `text()` and `inline_code()` to apply the current link URL to generated `Span`s using the new `Span::link()` method.
    - Note: Verified `RenderState` updates correctly handle nested styles and link scopes.

## 60. Final Codebase State
All tasks are complete. The codebase has been extensively refactored for Unicode correctness, hardened for security/reliability, and enhanced with hyperlink support. No further issues detected in the sampled files.

## 61. Presenter Cost Model Overflow
**File:** `crates/ftui-render/src/presenter.rs`
**Issue:** `digit_count` function capped return value at 3 for any input >= 100. This caused incorrect cost estimation for terminal dimensions >= 1000, potentially leading to suboptimal cursor movement strategies on large displays (e.g. 4K).
**Fix:**
    - Extended `digit_count` to handle 4 and 5 digit numbers (up to `u16::MAX`).

## 62. TextInput Pinned Cursor Bug
**File:** `crates/ftui-widgets/src/input.rs`
**Issue:** `TextInput` failed to persist horizontal scroll state because `render` is immutable and `scroll_cells` was never updated. This caused the cursor to stick to the right edge during scrolling (no hysteresis).
**Fix:**
    - Changed `scroll_cells` to `std::cell::Cell<usize>` for interior mutability.
    - Updated `effective_scroll` to persist the calculated scroll position.

## 63. Inline Mode Ghosting/Flicker
**File:** `crates/ftui-runtime/src/terminal_writer.rs`
**Issue:** `present_inline` unconditionally cleared the UI region rows before emitting the diff. This wiped the screen content, causing partial diffs (which rely on previous content) to leave unchanged rows blank, resulting in flickering or disappearing UI.
**Fix:**
    - Removed the unconditional `clear_rows` block.
    - Added logic to safely clear only the remainder rows if the new buffer is shorter than the visible UI height.

## 64. TextArea Forward Scrolling
**File:** `crates/ftui-widgets/src/textarea.rs`
**Issue:** `ensure_cursor_visible` used a hardcoded heuristic (width 40, height 20) to clamp the scroll offset. This caused premature and incorrect horizontal scrolling on wide terminals (e.g., width > 40), effectively limiting the usable view width.
**Fix:**
    - Removed the heuristic forward-scrolling checks (max-side clamping) in `ensure_cursor_visible_with_height`.
    - Allowed the `render` method (which knows the actual viewport size) to handle forward scrolling adjustments naturally.

## 65. Table Partial Row Rendering
**File:** `crates/ftui-widgets/src/table.rs`
**Issue:** The rendering loop in `Table` contained a check that strictly required the *full* row height to fit within the remaining viewport height. If a row (especially a tall, multiline row) was only partially visible at the bottom of the table, it was skipped entirely, leaving a blank gap instead of showing the visible portion.
**Fix:**
    - Changed the loop termination condition to check if `y >= max_y` instead of pre-checking row fit.
    - Relied on `Frame` clipping to safely render partially visible rows.

## 66. Scrollbar Unicode Rendering
**File:** `crates/ftui-widgets/src/scrollbar.rs`
**Issue:** Symbols were rendered using `symbol.chars().next()`, which breaks multi-byte grapheme clusters (e.g., emoji with modifiers, complex symbols).
**Fix:**
    - Refactored the rendering logic to use the `draw_text_span` helper, which correctly handles grapheme clusters and composition. Added `draw_text_span` to the imports.

## 67. TextInput Horizontal Clipping
**File:** `crates/ftui-widgets/src/input.rs`
**Issue:** `TextInput` rendering logic incorrectly handled wide characters (e.g., CJK) at the scrolling boundaries.
    - **Left Edge:** Partially scrolled-out wide characters were incorrectly drawn at position 0.
    - **Right Edge:** Wide characters overlapping the right boundary spilled into the adjacent buffer area because `buffer.set` checks buffer bounds, not widget area bounds.
**Fix:**
    - Updated rendering loops to skip drawing graphemes that are partially scrolled out to the left or partially overlapping the right edge.
    - This ensures correct clipping and prevents drawing outside the widget's allocated area.

## 68. Buffer Dirty Initialization (Ghosting Fix)
**File:** `crates/ftui-render/src/buffer.rs`
**Issue:** `Buffer::new` initialized `dirty_rows` to `false`. When a new buffer (e.g. from resize) was diffed against an old buffer using `compute_dirty`, the diff algorithm would skip all rows (because they were "clean"), incorrectly assuming the new empty buffer matched the old populated buffer. This would cause "ghosting" where old content remained on screen after a resize or clear.
**Fix:**
    - Changed initialization of `dirty_rows` to `true` in `Buffer::new`. This ensures any fresh buffer is treated as fully changed relative to any previous state, forcing a correct full diff.

## 69. Zero-Width Char Cursor Desync
**File:** `crates/ftui-render/src/presenter.rs`
**Issue:** `emit_cell` did not account for zero-width characters (like standalone combining marks) in the buffer. Because `emit_content` writes bytes but `CellContent::width()` returns 0, the `Presenter`'s internal cursor state (`cursor_x`) would desynchronize from the actual terminal cursor (which doesn't advance for zero-width chars). This caused subsequent characters in the same row to be drawn at the wrong position (shifted left).
**Fix:**
    - Updated `emit_cell` to detect non-empty, non-continuation cells with zero width.
    - Replaced such content with `U+FFFD` (Replacement Character, width 1) to ensure the visual grid alignment is maintained and the cursor advances correctly.
    - Added a regression test `zero_width_chars_replaced_with_placeholder`.

## 70. Inline Mode Ghosting (Overlay Invalidation)
**File:** `crates/ftui-runtime/src/terminal_writer.rs`
**Issue:** In inline mode with overlay strategy (no scroll region), writing logs scrolls the screen, invalidating the previous UI position. However, `TerminalWriter` was not invalidating `prev_buffer`, causing the next frame's diff to assume the screen still contained the old UI. This led to ghosting where the renderer failed to redraw the UI on the new, empty rows created by scrolling.
**Fix:**
    - Updated `write_log` to invalidate `prev_buffer` and `last_inline_region` if `!scroll_region_active`.
    - Updated `present_inline` to explicitly clear the UI region rows when `prev_buffer` is `None` (full redraw). This restores correctness for invalidated states while maintaining flicker-free diffing for stable states (addressing the root cause of the regression from Fix #63).

## 71. Scrollbar Wide-Character Corruption
**File:** `crates/ftui-widgets/src/scrollbar.rs`
**Issue:** The `Scrollbar` widget's rendering loop iterated by cell index (`i`), drawing a symbol at each position. When using wide Unicode characters (e.g., emojis "🔴", "👍") for the track or thumb, drawing a symbol at index `i` would populate cells `i` and `i+1`. The subsequent iteration at `i+1` would then overwrite the "tail" of the previous wide character with a new "head", resulting in visual corruption.
**Fix:**
    - Modified the `render` method to conditionally skip iteration indices based on the drawn symbol's width and orientation:
        - **Horizontal:** The loop now skips `symbol_width` cells after drawing, preserving wide characters.
        - **Vertical:** The loop continues to increment by 1 (row), as wide characters stack vertically without overlapping.

## 72. Grapheme Pool Garbage Collection
**File:** `crates/ftui-runtime/src/terminal_writer.rs`, `crates/ftui-runtime/src/program.rs`
**Issue:** The `GraphemePool` used for interning complex characters (emoji, ZWJ sequences) never released its slots because `garbage_collect` was never called by the runtime. In long-running applications with streaming content (like logs with many unique emojis), this would lead to unbounded memory growth.
**Fix:**
    - Added a `gc()` method to `TerminalWriter` that performs mark-and-sweep using the previous frame's buffer as the live set.
    - Updated `Program::run_event_loop` to trigger `writer.gc()` periodically (every 1000 loop iterations) to reclaim unused grapheme slots.


