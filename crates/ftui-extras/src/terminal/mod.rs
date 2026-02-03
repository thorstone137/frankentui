//! Terminal emulation components for embedded terminal widgets.
//!
//! This module provides ANSI escape sequence parsing and terminal state management
//! for building terminal emulator widgets.
//!
//! # Modules
//!
//! - [`parser`] - ANSI escape sequence parser using the `vte` crate.
//! - [`state`] - Terminal state machine (grid, cursor, scrollback).

pub mod parser;
pub mod state;

pub use parser::{AnsiHandler, AnsiParser};
pub use state::{
    Cell, CellAttrs, ClearRegion, Cursor, CursorShape, DirtyRegion, Grid, Pen, Scrollback,
    TerminalModes, TerminalState,
};
