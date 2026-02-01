#![forbid(unsafe_code)]

//! Core: terminal lifecycle, capability detection, events, and input parsing.

pub mod cursor;
pub mod event;
pub mod inline_mode;
pub mod input_parser;
pub mod logging;
pub mod terminal_capabilities;
pub mod terminal_session;

// Re-export tracing macros at crate root for ergonomic use.
#[cfg(feature = "tracing")]
pub use logging::{
    debug, debug_span, error, error_span, info, info_span, trace, trace_span, warn, warn_span,
};
