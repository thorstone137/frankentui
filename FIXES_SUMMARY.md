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
**Issue:** The rendering loop strictly required the full row height to be visible. If a row (especially a tall multiline row) partially extended below the viewport, it was skipped entirely, leaving empty space at the bottom.
**Fix:**
    - Changed the loop termination condition to check if `y >= max_y` instead of pre-checking row fit.
    - Relied on `Frame` clipping to safely render partially visible rows.

## 66. Scrollbar Unicode Rendering
**File:** `crates/ftui-widgets/src/scrollbar.rs`
**Issue:** Symbols were rendered using `symbol.chars().next()`, which breaks multi-byte graphemes (e.g., emoji with modifiers, complex symbols).
**Fix:**
    - Replaced manual `Cell` construction with `draw_text_span`, which correctly handles grapheme clusters.