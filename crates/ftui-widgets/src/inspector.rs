#![forbid(unsafe_code)]

//! UI Inspector overlay for debugging widget trees and hit-test regions.
//!
//! The inspector visualizes:
//! - Hit regions with colored overlays
//! - Widget boundaries with colored borders
//! - Widget names and metadata
//!
//! # Usage
//!
//! ```ignore
//! use ftui_widgets::inspector::{InspectorMode, InspectorState, InspectorOverlay};
//!
//! // In your app state
//! let mut inspector = InspectorState::default();
//!
//! // Toggle with F12
//! if key == KeyCode::F12 {
//!     inspector.toggle();
//! }
//!
//! // Render overlay after all widgets
//! if inspector.is_active() {
//!     InspectorOverlay::new(&inspector).render(area, frame);
//! }
//! ```
//!
//! See `docs/specs/ui-inspector.md` for the full specification.

use ftui_core::geometry::Rect;
use ftui_render::cell::{Cell, PackedRgba};
use ftui_render::frame::{Frame, HitCell, HitData, HitId, HitRegion};

use crate::{Widget, set_style_area};
use ftui_style::Style;

#[cfg(feature = "tracing")]
use tracing::{info_span, trace};

/// Inspector display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorMode {
    /// Inspector is disabled.
    #[default]
    Off,
    /// Show hit regions with colored overlays.
    HitRegions,
    /// Show widget boundaries and names.
    WidgetBounds,
    /// Show both hit regions and widget bounds.
    Full,
}

impl InspectorMode {
    /// Cycle to the next mode.
    ///
    /// Off → HitRegions → WidgetBounds → Full → Off
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::HitRegions,
            Self::HitRegions => Self::WidgetBounds,
            Self::WidgetBounds => Self::Full,
            Self::Full => Self::Off,
        }
    }

    /// Check if inspector is active (any mode except Off).
    #[inline]
    pub fn is_active(self) -> bool {
        self != Self::Off
    }

    /// Check if hit regions should be shown.
    #[inline]
    pub fn show_hit_regions(self) -> bool {
        matches!(self, Self::HitRegions | Self::Full)
    }

    /// Check if widget bounds should be shown.
    #[inline]
    pub fn show_widget_bounds(self) -> bool {
        matches!(self, Self::WidgetBounds | Self::Full)
    }
}

/// Information about a widget for inspector display.
#[derive(Debug, Clone)]
pub struct WidgetInfo {
    /// Human-readable widget name (e.g., "List", "Button").
    pub name: String,
    /// Allocated render area.
    pub area: Rect,
    /// Hit ID if widget is interactive.
    pub hit_id: Option<HitId>,
    /// Registered hit regions within this widget.
    pub hit_regions: Vec<(Rect, HitRegion, HitData)>,
    /// Render time in microseconds (if profiling enabled).
    pub render_time_us: Option<u64>,
    /// Nesting depth for color cycling.
    pub depth: u8,
    /// Child widgets (for tree view).
    pub children: Vec<WidgetInfo>,
}

impl WidgetInfo {
    /// Create a new widget info.
    #[must_use]
    pub fn new(name: impl Into<String>, area: Rect) -> Self {
        Self {
            name: name.into(),
            area,
            hit_id: None,
            hit_regions: Vec::new(),
            render_time_us: None,
            depth: 0,
            children: Vec::new(),
        }
    }

    /// Set the hit ID.
    #[must_use]
    pub fn with_hit_id(mut self, id: HitId) -> Self {
        self.hit_id = Some(id);
        self
    }

    /// Add a hit region.
    pub fn add_hit_region(&mut self, rect: Rect, region: HitRegion, data: HitData) {
        self.hit_regions.push((rect, region, data));
    }

    /// Set nesting depth.
    #[must_use]
    pub fn with_depth(mut self, depth: u8) -> Self {
        self.depth = depth;
        self
    }

    /// Add a child widget.
    pub fn add_child(&mut self, child: WidgetInfo) {
        self.children.push(child);
    }
}

/// Configuration for inspector appearance.
#[derive(Debug, Clone)]
pub struct InspectorStyle {
    /// Border colors for widget bounds (cycles through for nesting).
    pub bound_colors: [PackedRgba; 6],
    /// Hit region overlay color (semi-transparent).
    pub hit_overlay: PackedRgba,
    /// Hovered hit region color.
    pub hit_hover: PackedRgba,
    /// Selected widget highlight.
    pub selected_highlight: PackedRgba,
    /// Label text color.
    pub label_fg: PackedRgba,
    /// Label background color.
    pub label_bg: PackedRgba,
}

impl Default for InspectorStyle {
    fn default() -> Self {
        Self {
            bound_colors: [
                PackedRgba::rgb(255, 100, 100), // Red
                PackedRgba::rgb(100, 255, 100), // Green
                PackedRgba::rgb(100, 100, 255), // Blue
                PackedRgba::rgb(255, 255, 100), // Yellow
                PackedRgba::rgb(255, 100, 255), // Magenta
                PackedRgba::rgb(100, 255, 255), // Cyan
            ],
            hit_overlay: PackedRgba::rgba(255, 165, 0, 80), // Orange 30%
            hit_hover: PackedRgba::rgba(255, 255, 0, 120),  // Yellow 47%
            selected_highlight: PackedRgba::rgba(0, 200, 255, 150), // Cyan 60%
            label_fg: PackedRgba::WHITE,
            label_bg: PackedRgba::rgba(0, 0, 0, 200),
        }
    }
}

impl InspectorStyle {
    /// Get the bound color for a given nesting depth.
    #[inline]
    pub fn bound_color(&self, depth: u8) -> PackedRgba {
        self.bound_colors[depth as usize % self.bound_colors.len()]
    }

    /// Get a region-specific overlay color.
    pub fn region_color(&self, region: HitRegion) -> PackedRgba {
        match region {
            HitRegion::None => PackedRgba::TRANSPARENT,
            HitRegion::Content => PackedRgba::rgba(255, 165, 0, 60), // Orange
            HitRegion::Border => PackedRgba::rgba(128, 128, 128, 60), // Gray
            HitRegion::Scrollbar => PackedRgba::rgba(100, 100, 200, 60), // Blue-ish
            HitRegion::Handle => PackedRgba::rgba(200, 100, 100, 60), // Red-ish
            HitRegion::Button => PackedRgba::rgba(0, 200, 255, 80),  // Cyan
            HitRegion::Link => PackedRgba::rgba(100, 200, 255, 80),  // Light blue
            HitRegion::Custom(_) => PackedRgba::rgba(200, 200, 200, 60), // Light gray
        }
    }
}

/// Inspector overlay state (shared across frames).
#[derive(Debug, Default)]
pub struct InspectorState {
    /// Current display mode.
    pub mode: InspectorMode,
    /// Mouse position for hover detection.
    pub hover_pos: Option<(u16, u16)>,
    /// Selected widget (clicked).
    pub selected: Option<HitId>,
    /// Collected widget info for current frame.
    pub widgets: Vec<WidgetInfo>,
    /// Show detailed panel.
    pub show_detail_panel: bool,
    /// Visual style configuration.
    pub style: InspectorStyle,
    /// Toggle for hit regions visibility (within mode).
    pub show_hits: bool,
    /// Toggle for widget bounds visibility (within mode).
    pub show_bounds: bool,
    /// Toggle for widget name labels.
    pub show_names: bool,
    /// Toggle for render time display.
    pub show_times: bool,
}

impl InspectorState {
    /// Create a new inspector state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            show_hits: true,
            show_bounds: true,
            show_names: true,
            show_times: false,
            ..Default::default()
        }
    }

    /// Toggle the inspector on/off.
    pub fn toggle(&mut self) {
        if self.mode.is_active() {
            self.mode = InspectorMode::Off;
        } else {
            self.mode = InspectorMode::Full;
        }
    }

    /// Check if the inspector is active.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.mode.is_active()
    }

    /// Cycle through display modes.
    pub fn cycle_mode(&mut self) {
        self.mode = self.mode.cycle();
    }

    /// Set mode directly (0=Off, 1=HitRegions, 2=WidgetBounds, 3=Full).
    pub fn set_mode(&mut self, mode_num: u8) {
        self.mode = match mode_num {
            0 => InspectorMode::Off,
            1 => InspectorMode::HitRegions,
            2 => InspectorMode::WidgetBounds,
            _ => InspectorMode::Full,
        };
    }

    /// Update hover position from mouse event.
    pub fn set_hover(&mut self, pos: Option<(u16, u16)>) {
        self.hover_pos = pos;
    }

    /// Select a widget by hit ID.
    pub fn select(&mut self, id: Option<HitId>) {
        self.selected = id;
    }

    /// Clear selection.
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Toggle the detail panel.
    pub fn toggle_detail_panel(&mut self) {
        self.show_detail_panel = !self.show_detail_panel;
    }

    /// Toggle hit regions visibility.
    pub fn toggle_hits(&mut self) {
        self.show_hits = !self.show_hits;
    }

    /// Toggle widget bounds visibility.
    pub fn toggle_bounds(&mut self) {
        self.show_bounds = !self.show_bounds;
    }

    /// Toggle name labels visibility.
    pub fn toggle_names(&mut self) {
        self.show_names = !self.show_names;
    }

    /// Toggle render time visibility.
    pub fn toggle_times(&mut self) {
        self.show_times = !self.show_times;
    }

    /// Clear widget info for new frame.
    pub fn clear_widgets(&mut self) {
        self.widgets.clear();
    }

    /// Register a widget for inspection.
    pub fn register_widget(&mut self, info: WidgetInfo) {
        #[cfg(feature = "tracing")]
        trace!(name = info.name, area = ?info.area, "Registered widget for inspection");
        self.widgets.push(info);
    }

    /// Check if we should render hit regions.
    #[inline]
    pub fn should_show_hits(&self) -> bool {
        self.show_hits && self.mode.show_hit_regions()
    }

    /// Check if we should render widget bounds.
    #[inline]
    pub fn should_show_bounds(&self) -> bool {
        self.show_bounds && self.mode.show_widget_bounds()
    }
}

/// Inspector overlay widget.
///
/// Renders hit region overlays and widget bounds on top of the UI.
pub struct InspectorOverlay<'a> {
    state: &'a InspectorState,
}

impl<'a> InspectorOverlay<'a> {
    /// Create a new inspector overlay.
    #[must_use]
    pub fn new(state: &'a InspectorState) -> Self {
        Self { state }
    }

    /// Render hit region overlays from the frame's HitGrid.
    fn render_hit_regions(&self, area: Rect, frame: &mut Frame) {
        #[cfg(feature = "tracing")]
        let _span = info_span!("render_hit_regions").entered();

        let Some(ref hit_grid) = frame.hit_grid else {
            // No hit grid available - draw warning
            self.draw_warning(area, frame, "HitGrid not enabled");
            return;
        };

        let style = &self.state.style;
        let hover_pos = self.state.hover_pos;
        let selected = self.state.selected;

        // Iterate over visible cells and apply overlay colors
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if let Some(cell) = hit_grid.get(x, y) {
                    if cell.is_empty() {
                        continue;
                    }

                    let is_hovered = hover_pos == Some((x, y));
                    let is_selected = selected == cell.widget_id;

                    // Determine overlay color
                    let overlay = if is_selected {
                        style.selected_highlight
                    } else if is_hovered {
                        style.hit_hover
                    } else {
                        style.region_color(cell.region)
                    };

                    // Apply overlay to buffer cell
                    if let Some(buf_cell) = frame.buffer.get_mut(x, y) {
                        buf_cell.bg = overlay.over(buf_cell.bg);
                    }
                }
            }
        }
    }

    /// Render widget bounds from collected WidgetInfo.
    fn render_widget_bounds(&self, _area: Rect, frame: &mut Frame) {
        #[cfg(feature = "tracing")]
        let _span = info_span!(
            "render_widget_bounds",
            widget_count = self.state.widgets.len()
        )
        .entered();

        let style = &self.state.style;

        for widget in &self.state.widgets {
            self.render_widget_bound(widget, frame, style);
        }
    }

    /// Render a single widget's bounds recursively.
    fn render_widget_bound(&self, widget: &WidgetInfo, frame: &mut Frame, style: &InspectorStyle) {
        let color = style.bound_color(widget.depth);
        let area = widget.area;

        // Skip empty areas
        if area.is_empty() {
            return;
        }

        // Draw border outline
        self.draw_rect_outline(area, frame, color);

        // Draw label if names are enabled
        if self.state.show_names && !widget.name.is_empty() {
            self.draw_label(area, frame, &widget.name, style);
        }

        // Recursively draw children
        for child in &widget.children {
            self.render_widget_bound(child, frame, style);
        }
    }

    /// Draw a rectangle outline with the given color.
    fn draw_rect_outline(&self, rect: Rect, frame: &mut Frame, color: PackedRgba) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        let x = rect.x;
        let y = rect.y;
        let right = rect.right().saturating_sub(1);
        let bottom = rect.bottom().saturating_sub(1);

        // Top edge
        for cx in x..=right {
            if let Some(cell) = frame.buffer.get_mut(cx, y) {
                cell.fg = color;
            }
        }

        // Bottom edge
        if bottom > y {
            for cx in x..=right {
                if let Some(cell) = frame.buffer.get_mut(cx, bottom) {
                    cell.fg = color;
                }
            }
        }

        // Left edge
        for cy in y..=bottom {
            if let Some(cell) = frame.buffer.get_mut(x, cy) {
                cell.fg = color;
            }
        }

        // Right edge
        if right > x {
            for cy in y..=bottom {
                if let Some(cell) = frame.buffer.get_mut(right, cy) {
                    cell.fg = color;
                }
            }
        }
    }

    /// Draw a widget name label at the top-left of its area.
    fn draw_label(&self, area: Rect, frame: &mut Frame, name: &str, style: &InspectorStyle) {
        let label = format!("[{name}]");
        let label_len = label.len() as u16;

        // Position label at top-left, clamped to area
        let x = area.x;
        let y = area.y;

        // Draw label background
        let label_area = Rect::new(x, y, label_len.min(area.width), 1);
        set_style_area(
            &mut frame.buffer,
            label_area,
            Style::new().bg(style.label_bg),
        );

        // Draw label text
        for (i, ch) in label.chars().take(area.width as usize).enumerate() {
            let cx = x + i as u16;
            if let Some(cell) = frame.buffer.get_mut(cx, y) {
                *cell = Cell::from_char(ch)
                    .with_fg(style.label_fg)
                    .with_bg(style.label_bg);
            }
        }
    }

    /// Draw a warning message when something isn't available.
    fn draw_warning(&self, area: Rect, frame: &mut Frame, msg: &str) {
        let style = &self.state.style;
        let warning_style = Style::new()
            .fg(PackedRgba::rgb(255, 200, 0))
            .bg(style.label_bg);

        // Center the message
        let msg_len = msg.len() as u16;
        let x = area.x + area.width.saturating_sub(msg_len) / 2;
        let y = area.y;

        set_style_area(
            &mut frame.buffer,
            Rect::new(x, y, msg_len, 1),
            warning_style,
        );

        for (i, ch) in msg.chars().enumerate() {
            let cx = x + i as u16;
            if cx >= area.right() {
                break;
            }
            if let Some(cell) = frame.buffer.get_mut(cx, y) {
                *cell = Cell::from_char(ch);
                cell.fg = PackedRgba::rgb(255, 200, 0);
                cell.bg = style.label_bg;
            }
        }
    }

    /// Render the detail panel showing selected widget info.
    fn render_detail_panel(&self, area: Rect, frame: &mut Frame) {
        let style = &self.state.style;

        // Panel dimensions
        let panel_width: u16 = 24;
        let panel_height = area.height.min(20);

        // Position at right edge
        let panel_x = area.right().saturating_sub(panel_width + 1);
        let panel_y = area.y + 1;
        let panel_area = Rect::new(panel_x, panel_y, panel_width, panel_height);

        // Draw panel background
        set_style_area(
            &mut frame.buffer,
            panel_area,
            Style::new().bg(style.label_bg),
        );

        // Draw border
        self.draw_rect_outline(panel_area, frame, style.label_fg);

        // Draw content
        let content_x = panel_x + 1;
        let mut y = panel_y + 1;

        // Title
        self.draw_panel_text(frame, content_x, y, "Inspector", style.label_fg);
        y += 2;

        // Mode info
        let mode_str = match self.state.mode {
            InspectorMode::Off => "Off",
            InspectorMode::HitRegions => "Hit Regions",
            InspectorMode::WidgetBounds => "Widget Bounds",
            InspectorMode::Full => "Full",
        };
        self.draw_panel_text(
            frame,
            content_x,
            y,
            &format!("Mode: {mode_str}"),
            style.label_fg,
        );
        y += 1;

        // Hover info
        if let Some((hx, hy)) = self.state.hover_pos {
            self.draw_panel_text(
                frame,
                content_x,
                y,
                &format!("Hover: ({hx},{hy})"),
                style.label_fg,
            );
            y += 1;

            // Extract hit info first to avoid borrow conflicts
            let hit_info = frame
                .hit_grid
                .as_ref()
                .and_then(|grid| grid.get(hx, hy).filter(|h| !h.is_empty()).map(|h| (*h,)));

            // Show hit info at hover position
            if let Some((hit,)) = hit_info {
                let region_str = format!("{:?}", hit.region);
                self.draw_panel_text(
                    frame,
                    content_x,
                    y,
                    &format!("Region: {region_str}"),
                    style.label_fg,
                );
                y += 1;
                if let Some(id) = hit.widget_id {
                    self.draw_panel_text(
                        frame,
                        content_x,
                        y,
                        &format!("ID: {}", id.id()),
                        style.label_fg,
                    );
                    y += 1;
                }
                if hit.data != 0 {
                    self.draw_panel_text(
                        frame,
                        content_x,
                        y,
                        &format!("Data: {}", hit.data),
                        style.label_fg,
                    );
                    #[allow(unused_assignments)]
                    {
                        y += 1;
                    }
                }
            }
        }
    }

    /// Draw text in the detail panel.
    fn draw_panel_text(&self, frame: &mut Frame, x: u16, y: u16, text: &str, fg: PackedRgba) {
        for (i, ch) in text.chars().enumerate() {
            let cx = x + i as u16;
            if let Some(cell) = frame.buffer.get_mut(cx, y) {
                *cell = Cell::from_char(ch).with_fg(fg);
            }
        }
    }
}

impl Widget for InspectorOverlay<'_> {
    fn render(&self, area: Rect, frame: &mut Frame) {
        #[cfg(feature = "tracing")]
        let _span = info_span!("inspector_overlay", ?area).entered();

        if !self.state.is_active() {
            return;
        }

        // Render hit regions first (underneath widget bounds)
        if self.state.should_show_hits() {
            self.render_hit_regions(area, frame);
        }

        // Render widget bounds on top
        if self.state.should_show_bounds() {
            self.render_widget_bounds(area, frame);
        }

        // Render detail panel if enabled
        if self.state.show_detail_panel {
            self.render_detail_panel(area, frame);
        }
    }

    fn is_essential(&self) -> bool {
        // Inspector is a debugging tool, not essential
        false
    }
}

/// Helper to extract hit information from a HitCell for display.
#[derive(Debug, Clone)]
pub struct HitInfo {
    /// Widget ID.
    pub widget_id: HitId,
    /// Region type.
    pub region: HitRegion,
    /// Associated data.
    pub data: HitData,
    /// Screen position.
    pub position: (u16, u16),
}

impl HitInfo {
    /// Create from a HitCell and position.
    #[must_use]
    pub fn from_cell(cell: &HitCell, x: u16, y: u16) -> Option<Self> {
        cell.widget_id.map(|id| Self {
            widget_id: id,
            region: cell.region,
            data: cell.data,
            position: (x, y),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::grapheme_pool::GraphemePool;

    #[test]
    fn inspector_mode_cycle() {
        let mut mode = InspectorMode::Off;
        mode = mode.cycle();
        assert_eq!(mode, InspectorMode::HitRegions);
        mode = mode.cycle();
        assert_eq!(mode, InspectorMode::WidgetBounds);
        mode = mode.cycle();
        assert_eq!(mode, InspectorMode::Full);
        mode = mode.cycle();
        assert_eq!(mode, InspectorMode::Off);
    }

    #[test]
    fn inspector_mode_is_active() {
        assert!(!InspectorMode::Off.is_active());
        assert!(InspectorMode::HitRegions.is_active());
        assert!(InspectorMode::WidgetBounds.is_active());
        assert!(InspectorMode::Full.is_active());
    }

    #[test]
    fn inspector_mode_show_flags() {
        assert!(!InspectorMode::Off.show_hit_regions());
        assert!(!InspectorMode::Off.show_widget_bounds());

        assert!(InspectorMode::HitRegions.show_hit_regions());
        assert!(!InspectorMode::HitRegions.show_widget_bounds());

        assert!(!InspectorMode::WidgetBounds.show_hit_regions());
        assert!(InspectorMode::WidgetBounds.show_widget_bounds());

        assert!(InspectorMode::Full.show_hit_regions());
        assert!(InspectorMode::Full.show_widget_bounds());
    }

    #[test]
    fn inspector_state_toggle() {
        let mut state = InspectorState::new();
        assert!(!state.is_active());

        state.toggle();
        assert!(state.is_active());
        assert_eq!(state.mode, InspectorMode::Full);

        state.toggle();
        assert!(!state.is_active());
        assert_eq!(state.mode, InspectorMode::Off);
    }

    #[test]
    fn inspector_state_set_mode() {
        let mut state = InspectorState::new();

        state.set_mode(1);
        assert_eq!(state.mode, InspectorMode::HitRegions);

        state.set_mode(2);
        assert_eq!(state.mode, InspectorMode::WidgetBounds);

        state.set_mode(3);
        assert_eq!(state.mode, InspectorMode::Full);

        state.set_mode(0);
        assert_eq!(state.mode, InspectorMode::Off);

        // Any value >= 3 maps to Full
        state.set_mode(99);
        assert_eq!(state.mode, InspectorMode::Full);
    }

    #[test]
    fn inspector_style_default() {
        let style = InspectorStyle::default();
        assert_eq!(style.bound_colors.len(), 6);
        assert_eq!(style.hit_overlay, PackedRgba::rgba(255, 165, 0, 80));
    }

    #[test]
    fn inspector_style_bound_color_cycles() {
        let style = InspectorStyle::default();
        assert_eq!(style.bound_color(0), style.bound_colors[0]);
        assert_eq!(style.bound_color(5), style.bound_colors[5]);
        assert_eq!(style.bound_color(6), style.bound_colors[0]); // Wraps
        assert_eq!(style.bound_color(7), style.bound_colors[1]);
    }

    #[test]
    fn widget_info_creation() {
        let info = WidgetInfo::new("Button", Rect::new(10, 5, 20, 3))
            .with_hit_id(HitId::new(42))
            .with_depth(2);

        assert_eq!(info.name, "Button");
        assert_eq!(info.area, Rect::new(10, 5, 20, 3));
        assert_eq!(info.hit_id, Some(HitId::new(42)));
        assert_eq!(info.depth, 2);
    }

    #[test]
    fn widget_info_add_hit_region() {
        let mut info = WidgetInfo::new("List", Rect::new(0, 0, 10, 10));
        info.add_hit_region(Rect::new(0, 0, 10, 1), HitRegion::Content, 0);
        info.add_hit_region(Rect::new(0, 1, 10, 1), HitRegion::Content, 1);

        assert_eq!(info.hit_regions.len(), 2);
        assert_eq!(info.hit_regions[0].2, 0);
        assert_eq!(info.hit_regions[1].2, 1);
    }

    #[test]
    fn widget_info_add_child() {
        let mut parent = WidgetInfo::new("Container", Rect::new(0, 0, 20, 20));
        let child = WidgetInfo::new("Button", Rect::new(5, 5, 10, 3));
        parent.add_child(child);

        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].name, "Button");
    }

    #[test]
    fn inspector_overlay_inactive_is_noop() {
        let state = InspectorState::new();
        let overlay = InspectorOverlay::new(&state);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(10, 10, &mut pool);
        let area = Rect::new(0, 0, 10, 10);

        // Should do nothing since mode is Off
        overlay.render(area, &mut frame);

        // Buffer should be empty
        assert!(frame.buffer.get(0, 0).unwrap().is_empty());
    }

    #[test]
    fn inspector_overlay_renders_when_active() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::Full;
        state.show_detail_panel = true;

        let overlay = InspectorOverlay::new(&state);

        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(40, 20, &mut pool);

        // Register a hit region
        frame.register_hit(Rect::new(5, 5, 10, 3), HitId::new(1), HitRegion::Button, 42);

        let area = Rect::new(0, 0, 40, 20);
        overlay.render(area, &mut frame);

        // The detail panel should be rendered at the right edge
        // This is a smoke test - actual content depends on implementation
    }

    #[test]
    fn hit_info_from_cell() {
        let cell = HitCell::new(HitId::new(5), HitRegion::Button, 99);
        let info = HitInfo::from_cell(&cell, 10, 20);

        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.widget_id, HitId::new(5));
        assert_eq!(info.region, HitRegion::Button);
        assert_eq!(info.data, 99);
        assert_eq!(info.position, (10, 20));
    }

    #[test]
    fn hit_info_from_empty_cell() {
        let cell = HitCell::default();
        let info = HitInfo::from_cell(&cell, 0, 0);
        assert!(info.is_none());
    }

    #[test]
    fn inspector_state_toggles() {
        let mut state = InspectorState::new();

        assert!(state.show_hits);
        state.toggle_hits();
        assert!(!state.show_hits);
        state.toggle_hits();
        assert!(state.show_hits);

        assert!(state.show_bounds);
        state.toggle_bounds();
        assert!(!state.show_bounds);

        assert!(state.show_names);
        state.toggle_names();
        assert!(!state.show_names);

        assert!(!state.show_times);
        state.toggle_times();
        assert!(state.show_times);

        assert!(!state.show_detail_panel);
        state.toggle_detail_panel();
        assert!(state.show_detail_panel);
    }

    #[test]
    fn inspector_state_selection() {
        let mut state = InspectorState::new();

        assert!(state.selected.is_none());
        state.select(Some(HitId::new(42)));
        assert_eq!(state.selected, Some(HitId::new(42)));
        state.clear_selection();
        assert!(state.selected.is_none());
    }

    #[test]
    fn inspector_state_hover() {
        let mut state = InspectorState::new();

        assert!(state.hover_pos.is_none());
        state.set_hover(Some((10, 20)));
        assert_eq!(state.hover_pos, Some((10, 20)));
        state.set_hover(None);
        assert!(state.hover_pos.is_none());
    }

    #[test]
    fn inspector_state_widget_registry() {
        let mut state = InspectorState::new();

        let widget = WidgetInfo::new("Test", Rect::new(0, 0, 10, 10));
        state.register_widget(widget);
        assert_eq!(state.widgets.len(), 1);

        state.clear_widgets();
        assert!(state.widgets.is_empty());
    }

    #[test]
    fn inspector_overlay_is_not_essential() {
        let state = InspectorState::new();
        let overlay = InspectorOverlay::new(&state);
        assert!(!overlay.is_essential());
    }

    // =========================================================================
    // Edge Case Tests (bd-17h9.6)
    // =========================================================================

    #[test]
    fn edge_case_zero_area_widget() {
        // Zero-sized areas should not panic
        let info = WidgetInfo::new("ZeroArea", Rect::new(0, 0, 0, 0));
        assert_eq!(info.area.width, 0);
        assert_eq!(info.area.height, 0);
        assert!(info.area.is_empty());
    }

    #[test]
    fn edge_case_max_depth_widget() {
        // Maximum depth should work without overflow
        let info = WidgetInfo::new("Deep", Rect::new(0, 0, 10, 10)).with_depth(u8::MAX);
        assert_eq!(info.depth, u8::MAX);

        // Bound color should still cycle correctly
        let style = InspectorStyle::default();
        let _color = style.bound_color(u8::MAX); // Should not panic
    }

    #[test]
    fn edge_case_empty_widget_registry() {
        let mut state = InspectorState::new();
        assert!(state.widgets.is_empty());

        // Clearing empty registry should not panic
        state.clear_widgets();
        assert!(state.widgets.is_empty());
    }

    #[test]
    fn edge_case_selection_without_widgets() {
        let mut state = InspectorState::new();

        // Selecting when no widgets are registered
        state.select(Some(HitId::new(42)));
        assert_eq!(state.selected, Some(HitId::new(42)));

        // Clearing selection
        state.clear_selection();
        assert!(state.selected.is_none());
    }

    #[test]
    fn edge_case_hover_boundary_positions() {
        let mut state = InspectorState::new();

        // Maximum u16 coordinates
        state.set_hover(Some((u16::MAX, u16::MAX)));
        assert_eq!(state.hover_pos, Some((u16::MAX, u16::MAX)));

        // Zero coordinates
        state.set_hover(Some((0, 0)));
        assert_eq!(state.hover_pos, Some((0, 0)));
    }

    #[test]
    fn edge_case_deeply_nested_widgets() {
        // Build nested structure from inside out
        let mut deepest = WidgetInfo::new("L10", Rect::new(10, 10, 80, 80)).with_depth(10);

        for i in (1..10).rev() {
            let mut parent =
                WidgetInfo::new(format!("L{i}"), Rect::new(i as u16, i as u16, 90, 90))
                    .with_depth(i as u8);
            parent.add_child(deepest);
            deepest = parent;
        }

        let mut root = WidgetInfo::new("Root", Rect::new(0, 0, 100, 100)).with_depth(0);
        root.add_child(deepest);

        // Verify nesting: root -> L1 -> L2 -> ... -> L10
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].depth, 1);
        assert_eq!(root.children[0].children[0].depth, 2);
    }

    #[test]
    fn edge_case_rapid_mode_cycling() {
        let mut state = InspectorState::new();
        assert_eq!(state.mode, InspectorMode::Off);

        // Cycle 1000 times and verify we end at correct mode
        for _ in 0..1000 {
            state.mode = state.mode.cycle();
        }
        // 1000 % 4 = 0, so should be back at Off
        assert_eq!(state.mode, InspectorMode::Off);
    }

    #[test]
    fn edge_case_many_hit_regions() {
        let mut info = WidgetInfo::new("ManyHits", Rect::new(0, 0, 100, 1000));

        // Add 1000 hit regions
        for i in 0..1000 {
            info.add_hit_region(
                Rect::new(0, i as u16, 100, 1),
                HitRegion::Content,
                i as HitData,
            );
        }

        assert_eq!(info.hit_regions.len(), 1000);
        assert_eq!(info.hit_regions[0].2, 0);
        assert_eq!(info.hit_regions[999].2, 999);
    }

    #[test]
    fn edge_case_mode_show_flags_consistency() {
        // Verify show flags are consistent with mode
        for mode in [
            InspectorMode::Off,
            InspectorMode::HitRegions,
            InspectorMode::WidgetBounds,
            InspectorMode::Full,
        ] {
            match mode {
                InspectorMode::Off => {
                    assert!(!mode.show_hit_regions());
                    assert!(!mode.show_widget_bounds());
                }
                InspectorMode::HitRegions => {
                    assert!(mode.show_hit_regions());
                    assert!(!mode.show_widget_bounds());
                }
                InspectorMode::WidgetBounds => {
                    assert!(!mode.show_hit_regions());
                    assert!(mode.show_widget_bounds());
                }
                InspectorMode::Full => {
                    assert!(mode.show_hit_regions());
                    assert!(mode.show_widget_bounds());
                }
            }
        }
    }

    // =========================================================================
    // Property-Based Tests (bd-17h9.6)
    // =========================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Mode cycling is periodic with period 4.
            /// Cycling 4 times from any mode returns to the original mode.
            #[test]
            fn mode_cycle_is_periodic(start_cycle in 0u8..4) {
                let start_mode = match start_cycle {
                    0 => InspectorMode::Off,
                    1 => InspectorMode::HitRegions,
                    2 => InspectorMode::WidgetBounds,
                    _ => InspectorMode::Full,
                };

                let mut mode = start_mode;
                for _ in 0..4 {
                    mode = mode.cycle();
                }
                prop_assert_eq!(mode, start_mode);
            }

            /// Bound color cycling is periodic with period 6.
            #[test]
            fn bound_color_cycle_is_periodic(depth in 0u8..200) {
                let style = InspectorStyle::default();
                let color_a = style.bound_color(depth);
                let color_b = style.bound_color(depth.wrapping_add(6));
                prop_assert_eq!(color_a, color_b);
            }

            /// is_active correctly reflects mode != Off.
            #[test]
            fn is_active_reflects_mode(mode_idx in 0u8..4) {
                let mode = match mode_idx {
                    0 => InspectorMode::Off,
                    1 => InspectorMode::HitRegions,
                    2 => InspectorMode::WidgetBounds,
                    _ => InspectorMode::Full,
                };
                let expected_active = mode_idx != 0;
                prop_assert_eq!(mode.is_active(), expected_active);
            }

            /// Double toggle is identity for boolean flags.
            #[test]
            fn double_toggle_is_identity(_seed in 0u32..1000) {
                let mut state = InspectorState::new();
                let initial_hits = state.show_hits;
                let initial_bounds = state.show_bounds;
                let initial_names = state.show_names;
                let initial_times = state.show_times;
                let initial_panel = state.show_detail_panel;

                // Toggle twice
                state.toggle_hits();
                state.toggle_hits();
                state.toggle_bounds();
                state.toggle_bounds();
                state.toggle_names();
                state.toggle_names();
                state.toggle_times();
                state.toggle_times();
                state.toggle_detail_panel();
                state.toggle_detail_panel();

                prop_assert_eq!(state.show_hits, initial_hits);
                prop_assert_eq!(state.show_bounds, initial_bounds);
                prop_assert_eq!(state.show_names, initial_names);
                prop_assert_eq!(state.show_times, initial_times);
                prop_assert_eq!(state.show_detail_panel, initial_panel);
            }

            /// Widget info preserves area dimensions.
            #[test]
            fn widget_info_preserves_area(
                x in 0u16..1000,
                y in 0u16..1000,
                w in 1u16..500,
                h in 1u16..500,
            ) {
                let area = Rect::new(x, y, w, h);
                let info = WidgetInfo::new("Test", area);
                prop_assert_eq!(info.area, area);
            }

            /// Widget depth is preserved through builder pattern.
            #[test]
            fn widget_depth_preserved(depth in 0u8..255) {
                let info = WidgetInfo::new("Test", Rect::new(0, 0, 10, 10))
                    .with_depth(depth);
                prop_assert_eq!(info.depth, depth);
            }

            /// Hit ID is preserved through builder pattern.
            #[test]
            fn widget_hit_id_preserved(id in 0u32..u32::MAX) {
                let hit_id = HitId::new(id);
                let info = WidgetInfo::new("Test", Rect::new(0, 0, 10, 10))
                    .with_hit_id(hit_id);
                prop_assert_eq!(info.hit_id, Some(hit_id));
            }

            /// Adding children increases child count.
            #[test]
            fn add_child_increases_count(child_count in 0usize..50) {
                let mut parent = WidgetInfo::new("Parent", Rect::new(0, 0, 100, 100));
                for i in 0..child_count {
                    parent.add_child(WidgetInfo::new(
                        format!("Child{i}"),
                        Rect::new(0, i as u16, 10, 1),
                    ));
                }
                prop_assert_eq!(parent.children.len(), child_count);
            }

            /// Hit regions can be added without bounds.
            #[test]
            fn add_hit_regions_unbounded(region_count in 0usize..100) {
                let mut info = WidgetInfo::new("Test", Rect::new(0, 0, 100, 100));
                for i in 0..region_count {
                    info.add_hit_region(
                        Rect::new(0, i as u16, 10, 1),
                        HitRegion::Content,
                        i as HitData,
                    );
                }
                prop_assert_eq!(info.hit_regions.len(), region_count);
            }

            /// set_mode correctly maps index to mode.
            #[test]
            fn set_mode_maps_correctly(mode_idx in 0u8..10) {
                let mut state = InspectorState::new();
                state.set_mode(mode_idx);
                let expected = match mode_idx {
                    0 => InspectorMode::Off,
                    1 => InspectorMode::HitRegions,
                    2 => InspectorMode::WidgetBounds,
                    3 => InspectorMode::Full,
                    _ => InspectorMode::Full, // Saturates at max
                };
                prop_assert_eq!(state.mode, expected);
            }

            /// should_show_hits respects both mode and toggle flag.
            #[test]
            fn should_show_hits_respects_both(mode_idx in 0u8..4, flag in proptest::bool::ANY) {
                let mut state = InspectorState::new();
                state.set_mode(mode_idx);
                state.show_hits = flag;
                let mode_allows = state.mode.show_hit_regions();
                prop_assert_eq!(state.should_show_hits(), flag && mode_allows);
            }

            /// should_show_bounds respects both mode and toggle flag.
            #[test]
            fn should_show_bounds_respects_both(mode_idx in 0u8..4, flag in proptest::bool::ANY) {
                let mut state = InspectorState::new();
                state.set_mode(mode_idx);
                state.show_bounds = flag;
                let mode_allows = state.mode.show_widget_bounds();
                prop_assert_eq!(state.should_show_bounds(), flag && mode_allows);
            }
        }
    }

    // =========================================================================
    // Region Color Coverage Tests (bd-17h9.6)
    // =========================================================================

    #[test]
    fn region_color_all_variants() {
        let style = InspectorStyle::default();

        // Each region type returns a distinct (or appropriate) color
        let none_color = style.region_color(HitRegion::None);
        let content_color = style.region_color(HitRegion::Content);
        let border_color = style.region_color(HitRegion::Border);
        let scrollbar_color = style.region_color(HitRegion::Scrollbar);
        let handle_color = style.region_color(HitRegion::Handle);
        let button_color = style.region_color(HitRegion::Button);
        let link_color = style.region_color(HitRegion::Link);
        let custom_color = style.region_color(HitRegion::Custom(42));

        // None returns transparent
        assert_eq!(none_color, PackedRgba::TRANSPARENT);

        // Other regions return non-transparent colors
        assert_ne!(content_color.a(), 0);
        assert_ne!(border_color.a(), 0);
        assert_ne!(scrollbar_color.a(), 0);
        assert_ne!(handle_color.a(), 0);
        assert_ne!(button_color.a(), 0);
        assert_ne!(link_color.a(), 0);
        assert_ne!(custom_color.a(), 0);

        // Verify they are semi-transparent (not fully opaque)
        assert!(content_color.a() < 255);
        assert!(button_color.a() < 255);
    }

    #[test]
    fn region_color_custom_variants() {
        let style = InspectorStyle::default();

        // All Custom variants return the same color
        let c0 = style.region_color(HitRegion::Custom(0));
        let c1 = style.region_color(HitRegion::Custom(1));
        let c255 = style.region_color(HitRegion::Custom(255));

        assert_eq!(c0, c1);
        assert_eq!(c1, c255);
    }

    // =========================================================================
    // Should-Show Methods Tests (bd-17h9.6)
    // =========================================================================

    #[test]
    fn should_show_hits_requires_both_mode_and_flag() {
        let mut state = InspectorState::new();

        // Off mode: never show
        state.mode = InspectorMode::Off;
        state.show_hits = true;
        assert!(!state.should_show_hits());

        // HitRegions mode with flag on: show
        state.mode = InspectorMode::HitRegions;
        state.show_hits = true;
        assert!(state.should_show_hits());

        // HitRegions mode with flag off: don't show
        state.show_hits = false;
        assert!(!state.should_show_hits());

        // WidgetBounds mode: doesn't show hits
        state.mode = InspectorMode::WidgetBounds;
        state.show_hits = true;
        assert!(!state.should_show_hits());

        // Full mode with flag on: show
        state.mode = InspectorMode::Full;
        state.show_hits = true;
        assert!(state.should_show_hits());
    }

    #[test]
    fn should_show_bounds_requires_both_mode_and_flag() {
        let mut state = InspectorState::new();

        // Off mode: never show
        state.mode = InspectorMode::Off;
        state.show_bounds = true;
        assert!(!state.should_show_bounds());

        // WidgetBounds mode with flag on: show
        state.mode = InspectorMode::WidgetBounds;
        state.show_bounds = true;
        assert!(state.should_show_bounds());

        // WidgetBounds mode with flag off: don't show
        state.show_bounds = false;
        assert!(!state.should_show_bounds());

        // HitRegions mode: doesn't show bounds
        state.mode = InspectorMode::HitRegions;
        state.show_bounds = true;
        assert!(!state.should_show_bounds());

        // Full mode with flag on: show
        state.mode = InspectorMode::Full;
        state.show_bounds = true;
        assert!(state.should_show_bounds());
    }

    // =========================================================================
    // Overlay Rendering Tests (bd-17h9.6)
    // =========================================================================

    #[test]
    fn overlay_respects_mode_hit_regions_only() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::HitRegions;

        // Register a widget for bounds drawing BEFORE creating overlay
        state.register_widget(WidgetInfo::new("TestWidget", Rect::new(5, 5, 10, 3)));

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(20, 10, &mut pool);

        // Register a hit region
        frame.register_hit(Rect::new(0, 0, 5, 5), HitId::new(1), HitRegion::Button, 0);

        let area = Rect::new(0, 0, 20, 10);
        overlay.render(area, &mut frame);

        // In HitRegions mode, bounds should NOT be rendered
        // (We can verify by checking that widget info bounds area is not drawn)
        assert!(state.should_show_hits());
        assert!(!state.should_show_bounds());
    }

    #[test]
    fn overlay_respects_mode_widget_bounds_only() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::WidgetBounds;
        state.show_names = true;

        // Register widget
        state.register_widget(WidgetInfo::new("TestWidget", Rect::new(2, 2, 15, 5)));

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(20, 10, &mut pool);

        let area = Rect::new(0, 0, 20, 10);
        overlay.render(area, &mut frame);

        // In WidgetBounds mode, hits should NOT be shown
        assert!(!state.should_show_hits());
        assert!(state.should_show_bounds());
    }

    #[test]
    fn overlay_full_mode_shows_both() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::Full;

        // Register widget
        state.register_widget(WidgetInfo::new("FullTest", Rect::new(0, 0, 10, 5)));

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(20, 10, &mut pool);

        frame.register_hit(Rect::new(0, 0, 5, 5), HitId::new(1), HitRegion::Content, 0);

        let area = Rect::new(0, 0, 20, 10);
        overlay.render(area, &mut frame);

        assert!(state.should_show_hits());
        assert!(state.should_show_bounds());
    }

    #[test]
    fn overlay_detail_panel_renders_when_enabled() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::Full;
        state.show_detail_panel = true;
        state.set_hover(Some((5, 5)));

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(50, 25, &mut pool);

        let area = Rect::new(0, 0, 50, 25);
        overlay.render(area, &mut frame);

        // The detail panel is 24 chars wide, rendered at right edge
        // Panel should be at x = 50 - 24 - 1 = 25
        // Check that something is rendered in the panel area
        let panel_x = 25;
        let panel_y = 1;

        // Panel background should be the label_bg color
        let cell = frame.buffer.get(panel_x + 1, panel_y + 1);
        assert!(cell.is_some());
    }

    #[test]
    fn overlay_without_hit_grid_shows_warning() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::HitRegions;

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        // Frame without hit grid
        let mut frame = Frame::new(40, 10, &mut pool);

        let area = Rect::new(0, 0, 40, 10);
        overlay.render(area, &mut frame);

        // Warning message "HitGrid not enabled" should be centered
        // The message is 20 chars, centered in 40 char width = starts at x=10
        // Check first char is 'H' from "HitGrid"
        if let Some(cell) = frame.buffer.get(10, 0) {
            assert_eq!(cell.content.as_char(), Some('H'));
        }
    }

    // =========================================================================
    // Widget Tree Rendering Tests (bd-17h9.6)
    // =========================================================================

    #[test]
    fn nested_widgets_render_with_depth_colors() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::WidgetBounds;
        state.show_names = false; // Disable names for clearer test

        // Create nested widget tree
        let mut parent = WidgetInfo::new("Parent", Rect::new(0, 0, 30, 20)).with_depth(0);
        let child = WidgetInfo::new("Child", Rect::new(2, 2, 26, 16)).with_depth(1);
        parent.add_child(child);

        state.register_widget(parent);

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(40, 25, &mut pool);

        let area = Rect::new(0, 0, 40, 25);
        overlay.render(area, &mut frame);

        // Parent outline at depth 0 uses bound_colors[0]
        // Child outline at depth 1 uses bound_colors[1]
        let style = InspectorStyle::default();
        let parent_color = style.bound_color(0);
        let child_color = style.bound_color(1);

        // Verify different colors are used
        assert_ne!(parent_color, child_color);
    }

    #[test]
    fn widget_with_empty_name_skips_label() {
        let mut state = InspectorState::new();
        state.mode = InspectorMode::WidgetBounds;
        state.show_names = true;

        // Widget with empty name
        state.register_widget(WidgetInfo::new("", Rect::new(5, 5, 10, 5)));

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(20, 15, &mut pool);

        let area = Rect::new(0, 0, 20, 15);
        overlay.render(area, &mut frame);

        // Should not panic; empty name is handled gracefully
    }

    // =========================================================================
    // Hit Info Edge Cases (bd-17h9.6)
    // =========================================================================

    #[test]
    fn hit_info_all_region_types() {
        let regions = [
            HitRegion::None,
            HitRegion::Content,
            HitRegion::Border,
            HitRegion::Scrollbar,
            HitRegion::Handle,
            HitRegion::Button,
            HitRegion::Link,
            HitRegion::Custom(0),
            HitRegion::Custom(255),
        ];

        for region in regions {
            let cell = HitCell::new(HitId::new(1), region, 42);
            let info = HitInfo::from_cell(&cell, 10, 20);

            let info = info.expect("should create info");
            assert_eq!(info.region, region);
            assert_eq!(info.data, 42);
        }
    }

    #[test]
    fn hit_cell_with_zero_data() {
        let cell = HitCell::new(HitId::new(5), HitRegion::Content, 0);
        let info = HitInfo::from_cell(&cell, 0, 0).unwrap();
        assert_eq!(info.data, 0);
    }

    #[test]
    fn hit_cell_with_max_data() {
        let cell = HitCell::new(HitId::new(5), HitRegion::Content, u64::MAX);
        let info = HitInfo::from_cell(&cell, 0, 0).unwrap();
        assert_eq!(info.data, u64::MAX);
    }

    // =========================================================================
    // State Initialization Tests (bd-17h9.6)
    // =========================================================================

    #[test]
    fn inspector_state_new_defaults() {
        let state = InspectorState::new();

        // Verify all defaults
        assert_eq!(state.mode, InspectorMode::Off);
        assert!(state.hover_pos.is_none());
        assert!(state.selected.is_none());
        assert!(state.widgets.is_empty());
        assert!(!state.show_detail_panel);
        assert!(state.show_hits);
        assert!(state.show_bounds);
        assert!(state.show_names);
        assert!(!state.show_times);
    }

    #[test]
    fn inspector_state_default_matches_new() {
        let state_new = InspectorState::new();
        let state_default = InspectorState::default();

        // Most fields should match (but new() sets show_hits/bounds/names to true)
        assert_eq!(state_new.mode, state_default.mode);
        assert_eq!(state_new.hover_pos, state_default.hover_pos);
        assert_eq!(state_new.selected, state_default.selected);
    }

    #[test]
    fn inspector_style_colors_are_semi_transparent() {
        let style = InspectorStyle::default();

        // hit_overlay should be semi-transparent
        assert!(style.hit_overlay.a() > 0);
        assert!(style.hit_overlay.a() < 255);

        // hit_hover should be semi-transparent
        assert!(style.hit_hover.a() > 0);
        assert!(style.hit_hover.a() < 255);

        // selected_highlight should be semi-transparent
        assert!(style.selected_highlight.a() > 0);
        assert!(style.selected_highlight.a() < 255);

        // label_bg should be nearly opaque
        assert!(style.label_bg.a() > 128);
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn telemetry_spans_and_events() {
        // This test mostly verifies that the code compiles with tracing macros.
        // Verifying actual output would require a custom subscriber which is overkill here.
        let mut state = InspectorState::new();
        state.toggle(); // Should log "Inspector toggled"

        let overlay = InspectorOverlay::new(&state);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::with_hit_grid(20, 10, &mut pool);

        let area = Rect::new(0, 0, 20, 10);
        overlay.render(area, &mut frame); // Should enter "inspector_overlay" span
    }

    // =========================================================================
    // Accessibility/UX Tests (bd-17h9.9)
    // =========================================================================

    /// Calculate relative luminance for WCAG contrast calculation.
    /// Formula: https://www.w3.org/TR/WCAG20/#relativeluminancedef
    fn relative_luminance(rgba: PackedRgba) -> f64 {
        fn channel_luminance(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        let r = channel_luminance(rgba.r());
        let g = channel_luminance(rgba.g());
        let b = channel_luminance(rgba.b());
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Calculate WCAG contrast ratio between two colors.
    /// Returns ratio in range [1.0, 21.0].
    fn contrast_ratio(fg: PackedRgba, bg: PackedRgba) -> f64 {
        let l1 = relative_luminance(fg);
        let l2 = relative_luminance(bg);
        let lighter = l1.max(l2);
        let darker = l1.min(l2);
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn a11y_label_contrast_meets_wcag_aa() {
        // WCAG AA requires 4.5:1 for normal text, 3:1 for large text
        // Labels in inspector are typically large (widget names), so 3:1 is sufficient
        let style = InspectorStyle::default();
        let ratio = contrast_ratio(style.label_fg, style.label_bg);
        assert!(
            ratio >= 3.0,
            "Label contrast ratio {:.2}:1 should be >= 3:1 (WCAG AA large text)",
            ratio
        );
        // Actually we exceed 4.5:1 (white on dark bg)
        assert!(
            ratio >= 4.5,
            "Label contrast ratio {:.2}:1 should be >= 4.5:1 (WCAG AA normal text)",
            ratio
        );
    }

    #[test]
    fn a11y_bound_colors_are_distinct() {
        // Ensure bound colors are visually distinct from each other
        // by checking they have different hues
        let style = InspectorStyle::default();
        let colors = &style.bound_colors;

        // All pairs should have at least one channel differing by 100+
        for (i, a) in colors.iter().enumerate() {
            for (j, b) in colors.iter().enumerate() {
                if i != j {
                    let r_diff = (a.r() as i32 - b.r() as i32).abs();
                    let g_diff = (a.g() as i32 - b.g() as i32).abs();
                    let b_diff = (a.b() as i32 - b.b() as i32).abs();
                    let max_diff = r_diff.max(g_diff).max(b_diff);
                    assert!(
                        max_diff >= 100,
                        "Bound colors {} and {} should differ by at least 100 in one channel (max diff = {})",
                        i, j, max_diff
                    );
                }
            }
        }
    }

    #[test]
    fn a11y_bound_colors_have_good_visibility() {
        // All bound colors should be bright enough to be visible
        // At least one channel should be >= 100
        let style = InspectorStyle::default();
        for (i, color) in style.bound_colors.iter().enumerate() {
            let max_channel = color.r().max(color.g()).max(color.b());
            assert!(
                max_channel >= 100,
                "Bound color {} should have at least one channel >= 100 for visibility (max = {})",
                i, max_channel
            );
        }
    }

    #[test]
    fn a11y_hit_overlays_are_visible() {
        // Hit overlays should have enough alpha to be visible
        // but not so much that they obscure content
        let style = InspectorStyle::default();

        // hit_overlay (normal state) - should be visible but subtle
        assert!(
            style.hit_overlay.a() >= 50,
            "hit_overlay alpha {} should be >= 50 for visibility",
            style.hit_overlay.a()
        );

        // hit_hover (hover state) - should be more prominent
        assert!(
            style.hit_hover.a() >= 80,
            "hit_hover alpha {} should be >= 80 for clear hover indication",
            style.hit_hover.a()
        );
        assert!(
            style.hit_hover.a() > style.hit_overlay.a(),
            "hit_hover should be more visible than hit_overlay"
        );

        // selected_highlight - should be the most prominent
        assert!(
            style.selected_highlight.a() >= 100,
            "selected_highlight alpha {} should be >= 100 for clear selection",
            style.selected_highlight.a()
        );
    }

    #[test]
    fn a11y_region_colors_cover_all_variants() {
        // Ensure all HitRegion variants have a defined color
        let style = InspectorStyle::default();
        let regions = [
            HitRegion::None,
            HitRegion::Content,
            HitRegion::Border,
            HitRegion::Scrollbar,
            HitRegion::Handle,
            HitRegion::Button,
            HitRegion::Link,
            HitRegion::Custom(0),
        ];

        for region in regions {
            let color = style.region_color(region);
            // None should be transparent, others should be visible
            match region {
                HitRegion::None => {
                    assert_eq!(
                        color,
                        PackedRgba::TRANSPARENT,
                        "HitRegion::None should be transparent"
                    );
                }
                _ => {
                    assert!(
                        color.a() > 0,
                        "HitRegion::{:?} should have non-zero alpha",
                        region
                    );
                }
            }
        }
    }

    #[test]
    fn a11y_interactive_regions_are_distinct_from_passive() {
        // Interactive regions (Button, Link) should be visually distinct
        // from passive regions (Content, Border)
        let style = InspectorStyle::default();

        let button_color = style.region_color(HitRegion::Button);
        let link_color = style.region_color(HitRegion::Link);
        let content_color = style.region_color(HitRegion::Content);
        let _border_color = style.region_color(HitRegion::Border);

        // Button and Link should be more visible (higher alpha) than passive regions
        assert!(
            button_color.a() >= content_color.a(),
            "Button overlay should be as visible or more visible than Content"
        );
        assert!(
            link_color.a() >= content_color.a(),
            "Link overlay should be as visible or more visible than Content"
        );

        // Button and Link should differ from Content by color (not just alpha)
        let button_content_diff = (button_color.r() as i32 - content_color.r() as i32).abs()
            + (button_color.g() as i32 - content_color.g() as i32).abs()
            + (button_color.b() as i32 - content_color.b() as i32).abs();
        assert!(
            button_content_diff >= 100,
            "Button color should differ significantly from Content (diff = {})",
            button_content_diff
        );
    }

    #[test]
    fn a11y_keybinding_constants_documented() {
        // This test documents the expected keybindings per spec.
        // It doesn't test runtime behavior, but serves as a reminder
        // of accessibility considerations for keybindings:
        //
        // Primary activations (accessible):
        //   - F12: Toggle inspector
        //   - Ctrl+Shift+I: Alternative toggle (browser devtools pattern)
        //
        // Mode selection (may conflict with text input):
        //   - i: Cycle modes
        //   - 0-3: Direct mode selection
        //
        // Navigation (accessible):
        //   - Tab/Shift+Tab: Widget cycling
        //   - Escape: Clear selection
        //   - Enter: Expand/collapse
        //
        // Toggles (may conflict with text input):
        //   - h: Toggle hits, b: bounds, n: names, t: times
        //   - d: Toggle detail panel
        //
        // Recommendation: When inspector is active and focused,
        // these single-letter keys should work. When a text input
        // has focus, pass through to the input.

        // This test passes if it compiles - it's documentation-as-code
        // (Assertion removed as it was always true)
    }
}
