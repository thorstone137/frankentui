#![forbid(unsafe_code)]

//! Text wrapping with Unicode correctness.
//!
//! This module provides width-correct text wrapping that respects:
//! - Grapheme cluster boundaries (never break emoji, ZWJ sequences, etc.)
//! - Cell widths (CJK characters are 2 cells wide)
//! - Word boundaries when possible
//!
//! # Example
//! ```
//! use ftui_text::wrap::{wrap_text, WrapMode};
//!
//! // Word wrap
//! let lines = wrap_text("Hello world foo bar", 10, WrapMode::Word);
//! assert_eq!(lines, vec!["Hello", "world foo", "bar"]);
//!
//! // Character wrap (for long words)
//! let lines = wrap_text("Supercalifragilistic", 10, WrapMode::Char);
//! assert_eq!(lines.len(), 2);
//! ```

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Text wrapping mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapMode {
    /// No wrapping - lines may exceed width.
    None,
    /// Wrap at word boundaries when possible.
    #[default]
    Word,
    /// Wrap at character (grapheme) boundaries.
    Char,
    /// Word wrap with character fallback for long words.
    WordChar,
}

/// Options for text wrapping.
#[derive(Debug, Clone)]
pub struct WrapOptions {
    /// Maximum width in cells.
    pub width: usize,
    /// Wrapping mode.
    pub mode: WrapMode,
    /// Preserve leading whitespace on continued lines.
    pub preserve_indent: bool,
    /// Trim trailing whitespace from wrapped lines.
    pub trim_trailing: bool,
}

impl WrapOptions {
    /// Create new wrap options with the given width.
    #[must_use]
    pub fn new(width: usize) -> Self {
        Self {
            width,
            mode: WrapMode::Word,
            preserve_indent: false,
            trim_trailing: true,
        }
    }

    /// Set the wrap mode.
    #[must_use]
    pub fn mode(mut self, mode: WrapMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set whether to preserve indentation.
    #[must_use]
    pub fn preserve_indent(mut self, preserve: bool) -> Self {
        self.preserve_indent = preserve;
        self
    }

    /// Set whether to trim trailing whitespace.
    #[must_use]
    pub fn trim_trailing(mut self, trim: bool) -> Self {
        self.trim_trailing = trim;
        self
    }
}

impl Default for WrapOptions {
    fn default() -> Self {
        Self::new(80)
    }
}

/// Wrap text to the specified width.
///
/// This is a convenience function using default word-wrap mode.
#[must_use]
pub fn wrap_text(text: &str, width: usize, mode: WrapMode) -> Vec<String> {
    wrap_with_options(text, &WrapOptions::new(width).mode(mode))
}

/// Wrap text with full options.
#[must_use]
pub fn wrap_with_options(text: &str, options: &WrapOptions) -> Vec<String> {
    if options.width == 0 {
        return vec![text.to_string()];
    }

    match options.mode {
        WrapMode::None => vec![text.to_string()],
        WrapMode::Char => wrap_chars(text, options),
        WrapMode::Word => wrap_words(text, options, false),
        WrapMode::WordChar => wrap_words(text, options, true),
    }
}

/// Wrap at grapheme boundaries (character wrap).
fn wrap_chars(text: &str, options: &WrapOptions) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for grapheme in text.graphemes(true) {
        // Handle newlines
        if grapheme == "\n" {
            lines.push(finalize_line(&current_line, options));
            current_line.clear();
            current_width = 0;
            continue;
        }

        let grapheme_width = grapheme.width();

        // Check if this grapheme fits
        if current_width + grapheme_width > options.width && !current_line.is_empty() {
            lines.push(finalize_line(&current_line, options));
            current_line.clear();
            current_width = 0;
        }

        // Add grapheme to current line
        current_line.push_str(grapheme);
        current_width += grapheme_width;
    }

    // Don't forget the last line
    if !current_line.is_empty() || lines.is_empty() {
        lines.push(finalize_line(&current_line, options));
    }

    lines
}

/// Wrap at word boundaries.
fn wrap_words(text: &str, options: &WrapOptions, char_fallback: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    // Split by existing newlines first
    for paragraph in text.split('\n') {
        if !current_line.is_empty() {
            lines.push(finalize_line(&current_line, options));
            current_line.clear();
            current_width = 0;
        }

        wrap_paragraph(
            paragraph,
            options,
            char_fallback,
            &mut lines,
            &mut current_line,
            &mut current_width,
        );
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(finalize_line(&current_line, options));
    }

    // Ensure at least one line
    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Wrap a single paragraph (no embedded newlines).
fn wrap_paragraph(
    text: &str,
    options: &WrapOptions,
    char_fallback: bool,
    lines: &mut Vec<String>,
    current_line: &mut String,
    current_width: &mut usize,
) {
    for word in split_words(text) {
        let word_width = word.width();

        // If word fits on current line
        if *current_width + word_width <= options.width {
            current_line.push_str(&word);
            *current_width += word_width;
            continue;
        }

        // Word doesn't fit - need to wrap
        if !current_line.is_empty() {
            lines.push(finalize_line(current_line, options));
            current_line.clear();
            *current_width = 0;
        }

        // Check if word itself exceeds width
        if word_width > options.width {
            if char_fallback {
                // Break the long word into pieces
                wrap_long_word(&word, options, lines, current_line, current_width);
            } else {
                // Just put the long word on its own line
                lines.push(finalize_line(&word, options));
            }
        } else {
            // Word fits on a fresh line
            let trimmed = word.trim_start();
            current_line.push_str(trimmed);
            *current_width = trimmed.width();
        }
    }
}

/// Break a long word that exceeds the width limit.
fn wrap_long_word(
    word: &str,
    options: &WrapOptions,
    lines: &mut Vec<String>,
    current_line: &mut String,
    current_width: &mut usize,
) {
    for grapheme in word.graphemes(true) {
        let grapheme_width = grapheme.width();

        // Skip leading whitespace on new lines
        if *current_width == 0 && grapheme.trim().is_empty() {
            continue;
        }

        if *current_width + grapheme_width > options.width && !current_line.is_empty() {
            lines.push(finalize_line(current_line, options));
            current_line.clear();
            *current_width = 0;

            // Skip leading whitespace after wrap
            if grapheme.trim().is_empty() {
                continue;
            }
        }

        current_line.push_str(grapheme);
        *current_width += grapheme_width;
    }
}

/// Split text into words (preserving whitespace with words).
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for grapheme in text.graphemes(true) {
        if grapheme.chars().all(|c| c.is_whitespace()) {
            current.push_str(grapheme);
        } else {
            if current.chars().all(|c| c.is_whitespace()) && !current.is_empty() {
                // We have accumulated whitespace, and now hit a non-whitespace char
                // Start a new word that includes this whitespace
                current.push_str(grapheme);
            } else {
                current.push_str(grapheme);
            }
        }

        // End of word is transition from non-whitespace to whitespace
        // Actually, let's use a simpler approach: split on whitespace boundaries
    }

    if !current.is_empty() {
        words.push(current);
    }

    // Simpler approach: use unicode word boundaries
    words.clear();
    let mut current = String::new();
    let mut in_whitespace = false;

    for grapheme in text.graphemes(true) {
        let is_ws = grapheme.chars().all(|c| c.is_whitespace());

        if is_ws != in_whitespace && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }

        current.push_str(grapheme);
        in_whitespace = is_ws;
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

/// Finalize a line (apply trimming, etc.).
fn finalize_line(line: &str, options: &WrapOptions) -> String {
    if options.trim_trailing {
        line.trim_end().to_string()
    } else {
        line.to_string()
    }
}

/// Truncate text to fit within a width, adding ellipsis if needed.
///
/// This function respects grapheme boundaries - it will never break
/// an emoji, ZWJ sequence, or combining character sequence.
#[must_use]
pub fn truncate_with_ellipsis(text: &str, max_width: usize, ellipsis: &str) -> String {
    let text_width = text.width();

    if text_width <= max_width {
        return text.to_string();
    }

    let ellipsis_width = ellipsis.width();

    // If ellipsis alone exceeds width, just truncate without ellipsis
    if ellipsis_width >= max_width {
        return truncate_to_width(text, max_width);
    }

    let target_width = max_width - ellipsis_width;
    let mut result = truncate_to_width(text, target_width);
    result.push_str(ellipsis);
    result
}

/// Truncate text to exactly fit within a width (no ellipsis).
///
/// Respects grapheme boundaries.
#[must_use]
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut current_width = 0;

    for grapheme in text.graphemes(true) {
        let grapheme_width = grapheme.width();

        if current_width + grapheme_width > max_width {
            break;
        }

        result.push_str(grapheme);
        current_width += grapheme_width;
    }

    result
}

/// Calculate the display width of text in cells.
#[inline]
#[must_use]
pub fn display_width(text: &str) -> usize {
    text.width()
}

/// Check if a string contains any wide characters (width > 1).
#[must_use]
pub fn has_wide_chars(text: &str) -> bool {
    text.graphemes(true).any(|g| g.width() > 1)
}

/// Check if a string is ASCII-only (fast path possible).
#[must_use]
pub fn is_ascii_only(text: &str) -> bool {
    text.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // wrap_text tests
    // ==========================================================================

    #[test]
    fn wrap_text_no_wrap_needed() {
        let lines = wrap_text("hello", 10, WrapMode::Word);
        assert_eq!(lines, vec!["hello"]);
    }

    #[test]
    fn wrap_text_single_word_wrap() {
        let lines = wrap_text("hello world", 5, WrapMode::Word);
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn wrap_text_multiple_words() {
        let lines = wrap_text("hello world foo bar", 11, WrapMode::Word);
        assert_eq!(lines, vec!["hello world", "foo bar"]);
    }

    #[test]
    fn wrap_text_preserves_newlines() {
        let lines = wrap_text("line1\nline2", 20, WrapMode::Word);
        assert_eq!(lines, vec!["line1", "line2"]);
    }

    #[test]
    fn wrap_text_empty_string() {
        let lines = wrap_text("", 10, WrapMode::Word);
        assert_eq!(lines, vec![""]);
    }

    #[test]
    fn wrap_text_long_word_no_fallback() {
        let lines = wrap_text("supercalifragilistic", 10, WrapMode::Word);
        // Without fallback, long word stays on its own line
        assert_eq!(lines, vec!["supercalifragilistic"]);
    }

    #[test]
    fn wrap_text_long_word_with_fallback() {
        let lines = wrap_text("supercalifragilistic", 10, WrapMode::WordChar);
        // With fallback, long word is broken
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.width() <= 10);
        }
    }

    #[test]
    fn wrap_char_mode() {
        let lines = wrap_text("hello world", 5, WrapMode::Char);
        assert_eq!(lines, vec!["hello", " worl", "d"]);
    }

    #[test]
    fn wrap_none_mode() {
        let lines = wrap_text("hello world", 5, WrapMode::None);
        assert_eq!(lines, vec!["hello world"]);
    }

    // ==========================================================================
    // CJK wrapping tests
    // ==========================================================================

    #[test]
    fn wrap_cjk_respects_width() {
        // Each CJK char is 2 cells
        let lines = wrap_text("你好世界", 4, WrapMode::Char);
        assert_eq!(lines, vec!["你好", "世界"]);
    }

    #[test]
    fn wrap_cjk_odd_width() {
        // Width 5 can fit 2 CJK chars (4 cells)
        let lines = wrap_text("你好世", 5, WrapMode::Char);
        assert_eq!(lines, vec!["你好", "世"]);
    }

    #[test]
    fn wrap_mixed_ascii_cjk() {
        let lines = wrap_text("hi你好", 4, WrapMode::Char);
        assert_eq!(lines, vec!["hi你", "好"]);
    }

    // ==========================================================================
    // Emoji/ZWJ tests
    // ==========================================================================

    #[test]
    fn wrap_emoji_as_unit() {
        // Emoji should not be broken
        let lines = wrap_text("😀😀😀", 4, WrapMode::Char);
        // Each emoji is typically 2 cells, so 2 per line
        assert_eq!(lines.len(), 2);
        for line in &lines {
            // No partial emoji
            assert!(!line.contains("\\u"));
        }
    }

    #[test]
    fn wrap_zwj_sequence_as_unit() {
        // Family emoji (ZWJ sequence) - should stay together
        let text = "👨‍👩‍👧";
        let lines = wrap_text(text, 2, WrapMode::Char);
        // The ZWJ sequence should not be broken
        // It will exceed width but stay as one unit
        assert!(lines.iter().any(|l| l.contains("👨‍👩‍👧")));
    }

    // ==========================================================================
    // Truncation tests
    // ==========================================================================

    #[test]
    fn truncate_no_change_if_fits() {
        let result = truncate_with_ellipsis("hello", 10, "...");
        assert_eq!(result, "hello");
    }

    #[test]
    fn truncate_with_ellipsis_ascii() {
        let result = truncate_with_ellipsis("hello world", 8, "...");
        assert_eq!(result, "hello...");
    }

    #[test]
    fn truncate_cjk() {
        let result = truncate_with_ellipsis("你好世界", 6, "...");
        // 6 - 3 (ellipsis) = 3 cells for content
        // 你 = 2 cells fits, 好 = 2 cells doesn't fit
        assert_eq!(result, "你...");
    }

    #[test]
    fn truncate_to_width_basic() {
        let result = truncate_to_width("hello world", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn truncate_to_width_cjk() {
        let result = truncate_to_width("你好世界", 4);
        assert_eq!(result, "你好");
    }

    #[test]
    fn truncate_to_width_odd_boundary() {
        // Can't fit half a CJK char
        let result = truncate_to_width("你好", 3);
        assert_eq!(result, "你");
    }

    #[test]
    fn truncate_combining_chars() {
        // e + combining acute accent
        let text = "e\u{0301}test";
        let result = truncate_to_width(text, 2);
        // Should keep é together and add 't'
        assert_eq!(result.chars().count(), 3); // e + combining + t
    }

    // ==========================================================================
    // Helper function tests
    // ==========================================================================

    #[test]
    fn display_width_ascii() {
        assert_eq!(display_width("hello"), 5);
    }

    #[test]
    fn display_width_cjk() {
        assert_eq!(display_width("你好"), 4);
    }

    #[test]
    fn display_width_empty() {
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn has_wide_chars_true() {
        assert!(has_wide_chars("hi你好"));
    }

    #[test]
    fn has_wide_chars_false() {
        assert!(!has_wide_chars("hello"));
    }

    #[test]
    fn is_ascii_only_true() {
        assert!(is_ascii_only("hello world 123"));
    }

    #[test]
    fn is_ascii_only_false() {
        assert!(!is_ascii_only("héllo"));
    }

    // ==========================================================================
    // WrapOptions tests
    // ==========================================================================

    #[test]
    fn wrap_options_builder() {
        let opts = WrapOptions::new(40)
            .mode(WrapMode::Char)
            .preserve_indent(true)
            .trim_trailing(false);

        assert_eq!(opts.width, 40);
        assert_eq!(opts.mode, WrapMode::Char);
        assert!(opts.preserve_indent);
        assert!(!opts.trim_trailing);
    }

    #[test]
    fn wrap_options_trim_trailing() {
        let opts = WrapOptions::new(10).trim_trailing(true);
        let lines = wrap_with_options("hello   world", &opts);
        // Trailing spaces should be trimmed
        assert!(!lines.iter().any(|l| l.ends_with(' ')));
    }

    #[test]
    fn wrap_zero_width() {
        let lines = wrap_text("hello", 0, WrapMode::Word);
        // Zero width returns original text
        assert_eq!(lines, vec!["hello"]);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn wrapped_lines_never_exceed_width(s in "[a-zA-Z ]{1,100}", width in 5usize..50) {
            let lines = wrap_text(&s, width, WrapMode::Char);
            for line in &lines {
                prop_assert!(line.width() <= width, "Line '{}' exceeds width {}", line, width);
            }
        }

        #[test]
        fn wrapped_content_preserved(s in "[a-zA-Z]{1,50}", width in 5usize..20) {
            let lines = wrap_text(&s, width, WrapMode::Char);
            let rejoined: String = lines.join("");
            // Content should be preserved (though whitespace may change)
            prop_assert_eq!(s.replace(" ", ""), rejoined.replace(" ", ""));
        }

        #[test]
        fn truncate_never_exceeds_width(s in "[a-zA-Z0-9]{1,50}", width in 5usize..30) {
            let result = truncate_with_ellipsis(&s, width, "...");
            prop_assert!(result.width() <= width, "Result '{}' exceeds width {}", result, width);
        }

        #[test]
        fn truncate_to_width_exact(s in "[a-zA-Z]{1,50}", width in 1usize..30) {
            let result = truncate_to_width(&s, width);
            prop_assert!(result.width() <= width);
            // If original was longer, result should be at max width or close
            if s.width() > width {
                // Should be close to width (may be less due to wide char at boundary)
                prop_assert!(result.width() >= width.saturating_sub(1) || s.width() <= width);
            }
        }

        #[test]
        fn wordchar_mode_respects_width(s in "[a-zA-Z ]{1,100}", width in 5usize..30) {
            let lines = wrap_text(&s, width, WrapMode::WordChar);
            for line in &lines {
                prop_assert!(line.width() <= width, "Line '{}' exceeds width {}", line, width);
            }
        }
    }
}
