#![forbid(unsafe_code)]

//! Text handling for FrankenTUI.
//!
//! This crate provides text primitives for styled text rendering:
//! - [`Segment`] - atomic unit of styled text with cell-aware splitting
//! - [`SegmentLine`] - a line of segments
//! - [`SegmentLines`] - multi-line text
//! - [`Span`] - styled text span for ergonomic construction
//! - [`Line`] - a line of styled spans
//! - [`Text`] - multi-line styled text
//! - [`WidthCache`] - LRU cache for text width measurements
//!
//! # Example
//! ```
//! use ftui_text::{Segment, Text, Span, Line, WidthCache};
//! use ftui_style::Style;
//!
//! // Create styled segments (low-level)
//! let seg = Segment::styled("Error:", Style::new().bold());
//!
//! // Create styled text (high-level)
//! let text = Text::from_spans([
//!     Span::raw("Status: "),
//!     Span::styled("OK", Style::new().bold()),
//! ]);
//!
//! // Multi-line text
//! let text = Text::raw("line 1\nline 2\nline 3");
//! assert_eq!(text.height(), 3);
//!
//! // Truncate with ellipsis
//! let mut text = Text::raw("hello world");
//! text.truncate(8, Some("..."));
//! assert_eq!(text.to_plain_text(), "hello...");
//!
//! // Cache text widths for performance
//! let mut cache = WidthCache::new(1000);
//! let width = cache.get_or_compute("Hello, world!");
//! assert_eq!(width, 13);
//! ```

pub mod segment;
pub mod text;
pub mod width_cache;
pub mod wrap;

pub use segment::{ControlCode, Segment, SegmentLine, SegmentLines, join_lines, split_into_lines};
pub use text::{Line, Span, Text};
pub use width_cache::{CacheStats, DEFAULT_CACHE_CAPACITY, WidthCache};
pub use wrap::{
    display_width, has_wide_chars, is_ascii_only, truncate_to_width, truncate_with_ellipsis,
    wrap_text, wrap_with_options, WrapMode, WrapOptions,
};
