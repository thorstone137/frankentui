#![forbid(unsafe_code)]

//! A scrolling log viewer widget optimized for streaming append-only content.
//!
//! `LogViewer` is THE essential widget for agent harness UIs. It displays streaming
//! logs with scrollback while maintaining UI chrome and handles:
//!
//! - High-frequency log line additions without flicker
//! - Auto-scroll behavior for "follow" mode
//! - Manual scrolling to inspect history
//! - Memory bounds via circular buffer eviction
//! - Substring filtering for log lines
//! - Text search with next/prev match navigation
//!
//! # Architecture
//!
//! LogViewer delegates storage and scroll state to [`Virtualized<Text>`], gaining
//! momentum scrolling, overscan, and page navigation for free. LogViewer adds
//! capacity management (eviction), wrapping, filtering, and search on top.
//!
//! # Example
//! ```ignore
//! use ftui_widgets::log_viewer::{LogViewer, LogViewerState, LogWrapMode};
//! use ftui_text::Text;
//!
//! // Create a viewer with 10,000 line capacity
//! let mut viewer = LogViewer::new(10_000);
//!
//! // Push log lines (styled or plain)
//! viewer.push("Starting process...");
//! viewer.push(Text::styled("ERROR: failed", Style::new().fg(Color::Red)));
//!
//! // Render with state
//! let mut state = LogViewerState::default();
//! viewer.render(area, frame, &mut state);
//! ```

use ftui_core::geometry::Rect;
use ftui_render::frame::Frame;
use ftui_style::Style;
use ftui_text::{Text, WrapMode, WrapOptions, display_width, wrap_with_options};

use crate::virtualized::Virtualized;
use crate::{StatefulWidget, draw_text_span, draw_text_span_with_link};

/// Line wrapping mode for log lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogWrapMode {
    /// No wrapping, truncate long lines.
    #[default]
    NoWrap,
    /// Wrap at any character boundary.
    CharWrap,
    /// Wrap at word boundaries (Unicode-aware).
    WordWrap,
}

impl From<LogWrapMode> for WrapMode {
    fn from(mode: LogWrapMode) -> Self {
        match mode {
            LogWrapMode::NoWrap => WrapMode::None,
            LogWrapMode::CharWrap => WrapMode::Char,
            LogWrapMode::WordWrap => WrapMode::Word,
        }
    }
}

/// Search state for text search within the log.
#[derive(Debug, Clone)]
struct SearchState {
    /// The search query string (retained for re-search after eviction).
    #[allow(dead_code)]
    query: String,
    /// Indices of matching lines.
    matches: Vec<usize>,
    /// Current match index within the matches vector.
    current: usize,
}

/// A scrolling log viewer optimized for streaming append-only content.
///
/// Internally uses [`Virtualized<Text>`] for storage and scroll management,
/// adding capacity enforcement, wrapping, filtering, and search on top.
///
/// # Design Rationale
/// - Virtualized handles scroll offset, follow mode, momentum, page navigation
/// - LogViewer adds max_lines eviction (Virtualized has no built-in capacity limit)
/// - Separate scroll semantics: Virtualized uses "offset from top"; LogViewer
///   exposes "follow mode" (newest at bottom) as the default behavior
/// - wrap_mode configurable per-instance for different use cases
/// - Stateful widget pattern for scroll state preservation across renders
#[derive(Debug, Clone)]
pub struct LogViewer {
    /// Virtualized storage with scroll state management.
    virt: Virtualized<Text>,
    /// Maximum lines to retain (memory bound).
    max_lines: usize,
    /// Line wrapping mode.
    wrap_mode: LogWrapMode,
    /// Default style for lines.
    style: Style,
    /// Highlight style for selected/focused line.
    highlight_style: Option<Style>,
    /// Active filter pattern (plain substring match).
    filter: Option<String>,
    /// Indices of lines matching the filter (None = show all).
    filtered_indices: Option<Vec<usize>>,
    /// Scroll offset within the filtered set (top index of filtered list).
    filtered_scroll_offset: usize,
    /// Active search state.
    search: Option<SearchState>,
}

/// Separate state for StatefulWidget pattern.
#[derive(Debug, Clone, Default)]
pub struct LogViewerState {
    /// Viewport height from last render (for page up/down).
    pub last_viewport_height: u16,
    /// Total visible line count from last render.
    pub last_visible_lines: usize,
    /// Selected line index (for copy/selection features).
    pub selected_line: Option<usize>,
}

impl LogViewer {
    /// Create a new LogViewer with specified max line capacity.
    ///
    /// # Arguments
    /// * `max_lines` - Maximum lines to retain. When exceeded, oldest lines
    ///   are evicted. Recommend 10,000-100,000 for typical agent use cases.
    #[must_use]
    pub fn new(max_lines: usize) -> Self {
        Self {
            virt: Virtualized::new(max_lines).with_follow(true),
            max_lines,
            wrap_mode: LogWrapMode::NoWrap,
            style: Style::default(),
            highlight_style: None,
            filter: None,
            filtered_indices: None,
            filtered_scroll_offset: 0,
            search: None,
        }
    }

    /// Set the wrap mode.
    #[must_use]
    pub fn wrap_mode(mut self, mode: LogWrapMode) -> Self {
        self.wrap_mode = mode;
        self
    }

    /// Set the default style for lines.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the highlight style for selected lines.
    #[must_use]
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = Some(style);
        self
    }

    /// Append a single log line.
    ///
    /// # Performance
    /// - O(1) amortized for append
    /// - O(1) for eviction when at capacity
    ///
    /// # Auto-scroll Behavior
    /// If follow mode is enabled, view stays at bottom after push.
    pub fn push(&mut self, line: impl Into<Text>) {
        let follow_filtered = self.filtered_indices.as_ref().is_some_and(|indices| {
            self.is_filtered_at_bottom(indices.len(), self.virt.visible_count())
        });
        let text: Text = line.into();

        // Split multi-line text into individual items for smooth scrolling
        for line in text.into_iter() {
            let item = Text::from_line(line);

            // Update filter index if active
            if let Some(filter) = self.filter.as_ref()
                && item.to_plain_text().contains(filter.as_str())
                && let Some(indices) = self.filtered_indices.as_mut()
            {
                let idx = self.virt.len();
                indices.push(idx);
            }

            self.virt.push(item);

            // Enforce capacity
            if self.virt.len() > self.max_lines {
                let removed = self.virt.trim_front(self.max_lines);

                // Adjust filtered indices
                if let Some(ref mut indices) = self.filtered_indices {
                    let mut filtered_removed = 0usize;
                    indices.retain_mut(|idx| {
                        if *idx < removed {
                            filtered_removed += 1;
                            false
                        } else {
                            *idx -= removed;
                            true
                        }
                    });
                    if filtered_removed > 0 {
                        self.filtered_scroll_offset =
                            self.filtered_scroll_offset.saturating_sub(filtered_removed);
                    }
                    if indices.is_empty() {
                        self.filtered_scroll_offset = 0;
                    }
                }

                // Adjust search match indices
                if let Some(ref mut search) = self.search {
                    search.matches.retain_mut(|idx| {
                        if *idx < removed {
                            false
                        } else {
                            *idx -= removed;
                            true
                        }
                    });
                    // Clamp current to valid range
                    if !search.matches.is_empty() {
                        search.current = search.current.min(search.matches.len() - 1);
                    }
                }
            }

            if follow_filtered
                && let Some(indices) = self.filtered_indices.as_ref()
                && !indices.is_empty()
            {
                self.filtered_scroll_offset = indices.len().saturating_sub(1);
            }
        }
    }

    /// Append multiple lines efficiently.
    pub fn push_many(&mut self, lines: impl IntoIterator<Item = impl Into<Text>>) {
        for line in lines {
            self.push(line);
        }
    }

    /// Scroll up by N lines. Disables follow mode.
    pub fn scroll_up(&mut self, lines: usize) {
        if self.filtered_indices.is_some() {
            self.filtered_scroll_offset = self.filtered_scroll_offset.saturating_sub(lines);
        } else {
            self.virt.scroll(-(lines as i32));
        }
    }

    /// Scroll down by N lines. Re-enables follow mode if at bottom.
    pub fn scroll_down(&mut self, lines: usize) {
        if let Some(filtered_total) = self.filtered_indices.as_ref().map(Vec::len) {
            if filtered_total == 0 {
                self.filtered_scroll_offset = 0;
            } else {
                self.filtered_scroll_offset = self.filtered_scroll_offset.saturating_add(lines);
                let max_offset = filtered_total.saturating_sub(1);
                if self.filtered_scroll_offset > max_offset {
                    self.filtered_scroll_offset = max_offset;
                }
            }
        } else {
            self.virt.scroll(lines as i32);
            if self.virt.is_at_bottom() {
                self.virt.set_follow(true);
            }
        }
    }

    /// Jump to top of log history.
    pub fn scroll_to_top(&mut self) {
        if self.filtered_indices.is_some() {
            self.filtered_scroll_offset = 0;
        } else {
            self.virt.scroll_to_top();
        }
    }

    /// Jump to bottom and re-enable follow mode.
    pub fn scroll_to_bottom(&mut self) {
        if let Some(filtered_total) = self.filtered_indices.as_ref().map(Vec::len) {
            if filtered_total == 0 {
                self.filtered_scroll_offset = 0;
            } else if self.virt.visible_count() > 0 {
                self.filtered_scroll_offset =
                    filtered_total.saturating_sub(self.virt.visible_count());
            } else {
                self.filtered_scroll_offset = filtered_total.saturating_sub(1);
            }
        } else {
            self.virt.scroll_to_end();
        }
    }

    /// Page up (scroll by viewport height).
    ///
    /// Uses the visible count tracked by the Virtualized container.
    /// The `state` parameter is accepted for API compatibility.
    pub fn page_up(&mut self, _state: &LogViewerState) {
        if self.filtered_indices.is_some() {
            let lines = _state.last_viewport_height as usize;
            if lines > 0 {
                self.scroll_up(lines);
            }
        } else {
            self.virt.page_up();
        }
    }

    /// Page down (scroll by viewport height).
    ///
    /// Uses the visible count tracked by the Virtualized container.
    /// The `state` parameter is accepted for API compatibility.
    pub fn page_down(&mut self, _state: &LogViewerState) {
        if self.filtered_indices.is_some() {
            let lines = _state.last_viewport_height as usize;
            if lines > 0 {
                self.scroll_down(lines);
            }
        } else {
            self.virt.page_down();
            if self.virt.is_at_bottom() {
                self.virt.set_follow(true);
            }
        }
    }

    /// Check if currently scrolled to the bottom.
    ///
    /// Returns `true` when follow mode is active (even before first render
    /// when the viewport size is unknown).
    #[must_use]
    pub fn is_at_bottom(&self) -> bool {
        if let Some(indices) = self.filtered_indices.as_ref() {
            self.is_filtered_at_bottom(indices.len(), self.virt.visible_count())
        } else {
            self.virt.follow_mode() || self.virt.is_at_bottom()
        }
    }

    /// Total line count in buffer.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.virt.len()
    }

    /// Check if follow mode (auto-scroll) is enabled.
    #[must_use]
    pub fn auto_scroll_enabled(&self) -> bool {
        self.virt.follow_mode()
    }

    /// Set follow mode (auto-scroll) state.
    pub fn set_auto_scroll(&mut self, enabled: bool) {
        self.virt.set_follow(enabled);
    }

    /// Toggle follow mode on/off.
    pub fn toggle_follow(&mut self) {
        let current = self.virt.follow_mode();
        self.virt.set_follow(!current);
    }

    /// Clear all lines.
    pub fn clear(&mut self) {
        self.virt.clear();
        self.filtered_indices = self.filter.as_ref().map(|_| Vec::new());
        self.filtered_scroll_offset = 0;
        self.search = None;
    }

    /// Set a filter pattern (plain substring match).
    ///
    /// Only lines containing the pattern will be shown. Pass `None` to clear.
    pub fn set_filter(&mut self, pattern: Option<&str>) {
        match pattern {
            Some(pat) if !pat.is_empty() => {
                // Rebuild filtered indices
                let mut indices = Vec::new();
                for idx in 0..self.virt.len() {
                    if let Some(item) = self.virt.get(idx)
                        && item.to_plain_text().contains(pat)
                    {
                        indices.push(idx);
                    }
                }
                self.filter = Some(pat.to_string());
                self.filtered_indices = Some(indices);
                self.filtered_scroll_offset = if let Some(indices) = self.filtered_indices.as_ref()
                {
                    if indices.is_empty() {
                        0
                    } else if self.virt.follow_mode() || self.virt.is_at_bottom() {
                        indices.len().saturating_sub(1)
                    } else {
                        let scroll_offset = self.virt.scroll_offset();
                        indices.partition_point(|&idx| idx < scroll_offset)
                    }
                } else {
                    0
                };
                self.search = None;
            }
            _ => {
                self.filter = None;
                self.filtered_indices = None;
                self.filtered_scroll_offset = 0;
                self.search = None;
            }
        }
    }

    /// Search for text and return match count.
    ///
    /// Sets up search state for navigation with `next_match` / `prev_match`.
    pub fn search(&mut self, query: &str) -> usize {
        if query.is_empty() {
            self.search = None;
            return 0;
        }

        let mut matches = Vec::new();
        if let Some(indices) = self.filtered_indices.as_ref() {
            for &idx in indices {
                if let Some(item) = self.virt.get(idx)
                    && item.to_plain_text().contains(query)
                {
                    matches.push(idx);
                }
            }
        } else {
            for idx in 0..self.virt.len() {
                if let Some(item) = self.virt.get(idx)
                    && item.to_plain_text().contains(query)
                {
                    matches.push(idx);
                }
            }
        }

        let count = matches.len();
        self.search = Some(SearchState {
            query: query.to_string(),
            matches,
            current: 0,
        });

        // Jump to first match
        if let Some(ref search) = self.search
            && let Some(&idx) = search.matches.first()
        {
            self.scroll_to_match(idx);
        }

        count
    }

    /// Jump to next search match.
    pub fn next_match(&mut self) {
        if let Some(ref mut search) = self.search
            && !search.matches.is_empty()
        {
            search.current = (search.current + 1) % search.matches.len();
            let idx = search.matches[search.current];
            self.scroll_to_match(idx);
        }
    }

    /// Jump to previous search match.
    pub fn prev_match(&mut self) {
        if let Some(ref mut search) = self.search
            && !search.matches.is_empty()
        {
            search.current = if search.current == 0 {
                search.matches.len() - 1
            } else {
                search.current - 1
            };
            let idx = search.matches[search.current];
            self.scroll_to_match(idx);
        }
    }

    /// Clear active search.
    pub fn clear_search(&mut self) {
        self.search = None;
    }

    /// Get current search match info: (current_match_1indexed, total_matches).
    #[must_use]
    pub fn search_info(&self) -> Option<(usize, usize)> {
        self.search.as_ref().and_then(|s| {
            if s.matches.is_empty() {
                None
            } else {
                Some((s.current + 1, s.matches.len()))
            }
        })
    }

    /// Render a single line with optional wrapping.
    #[allow(clippy::too_many_arguments)]
    fn render_line(
        &self,
        text: &Text,
        x: u16,
        y: u16,
        width: u16,
        max_y: u16,
        frame: &mut Frame,
        is_selected: bool,
    ) -> u16 {
        let effective_style = if is_selected {
            self.highlight_style.unwrap_or(self.style)
        } else {
            self.style
        };

        let line = text.lines().first();
        let content = text.to_plain_text();
        let content_width = display_width(&content);

        // Handle wrapping
        match self.wrap_mode {
            LogWrapMode::NoWrap => {
                // Truncate if needed
                if y < max_y {
                    self.draw_text_line(
                        line,
                        &content,
                        x,
                        y,
                        x.saturating_add(width),
                        frame,
                        effective_style,
                    );
                }
                1
            }
            LogWrapMode::CharWrap | LogWrapMode::WordWrap => {
                if content_width <= width as usize {
                    // No wrap needed
                    if y < max_y {
                        self.draw_text_line(
                            line,
                            &content,
                            x,
                            y,
                            x.saturating_add(width),
                            frame,
                            effective_style,
                        );
                    }
                    1
                } else {
                    // Wrap the line
                    let options = WrapOptions::new(width as usize).mode(self.wrap_mode.into());
                    let wrapped = wrap_with_options(&content, &options);
                    let mut lines_rendered = 0u16;

                    for (i, part) in wrapped.into_iter().enumerate() {
                        let line_y = y.saturating_add(i as u16);
                        if line_y >= max_y {
                            break;
                        }
                        draw_text_span(
                            frame,
                            x,
                            line_y,
                            &part,
                            effective_style,
                            x.saturating_add(width),
                        );
                        lines_rendered += 1;
                    }

                    lines_rendered.max(1)
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_line(
        &self,
        line: Option<&ftui_text::Line>,
        fallback: &str,
        x: u16,
        y: u16,
        max_x: u16,
        frame: &mut Frame,
        base_style: Style,
    ) {
        if let Some(line) = line {
            let mut cursor_x = x;
            for span in line.spans() {
                if cursor_x >= max_x {
                    break;
                }
                let span_style = span
                    .style
                    .map_or(base_style, |style| style.merge(&base_style));
                cursor_x = draw_text_span_with_link(
                    frame,
                    cursor_x,
                    y,
                    span.as_str(),
                    span_style,
                    max_x,
                    span.link.as_deref(),
                );
            }
        } else {
            draw_text_span(frame, x, y, fallback, base_style, max_x);
        }
    }

    fn scroll_to_match(&mut self, idx: usize) {
        if let Some(indices) = self.filtered_indices.as_ref() {
            let position = indices.partition_point(|&v| v < idx);
            self.filtered_scroll_offset = position.min(indices.len().saturating_sub(1));
        } else {
            self.virt.scroll_to(idx);
        }
    }

    fn is_filtered_at_bottom(&self, total: usize, visible_count: usize) -> bool {
        if total == 0 || visible_count == 0 {
            return true;
        }
        self.filtered_scroll_offset >= total.saturating_sub(visible_count)
    }
}

impl StatefulWidget for LogViewer {
    type State = LogViewerState;

    fn render(&self, area: Rect, frame: &mut Frame, state: &mut Self::State) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Update state with current viewport info
        state.last_viewport_height = area.height;

        let total_lines = self.virt.len();
        if total_lines == 0 {
            state.last_visible_lines = 0;
            return;
        }

        // Use filtered indices if a filter is active
        let render_indices: Option<&[usize]> = self.filtered_indices.as_deref();

        // Calculate visible range using Virtualized's scroll state
        let visible_count = area.height as usize;

        // Determine which lines to show
        let (start_idx, end_idx, at_bottom) = if let Some(indices) = render_indices {
            // Filtered mode: show lines matching the filter
            let filtered_total = indices.len();
            if filtered_total == 0 {
                state.last_visible_lines = 0;
                return;
            }
            // Clamp scroll to filtered set
            let max_offset = filtered_total.saturating_sub(visible_count);
            let offset = self.filtered_scroll_offset.min(max_offset);
            let start = offset;
            let end = (offset + visible_count).min(filtered_total);
            let is_bottom = offset >= max_offset;
            (start, end, is_bottom)
        } else {
            // Unfiltered mode: use Virtualized's range directly
            let range = self.virt.visible_range(area.height);
            (range.start, range.end, self.virt.is_at_bottom())
        };

        let mut y = area.y;
        let mut lines_rendered = 0;

        for display_idx in start_idx..end_idx {
            if y >= area.bottom() {
                break;
            }

            // Resolve to actual line index
            let line_idx = if let Some(indices) = render_indices {
                indices[display_idx]
            } else {
                display_idx
            };

            let Some(line) = self.virt.get(line_idx) else {
                continue;
            };

            let is_selected = state.selected_line == Some(line_idx);

            let lines_used = self.render_line(
                line,
                area.x,
                y,
                area.width,
                area.bottom(),
                frame,
                is_selected,
            );

            y = y.saturating_add(lines_used);
            lines_rendered += 1;
        }

        state.last_visible_lines = lines_rendered;

        // Render scroll indicator if not at bottom
        if !at_bottom && area.width >= 4 {
            let lines_below = if let Some(indices) = render_indices {
                indices.len().saturating_sub(end_idx)
            } else {
                total_lines.saturating_sub(end_idx)
            };
            let indicator = format!(" {} ", lines_below);
            let indicator_len = indicator.len() as u16;
            if indicator_len < area.width {
                let indicator_x = area.right().saturating_sub(indicator_len);
                let indicator_y = area.bottom().saturating_sub(1);
                draw_text_span(
                    frame,
                    indicator_x,
                    indicator_y,
                    &indicator,
                    Style::new().bold(),
                    area.right(),
                );
            }
        }

        // Render search indicator if active
        if let Some((current, total)) = self.search_info()
            && area.width >= 10
        {
            let search_indicator = format!(" {}/{} ", current, total);
            let ind_len = search_indicator.len() as u16;
            if ind_len < area.width {
                let ind_x = area.x;
                let ind_y = area.bottom().saturating_sub(1);
                draw_text_span(
                    frame,
                    ind_x,
                    ind_y,
                    &search_indicator,
                    Style::new().bold(),
                    ind_x.saturating_add(ind_len),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::grapheme_pool::GraphemePool;

    #[test]
    fn test_push_appends_to_end() {
        let mut log = LogViewer::new(100);
        log.push("line 1");
        log.push("line 2");
        assert_eq!(log.line_count(), 2);
    }

    #[test]
    fn test_circular_buffer_eviction() {
        let mut log = LogViewer::new(3);
        log.push("line 1");
        log.push("line 2");
        log.push("line 3");
        log.push("line 4"); // Should evict "line 1"
        assert_eq!(log.line_count(), 3);
    }

    #[test]
    fn test_auto_scroll_stays_at_bottom() {
        let mut log = LogViewer::new(100);
        log.push("line 1");
        assert!(log.is_at_bottom());
        log.push("line 2");
        assert!(log.is_at_bottom());
    }

    #[test]
    fn test_manual_scroll_disables_auto_scroll() {
        let mut log = LogViewer::new(100);
        log.virt.set_visible_count(10);
        for i in 0..50 {
            log.push(format!("line {}", i));
        }
        log.scroll_up(10);
        assert!(!log.auto_scroll_enabled());
        log.push("new line");
        assert!(!log.auto_scroll_enabled()); // Still scrolled up
    }

    #[test]
    fn test_scroll_to_bottom_reengages_auto_scroll() {
        let mut log = LogViewer::new(100);
        log.virt.set_visible_count(10);
        for i in 0..50 {
            log.push(format!("line {}", i));
        }
        log.scroll_up(10);
        log.scroll_to_bottom();
        assert!(log.is_at_bottom());
        assert!(log.auto_scroll_enabled());
    }

    #[test]
    fn test_scroll_down_reengages_at_bottom() {
        let mut log = LogViewer::new(100);
        log.virt.set_visible_count(10);
        for i in 0..50 {
            log.push(format!("line {}", i));
        }
        log.scroll_up(5);
        assert!(!log.auto_scroll_enabled());

        log.scroll_down(5);
        if log.is_at_bottom() {
            assert!(log.auto_scroll_enabled());
        }
    }

    #[test]
    fn test_scroll_to_top() {
        let mut log = LogViewer::new(100);
        for i in 0..50 {
            log.push(format!("line {}", i));
        }
        log.scroll_to_top();
        assert!(!log.auto_scroll_enabled());
    }

    #[test]
    fn test_page_up_down() {
        let mut log = LogViewer::new(100);
        log.virt.set_visible_count(10);
        for i in 0..50 {
            log.push(format!("line {}", i));
        }

        let state = LogViewerState {
            last_viewport_height: 10,
            ..Default::default()
        };

        assert!(log.is_at_bottom());

        log.page_up(&state);
        assert!(!log.is_at_bottom());

        log.page_down(&state);
        // After paging down from near-bottom, should be closer to bottom
    }

    #[test]
    fn test_clear() {
        let mut log = LogViewer::new(100);
        log.push("line 1");
        log.push("line 2");
        log.clear();
        assert_eq!(log.line_count(), 0);
    }

    #[test]
    fn test_push_many() {
        let mut log = LogViewer::new(100);
        log.push_many(["line 1", "line 2", "line 3"]);
        assert_eq!(log.line_count(), 3);
    }

    #[test]
    fn test_render_empty() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 24, &mut pool);
        let log = LogViewer::new(100);
        let mut state = LogViewerState::default();

        log.render(Rect::new(0, 0, 80, 24), &mut frame, &mut state);

        assert_eq!(state.last_visible_lines, 0);
    }

    #[test]
    fn test_render_some_lines() {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 10, &mut pool);
        let mut log = LogViewer::new(100);

        for i in 0..5 {
            log.push(format!("Line {}", i));
        }

        let mut state = LogViewerState::default();
        log.render(Rect::new(0, 0, 80, 10), &mut frame, &mut state);

        assert_eq!(state.last_viewport_height, 10);
        assert_eq!(state.last_visible_lines, 5);
    }

    #[test]
    fn test_toggle_follow() {
        let mut log = LogViewer::new(100);
        assert!(log.auto_scroll_enabled());
        log.toggle_follow();
        assert!(!log.auto_scroll_enabled());
        log.toggle_follow();
        assert!(log.auto_scroll_enabled());
    }

    #[test]
    fn test_filter_shows_matching_lines() {
        let mut log = LogViewer::new(100);
        log.push("INFO: starting");
        log.push("ERROR: something failed");
        log.push("INFO: processing");
        log.push("ERROR: another failure");
        log.push("INFO: done");

        log.set_filter(Some("ERROR"));
        assert_eq!(log.filtered_indices.as_ref().unwrap().len(), 2);

        // Clear filter
        log.set_filter(None);
        assert!(log.filtered_indices.is_none());
    }

    #[test]
    fn test_search_finds_matches() {
        let mut log = LogViewer::new(100);
        log.push("hello world");
        log.push("goodbye world");
        log.push("hello again");

        let count = log.search("hello");
        assert_eq!(count, 2);
        assert_eq!(log.search_info(), Some((1, 2)));
    }

    #[test]
    fn test_search_respects_filter() {
        let mut log = LogViewer::new(100);
        log.push("INFO: ok");
        log.push("ERROR: first");
        log.push("WARN: mid");
        log.push("ERROR: second");

        log.set_filter(Some("ERROR"));
        assert_eq!(log.search("WARN"), 0);
        assert_eq!(log.search("ERROR"), 2);
    }

    #[test]
    fn test_filter_clears_search() {
        let mut log = LogViewer::new(100);
        log.push("alpha");
        log.search("alpha");
        assert!(log.search_info().is_some());

        log.set_filter(Some("alpha"));
        assert!(log.search_info().is_none());
    }

    #[test]
    fn test_search_sets_filtered_scroll_offset() {
        let mut log = LogViewer::new(100);
        log.push("match one");
        log.push("line two");
        log.push("match three");
        log.push("match four");

        log.set_filter(Some("match"));
        log.search("match");

        assert_eq!(log.filtered_scroll_offset, 0);
        log.next_match();
        assert_eq!(log.filtered_scroll_offset, 1);
    }

    #[test]
    fn test_search_next_prev() {
        let mut log = LogViewer::new(100);
        log.push("match A");
        log.push("nothing here");
        log.push("match B");
        log.push("match C");

        log.search("match");
        assert_eq!(log.search_info(), Some((1, 3)));

        log.next_match();
        assert_eq!(log.search_info(), Some((2, 3)));

        log.next_match();
        assert_eq!(log.search_info(), Some((3, 3)));

        log.next_match(); // wraps around
        assert_eq!(log.search_info(), Some((1, 3)));

        log.prev_match(); // wraps back
        assert_eq!(log.search_info(), Some((3, 3)));
    }

    #[test]
    fn test_clear_search() {
        let mut log = LogViewer::new(100);
        log.push("hello");
        log.search("hello");
        assert!(log.search_info().is_some());

        log.clear_search();
        assert!(log.search_info().is_none());
    }

    #[test]
    fn test_filter_with_push() {
        let mut log = LogViewer::new(100);
        log.set_filter(Some("ERROR"));
        log.push("INFO: ok");
        log.push("ERROR: bad");
        log.push("INFO: fine");

        assert_eq!(log.filtered_indices.as_ref().unwrap().len(), 1);
        assert_eq!(log.filtered_indices.as_ref().unwrap()[0], 1);
    }

    #[test]
    fn test_eviction_adjusts_filter_indices() {
        let mut log = LogViewer::new(3);
        log.set_filter(Some("x"));
        log.push("x1");
        log.push("y2");
        log.push("x3");
        // At capacity: indices [0, 2]
        assert_eq!(log.filtered_indices.as_ref().unwrap(), &[0, 2]);

        log.push("y4"); // evicts "x1", indices should adjust
        // After eviction of 1 item: "x3" was at 2, now at 1
        assert_eq!(log.filtered_indices.as_ref().unwrap(), &[1]);
    }

    #[test]
    fn test_filter_scroll_offset_tracks_unfiltered_position() {
        let mut log = LogViewer::new(100);
        for i in 0..20 {
            if i == 2 || i == 10 || i == 15 {
                log.push(format!("match {}", i));
            } else {
                log.push(format!("line {}", i));
            }
        }

        log.virt.scroll_to(12);
        log.set_filter(Some("match"));

        // Matches before index 12 are at 2 and 10 -> offset should be 2.
        assert_eq!(log.filtered_scroll_offset, 2);
    }

    #[test]
    fn test_filtered_scroll_down_moves_within_filtered_list() {
        let mut log = LogViewer::new(100);
        log.push("match one");
        log.push("line two");
        log.push("match three");
        log.push("line four");
        log.push("match five");

        log.set_filter(Some("match"));
        log.scroll_to_top();
        log.scroll_down(1);

        assert_eq!(log.filtered_scroll_offset, 1);
    }
}
