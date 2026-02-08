//! Convert ftui-render Buffers and BufferDiffs into WebGPU CellPatches.
//!
//! This module bridges the ftui rendering pipeline to the FrankenTerm WebGPU
//! renderer. The key entry points are:
//!
//! - [`cell_from_render`]: convert a single ftui-render `Cell` to a GPU `CellData`.
//! - [`diff_to_patches`]: convert a `BufferDiff` into contiguous `CellPatch` spans.
//! - [`full_buffer_patch`]: produce a single patch covering the entire buffer.
//!
//! The output patches are ready for `WebGpuRenderer::apply_patches()`.

use crate::renderer::{CellData, CellPatch};
use ftui_render::buffer::Buffer;
use ftui_render::cell::{Cell, CellContent};
use ftui_render::diff::BufferDiff;

/// Convert a single ftui-render `Cell` to a GPU-ready `CellData`.
///
/// Color mapping: `PackedRgba(u32)` is passed through directly (same
/// encoding: R in high byte, A in low byte).
///
/// Glyph ID: the Unicode codepoint is used directly for now. The glyph
/// atlas (bd-lff4p.2.4) will eventually provide a lookup.
///
/// Attributes: `StyleFlags` bits are passed through as-is.
#[must_use]
pub fn cell_from_render(cell: &Cell) -> CellData {
    let glyph_id = match cell.content {
        CellContent::EMPTY | CellContent::CONTINUATION => 0,
        other => other.as_char().map_or(0, |c| c as u32),
    };

    CellData {
        bg_rgba: cell.bg.0,
        fg_rgba: cell.fg.0,
        glyph_id,
        attrs: cell.attrs.flags().bits() as u32,
    }
}

/// Convert a `BufferDiff` into contiguous `CellPatch` spans for GPU upload.
///
/// Adjacent dirty cells are coalesced into a single patch to minimize
/// `queue.write_buffer` calls. The output is sorted by linear offset.
///
/// Returns an empty vec if the diff is empty.
#[must_use]
pub fn diff_to_patches(buffer: &Buffer, diff: &BufferDiff) -> Vec<CellPatch> {
    if diff.is_empty() {
        return Vec::new();
    }

    let cols = buffer.width();

    // Sort changes by linear offset for coalescing.
    let mut offsets: Vec<u32> = diff
        .changes()
        .iter()
        .map(|&(x, y)| y as u32 * cols as u32 + x as u32)
        .collect();
    offsets.sort_unstable();
    offsets.dedup();

    // Coalesce into contiguous spans.
    let mut patches = Vec::new();
    let mut span_start = offsets[0];
    let mut span_cells = vec![cell_at_offset(buffer, cols, span_start)];

    for &offset in &offsets[1..] {
        if offset == span_start + span_cells.len() as u32 {
            // Contiguous: extend current span.
            span_cells.push(cell_at_offset(buffer, cols, offset));
        } else {
            // Gap: flush current span and start a new one.
            patches.push(CellPatch {
                offset: span_start,
                cells: std::mem::take(&mut span_cells),
            });
            span_start = offset;
            span_cells.push(cell_at_offset(buffer, cols, offset));
        }
    }

    // Flush last span.
    if !span_cells.is_empty() {
        patches.push(CellPatch {
            offset: span_start,
            cells: span_cells,
        });
    }

    patches
}

/// Produce a single patch covering the entire buffer.
///
/// Used for full repaints (first frame, after resize, etc.).
#[must_use]
pub fn full_buffer_patch(buffer: &Buffer) -> CellPatch {
    let cols = buffer.width();
    let rows = buffer.height();
    let total = cols as usize * rows as usize;

    let mut cells = Vec::with_capacity(total);
    for y in 0..rows {
        for x in 0..cols {
            cells.push(match buffer.get(x, y) {
                Some(cell) => cell_from_render(cell),
                None => CellData::EMPTY,
            });
        }
    }

    CellPatch { offset: 0, cells }
}

fn cell_at_offset(buffer: &Buffer, cols: u16, offset: u32) -> CellData {
    let x = (offset % cols as u32) as u16;
    let y = (offset / cols as u32) as u16;
    match buffer.get(x, y) {
        Some(cell) => cell_from_render(cell),
        None => CellData::EMPTY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::cell::{CellAttrs, PackedRgba, StyleFlags};

    fn make_cell(ch: char, fg: u32, bg: u32, flags: StyleFlags) -> Cell {
        Cell {
            content: CellContent::from_char(ch),
            fg: PackedRgba(fg),
            bg: PackedRgba(bg),
            attrs: CellAttrs::new(flags, 0),
        }
    }

    #[test]
    fn cell_from_render_basic() {
        let cell = make_cell('A', 0xFFFFFFFF, 0x000000FF, StyleFlags::empty());
        let gpu = cell_from_render(&cell);
        assert_eq!(gpu.bg_rgba, 0x000000FF);
        assert_eq!(gpu.fg_rgba, 0xFFFFFFFF);
        assert_eq!(gpu.glyph_id, 'A' as u32);
        assert_eq!(gpu.attrs, 0);
    }

    #[test]
    fn cell_from_render_with_attrs() {
        let flags = StyleFlags::BOLD | StyleFlags::UNDERLINE;
        let cell = make_cell('X', 0xFF0000FF, 0x00FF00FF, flags);
        let gpu = cell_from_render(&cell);
        assert_eq!(gpu.attrs, flags.bits() as u32);
        assert_ne!(gpu.attrs, 0);
    }

    #[test]
    fn cell_from_render_empty() {
        let cell = Cell::default();
        let gpu = cell_from_render(&cell);
        assert_eq!(gpu.glyph_id, 0);
    }

    #[test]
    fn cell_from_render_continuation() {
        let cell = Cell::CONTINUATION;
        let gpu = cell_from_render(&cell);
        assert_eq!(gpu.glyph_id, 0);
    }

    #[test]
    fn full_buffer_patch_size() {
        let mut buf = Buffer::new(10, 5);
        buf.set_raw(0, 0, Cell::from_char('A'));

        let patch = full_buffer_patch(&buf);
        assert_eq!(patch.offset, 0);
        assert_eq!(patch.cells.len(), 50); // 10 * 5
        assert_eq!(patch.cells[0].glyph_id, 'A' as u32);
    }

    #[test]
    fn diff_to_patches_empty_diff() {
        let buf = Buffer::new(10, 5);
        let diff = BufferDiff::new();
        let patches = diff_to_patches(&buf, &diff);
        assert!(patches.is_empty());
    }

    #[test]
    fn diff_to_patches_single_change() {
        let old = Buffer::new(10, 5);
        let mut new = Buffer::new(10, 5);
        new.set_raw(3, 2, Cell::from_char('Z'));

        let diff = BufferDiff::compute(&old, &new);
        let patches = diff_to_patches(&new, &diff);

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].offset, 2 * 10 + 3); // row 2, col 3
        assert_eq!(patches[0].cells.len(), 1);
        assert_eq!(patches[0].cells[0].glyph_id, 'Z' as u32);
    }

    #[test]
    fn diff_to_patches_coalesces_adjacent() {
        let old = Buffer::new(10, 5);
        let mut new = Buffer::new(10, 5);
        // Set three consecutive cells in row 0.
        new.set_raw(2, 0, Cell::from_char('A'));
        new.set_raw(3, 0, Cell::from_char('B'));
        new.set_raw(4, 0, Cell::from_char('C'));

        let diff = BufferDiff::compute(&old, &new);
        let patches = diff_to_patches(&new, &diff);

        // Should coalesce into one span.
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].offset, 2);
        assert_eq!(patches[0].cells.len(), 3);
        assert_eq!(patches[0].cells[0].glyph_id, 'A' as u32);
        assert_eq!(patches[0].cells[1].glyph_id, 'B' as u32);
        assert_eq!(patches[0].cells[2].glyph_id, 'C' as u32);
    }

    #[test]
    fn diff_to_patches_separate_spans() {
        let old = Buffer::new(10, 5);
        let mut new = Buffer::new(10, 5);
        // Two non-adjacent changes.
        new.set_raw(1, 0, Cell::from_char('X'));
        new.set_raw(8, 0, Cell::from_char('Y'));

        let diff = BufferDiff::compute(&old, &new);
        let patches = diff_to_patches(&new, &diff);

        // Should produce two separate patches.
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].offset, 1);
        assert_eq!(patches[0].cells[0].glyph_id, 'X' as u32);
        assert_eq!(patches[1].offset, 8);
        assert_eq!(patches[1].cells[0].glyph_id, 'Y' as u32);
    }

    #[test]
    fn diff_to_patches_cross_row() {
        let old = Buffer::new(5, 3);
        let mut new = Buffer::new(5, 3);
        // Last cell of row 0 and first cell of row 1 (contiguous in linear layout).
        new.set_raw(4, 0, Cell::from_char('A'));
        new.set_raw(0, 1, Cell::from_char('B'));

        let diff = BufferDiff::compute(&old, &new);
        let patches = diff_to_patches(&new, &diff);

        // Offsets 4 and 5 are contiguous — should coalesce.
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].offset, 4);
        assert_eq!(patches[0].cells.len(), 2);
    }
}
