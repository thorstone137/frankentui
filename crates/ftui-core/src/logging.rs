#![forbid(unsafe_code)]

//! Logging and tracing support.
//!
//! This module provides re-exports of tracing macros when the `tracing` feature is enabled.
//! When the feature is disabled, no-op macros are provided for compatibility.

#[cfg(feature = "tracing")]
pub use tracing::{
    debug, debug_span, error, error_span, info, info_span, trace, trace_span, warn, warn_span,
};

// When tracing is not enabled, provide no-op macros
#[cfg(not(feature = "tracing"))]
mod noop_macros {
    /// No-op debug macro when tracing is disabled.
    #[macro_export]
    macro_rules! debug {
        ($($arg:tt)*) => {};
    }

    /// No-op debug_span macro when tracing is disabled.
    #[macro_export]
    macro_rules! debug_span {
        ($($arg:tt)*) => {
            $crate::logging::NoopSpan
        };
    }

    /// No-op error macro when tracing is disabled.
    #[macro_export]
    macro_rules! error {
        ($($arg:tt)*) => {};
    }

    /// No-op error_span macro when tracing is disabled.
    #[macro_export]
    macro_rules! error_span {
        ($($arg:tt)*) => {
            $crate::logging::NoopSpan
        };
    }

    /// No-op info macro when tracing is disabled.
    #[macro_export]
    macro_rules! info {
        ($($arg:tt)*) => {};
    }

    /// No-op info_span macro when tracing is disabled.
    #[macro_export]
    macro_rules! info_span {
        ($($arg:tt)*) => {
            $crate::logging::NoopSpan
        };
    }

    /// No-op trace macro when tracing is disabled.
    #[macro_export]
    macro_rules! trace {
        ($($arg:tt)*) => {};
    }

    /// No-op trace_span macro when tracing is disabled.
    #[macro_export]
    macro_rules! trace_span {
        ($($arg:tt)*) => {
            $crate::logging::NoopSpan
        };
    }

    /// No-op warn macro when tracing is disabled.
    #[macro_export]
    macro_rules! warn {
        ($($arg:tt)*) => {};
    }

    /// No-op warn_span macro when tracing is disabled.
    #[macro_export]
    macro_rules! warn_span {
        ($($arg:tt)*) => {
            $crate::logging::NoopSpan
        };
    }
}

// Note: Macros are exported at crate root via #[macro_export],
// so we don't need to re-export noop_macros::* here.

/// A no-op span guard for when tracing is disabled.
#[cfg(not(feature = "tracing"))]
pub struct NoopSpan;

#[cfg(not(feature = "tracing"))]
impl NoopSpan {
    /// Enter the no-op span (does nothing).
    pub fn enter(&self) -> NoopGuard {
        NoopGuard
    }
}

/// A no-op span guard.
#[cfg(not(feature = "tracing"))]
pub struct NoopGuard;
