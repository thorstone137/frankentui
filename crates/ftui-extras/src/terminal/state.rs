//! Terminal state machine for tracking terminal content and cursor.
//!
//! This module provides a grid-based terminal state that can be updated
//! by parsing ANSI escape sequences via the [`AnsiHandler`] trait.
//!
//! # Invariants
//!
//! 1. **Cursor bounds**: Cursor position is always within grid bounds (0..width, 0..height).
//! 2. **Grid consistency**: Grid size always matches (width × height) cells.
//! 3. **Scrollback limit**: Scrollback never exceeds `max_scrollback` lines.
//!
//! # Failure Modes
//!
//! | Failure | Cause | Behavior |
//! |---------|-------|----------|
//! | Out of bounds | Invalid coordinates | Clamped to valid range |
//! | Zero size | Resize to 0x0 | Minimum 1x1 enforced |
//! | Scrollback overflow | Too many lines | Oldest lines dropped |

use std::collections::VecDeque;

use ftui_style::Color;

/// Terminal cell attributes (bitflags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellAttrs(u8);

impl CellAttrs {
    /// No attributes set.
    pub const NONE: Self = Self(0);
    /// Bold/bright.
    pub const BOLD: Self = Self(0b0000_0001);
    /// Dim/faint.
    pub const DIM: Self = Self(0b0000_0010);
    /// Italic.
    pub const ITALIC: Self = Self(0b0000_0100);
    /// Underline.
    pub const UNDERLINE: Self = Self(0b0000_1000);
    /// Blink.
    pub const BLINK: Self = Self(0b0001_0000);
    /// Reverse video.
    pub const REVERSE: Self = Self(0b0010_0000);
    /// Hidden/invisible.
    pub const HIDDEN: Self = Self(0b0100_0000);
    /// Strikethrough.
    pub const STRIKETHROUGH: Self = Self(0b1000_0000);

    /// Check if an attribute is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Set an attribute.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Clear an attribute.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Set or clear an attribute based on a boolean.
    #[must_use]
    pub const fn set(self, attr: Self, enabled: bool) -> Self {
        if enabled {
            self.with(attr)
        } else {
            self.without(attr)
        }
    }
}

/// A single terminal cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The character in this cell (space if empty).
    pub ch: char,
    /// Foreground color (None = default).
    pub fg: Option<Color>,
    /// Background color (None = default).
    pub bg: Option<Color>,
    /// Text attributes.
    pub attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            attrs: CellAttrs::NONE,
        }
    }
}

impl Cell {
    /// Create a new cell with the given character.
    #[must_use]
    pub const fn new(ch: char) -> Self {
        Self {
            ch,
            fg: None,
            bg: None,
            attrs: CellAttrs::NONE,
        }
    }

    /// Check if this cell is "empty" (space with default colors and no attrs).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ch == ' ' && self.fg.is_none() && self.bg.is_none() && self.attrs.0 == 0
    }
}

/// Cursor shape for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    /// Block cursor (default).
    #[default]
    Block,
    /// Underline cursor.
    Underline,
    /// Bar/beam cursor.
    Bar,
}

/// Cursor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Column (0-indexed).
    pub x: u16,
    /// Row (0-indexed).
    pub y: u16,
    /// Whether cursor is visible.
    pub visible: bool,
    /// Cursor shape.
    pub shape: CursorShape,
    /// Saved cursor position (DECSC/DECRC).
    pub saved: Option<(u16, u16)>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
            shape: CursorShape::Block,
            saved: None,
        }
    }
}

impl Cursor {
    /// Create a new cursor at the origin.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
            shape: CursorShape::Block,
            saved: None,
        }
    }
}

/// Terminal mode flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalModes(u32);

impl TerminalModes {
    /// Auto-wrap mode (DECAWM).
    pub const WRAP: Self = Self(0b0000_0001);
    /// Origin mode (DECOM).
    pub const ORIGIN: Self = Self(0b0000_0010);
    /// Insert mode (IRM).
    pub const INSERT: Self = Self(0b0000_0100);
    /// Cursor visible (DECTCEM).
    pub const CURSOR_VISIBLE: Self = Self(0b0000_1000);
    /// Alternate screen buffer.
    pub const ALT_SCREEN: Self = Self(0b0001_0000);
    /// Bracketed paste mode.
    pub const BRACKETED_PASTE: Self = Self(0b0010_0000);
    /// Mouse tracking enabled.
    pub const MOUSE_TRACKING: Self = Self(0b0100_0000);
    /// Focus events enabled.
    pub const FOCUS_EVENTS: Self = Self(0b1000_0000);

    /// Check if a mode is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Set a mode.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Clear a mode.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Set or clear a mode based on a boolean.
    #[must_use]
    pub const fn set(self, mode: Self, enabled: bool) -> Self {
        if enabled {
            self.with(mode)
        } else {
            self.without(mode)
        }
    }
}

/// Region to clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearRegion {
    /// Clear from cursor to end of screen.
    CursorToEnd,
    /// Clear from start of screen to cursor.
    StartToCursor,
    /// Clear entire screen.
    All,
    /// Clear from cursor to end of line.
    LineFromCursor,
    /// Clear from start of line to cursor.
    LineToCursor,
    /// Clear entire line.
    Line,
}

/// Dirty region tracking.
///
/// Uses a bitmap for efficient tracking of which cells have changed.
#[derive(Debug, Clone)]
pub struct DirtyRegion {
    /// Bitmap: 1 bit per cell, row-major order.
    bits: Vec<u64>,
    /// Width of the grid.
    width: u16,
    /// Height of the grid.
    height: u16,
    /// Whether any cell is dirty.
    any_dirty: bool,
}

impl DirtyRegion {
    /// Create a new dirty region for the given dimensions.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let total_cells = (width as usize) * (height as usize);
        let num_words = total_cells.div_ceil(64);
        Self {
            bits: vec![0; num_words],
            width,
            height,
            any_dirty: false,
        }
    }

    /// Mark a cell as dirty.
    pub fn mark(&mut self, x: u16, y: u16) {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            let word = idx / 64;
            let bit = idx % 64;
            self.bits[word] |= 1 << bit;
            self.any_dirty = true;
        }
    }

    /// Mark a rectangular region as dirty.
    pub fn mark_rect(&mut self, x: u16, y: u16, w: u16, h: u16) {
        for row in y..y.saturating_add(h).min(self.height) {
            for col in x..x.saturating_add(w).min(self.width) {
                self.mark(col, row);
            }
        }
    }

    /// Mark the entire grid as dirty.
    pub fn mark_all(&mut self) {
        self.bits.fill(u64::MAX);
        self.any_dirty = true;
    }

    /// Check if a cell is dirty.
    #[must_use]
    pub fn is_dirty(&self, x: u16, y: u16) -> bool {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            let word = idx / 64;
            let bit = idx % 64;
            (self.bits[word] >> bit) & 1 == 1
        } else {
            false
        }
    }

    /// Check if any cell is dirty.
    #[must_use]
    pub fn has_dirty(&self) -> bool {
        self.any_dirty
    }

    /// Clear all dirty flags.
    pub fn clear(&mut self) {
        self.bits.fill(0);
        self.any_dirty = false;
    }

    /// Resize the dirty region (clears all flags).
    pub fn resize(&mut self, width: u16, height: u16) {
        let total_cells = (width as usize) * (height as usize);
        let num_words = total_cells.div_ceil(64);
        self.bits.resize(num_words, 0);
        self.bits.fill(0);
        self.width = width;
        self.height = height;
        self.any_dirty = false;
    }
}

/// Terminal grid (visible area).
#[derive(Debug, Clone)]
pub struct Grid {
    /// Cells in row-major order.
    cells: Vec<Cell>,
    /// Width in columns.
    width: u16,
    /// Height in rows.
    height: u16,
}

impl Grid {
    /// Create a new grid with the given dimensions.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let size = (width as usize) * (height as usize);
        Self {
            cells: vec![Cell::default(); size],
            width,
            height,
        }
    }

    /// Get grid width.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Get grid height.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// Get a reference to a cell.
    #[must_use]
    pub fn cell(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.cells.get(idx)
        } else {
            None
        }
    }

    /// Get a mutable reference to a cell.
    pub fn cell_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.cells.get_mut(idx)
        } else {
            None
        }
    }

    /// Clear a row to default cells.
    pub fn clear_row(&mut self, y: u16) {
        if y < self.height {
            let start = (y as usize) * (self.width as usize);
            let end = start + (self.width as usize);
            for cell in &mut self.cells[start..end] {
                *cell = Cell::default();
            }
        }
    }

    /// Resize the grid, preserving content where possible.
    pub fn resize(&mut self, new_width: u16, new_height: u16) {
        let new_width = new_width.max(1);
        let new_height = new_height.max(1);

        if new_width == self.width && new_height == self.height {
            return;
        }

        let mut new_cells = vec![Cell::default(); (new_width as usize) * (new_height as usize)];

        // Copy existing content
        let copy_width = self.width.min(new_width) as usize;
        let copy_height = self.height.min(new_height) as usize;

        for y in 0..copy_height {
            let old_start = y * (self.width as usize);
            let new_start = y * (new_width as usize);
            new_cells[new_start..new_start + copy_width]
                .copy_from_slice(&self.cells[old_start..old_start + copy_width]);
        }

        self.cells = new_cells;
        self.width = new_width;
        self.height = new_height;
    }

    /// Scroll the grid up by n lines, filling bottom with empty lines.
    /// Returns the lines that scrolled off the top.
    pub fn scroll_up(&mut self, n: u16) -> Vec<Vec<Cell>> {
        let n = n.min(self.height) as usize;
        if n == 0 {
            return Vec::new();
        }

        let mut scrolled_off = Vec::with_capacity(n);

        // Collect lines that will scroll off
        for y in 0..n {
            let start = y * (self.width as usize);
            let end = start + (self.width as usize);
            scrolled_off.push(self.cells[start..end].to_vec());
        }

        // Shift remaining lines up
        let shift_count = (self.height as usize - n) * (self.width as usize);
        self.cells.copy_within(n * (self.width as usize).., 0);

        // Clear bottom lines
        for cell in &mut self.cells[shift_count..] {
            *cell = Cell::default();
        }

        scrolled_off
    }

    /// Scroll the grid down by n lines, filling top with empty lines.
    pub fn scroll_down(&mut self, n: u16) {
        let n = n.min(self.height) as usize;
        if n == 0 {
            return;
        }

        let width = self.width as usize;
        let height = self.height as usize;

        // Shift lines down
        self.cells.copy_within(0..(height - n) * width, n * width);

        // Clear top lines
        for cell in &mut self.cells[0..n * width] {
            *cell = Cell::default();
        }
    }
}

/// Scrollback buffer.
#[derive(Debug, Clone)]
pub struct Scrollback {
    /// Lines in the scrollback buffer.
    lines: VecDeque<Vec<Cell>>,
    /// Maximum number of lines to keep.
    max_lines: usize,
}

impl Scrollback {
    /// Create a new scrollback buffer.
    #[must_use]
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines,
        }
    }

    /// Add a line to the scrollback.
    pub fn push(&mut self, line: Vec<Cell>) {
        if self.max_lines == 0 {
            return;
        }

        while self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Add multiple lines to the scrollback.
    pub fn push_many(&mut self, lines: impl IntoIterator<Item = Vec<Cell>>) {
        for line in lines {
            self.push(line);
        }
    }

    /// Get the number of lines in scrollback.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Check if scrollback is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Get a line from scrollback (0 = most recent).
    #[must_use]
    pub fn line(&self, index: usize) -> Option<&[Cell]> {
        if index < self.lines.len() {
            self.lines
                .get(self.lines.len() - 1 - index)
                .map(Vec::as_slice)
        } else {
            None
        }
    }

    /// Clear the scrollback.
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

/// Current pen state for writing characters.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pen {
    /// Foreground color.
    pub fg: Option<Color>,
    /// Background color.
    pub bg: Option<Color>,
    /// Text attributes.
    pub attrs: CellAttrs,
}

impl Pen {
    /// Reset pen to defaults.
    pub fn reset(&mut self) {
        self.fg = None;
        self.bg = None;
        self.attrs = CellAttrs::NONE;
    }
}

/// Complete terminal state.
#[derive(Debug, Clone)]
pub struct TerminalState {
    /// The visible grid.
    grid: Grid,
    /// Cursor state.
    cursor: Cursor,
    /// Scrollback buffer.
    scrollback: Scrollback,
    /// Terminal modes.
    modes: TerminalModes,
    /// Dirty region tracking.
    dirty: DirtyRegion,
    /// Current pen for new characters.
    pen: Pen,
    /// Scroll region (top, bottom) - 0-indexed, inclusive.
    scroll_region: (u16, u16),
    /// Window title (from OSC sequences).
    title: String,
}

impl TerminalState {
    /// Create a new terminal state with the given dimensions.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            grid: Grid::new(width, height),
            cursor: Cursor::new(),
            scrollback: Scrollback::new(1000), // Default 1000 lines
            modes: TerminalModes::WRAP.with(TerminalModes::CURSOR_VISIBLE),
            dirty: DirtyRegion::new(width, height),
            pen: Pen::default(),
            scroll_region: (0, height.saturating_sub(1)),
            title: String::new(),
        }
    }

    /// Create with custom scrollback limit.
    #[must_use]
    pub fn with_scrollback(width: u16, height: u16, max_scrollback: usize) -> Self {
        let mut state = Self::new(width, height);
        state.scrollback = Scrollback::new(max_scrollback);
        state
    }

    /// Get grid width.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.grid.width()
    }

    /// Get grid height.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.grid.height()
    }

    /// Get a reference to the grid.
    #[must_use]
    pub const fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Get a reference to a cell.
    #[must_use]
    pub fn cell(&self, x: u16, y: u16) -> Option<&Cell> {
        self.grid.cell(x, y)
    }

    /// Get a mutable reference to a cell (marks as dirty).
    pub fn cell_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        self.dirty.mark(x, y);
        self.grid.cell_mut(x, y)
    }

    /// Get cursor state.
    #[must_use]
    pub const fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Get terminal modes.
    #[must_use]
    pub const fn modes(&self) -> TerminalModes {
        self.modes
    }

    /// Get the dirty region.
    #[must_use]
    pub const fn dirty(&self) -> &DirtyRegion {
        &self.dirty
    }

    /// Get the scrollback buffer.
    #[must_use]
    pub const fn scrollback(&self) -> &Scrollback {
        &self.scrollback
    }

    /// Get the current pen.
    #[must_use]
    pub const fn pen(&self) -> &Pen {
        &self.pen
    }

    /// Get the window title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Mark all cells as clean.
    pub fn mark_clean(&mut self) {
        self.dirty.clear();
    }

    /// Move cursor to absolute position (clamped to bounds).
    pub fn move_cursor(&mut self, x: u16, y: u16) {
        self.cursor.x = x.min(self.grid.width().saturating_sub(1));
        self.cursor.y = y.min(self.grid.height().saturating_sub(1));
    }

    /// Move cursor relative to current position.
    pub fn move_cursor_relative(&mut self, dx: i16, dy: i16) {
        let new_x = (self.cursor.x as i32 + dx as i32)
            .max(0)
            .min(self.grid.width() as i32 - 1) as u16;
        let new_y = (self.cursor.y as i32 + dy as i32)
            .max(0)
            .min(self.grid.height() as i32 - 1) as u16;
        self.cursor.x = new_x;
        self.cursor.y = new_y;
    }

    /// Set cursor visibility.
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor.visible = visible;
        self.modes = self.modes.set(TerminalModes::CURSOR_VISIBLE, visible);
    }

    /// Save cursor position.
    pub fn save_cursor(&mut self) {
        self.cursor.saved = Some((self.cursor.x, self.cursor.y));
    }

    /// Restore cursor position.
    pub fn restore_cursor(&mut self) {
        if let Some((x, y)) = self.cursor.saved {
            self.move_cursor(x, y);
        }
    }

    /// Write a character at the cursor position.
    pub fn put_char(&mut self, ch: char) {
        let x = self.cursor.x;
        let y = self.cursor.y;

        if let Some(cell) = self.grid.cell_mut(x, y) {
            cell.ch = ch;
            cell.fg = self.pen.fg;
            cell.bg = self.pen.bg;
            cell.attrs = self.pen.attrs;
            self.dirty.mark(x, y);
        }

        // Advance cursor
        self.cursor.x += 1;

        // Handle wrap
        if self.cursor.x >= self.grid.width() {
            if self.modes.contains(TerminalModes::WRAP) {
                self.cursor.x = 0;
                self.cursor.y += 1;

                // Scroll if needed
                if self.cursor.y > self.scroll_region.1 {
                    self.scroll_up(1);
                    self.cursor.y = self.scroll_region.1;
                }
            } else {
                self.cursor.x = self.grid.width().saturating_sub(1);
            }
        }
    }

    /// Scroll the screen up by n lines.
    pub fn scroll_up(&mut self, n: u16) {
        let (top, bottom) = self.scroll_region;
        let n = n.min(bottom.saturating_sub(top) + 1);

        if n == 0 {
            return;
        }

        // If scrolling the entire screen, use grid method
        if top == 0 && bottom == self.grid.height().saturating_sub(1) {
            let scrolled_off = self.grid.scroll_up(n);
            self.scrollback.push_many(scrolled_off);
        } else {
            // Scroll within region
            let width = self.grid.width() as usize;
            for y in top..=bottom.saturating_sub(n) {
                let src_y = y + n;
                if src_y <= bottom {
                    // Copy row src_y to row y
                    let src_start = (src_y as usize) * width;
                    let dst_start = (y as usize) * width;
                    self.grid
                        .cells
                        .copy_within(src_start..src_start + width, dst_start);
                }
            }
            // Clear bottom lines of region
            for y in (bottom + 1).saturating_sub(n)..=bottom {
                self.grid.clear_row(y);
            }
        }

        // Mark entire scroll region as dirty
        self.dirty
            .mark_rect(0, top, self.grid.width(), bottom - top + 1);
    }

    /// Scroll the screen down by n lines.
    pub fn scroll_down(&mut self, n: u16) {
        let (top, bottom) = self.scroll_region;
        let n = n.min(bottom.saturating_sub(top) + 1);

        if n == 0 {
            return;
        }

        if top == 0 && bottom == self.grid.height().saturating_sub(1) {
            self.grid.scroll_down(n);
        } else {
            // Scroll within region
            let width = self.grid.width() as usize;
            for y in (top + n..=bottom).rev() {
                let src_y = y - n;
                if src_y >= top {
                    // Copy row src_y to row y
                    let src_start = (src_y as usize) * width;
                    let dst_start = (y as usize) * width;
                    self.grid
                        .cells
                        .copy_within(src_start..src_start + width, dst_start);
                }
            }
            // Clear top lines of region
            for y in top..top + n {
                self.grid.clear_row(y);
            }
        }

        self.dirty
            .mark_rect(0, top, self.grid.width(), bottom - top + 1);
    }

    /// Clear a region of the screen.
    pub fn clear_region(&mut self, region: ClearRegion) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let width = self.grid.width();
        let height = self.grid.height();

        match region {
            ClearRegion::CursorToEnd => {
                // Clear from cursor to end of line
                for col in x..width {
                    if let Some(cell) = self.grid.cell_mut(col, y) {
                        *cell = Cell::default();
                        self.dirty.mark(col, y);
                    }
                }
                // Clear remaining lines
                for row in y + 1..height {
                    self.grid.clear_row(row);
                }
                self.dirty.mark_rect(0, y + 1, width, height - y - 1);
            }
            ClearRegion::StartToCursor => {
                // Clear lines before cursor
                for row in 0..y {
                    self.grid.clear_row(row);
                }
                self.dirty.mark_rect(0, 0, width, y);
                // Clear from start of line to cursor
                for col in 0..=x {
                    if let Some(cell) = self.grid.cell_mut(col, y) {
                        *cell = Cell::default();
                        self.dirty.mark(col, y);
                    }
                }
            }
            ClearRegion::All => {
                for row in 0..height {
                    self.grid.clear_row(row);
                }
                self.dirty.mark_all();
            }
            ClearRegion::LineFromCursor => {
                for col in x..width {
                    if let Some(cell) = self.grid.cell_mut(col, y) {
                        *cell = Cell::default();
                        self.dirty.mark(col, y);
                    }
                }
            }
            ClearRegion::LineToCursor => {
                for col in 0..=x {
                    if let Some(cell) = self.grid.cell_mut(col, y) {
                        *cell = Cell::default();
                        self.dirty.mark(col, y);
                    }
                }
            }
            ClearRegion::Line => {
                self.grid.clear_row(y);
                self.dirty.mark_rect(0, y, width, 1);
            }
        }
    }

    /// Set a terminal mode.
    pub fn set_mode(&mut self, mode: TerminalModes, enabled: bool) {
        self.modes = self.modes.set(mode, enabled);
    }

    /// Set the scroll region.
    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let top = top.min(self.grid.height().saturating_sub(1));
        let bottom = bottom.min(self.grid.height().saturating_sub(1)).max(top);
        self.scroll_region = (top, bottom);
    }

    /// Reset scroll region to full screen.
    pub fn reset_scroll_region(&mut self) {
        self.scroll_region = (0, self.grid.height().saturating_sub(1));
    }

    /// Set the window title.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Resize the terminal.
    pub fn resize(&mut self, width: u16, height: u16) {
        let width = width.max(1);
        let height = height.max(1);

        self.grid.resize(width, height);
        self.dirty.resize(width, height);
        self.dirty.mark_all();

        // Clamp cursor
        self.cursor.x = self.cursor.x.min(width.saturating_sub(1));
        self.cursor.y = self.cursor.y.min(height.saturating_sub(1));

        // Reset scroll region
        self.scroll_region = (0, height.saturating_sub(1));
    }

    /// Get a mutable reference to the pen.
    pub fn pen_mut(&mut self) -> &mut Pen {
        &mut self.pen
    }

    /// Reset the terminal to initial state.
    pub fn reset(&mut self) {
        let width = self.grid.width();
        let height = self.grid.height();

        self.grid = Grid::new(width, height);
        self.cursor = Cursor::new();
        self.modes = TerminalModes::WRAP.with(TerminalModes::CURSOR_VISIBLE);
        self.pen = Pen::default();
        self.scroll_region = (0, height.saturating_sub(1));
        self.title.clear();
        self.dirty.clear();
        self.dirty.mark_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_default() {
        let cell = Cell::default();
        assert_eq!(cell.ch, ' ');
        assert!(cell.fg.is_none());
        assert!(cell.bg.is_none());
        assert_eq!(cell.attrs.0, 0);
        assert!(cell.is_empty());
    }

    #[test]
    fn test_cell_attrs() {
        let attrs = CellAttrs::BOLD.with(CellAttrs::ITALIC);
        assert!(attrs.contains(CellAttrs::BOLD));
        assert!(attrs.contains(CellAttrs::ITALIC));
        assert!(!attrs.contains(CellAttrs::UNDERLINE));

        let attrs = attrs.without(CellAttrs::BOLD);
        assert!(!attrs.contains(CellAttrs::BOLD));
        assert!(attrs.contains(CellAttrs::ITALIC));
    }

    #[test]
    fn test_cursor_movement() {
        let mut state = TerminalState::new(80, 24);
        assert_eq!(state.cursor().x, 0);
        assert_eq!(state.cursor().y, 0);

        state.move_cursor(10, 5);
        assert_eq!(state.cursor().x, 10);
        assert_eq!(state.cursor().y, 5);

        // Test clamping
        state.move_cursor(100, 50);
        assert_eq!(state.cursor().x, 79);
        assert_eq!(state.cursor().y, 23);
    }

    #[test]
    fn test_cursor_relative_movement() {
        let mut state = TerminalState::new(80, 24);
        state.move_cursor(10, 10);

        state.move_cursor_relative(-5, 3);
        assert_eq!(state.cursor().x, 5);
        assert_eq!(state.cursor().y, 13);

        // Test clamping at boundaries
        state.move_cursor_relative(-100, -100);
        assert_eq!(state.cursor().x, 0);
        assert_eq!(state.cursor().y, 0);
    }

    #[test]
    fn test_put_char() {
        let mut state = TerminalState::new(80, 24);
        state.put_char('A');

        assert_eq!(state.cell(0, 0).unwrap().ch, 'A');
        assert_eq!(state.cursor().x, 1);
        assert!(state.dirty().is_dirty(0, 0));
    }

    #[test]
    fn test_scroll_up() {
        let mut state = TerminalState::new(10, 5);

        // Fill first line with 'A's
        for i in 0..10 {
            state.move_cursor(i, 0);
            state.put_char('A');
        }

        // Fill second line with 'B's
        for i in 0..10 {
            state.move_cursor(i, 1);
            state.put_char('B');
        }

        state.scroll_up(1);

        // First line should now have 'B's
        assert_eq!(state.cell(0, 0).unwrap().ch, 'B');
        // Scrollback should have 'A's
        assert_eq!(state.scrollback().line(0).unwrap()[0].ch, 'A');
    }

    #[test]
    fn test_scroll_down() {
        let mut state = TerminalState::new(10, 5);

        // Fill first line with 'A's
        for i in 0..10 {
            state.move_cursor(i, 0);
            state.put_char('A');
        }

        state.scroll_down(1);

        // First line should be empty
        assert_eq!(state.cell(0, 0).unwrap().ch, ' ');
        // Second line should have 'A's
        assert_eq!(state.cell(0, 1).unwrap().ch, 'A');
    }

    #[test]
    fn test_wrap_mode() {
        let mut state = TerminalState::new(5, 3);
        assert!(state.modes().contains(TerminalModes::WRAP));

        // Write past end of line
        for ch in "HELLO WORLD".chars() {
            state.put_char(ch);
        }

        // Should have wrapped
        assert_eq!(state.cell(0, 0).unwrap().ch, 'H');
        assert_eq!(state.cell(4, 0).unwrap().ch, 'O');
        assert_eq!(state.cell(0, 1).unwrap().ch, ' ');
        assert_eq!(state.cell(0, 2).unwrap().ch, 'D');
    }

    #[test]
    fn test_resize() {
        let mut state = TerminalState::new(10, 5);
        state.move_cursor(5, 3);
        state.put_char('X');

        state.resize(20, 10);

        assert_eq!(state.width(), 20);
        assert_eq!(state.height(), 10);
        // Content should be preserved
        assert_eq!(state.cell(5, 3).unwrap().ch, 'X');
    }

    #[test]
    fn test_resize_smaller() {
        let mut state = TerminalState::new(10, 5);
        state.move_cursor(8, 4);

        state.resize(5, 3);

        // Cursor should be clamped
        assert_eq!(state.cursor().x, 4);
        assert_eq!(state.cursor().y, 2);
    }

    #[test]
    fn test_dirty_tracking() {
        let mut state = TerminalState::new(10, 5);
        assert!(!state.dirty().has_dirty());

        state.put_char('A');
        assert!(state.dirty().has_dirty());
        assert!(state.dirty().is_dirty(0, 0));
        assert!(!state.dirty().is_dirty(1, 0));

        state.mark_clean();
        assert!(!state.dirty().has_dirty());
    }

    #[test]
    fn test_clear_region_all() {
        let mut state = TerminalState::new(10, 5);

        // Fill with content
        for i in 0..10 {
            state.move_cursor(i, 0);
            state.put_char('A');
        }

        state.clear_region(ClearRegion::All);

        // All cells should be empty
        for y in 0..5 {
            for x in 0..10 {
                assert!(state.cell(x, y).unwrap().is_empty());
            }
        }
    }

    #[test]
    fn test_clear_region_line() {
        let mut state = TerminalState::new(10, 5);

        // Fill line 2 with content
        for i in 0..10 {
            state.move_cursor(i, 2);
            state.put_char('A');
        }

        state.move_cursor(5, 2);
        state.clear_region(ClearRegion::Line);

        // Line 2 should be empty
        for x in 0..10 {
            assert!(state.cell(x, 2).unwrap().is_empty());
        }
    }

    #[test]
    fn test_save_restore_cursor() {
        let mut state = TerminalState::new(80, 24);
        state.move_cursor(10, 5);
        state.save_cursor();

        state.move_cursor(50, 20);
        assert_eq!(state.cursor().x, 50);

        state.restore_cursor();
        assert_eq!(state.cursor().x, 10);
        assert_eq!(state.cursor().y, 5);
    }

    #[test]
    fn test_scroll_region() {
        let mut state = TerminalState::new(10, 10);
        state.set_scroll_region(2, 7);

        // Fill line 2 with 'A's
        for i in 0..10 {
            state.move_cursor(i, 2);
            state.put_char('A');
        }

        // Scroll within region
        state.scroll_up(1);

        // Line 2 should now be empty (line 3 moved up)
        // Actually, let me check: line 3 content moved to line 2
        // Since line 3 was empty, line 2 should now be empty
        // But line 2 had 'A's, they should have scrolled into scrollback
        // No wait, scroll region doesn't go to scrollback if not at top

        // Reset and test properly
        state.reset();
        state.set_scroll_region(2, 7);

        for i in 0..10 {
            state.move_cursor(i, 2);
            state.put_char('A');
        }
        for i in 0..10 {
            state.move_cursor(i, 3);
            state.put_char('B');
        }

        state.scroll_up(1);

        // Line 2 should now have 'B's (from line 3)
        assert_eq!(state.cell(0, 2).unwrap().ch, 'B');
        // Line 7 should be cleared
        assert!(state.cell(0, 7).unwrap().is_empty());
    }

    #[test]
    fn test_scrollback() {
        let mut state = TerminalState::with_scrollback(10, 3, 10);

        // Disable wrap to prevent auto-scroll when filling last column
        state.set_mode(TerminalModes::WRAP, false);

        // Fill all lines
        for y in 0..3 {
            for x in 0..10 {
                state.move_cursor(x, y);
                state.put_char(char::from(b'A' + y as u8));
            }
        }

        // Scroll up
        state.scroll_up(1);

        // Check scrollback has the 'A' line
        assert_eq!(state.scrollback().len(), 1);
        assert_eq!(state.scrollback().line(0).unwrap()[0].ch, 'A');
    }

    #[test]
    fn test_pen_attributes() {
        let mut state = TerminalState::new(10, 5);

        state.pen_mut().attrs = CellAttrs::BOLD;
        state.pen_mut().fg = Some(Color::rgb(255, 0, 0));
        state.put_char('X');

        let cell = state.cell(0, 0).unwrap();
        assert!(cell.attrs.contains(CellAttrs::BOLD));
        assert_eq!(cell.fg, Some(Color::rgb(255, 0, 0)));
    }

    #[test]
    fn test_terminal_modes() {
        let modes = TerminalModes::WRAP.with(TerminalModes::CURSOR_VISIBLE);
        assert!(modes.contains(TerminalModes::WRAP));
        assert!(modes.contains(TerminalModes::CURSOR_VISIBLE));
        assert!(!modes.contains(TerminalModes::ALT_SCREEN));

        let modes = modes.set(TerminalModes::ALT_SCREEN, true);
        assert!(modes.contains(TerminalModes::ALT_SCREEN));

        let modes = modes.without(TerminalModes::WRAP);
        assert!(!modes.contains(TerminalModes::WRAP));
    }

    #[test]
    fn test_grid_resize_preserves_content() {
        let mut grid = Grid::new(10, 5);

        // Put 'X' at position (3, 2)
        if let Some(cell) = grid.cell_mut(3, 2) {
            cell.ch = 'X';
        }

        grid.resize(20, 10);

        assert_eq!(grid.cell(3, 2).unwrap().ch, 'X');
    }

    #[test]
    fn test_minimum_size() {
        let state = TerminalState::new(0, 0);
        assert_eq!(state.width(), 1);
        assert_eq!(state.height(), 1);
    }

    #[test]
    fn test_reset() {
        let mut state = TerminalState::new(10, 5);
        state.move_cursor(5, 3);
        state.put_char('X');
        state.set_title("Test");

        state.reset();

        assert_eq!(state.cursor().x, 0);
        assert_eq!(state.cursor().y, 0);
        assert!(state.cell(5, 3).unwrap().is_empty());
        assert!(state.title().is_empty());
    }
}
