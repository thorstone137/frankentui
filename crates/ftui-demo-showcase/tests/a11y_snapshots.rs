#![forbid(unsafe_code)]

//! Accessibility mode snapshot tests for the FrankenTUI Demo Showcase.
//!
//! This module tests visual output under different accessibility configurations:
//! - High contrast mode
//! - Large text mode
//! - Reduced motion mode
//! - Combined accessibility modes
//!
//! Run `BLESS=1 cargo test -p ftui-demo-showcase` to update snapshot baselines.
//! Set `E2E_JSONL=1` or run in CI for verbose JSONL logging.
//!
//! Naming convention: `a11y_{mode}_{screen}_{WIDTHxHEIGHT}`

use ftui_core::geometry::Rect;
use ftui_demo_showcase::app::AppModel;
use ftui_demo_showcase::screens::Screen;
use ftui_demo_showcase::theme;
use ftui_harness::assert_snapshot;
use ftui_render::frame::Frame;
use ftui_render::grapheme_pool::GraphemePool;
use ftui_runtime::program::Model;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;

// ---------------------------------------------------------------------------
// JSONL Logging Infrastructure
// ---------------------------------------------------------------------------

fn generate_run_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("a11y-{:x}", ts)
}

fn hash_frame(frame: &ftui_render::buffer::Buffer) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Hash dimensions
    frame.width().hash(&mut hasher);
    frame.height().hash(&mut hasher);
    // Hash raw buffer content via cell count and sampling
    let total_cells = (frame.width() as usize) * (frame.height() as usize);
    total_cells.hash(&mut hasher);
    // Sample cells at regular intervals for a fast hash
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            if let Some(cell) = frame.get(x, y) {
                cell.content.raw().hash(&mut hasher);
                // Only hash every 4th cell's colors for speed
                if (x + y) % 4 == 0 {
                    cell.fg.hash(&mut hasher);
                    cell.bg.hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

fn hash_a11y_settings(settings: &theme::A11ySettings) -> u64 {
    let mut hasher = DefaultHasher::new();
    settings.high_contrast.hash(&mut hasher);
    settings.reduced_motion.hash(&mut hasher);
    settings.large_text.hash(&mut hasher);
    hasher.finish()
}

fn log_e2e(
    run_id: &str,
    case: &str,
    a11y: &theme::A11ySettings,
    width: u16,
    height: u16,
    frame_hash: u64,
    settings_hash: u64,
    setup_ms: f64,
    render_ms: f64,
    snapshot_ms: f64,
    total_ms: f64,
) {
    if std::env::var("E2E_JSONL").is_ok() || std::env::var("CI").is_ok() {
        eprintln!(
            r#"{{"run_id":"{}","case":"{}","env":{{"os":"{}","test_module":"a11y_snapshots"}},"seed":null,"timings":{{"setup_ms":{:.3},"render_ms":{:.3},"snapshot_ms":{:.3},"total_ms":{:.3}}},"checksums":{{"frame_hash":{},"a11y_settings_hash":{}}},"capabilities":{{"high_contrast":{},"reduced_motion":{},"large_text":{},"terminal_width":{},"terminal_height":{}}},"outcome":"pass"}}"#,
            run_id,
            case,
            std::env::consts::OS,
            setup_ms,
            render_ms,
            snapshot_ms,
            total_ms,
            frame_hash,
            settings_hash,
            a11y.high_contrast,
            a11y.reduced_motion,
            a11y.large_text,
            width,
            height
        );
    }
}

// ---------------------------------------------------------------------------
// Test Helpers
// ---------------------------------------------------------------------------

struct A11yTestContext {
    app: AppModel,
    run_id: String,
    setup_start: Instant,
}

impl A11yTestContext {
    fn new() -> Self {
        let setup_start = Instant::now();
        Self {
            app: AppModel::new(),
            run_id: generate_run_id(),
            setup_start,
        }
    }

    fn with_high_contrast(mut self) -> Self {
        self.app.a11y.high_contrast = true;
        self
    }

    fn with_reduced_motion(mut self) -> Self {
        self.app.a11y.reduced_motion = true;
        self
    }

    fn with_large_text(mut self) -> Self {
        self.app.a11y.large_text = true;
        self
    }

    fn with_all_a11y(mut self) -> Self {
        self.app.a11y = theme::A11ySettings::all();
        self
    }

    fn render_and_snapshot(mut self, name: &str, width: u16, height: u16) {
        let setup_elapsed = self.setup_start.elapsed();

        // Always set theme state explicitly for test isolation
        let theme_id = if self.app.a11y.high_contrast {
            theme::ThemeId::Darcula
        } else {
            theme::ThemeId::CyberpunkAurora
        };
        theme::set_theme(theme_id);
        theme::set_motion_scale(if self.app.a11y.reduced_motion {
            0.0
        } else {
            1.0
        });
        theme::set_large_text(self.app.a11y.large_text);

        // Update terminal dimensions for proper rendering
        self.app.terminal_width = width;
        self.app.terminal_height = height;

        let render_start = Instant::now();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(width, height, &mut pool);
        self.app.view(&mut frame);
        let render_elapsed = render_start.elapsed();

        let frame_hash = hash_frame(&frame.buffer);
        let settings_hash = hash_a11y_settings(&self.app.a11y);

        let snapshot_start = Instant::now();
        assert_snapshot!(name, &frame.buffer);
        let snapshot_elapsed = snapshot_start.elapsed();

        let total_elapsed = self.setup_start.elapsed();

        log_e2e(
            &self.run_id,
            name,
            &self.app.a11y,
            width,
            height,
            frame_hash,
            settings_hash,
            setup_elapsed.as_secs_f64() * 1000.0,
            render_elapsed.as_secs_f64() * 1000.0,
            snapshot_elapsed.as_secs_f64() * 1000.0,
            total_elapsed.as_secs_f64() * 1000.0,
        );
    }
}

/// Helper for per-screen a11y testing.
/// Always sets theme state explicitly to ensure test isolation.
fn render_screen_with_a11y<S: Screen>(
    screen: &S,
    a11y: &theme::A11ySettings,
    width: u16,
    height: u16,
) -> Frame<'static> {
    // Always set theme state explicitly for test isolation
    let theme_id = if a11y.high_contrast {
        theme::ThemeId::Darcula
    } else {
        theme::ThemeId::CyberpunkAurora
    };
    theme::set_theme(theme_id);
    theme::set_motion_scale(if a11y.reduced_motion { 0.0 } else { 1.0 });
    theme::set_large_text(a11y.large_text);

    let pool = Box::leak(Box::new(GraphemePool::new()));
    let mut frame = Frame::new(width, height, pool);
    let area = Rect::new(0, 0, width, height);
    screen.view(&mut frame, area);
    frame
}

// ============================================================================
// High Contrast Mode Tests
// ============================================================================

#[test]
fn a11y_high_contrast_dashboard_80x24() {
    A11yTestContext::new()
        .with_high_contrast()
        .render_and_snapshot("a11y_high_contrast_dashboard_80x24", 80, 24);
}

#[test]
fn a11y_high_contrast_dashboard_120x40() {
    A11yTestContext::new()
        .with_high_contrast()
        .render_and_snapshot("a11y_high_contrast_dashboard_120x40", 120, 40);
}

#[test]
fn a11y_high_contrast_shakespeare_80x24() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::Shakespeare;
    ctx.render_and_snapshot("a11y_high_contrast_shakespeare_80x24", 80, 24);
}

#[test]
fn a11y_high_contrast_shakespeare_120x40() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::Shakespeare;
    ctx.render_and_snapshot("a11y_high_contrast_shakespeare_120x40", 120, 40);
}

#[test]
fn a11y_high_contrast_widget_gallery_80x24() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::WidgetGallery;
    ctx.render_and_snapshot("a11y_high_contrast_widget_gallery_80x24", 80, 24);
}

#[test]
fn a11y_high_contrast_widget_gallery_120x40() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::WidgetGallery;
    ctx.render_and_snapshot("a11y_high_contrast_widget_gallery_120x40", 120, 40);
}

#[test]
fn a11y_high_contrast_forms_input_80x24() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::FormsInput;
    ctx.render_and_snapshot("a11y_high_contrast_forms_input_80x24", 80, 24);
}

#[test]
fn a11y_high_contrast_forms_input_120x40() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::FormsInput;
    ctx.render_and_snapshot("a11y_high_contrast_forms_input_120x40", 120, 40);
}

#[test]
fn a11y_high_contrast_data_viz_80x24() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::DataViz;
    ctx.render_and_snapshot("a11y_high_contrast_data_viz_80x24", 80, 24);
}

#[test]
fn a11y_high_contrast_data_viz_120x40() {
    let mut ctx = A11yTestContext::new().with_high_contrast();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::DataViz;
    ctx.render_and_snapshot("a11y_high_contrast_data_viz_120x40", 120, 40);
}

#[test]
fn a11y_high_contrast_tiny_40x10() {
    A11yTestContext::new()
        .with_high_contrast()
        .render_and_snapshot("a11y_high_contrast_tiny_40x10", 40, 10);
}

// ============================================================================
// Large Text Mode Tests
// ============================================================================

#[test]
fn a11y_large_text_dashboard_80x24() {
    A11yTestContext::new()
        .with_large_text()
        .render_and_snapshot("a11y_large_text_dashboard_80x24", 80, 24);
}

#[test]
fn a11y_large_text_dashboard_120x40() {
    A11yTestContext::new()
        .with_large_text()
        .render_and_snapshot("a11y_large_text_dashboard_120x40", 120, 40);
}

#[test]
fn a11y_large_text_shakespeare_80x24() {
    let mut ctx = A11yTestContext::new().with_large_text();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::Shakespeare;
    ctx.render_and_snapshot("a11y_large_text_shakespeare_80x24", 80, 24);
}

#[test]
fn a11y_large_text_shakespeare_120x40() {
    let mut ctx = A11yTestContext::new().with_large_text();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::Shakespeare;
    ctx.render_and_snapshot("a11y_large_text_shakespeare_120x40", 120, 40);
}

#[test]
fn a11y_large_text_widget_gallery_80x24() {
    let mut ctx = A11yTestContext::new().with_large_text();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::WidgetGallery;
    ctx.render_and_snapshot("a11y_large_text_widget_gallery_80x24", 80, 24);
}

#[test]
fn a11y_large_text_widget_gallery_120x40() {
    let mut ctx = A11yTestContext::new().with_large_text();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::WidgetGallery;
    ctx.render_and_snapshot("a11y_large_text_widget_gallery_120x40", 120, 40);
}

#[test]
fn a11y_large_text_forms_input_80x24() {
    let mut ctx = A11yTestContext::new().with_large_text();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::FormsInput;
    ctx.render_and_snapshot("a11y_large_text_forms_input_80x24", 80, 24);
}

#[test]
fn a11y_large_text_forms_input_120x40() {
    let mut ctx = A11yTestContext::new().with_large_text();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::FormsInput;
    ctx.render_and_snapshot("a11y_large_text_forms_input_120x40", 120, 40);
}

#[test]
fn a11y_large_text_tiny_40x10() {
    A11yTestContext::new()
        .with_large_text()
        .render_and_snapshot("a11y_large_text_tiny_40x10", 40, 10);
}

// ============================================================================
// Reduced Motion Mode Tests
// ============================================================================

#[test]
fn a11y_reduced_motion_dashboard_80x24() {
    A11yTestContext::new()
        .with_reduced_motion()
        .render_and_snapshot("a11y_reduced_motion_dashboard_80x24", 80, 24);
}

#[test]
fn a11y_reduced_motion_dashboard_120x40() {
    A11yTestContext::new()
        .with_reduced_motion()
        .render_and_snapshot("a11y_reduced_motion_dashboard_120x40", 120, 40);
}

#[test]
fn a11y_reduced_motion_data_viz_80x24() {
    let mut ctx = A11yTestContext::new().with_reduced_motion();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::DataViz;
    ctx.render_and_snapshot("a11y_reduced_motion_data_viz_80x24", 80, 24);
}

#[test]
fn a11y_reduced_motion_data_viz_120x40() {
    let mut ctx = A11yTestContext::new().with_reduced_motion();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::DataViz;
    ctx.render_and_snapshot("a11y_reduced_motion_data_viz_120x40", 120, 40);
}

#[test]
fn a11y_reduced_motion_widget_gallery_80x24() {
    let mut ctx = A11yTestContext::new().with_reduced_motion();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::WidgetGallery;
    ctx.render_and_snapshot("a11y_reduced_motion_widget_gallery_80x24", 80, 24);
}

#[test]
fn a11y_reduced_motion_widget_gallery_120x40() {
    let mut ctx = A11yTestContext::new().with_reduced_motion();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::WidgetGallery;
    ctx.render_and_snapshot("a11y_reduced_motion_widget_gallery_120x40", 120, 40);
}

#[test]
fn a11y_reduced_motion_after_ticks_120x40() {
    let mut ctx = A11yTestContext::new().with_reduced_motion();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::DataViz;
    // With reduced motion enabled, animations should be static.
    // We test the visual output to ensure motion_scale=0 is applied correctly.
    ctx.render_and_snapshot("a11y_reduced_motion_after_ticks_120x40", 120, 40);
}

#[test]
fn a11y_reduced_motion_tiny_40x10() {
    A11yTestContext::new()
        .with_reduced_motion()
        .render_and_snapshot("a11y_reduced_motion_tiny_40x10", 40, 10);
}

// ============================================================================
// Combined Accessibility Modes Tests
// ============================================================================

#[test]
fn a11y_all_modes_dashboard_80x24() {
    A11yTestContext::new().with_all_a11y().render_and_snapshot(
        "a11y_all_modes_dashboard_80x24",
        80,
        24,
    );
}

#[test]
fn a11y_all_modes_dashboard_120x40() {
    A11yTestContext::new().with_all_a11y().render_and_snapshot(
        "a11y_all_modes_dashboard_120x40",
        120,
        40,
    );
}

#[test]
fn a11y_all_modes_shakespeare_80x24() {
    let mut ctx = A11yTestContext::new().with_all_a11y();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::Shakespeare;
    ctx.render_and_snapshot("a11y_all_modes_shakespeare_80x24", 80, 24);
}

#[test]
fn a11y_all_modes_widget_gallery_120x40() {
    let mut ctx = A11yTestContext::new().with_all_a11y();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::WidgetGallery;
    ctx.render_and_snapshot("a11y_all_modes_widget_gallery_120x40", 120, 40);
}

#[test]
fn a11y_all_modes_forms_input_120x40() {
    let mut ctx = A11yTestContext::new().with_all_a11y();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::FormsInput;
    ctx.render_and_snapshot("a11y_all_modes_forms_input_120x40", 120, 40);
}

#[test]
fn a11y_all_modes_data_viz_120x40() {
    let mut ctx = A11yTestContext::new().with_all_a11y();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::DataViz;
    ctx.render_and_snapshot("a11y_all_modes_data_viz_120x40", 120, 40);
}

#[test]
fn a11y_all_modes_tiny_40x10() {
    A11yTestContext::new()
        .with_all_a11y()
        .render_and_snapshot("a11y_all_modes_tiny_40x10", 40, 10);
}

#[test]
fn a11y_all_modes_wide_200x50() {
    A11yTestContext::new().with_all_a11y().render_and_snapshot(
        "a11y_all_modes_wide_200x50",
        200,
        50,
    );
}

// ============================================================================
// High Contrast + Large Text (common combination)
// ============================================================================

#[test]
fn a11y_high_contrast_large_text_dashboard_80x24() {
    A11yTestContext::new()
        .with_high_contrast()
        .with_large_text()
        .render_and_snapshot("a11y_high_contrast_large_text_dashboard_80x24", 80, 24);
}

#[test]
fn a11y_high_contrast_large_text_dashboard_120x40() {
    A11yTestContext::new()
        .with_high_contrast()
        .with_large_text()
        .render_and_snapshot("a11y_high_contrast_large_text_dashboard_120x40", 120, 40);
}

#[test]
fn a11y_high_contrast_large_text_forms_input_120x40() {
    let mut ctx = A11yTestContext::new()
        .with_high_contrast()
        .with_large_text();
    ctx.app.current_screen = ftui_demo_showcase::app::ScreenId::FormsInput;
    ctx.render_and_snapshot("a11y_high_contrast_large_text_forms_input_120x40", 120, 40);
}

// ============================================================================
// Per-Screen Individual A11y Tests (Dashboard screen variants)
// ============================================================================

#[test]
fn a11y_dashboard_screen_high_contrast_80x24() {
    let screen = ftui_demo_showcase::screens::dashboard::Dashboard::new();
    let a11y = theme::A11ySettings {
        high_contrast: true,
        reduced_motion: false,
        large_text: false,
    };
    let frame = render_screen_with_a11y(&screen, &a11y, 80, 24);
    assert_snapshot!("a11y_dashboard_screen_high_contrast_80x24", &frame.buffer);
}

#[test]
fn a11y_dashboard_screen_large_text_80x24() {
    let screen = ftui_demo_showcase::screens::dashboard::Dashboard::new();
    let a11y = theme::A11ySettings {
        high_contrast: false,
        reduced_motion: false,
        large_text: true,
    };
    let frame = render_screen_with_a11y(&screen, &a11y, 80, 24);
    assert_snapshot!("a11y_dashboard_screen_large_text_80x24", &frame.buffer);
}

#[test]
fn a11y_dashboard_screen_reduced_motion_80x24() {
    let screen = ftui_demo_showcase::screens::dashboard::Dashboard::new();
    let a11y = theme::A11ySettings {
        high_contrast: false,
        reduced_motion: true,
        large_text: false,
    };
    let frame = render_screen_with_a11y(&screen, &a11y, 80, 24);
    assert_snapshot!("a11y_dashboard_screen_reduced_motion_80x24", &frame.buffer);
}

// ============================================================================
// Edge Cases & Regression Tests
// ============================================================================

#[test]
fn a11y_zero_area_high_contrast() {
    let mut app = AppModel::new();
    app.a11y.high_contrast = true;
    theme::set_theme(theme::ThemeId::Darcula);
    let mut pool = GraphemePool::new();
    let mut frame = Frame::new(1, 1, &mut pool);
    app.view(&mut frame);
    // No panic = success
}

#[test]
fn a11y_zero_area_large_text() {
    let mut app = AppModel::new();
    app.a11y.large_text = true;
    theme::set_large_text(true);
    let mut pool = GraphemePool::new();
    let mut frame = Frame::new(1, 1, &mut pool);
    app.view(&mut frame);
    // No panic = success
}

#[test]
fn a11y_zero_area_all_modes() {
    let mut app = AppModel::new();
    app.a11y = theme::A11ySettings::all();
    theme::set_theme(theme::ThemeId::Darcula);
    theme::set_motion_scale(0.0);
    theme::set_large_text(true);
    let mut pool = GraphemePool::new();
    let mut frame = Frame::new(1, 1, &mut pool);
    app.view(&mut frame);
    // No panic = success
}

#[test]
fn a11y_settings_toggle_idempotent() {
    // Verify toggling settings twice returns to original state
    let mut app = AppModel::new();
    let original_high_contrast = app.a11y.high_contrast;
    let original_reduced_motion = app.a11y.reduced_motion;
    let original_large_text = app.a11y.large_text;

    // Toggle on
    app.a11y.high_contrast = !app.a11y.high_contrast;
    app.a11y.reduced_motion = !app.a11y.reduced_motion;
    app.a11y.large_text = !app.a11y.large_text;

    // Toggle off (back to original)
    app.a11y.high_contrast = !app.a11y.high_contrast;
    app.a11y.reduced_motion = !app.a11y.reduced_motion;
    app.a11y.large_text = !app.a11y.large_text;

    assert_eq!(app.a11y.high_contrast, original_high_contrast);
    assert_eq!(app.a11y.reduced_motion, original_reduced_motion);
    assert_eq!(app.a11y.large_text, original_large_text);
}

#[test]
fn a11y_settings_all_equals_individual() {
    // Verify A11ySettings::all() matches individual true settings
    let all = theme::A11ySettings::all();
    let individual = theme::A11ySettings {
        high_contrast: true,
        reduced_motion: true,
        large_text: true,
    };
    assert_eq!(all, individual);
}

#[test]
fn a11y_settings_none_equals_default() {
    // Verify A11ySettings::none() matches default
    let none = theme::A11ySettings::none();
    let default = theme::A11ySettings::default();
    assert_eq!(none, default);
}

// ============================================================================
// Full App Integration with A11y Panel Visible
// ============================================================================

#[test]
fn a11y_panel_visible_80x24() {
    // Set theme explicitly for test isolation
    theme::set_theme(theme::ThemeId::CyberpunkAurora);
    theme::set_motion_scale(1.0);
    theme::set_large_text(false);
    let mut app = AppModel::new();
    app.a11y_panel_visible = true;
    let mut pool = GraphemePool::new();
    let mut frame = Frame::new(80, 24, &mut pool);
    app.view(&mut frame);
    assert_snapshot!("a11y_panel_visible_80x24", &frame.buffer);
}

#[test]
fn a11y_panel_visible_120x40() {
    // Set theme explicitly for test isolation
    theme::set_theme(theme::ThemeId::CyberpunkAurora);
    theme::set_motion_scale(1.0);
    theme::set_large_text(false);
    let mut app = AppModel::new();
    app.a11y_panel_visible = true;
    let mut pool = GraphemePool::new();
    let mut frame = Frame::new(120, 40, &mut pool);
    app.view(&mut frame);
    assert_snapshot!("a11y_panel_visible_120x40", &frame.buffer);
}

#[test]
fn a11y_panel_with_high_contrast_120x40() {
    let mut app = AppModel::new();
    app.a11y_panel_visible = true;
    app.a11y.high_contrast = true;
    theme::set_theme(theme::ThemeId::Darcula);
    let mut pool = GraphemePool::new();
    let mut frame = Frame::new(120, 40, &mut pool);
    app.view(&mut frame);
    assert_snapshot!("a11y_panel_with_high_contrast_120x40", &frame.buffer);
}

#[test]
fn a11y_panel_with_all_modes_120x40() {
    let mut app = AppModel::new();
    app.a11y_panel_visible = true;
    app.a11y = theme::A11ySettings::all();
    theme::set_theme(theme::ThemeId::Darcula);
    theme::set_motion_scale(0.0);
    theme::set_large_text(true);
    let mut pool = GraphemePool::new();
    let mut frame = Frame::new(120, 40, &mut pool);
    app.view(&mut frame);
    assert_snapshot!("a11y_panel_with_all_modes_120x40", &frame.buffer);
}

// ============================================================================
// Determinism Tests (verify same input produces same output)
// ============================================================================

#[test]
fn a11y_determinism_high_contrast() {
    // Render twice and verify identical output
    let mut app1 = AppModel::new();
    app1.a11y.high_contrast = true;
    theme::set_theme(theme::ThemeId::Darcula);
    let mut pool1 = GraphemePool::new();
    let mut frame1 = Frame::new(80, 24, &mut pool1);
    app1.view(&mut frame1);
    let hash1 = hash_frame(&frame1.buffer);

    let mut app2 = AppModel::new();
    app2.a11y.high_contrast = true;
    theme::set_theme(theme::ThemeId::Darcula);
    let mut pool2 = GraphemePool::new();
    let mut frame2 = Frame::new(80, 24, &mut pool2);
    app2.view(&mut frame2);
    let hash2 = hash_frame(&frame2.buffer);

    assert_eq!(
        hash1, hash2,
        "High contrast mode should produce deterministic output"
    );
}

#[test]
fn a11y_determinism_all_modes() {
    // Render twice with all a11y modes and verify identical output
    let mut app1 = AppModel::new();
    app1.a11y = theme::A11ySettings::all();
    theme::set_theme(theme::ThemeId::Darcula);
    theme::set_motion_scale(0.0);
    theme::set_large_text(true);
    let mut pool1 = GraphemePool::new();
    let mut frame1 = Frame::new(120, 40, &mut pool1);
    app1.view(&mut frame1);
    let hash1 = hash_frame(&frame1.buffer);

    let mut app2 = AppModel::new();
    app2.a11y = theme::A11ySettings::all();
    theme::set_theme(theme::ThemeId::Darcula);
    theme::set_motion_scale(0.0);
    theme::set_large_text(true);
    let mut pool2 = GraphemePool::new();
    let mut frame2 = Frame::new(120, 40, &mut pool2);
    app2.view(&mut frame2);
    let hash2 = hash_frame(&frame2.buffer);

    assert_eq!(
        hash1, hash2,
        "All a11y modes should produce deterministic output"
    );
}
