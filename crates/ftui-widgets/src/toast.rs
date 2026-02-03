#![forbid(unsafe_code)]

//! Toast widget for displaying transient notifications.
//!
//! A toast is a non-blocking notification that appears temporarily and
//! can be dismissed automatically or manually. Toasts support:
//!
//! - Multiple positions (corners and center top/bottom)
//! - Automatic dismissal with configurable duration
//! - Icons for different message types (success, error, warning, info)
//! - Semantic styling that integrates with the theme system
//!
//! # Example
//!
//! ```ignore
//! let toast = Toast::new("File saved successfully")
//!     .icon(ToastIcon::Success)
//!     .position(ToastPosition::TopRight)
//!     .duration(Duration::from_secs(3));
//! ```

use std::time::{Duration, Instant};

use ftui_core::geometry::Rect;
use ftui_render::cell::Cell;
use ftui_render::frame::Frame;
use ftui_style::Style;
use unicode_width::UnicodeWidthStr;

use crate::{Widget, set_style_area};

/// Unique identifier for a toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToastId(pub u64);

impl ToastId {
    /// Create a new toast ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Position where the toast should be displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastPosition {
    /// Top-left corner.
    TopLeft,
    /// Top center.
    TopCenter,
    /// Top-right corner.
    #[default]
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom center.
    BottomCenter,
    /// Bottom-right corner.
    BottomRight,
}

impl ToastPosition {
    /// Calculate the toast's top-left position within a terminal area.
    ///
    /// Returns `(x, y)` for the toast's origin given its dimensions.
    pub fn calculate_position(
        self,
        terminal_width: u16,
        terminal_height: u16,
        toast_width: u16,
        toast_height: u16,
        margin: u16,
    ) -> (u16, u16) {
        let x = match self {
            Self::TopLeft | Self::BottomLeft => margin,
            Self::TopCenter | Self::BottomCenter => terminal_width.saturating_sub(toast_width) / 2,
            Self::TopRight | Self::BottomRight => terminal_width
                .saturating_sub(toast_width)
                .saturating_sub(margin),
        };

        let y = match self {
            Self::TopLeft | Self::TopCenter | Self::TopRight => margin,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight => terminal_height
                .saturating_sub(toast_height)
                .saturating_sub(margin),
        };

        (x, y)
    }
}

/// Icon displayed in the toast to indicate message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastIcon {
    /// Success indicator (checkmark).
    Success,
    /// Error indicator (X mark).
    Error,
    /// Warning indicator (exclamation).
    Warning,
    /// Information indicator (i).
    #[default]
    Info,
    /// Custom single character.
    Custom(char),
}

impl ToastIcon {
    /// Get the display character for this icon.
    pub fn as_char(self) -> char {
        match self {
            Self::Success => '\u{2713}', // ✓
            Self::Error => '\u{2717}',   // ✗
            Self::Warning => '!',
            Self::Info => 'i',
            Self::Custom(c) => c,
        }
    }

    /// Get the fallback ASCII character for degraded rendering.
    pub fn as_ascii(self) -> char {
        match self {
            Self::Success => '+',
            Self::Error => 'x',
            Self::Warning => '!',
            Self::Info => 'i',
            Self::Custom(c) if c.is_ascii() => c,
            Self::Custom(_) => '*',
        }
    }
}

/// Visual style variant for the toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastStyle {
    /// Success style (typically green).
    Success,
    /// Error style (typically red).
    Error,
    /// Warning style (typically yellow/orange).
    Warning,
    /// Informational style (typically blue).
    #[default]
    Info,
    /// Neutral style (no semantic coloring).
    Neutral,
}

// ============================================================================
// Animation Types
// ============================================================================

/// Animation phase for toast lifecycle.
///
/// Toasts progress through these phases: Entering → Visible → Exiting → Hidden.
/// The animation system tracks progress within each phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastAnimationPhase {
    /// Toast is animating in (slide/fade entrance).
    Entering,
    /// Toast is fully visible (no animation).
    #[default]
    Visible,
    /// Toast is animating out (slide/fade exit).
    Exiting,
    /// Toast has completed exit animation.
    Hidden,
}

/// Entrance animation type.
///
/// Determines how the toast appears on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastEntranceAnimation {
    /// Slide in from the top edge.
    SlideFromTop,
    /// Slide in from the right edge.
    #[default]
    SlideFromRight,
    /// Slide in from the bottom edge.
    SlideFromBottom,
    /// Slide in from the left edge.
    SlideFromLeft,
    /// Fade in (opacity transition).
    FadeIn,
    /// No animation (instant appear).
    None,
}

impl ToastEntranceAnimation {
    /// Get the initial offset for this entrance animation.
    ///
    /// Returns (dx, dy) offset in cells from the final position.
    pub fn initial_offset(self, toast_width: u16, toast_height: u16) -> (i16, i16) {
        match self {
            Self::SlideFromTop => (0, -(toast_height as i16)),
            Self::SlideFromRight => (toast_width as i16, 0),
            Self::SlideFromBottom => (0, toast_height as i16),
            Self::SlideFromLeft => (-(toast_width as i16), 0),
            Self::FadeIn | Self::None => (0, 0),
        }
    }

    /// Calculate the offset at a given progress (0.0 to 1.0).
    ///
    /// Progress of 0.0 = initial offset, 1.0 = no offset.
    pub fn offset_at_progress(
        self,
        progress: f64,
        toast_width: u16,
        toast_height: u16,
    ) -> (i16, i16) {
        let (dx, dy) = self.initial_offset(toast_width, toast_height);
        let inv_progress = 1.0 - progress.clamp(0.0, 1.0);
        (
            (dx as f64 * inv_progress).round() as i16,
            (dy as f64 * inv_progress).round() as i16,
        )
    }

    /// Check if this animation affects position (vs. just opacity).
    pub fn affects_position(self) -> bool {
        !matches!(self, Self::FadeIn | Self::None)
    }
}

/// Exit animation type.
///
/// Determines how the toast disappears from screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastExitAnimation {
    /// Fade out (opacity transition).
    #[default]
    FadeOut,
    /// Slide out in the reverse of entrance direction.
    SlideOut,
    /// Slide out to the specified edge.
    SlideToTop,
    SlideToRight,
    SlideToBottom,
    SlideToLeft,
    /// No animation (instant disappear).
    None,
}

impl ToastExitAnimation {
    /// Get the final offset for this exit animation.
    ///
    /// Returns (dx, dy) offset in cells from the starting position.
    pub fn final_offset(
        self,
        toast_width: u16,
        toast_height: u16,
        entrance: ToastEntranceAnimation,
    ) -> (i16, i16) {
        match self {
            Self::SlideOut => {
                // Reverse of entrance direction
                let (dx, dy) = entrance.initial_offset(toast_width, toast_height);
                (-dx, -dy)
            }
            Self::SlideToTop => (0, -(toast_height as i16)),
            Self::SlideToRight => (toast_width as i16, 0),
            Self::SlideToBottom => (0, toast_height as i16),
            Self::SlideToLeft => (-(toast_width as i16), 0),
            Self::FadeOut | Self::None => (0, 0),
        }
    }

    /// Calculate the offset at a given progress (0.0 to 1.0).
    ///
    /// Progress of 0.0 = no offset, 1.0 = final offset.
    pub fn offset_at_progress(
        self,
        progress: f64,
        toast_width: u16,
        toast_height: u16,
        entrance: ToastEntranceAnimation,
    ) -> (i16, i16) {
        let (dx, dy) = self.final_offset(toast_width, toast_height, entrance);
        let p = progress.clamp(0.0, 1.0);
        (
            (dx as f64 * p).round() as i16,
            (dy as f64 * p).round() as i16,
        )
    }

    /// Check if this animation affects position (vs. just opacity).
    pub fn affects_position(self) -> bool {
        !matches!(self, Self::FadeOut | Self::None)
    }
}

/// Easing function for animations.
///
/// Simplified subset of easing curves for toast animations.
/// For the full set, see `ftui_extras::text_effects::Easing`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ToastEasing {
    /// Linear interpolation.
    Linear,
    /// Smooth ease-out (decelerating).
    #[default]
    EaseOut,
    /// Smooth ease-in (accelerating).
    EaseIn,
    /// Smooth S-curve.
    EaseInOut,
    /// Bouncy effect.
    Bounce,
}

impl ToastEasing {
    /// Apply the easing function to a progress value (0.0 to 1.0).
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseOut => {
                let inv = 1.0 - t;
                1.0 - inv * inv * inv
            }
            Self::EaseIn => t * t * t,
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let inv = -2.0 * t + 2.0;
                    1.0 - inv * inv * inv / 2.0
                }
            }
            Self::Bounce => {
                let n1 = 7.5625;
                let d1 = 2.75;
                let mut t = t;
                if t < 1.0 / d1 {
                    n1 * t * t
                } else if t < 2.0 / d1 {
                    t -= 1.5 / d1;
                    n1 * t * t + 0.75
                } else if t < 2.5 / d1 {
                    t -= 2.25 / d1;
                    n1 * t * t + 0.9375
                } else {
                    t -= 2.625 / d1;
                    n1 * t * t + 0.984375
                }
            }
        }
    }
}

/// Animation configuration for a toast.
#[derive(Debug, Clone)]
pub struct ToastAnimationConfig {
    /// Entrance animation type.
    pub entrance: ToastEntranceAnimation,
    /// Exit animation type.
    pub exit: ToastExitAnimation,
    /// Duration of entrance animation.
    pub entrance_duration: Duration,
    /// Duration of exit animation.
    pub exit_duration: Duration,
    /// Easing function for entrance.
    pub entrance_easing: ToastEasing,
    /// Easing function for exit.
    pub exit_easing: ToastEasing,
    /// Whether to respect reduced-motion preference.
    pub respect_reduced_motion: bool,
}

impl Default for ToastAnimationConfig {
    fn default() -> Self {
        Self {
            entrance: ToastEntranceAnimation::default(),
            exit: ToastExitAnimation::default(),
            entrance_duration: Duration::from_millis(200),
            exit_duration: Duration::from_millis(150),
            entrance_easing: ToastEasing::EaseOut,
            exit_easing: ToastEasing::EaseIn,
            respect_reduced_motion: true,
        }
    }
}

impl ToastAnimationConfig {
    /// Create a config with no animations.
    pub fn none() -> Self {
        Self {
            entrance: ToastEntranceAnimation::None,
            exit: ToastExitAnimation::None,
            entrance_duration: Duration::ZERO,
            exit_duration: Duration::ZERO,
            ..Default::default()
        }
    }

    /// Check if animations are effectively disabled.
    pub fn is_disabled(&self) -> bool {
        matches!(self.entrance, ToastEntranceAnimation::None)
            && matches!(self.exit, ToastExitAnimation::None)
    }
}

/// Tracks the animation state for a toast.
#[derive(Debug, Clone)]
pub struct ToastAnimationState {
    /// Current animation phase.
    pub phase: ToastAnimationPhase,
    /// When the current phase started.
    pub phase_started: Instant,
    /// Whether reduced motion is active.
    pub reduced_motion: bool,
}

impl Default for ToastAnimationState {
    fn default() -> Self {
        Self {
            phase: ToastAnimationPhase::Entering,
            phase_started: Instant::now(),
            reduced_motion: false,
        }
    }
}

impl ToastAnimationState {
    /// Create a new animation state starting in the Entering phase.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a state with reduced motion enabled (skips to Visible).
    pub fn with_reduced_motion() -> Self {
        Self {
            phase: ToastAnimationPhase::Visible,
            phase_started: Instant::now(),
            reduced_motion: true,
        }
    }

    /// Get the progress within the current phase (0.0 to 1.0).
    pub fn progress(&self, phase_duration: Duration) -> f64 {
        if phase_duration.is_zero() {
            return 1.0;
        }
        let elapsed = self.phase_started.elapsed();
        (elapsed.as_secs_f64() / phase_duration.as_secs_f64()).min(1.0)
    }

    /// Transition to the next phase.
    pub fn transition_to(&mut self, phase: ToastAnimationPhase) {
        self.phase = phase;
        self.phase_started = Instant::now();
    }

    /// Start the exit animation.
    pub fn start_exit(&mut self) {
        if self.reduced_motion {
            self.transition_to(ToastAnimationPhase::Hidden);
        } else {
            self.transition_to(ToastAnimationPhase::Exiting);
        }
    }

    /// Check if the animation has completed (Hidden phase).
    pub fn is_complete(&self) -> bool {
        self.phase == ToastAnimationPhase::Hidden
    }

    /// Update the animation state based on elapsed time.
    ///
    /// Returns true if the phase changed.
    pub fn tick(&mut self, config: &ToastAnimationConfig) -> bool {
        let prev_phase = self.phase;

        match self.phase {
            ToastAnimationPhase::Entering => {
                let duration = if self.reduced_motion {
                    Duration::ZERO
                } else {
                    config.entrance_duration
                };
                if self.progress(duration) >= 1.0 {
                    self.transition_to(ToastAnimationPhase::Visible);
                }
            }
            ToastAnimationPhase::Exiting => {
                let duration = if self.reduced_motion {
                    Duration::ZERO
                } else {
                    config.exit_duration
                };
                if self.progress(duration) >= 1.0 {
                    self.transition_to(ToastAnimationPhase::Hidden);
                }
            }
            ToastAnimationPhase::Visible | ToastAnimationPhase::Hidden => {}
        }

        self.phase != prev_phase
    }

    /// Calculate the current animation offset.
    ///
    /// Returns (dx, dy) offset to apply to the toast position.
    pub fn current_offset(
        &self,
        config: &ToastAnimationConfig,
        toast_width: u16,
        toast_height: u16,
    ) -> (i16, i16) {
        if self.reduced_motion {
            return (0, 0);
        }

        match self.phase {
            ToastAnimationPhase::Entering => {
                let raw_progress = self.progress(config.entrance_duration);
                let eased_progress = config.entrance_easing.apply(raw_progress);
                config
                    .entrance
                    .offset_at_progress(eased_progress, toast_width, toast_height)
            }
            ToastAnimationPhase::Exiting => {
                let raw_progress = self.progress(config.exit_duration);
                let eased_progress = config.exit_easing.apply(raw_progress);
                config.exit.offset_at_progress(
                    eased_progress,
                    toast_width,
                    toast_height,
                    config.entrance,
                )
            }
            ToastAnimationPhase::Visible => (0, 0),
            ToastAnimationPhase::Hidden => (0, 0),
        }
    }

    /// Calculate the current opacity (0.0 to 1.0).
    ///
    /// Used for fade animations.
    pub fn current_opacity(&self, config: &ToastAnimationConfig) -> f64 {
        if self.reduced_motion {
            return if self.phase == ToastAnimationPhase::Hidden {
                0.0
            } else {
                1.0
            };
        }

        match self.phase {
            ToastAnimationPhase::Entering => {
                if matches!(config.entrance, ToastEntranceAnimation::FadeIn) {
                    let raw_progress = self.progress(config.entrance_duration);
                    config.entrance_easing.apply(raw_progress)
                } else {
                    1.0
                }
            }
            ToastAnimationPhase::Exiting => {
                if matches!(config.exit, ToastExitAnimation::FadeOut) {
                    let raw_progress = self.progress(config.exit_duration);
                    1.0 - config.exit_easing.apply(raw_progress)
                } else {
                    1.0
                }
            }
            ToastAnimationPhase::Visible => 1.0,
            ToastAnimationPhase::Hidden => 0.0,
        }
    }
}

/// Configuration for a toast notification.
#[derive(Debug, Clone)]
pub struct ToastConfig {
    /// Position on screen.
    pub position: ToastPosition,
    /// Auto-dismiss duration. `None` means persistent until dismissed.
    pub duration: Option<Duration>,
    /// Visual style variant.
    pub style_variant: ToastStyle,
    /// Maximum width in columns.
    pub max_width: u16,
    /// Margin from screen edges.
    pub margin: u16,
    /// Whether the toast can be dismissed by the user.
    pub dismissable: bool,
    /// Animation configuration.
    pub animation: ToastAnimationConfig,
}

impl Default for ToastConfig {
    fn default() -> Self {
        Self {
            position: ToastPosition::default(),
            duration: Some(Duration::from_secs(5)),
            style_variant: ToastStyle::default(),
            max_width: 50,
            margin: 1,
            dismissable: true,
            animation: ToastAnimationConfig::default(),
        }
    }
}

/// Content of a toast notification.
#[derive(Debug, Clone)]
pub struct ToastContent {
    /// Main message text.
    pub message: String,
    /// Optional icon.
    pub icon: Option<ToastIcon>,
    /// Optional title.
    pub title: Option<String>,
}

impl ToastContent {
    /// Create new content with just a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            icon: None,
            title: None,
        }
    }

    /// Set the icon.
    pub fn with_icon(mut self, icon: ToastIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Internal state tracking for a toast.
#[derive(Debug, Clone)]
pub struct ToastState {
    /// When the toast was created.
    pub created_at: Instant,
    /// Whether the toast has been dismissed.
    pub dismissed: bool,
    /// Animation state.
    pub animation: ToastAnimationState,
}

impl Default for ToastState {
    fn default() -> Self {
        Self {
            created_at: Instant::now(),
            dismissed: false,
            animation: ToastAnimationState::default(),
        }
    }
}

impl ToastState {
    /// Create a new state with reduced motion enabled.
    pub fn with_reduced_motion() -> Self {
        Self {
            created_at: Instant::now(),
            dismissed: false,
            animation: ToastAnimationState::with_reduced_motion(),
        }
    }
}

/// A toast notification widget.
///
/// Toasts display transient messages to the user, typically in a corner
/// of the screen. They can auto-dismiss after a duration or be manually
/// dismissed.
///
/// # Example
///
/// ```ignore
/// let toast = Toast::new("Operation completed")
///     .icon(ToastIcon::Success)
///     .position(ToastPosition::TopRight)
///     .duration(Duration::from_secs(3));
///
/// // Render the toast
/// toast.render(area, frame);
/// ```
#[derive(Debug, Clone)]
pub struct Toast {
    /// Unique identifier.
    pub id: ToastId,
    /// Toast content.
    pub content: ToastContent,
    /// Configuration.
    pub config: ToastConfig,
    /// Internal state.
    pub state: ToastState,
    /// Base style override.
    style: Style,
    /// Icon style override.
    icon_style: Style,
    /// Title style override.
    title_style: Style,
}

impl Toast {
    /// Create a new toast with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = ToastId::new(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed));

        Self {
            id,
            content: ToastContent::new(message),
            config: ToastConfig::default(),
            state: ToastState::default(),
            style: Style::default(),
            icon_style: Style::default(),
            title_style: Style::default(),
        }
    }

    /// Create a toast with a specific ID.
    pub fn with_id(id: ToastId, message: impl Into<String>) -> Self {
        Self {
            id,
            content: ToastContent::new(message),
            config: ToastConfig::default(),
            state: ToastState::default(),
            style: Style::default(),
            icon_style: Style::default(),
            title_style: Style::default(),
        }
    }

    // --- Builder methods ---

    /// Set the toast icon.
    pub fn icon(mut self, icon: ToastIcon) -> Self {
        self.content.icon = Some(icon);
        self
    }

    /// Set the toast title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.content.title = Some(title.into());
        self
    }

    /// Set the toast position.
    pub fn position(mut self, position: ToastPosition) -> Self {
        self.config.position = position;
        self
    }

    /// Set the auto-dismiss duration.
    pub fn duration(mut self, duration: Duration) -> Self {
        self.config.duration = Some(duration);
        self
    }

    /// Make the toast persistent (no auto-dismiss).
    pub fn persistent(mut self) -> Self {
        self.config.duration = None;
        self
    }

    /// Set the style variant.
    pub fn style_variant(mut self, variant: ToastStyle) -> Self {
        self.config.style_variant = variant;
        self
    }

    /// Set the maximum width.
    pub fn max_width(mut self, width: u16) -> Self {
        self.config.max_width = width;
        self
    }

    /// Set the margin from screen edges.
    pub fn margin(mut self, margin: u16) -> Self {
        self.config.margin = margin;
        self
    }

    /// Set whether the toast is dismissable.
    pub fn dismissable(mut self, dismissable: bool) -> Self {
        self.config.dismissable = dismissable;
        self
    }

    /// Set the base style.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the icon style.
    pub fn with_icon_style(mut self, style: Style) -> Self {
        self.icon_style = style;
        self
    }

    /// Set the title style.
    pub fn with_title_style(mut self, style: Style) -> Self {
        self.title_style = style;
        self
    }

    // --- Animation builder methods ---

    /// Set the entrance animation.
    pub fn entrance_animation(mut self, animation: ToastEntranceAnimation) -> Self {
        self.config.animation.entrance = animation;
        self
    }

    /// Set the exit animation.
    pub fn exit_animation(mut self, animation: ToastExitAnimation) -> Self {
        self.config.animation.exit = animation;
        self
    }

    /// Set the entrance animation duration.
    pub fn entrance_duration(mut self, duration: Duration) -> Self {
        self.config.animation.entrance_duration = duration;
        self
    }

    /// Set the exit animation duration.
    pub fn exit_duration(mut self, duration: Duration) -> Self {
        self.config.animation.exit_duration = duration;
        self
    }

    /// Set the entrance easing function.
    pub fn entrance_easing(mut self, easing: ToastEasing) -> Self {
        self.config.animation.entrance_easing = easing;
        self
    }

    /// Set the exit easing function.
    pub fn exit_easing(mut self, easing: ToastEasing) -> Self {
        self.config.animation.exit_easing = easing;
        self
    }

    /// Disable all animations.
    pub fn no_animation(mut self) -> Self {
        self.config.animation = ToastAnimationConfig::none();
        self.state.animation = ToastAnimationState {
            phase: ToastAnimationPhase::Visible,
            phase_started: Instant::now(),
            reduced_motion: true,
        };
        self
    }

    /// Enable reduced motion mode (skips animations).
    pub fn reduced_motion(mut self, enabled: bool) -> Self {
        self.config.animation.respect_reduced_motion = enabled;
        if enabled {
            self.state.animation = ToastAnimationState::with_reduced_motion();
        }
        self
    }

    // --- State methods ---

    /// Check if the toast has expired based on its duration.
    pub fn is_expired(&self) -> bool {
        if let Some(duration) = self.config.duration {
            self.state.created_at.elapsed() >= duration
        } else {
            false
        }
    }

    /// Check if the toast should be visible.
    ///
    /// A toast is visible if it's not dismissed, not expired, and not in
    /// the Hidden animation phase.
    pub fn is_visible(&self) -> bool {
        !self.state.dismissed
            && !self.is_expired()
            && self.state.animation.phase != ToastAnimationPhase::Hidden
    }

    /// Check if the toast is currently animating.
    pub fn is_animating(&self) -> bool {
        matches!(
            self.state.animation.phase,
            ToastAnimationPhase::Entering | ToastAnimationPhase::Exiting
        )
    }

    /// Dismiss the toast, starting exit animation.
    pub fn dismiss(&mut self) {
        if !self.state.dismissed {
            self.state.dismissed = true;
            self.state.animation.start_exit();
        }
    }

    /// Dismiss immediately without animation.
    pub fn dismiss_immediately(&mut self) {
        self.state.dismissed = true;
        self.state
            .animation
            .transition_to(ToastAnimationPhase::Hidden);
    }

    /// Update the animation state. Call this each frame.
    ///
    /// Returns true if the animation phase changed.
    pub fn tick_animation(&mut self) -> bool {
        self.state.animation.tick(&self.config.animation)
    }

    /// Get the current animation phase.
    pub fn animation_phase(&self) -> ToastAnimationPhase {
        self.state.animation.phase
    }

    /// Get the current animation offset for rendering.
    ///
    /// Returns (dx, dy) offset to apply to the position.
    pub fn animation_offset(&self) -> (i16, i16) {
        let (width, height) = self.calculate_dimensions();
        self.state
            .animation
            .current_offset(&self.config.animation, width, height)
    }

    /// Get the current opacity for rendering (0.0 to 1.0).
    pub fn animation_opacity(&self) -> f64 {
        self.state.animation.current_opacity(&self.config.animation)
    }

    /// Get the remaining time before auto-dismiss.
    pub fn remaining_time(&self) -> Option<Duration> {
        self.config.duration.map(|d| {
            let elapsed = self.state.created_at.elapsed();
            d.saturating_sub(elapsed)
        })
    }

    /// Calculate the toast dimensions based on content.
    pub fn calculate_dimensions(&self) -> (u16, u16) {
        let max_width = self.config.max_width as usize;

        // Calculate content width
        let icon_width = if self.content.icon.is_some() { 2 } else { 0 }; // icon + space
        let message_width = UnicodeWidthStr::width(self.content.message.as_str());
        let title_width = self
            .content
            .title
            .as_ref()
            .map(|t| UnicodeWidthStr::width(t.as_str()))
            .unwrap_or(0);

        // Content width is max of title and message (plus icon)
        let content_width = (icon_width + message_width).max(title_width);

        // Add padding (1 char each side) and border (1 char each side)
        let total_width = content_width.saturating_add(4).min(max_width);

        // Height: border (2) + optional title (1) + message (1) + padding (0)
        let has_title = self.content.title.is_some();
        let height = if has_title { 4 } else { 3 };

        (total_width as u16, height as u16)
    }
}

impl Widget for Toast {
    fn render(&self, area: Rect, frame: &mut Frame) {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "widget_render",
            widget = "Toast",
            x = area.x,
            y = area.y,
            w = area.width,
            h = area.height
        )
        .entered();

        if area.is_empty() || !self.is_visible() {
            return;
        }

        let deg = frame.buffer.degradation;

        // Calculate actual render area (use provided area or calculate from content)
        let (content_width, content_height) = self.calculate_dimensions();
        let width = area.width.min(content_width);
        let height = area.height.min(content_height);

        if width < 3 || height < 3 {
            return; // Too small to render
        }

        let render_area = Rect::new(area.x, area.y, width, height);

        // Apply base style to the entire area
        if deg.apply_styling() {
            set_style_area(&mut frame.buffer, render_area, self.style);
        }

        // Draw border
        let use_unicode = deg.apply_styling();
        let (tl, tr, bl, br, h, v) = if use_unicode {
            (
                '\u{250C}', '\u{2510}', '\u{2514}', '\u{2518}', '\u{2500}', '\u{2502}',
            )
        } else {
            ('+', '+', '+', '+', '-', '|')
        };

        // Top border
        if let Some(cell) = frame.buffer.get_mut(render_area.x, render_area.y) {
            *cell = Cell::from_char(tl);
            if deg.apply_styling() {
                crate::apply_style(cell, self.style);
            }
        }
        for x in (render_area.x + 1)..(render_area.right().saturating_sub(1)) {
            if let Some(cell) = frame.buffer.get_mut(x, render_area.y) {
                *cell = Cell::from_char(h);
                if deg.apply_styling() {
                    crate::apply_style(cell, self.style);
                }
            }
        }
        if let Some(cell) = frame
            .buffer
            .get_mut(render_area.right().saturating_sub(1), render_area.y)
        {
            *cell = Cell::from_char(tr);
            if deg.apply_styling() {
                crate::apply_style(cell, self.style);
            }
        }

        // Bottom border
        let bottom_y = render_area.bottom().saturating_sub(1);
        if let Some(cell) = frame.buffer.get_mut(render_area.x, bottom_y) {
            *cell = Cell::from_char(bl);
            if deg.apply_styling() {
                crate::apply_style(cell, self.style);
            }
        }
        for x in (render_area.x + 1)..(render_area.right().saturating_sub(1)) {
            if let Some(cell) = frame.buffer.get_mut(x, bottom_y) {
                *cell = Cell::from_char(h);
                if deg.apply_styling() {
                    crate::apply_style(cell, self.style);
                }
            }
        }
        if let Some(cell) = frame
            .buffer
            .get_mut(render_area.right().saturating_sub(1), bottom_y)
        {
            *cell = Cell::from_char(br);
            if deg.apply_styling() {
                crate::apply_style(cell, self.style);
            }
        }

        // Side borders
        for y in (render_area.y + 1)..bottom_y {
            if let Some(cell) = frame.buffer.get_mut(render_area.x, y) {
                *cell = Cell::from_char(v);
                if deg.apply_styling() {
                    crate::apply_style(cell, self.style);
                }
            }
            if let Some(cell) = frame
                .buffer
                .get_mut(render_area.right().saturating_sub(1), y)
            {
                *cell = Cell::from_char(v);
                if deg.apply_styling() {
                    crate::apply_style(cell, self.style);
                }
            }
        }

        // Draw content
        let content_x = render_area.x + 1; // After left border
        let content_width = width.saturating_sub(2); // Minus borders
        let mut content_y = render_area.y + 1;

        // Draw title if present
        if let Some(ref title) = self.content.title {
            let title_style = if deg.apply_styling() {
                self.title_style.merge(&self.style)
            } else {
                Style::default()
            };

            for (i, c) in title.chars().enumerate() {
                if i as u16 >= content_width {
                    break;
                }
                if let Some(cell) = frame.buffer.get_mut(content_x + i as u16, content_y) {
                    *cell = Cell::from_char(c);
                    if deg.apply_styling() {
                        crate::apply_style(cell, title_style);
                    }
                }
            }
            content_y += 1;
        }

        // Draw icon and message
        let mut msg_x = content_x;

        if let Some(icon) = self.content.icon {
            let icon_char = if use_unicode {
                icon.as_char()
            } else {
                icon.as_ascii()
            };

            if let Some(cell) = frame.buffer.get_mut(msg_x, content_y) {
                *cell = Cell::from_char(icon_char);
                if deg.apply_styling() {
                    let icon_style = self.icon_style.merge(&self.style);
                    crate::apply_style(cell, icon_style);
                }
            }
            msg_x += 1;

            // Space after icon
            if let Some(cell) = frame.buffer.get_mut(msg_x, content_y) {
                *cell = Cell::from_char(' ');
            }
            msg_x += 1;
        }

        // Draw message
        let remaining_width = content_width.saturating_sub(msg_x - content_x);
        for (i, c) in self.content.message.chars().enumerate() {
            if i as u16 >= remaining_width {
                break;
            }
            if let Some(cell) = frame.buffer.get_mut(msg_x + i as u16, content_y) {
                *cell = Cell::from_char(c);
                if deg.apply_styling() {
                    crate::apply_style(cell, self.style);
                }
            }
        }
    }

    fn is_essential(&self) -> bool {
        // Toasts are informational, not essential
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::grapheme_pool::GraphemePool;

    #[test]
    fn test_toast_new() {
        let toast = Toast::new("Hello");
        assert_eq!(toast.content.message, "Hello");
        assert!(toast.content.icon.is_none());
        assert!(toast.content.title.is_none());
        assert!(toast.is_visible());
    }

    #[test]
    fn test_toast_builder() {
        let toast = Toast::new("Test message")
            .icon(ToastIcon::Success)
            .title("Success")
            .position(ToastPosition::BottomRight)
            .duration(Duration::from_secs(10))
            .max_width(60);

        assert_eq!(toast.content.message, "Test message");
        assert_eq!(toast.content.icon, Some(ToastIcon::Success));
        assert_eq!(toast.content.title, Some("Success".to_string()));
        assert_eq!(toast.config.position, ToastPosition::BottomRight);
        assert_eq!(toast.config.duration, Some(Duration::from_secs(10)));
        assert_eq!(toast.config.max_width, 60);
    }

    #[test]
    fn test_toast_persistent() {
        let toast = Toast::new("Persistent").persistent();
        assert!(toast.config.duration.is_none());
        assert!(!toast.is_expired());
    }

    #[test]
    fn test_toast_dismiss() {
        let mut toast = Toast::new("Dismissable");
        assert!(toast.is_visible());
        toast.dismiss();
        assert!(!toast.is_visible());
        assert!(toast.state.dismissed);
    }

    #[test]
    fn test_toast_position_calculate() {
        let terminal_width = 80;
        let terminal_height = 24;
        let toast_width = 30;
        let toast_height = 3;
        let margin = 1;

        // Top-left
        let (x, y) = ToastPosition::TopLeft.calculate_position(
            terminal_width,
            terminal_height,
            toast_width,
            toast_height,
            margin,
        );
        assert_eq!(x, 1);
        assert_eq!(y, 1);

        // Top-right
        let (x, y) = ToastPosition::TopRight.calculate_position(
            terminal_width,
            terminal_height,
            toast_width,
            toast_height,
            margin,
        );
        assert_eq!(x, 80 - 30 - 1); // 49
        assert_eq!(y, 1);

        // Bottom-right
        let (x, y) = ToastPosition::BottomRight.calculate_position(
            terminal_width,
            terminal_height,
            toast_width,
            toast_height,
            margin,
        );
        assert_eq!(x, 49);
        assert_eq!(y, 24 - 3 - 1); // 20

        // Top-center
        let (x, y) = ToastPosition::TopCenter.calculate_position(
            terminal_width,
            terminal_height,
            toast_width,
            toast_height,
            margin,
        );
        assert_eq!(x, (80 - 30) / 2); // 25
        assert_eq!(y, 1);
    }

    #[test]
    fn test_toast_icon_chars() {
        assert_eq!(ToastIcon::Success.as_char(), '\u{2713}');
        assert_eq!(ToastIcon::Error.as_char(), '\u{2717}');
        assert_eq!(ToastIcon::Warning.as_char(), '!');
        assert_eq!(ToastIcon::Info.as_char(), 'i');
        assert_eq!(ToastIcon::Custom('*').as_char(), '*');

        // ASCII fallbacks
        assert_eq!(ToastIcon::Success.as_ascii(), '+');
        assert_eq!(ToastIcon::Error.as_ascii(), 'x');
    }

    #[test]
    fn test_toast_dimensions() {
        let toast = Toast::new("Short");
        let (w, h) = toast.calculate_dimensions();
        // "Short" = 5 chars + 4 (padding+border) = 9
        assert_eq!(w, 9);
        assert_eq!(h, 3); // No title

        let toast_with_title = Toast::new("Message").title("Title");
        let (_w, h) = toast_with_title.calculate_dimensions();
        assert_eq!(h, 4); // With title
    }

    #[test]
    fn test_toast_dimensions_with_icon() {
        let toast = Toast::new("Message").icon(ToastIcon::Success);
        let (w, _h) = toast.calculate_dimensions();
        // icon(1) + space(1) + "Message"(7) + padding+border(4) = 13
        assert_eq!(w, 13);
    }

    #[test]
    fn test_toast_dimensions_max_width() {
        let toast = Toast::new("This is a very long message that exceeds max width").max_width(20);
        let (w, _h) = toast.calculate_dimensions();
        assert!(w <= 20);
    }

    #[test]
    fn test_toast_render_basic() {
        let toast = Toast::new("Hello");
        let area = Rect::new(0, 0, 15, 5);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(15, 5, &mut pool);
        toast.render(area, &mut frame);

        // Check border corners
        assert_eq!(
            frame.buffer.get(0, 0).unwrap().content.as_char(),
            Some('\u{250C}')
        ); // ┌
        assert!(frame.buffer.get(1, 1).is_some()); // Content area exists
    }

    #[test]
    fn test_toast_render_with_icon() {
        let toast = Toast::new("OK").icon(ToastIcon::Success);
        let area = Rect::new(0, 0, 10, 5);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(10, 5, &mut pool);
        toast.render(area, &mut frame);

        // Icon should be at position (1, 1) - inside border
        let icon_cell = frame.buffer.get(1, 1).unwrap();
        assert_eq!(icon_cell.content.as_char(), Some('\u{2713}')); // ✓
    }

    #[test]
    fn test_toast_render_with_title() {
        let toast = Toast::new("Body").title("Head");
        let area = Rect::new(0, 0, 15, 6);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(15, 6, &mut pool);
        toast.render(area, &mut frame);

        // Title at row 1, message at row 2
        let title_cell = frame.buffer.get(1, 1).unwrap();
        assert_eq!(title_cell.content.as_char(), Some('H'));
    }

    #[test]
    fn test_toast_render_zero_area() {
        let toast = Toast::new("Test");
        let area = Rect::new(0, 0, 0, 0);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        toast.render(area, &mut frame); // Should not panic
    }

    #[test]
    fn test_toast_render_small_area() {
        let toast = Toast::new("Test");
        let area = Rect::new(0, 0, 2, 2);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(2, 2, &mut pool);
        toast.render(area, &mut frame); // Should not render (too small)
    }

    #[test]
    fn test_toast_not_visible_when_dismissed() {
        let mut toast = Toast::new("Test");
        toast.dismiss();
        let area = Rect::new(0, 0, 20, 5);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(20, 5, &mut pool);

        // Save original state
        let original = frame.buffer.get(0, 0).unwrap().content.as_char();

        toast.render(area, &mut frame);

        // Buffer should be unchanged (dismissed toast doesn't render)
        assert_eq!(frame.buffer.get(0, 0).unwrap().content.as_char(), original);
    }

    #[test]
    fn test_toast_is_not_essential() {
        let toast = Toast::new("Test");
        assert!(!toast.is_essential());
    }

    #[test]
    fn test_toast_id_uniqueness() {
        let toast1 = Toast::new("A");
        let toast2 = Toast::new("B");
        assert_ne!(toast1.id, toast2.id);
    }

    #[test]
    fn test_toast_style_variants() {
        let success = Toast::new("OK").style_variant(ToastStyle::Success);
        let error = Toast::new("Fail").style_variant(ToastStyle::Error);
        let warning = Toast::new("Warn").style_variant(ToastStyle::Warning);
        let info = Toast::new("Info").style_variant(ToastStyle::Info);
        let neutral = Toast::new("Neutral").style_variant(ToastStyle::Neutral);

        assert_eq!(success.config.style_variant, ToastStyle::Success);
        assert_eq!(error.config.style_variant, ToastStyle::Error);
        assert_eq!(warning.config.style_variant, ToastStyle::Warning);
        assert_eq!(info.config.style_variant, ToastStyle::Info);
        assert_eq!(neutral.config.style_variant, ToastStyle::Neutral);
    }

    #[test]
    fn test_toast_content_builder() {
        let content = ToastContent::new("Message")
            .with_icon(ToastIcon::Warning)
            .with_title("Alert");

        assert_eq!(content.message, "Message");
        assert_eq!(content.icon, Some(ToastIcon::Warning));
        assert_eq!(content.title, Some("Alert".to_string()));
    }

    // --- Animation Tests ---

    #[test]
    fn test_animation_phase_default() {
        let toast = Toast::new("Test");
        assert_eq!(toast.state.animation.phase, ToastAnimationPhase::Entering);
    }

    #[test]
    fn test_animation_phase_reduced_motion() {
        let toast = Toast::new("Test").reduced_motion(true);
        assert_eq!(toast.state.animation.phase, ToastAnimationPhase::Visible);
        assert!(toast.state.animation.reduced_motion);
    }

    #[test]
    fn test_animation_no_animation() {
        let toast = Toast::new("Test").no_animation();
        assert_eq!(toast.state.animation.phase, ToastAnimationPhase::Visible);
        assert!(toast.config.animation.is_disabled());
    }

    #[test]
    fn test_entrance_animation_builder() {
        let toast = Toast::new("Test")
            .entrance_animation(ToastEntranceAnimation::SlideFromTop)
            .entrance_duration(Duration::from_millis(300))
            .entrance_easing(ToastEasing::Bounce);

        assert_eq!(
            toast.config.animation.entrance,
            ToastEntranceAnimation::SlideFromTop
        );
        assert_eq!(
            toast.config.animation.entrance_duration,
            Duration::from_millis(300)
        );
        assert_eq!(toast.config.animation.entrance_easing, ToastEasing::Bounce);
    }

    #[test]
    fn test_exit_animation_builder() {
        let toast = Toast::new("Test")
            .exit_animation(ToastExitAnimation::SlideOut)
            .exit_duration(Duration::from_millis(100))
            .exit_easing(ToastEasing::EaseInOut);

        assert_eq!(toast.config.animation.exit, ToastExitAnimation::SlideOut);
        assert_eq!(
            toast.config.animation.exit_duration,
            Duration::from_millis(100)
        );
        assert_eq!(toast.config.animation.exit_easing, ToastEasing::EaseInOut);
    }

    #[test]
    fn test_entrance_animation_offsets() {
        let width = 30u16;
        let height = 5u16;

        // SlideFromTop: starts above, ends at (0, 0)
        let (dx, dy) = ToastEntranceAnimation::SlideFromTop.initial_offset(width, height);
        assert_eq!(dx, 0);
        assert_eq!(dy, -(height as i16));

        // At progress 0.0, should be at initial offset
        let (dx, dy) = ToastEntranceAnimation::SlideFromTop.offset_at_progress(0.0, width, height);
        assert_eq!(dx, 0);
        assert_eq!(dy, -(height as i16));

        // At progress 1.0, should be at (0, 0)
        let (dx, dy) = ToastEntranceAnimation::SlideFromTop.offset_at_progress(1.0, width, height);
        assert_eq!(dx, 0);
        assert_eq!(dy, 0);

        // SlideFromRight: starts to the right
        let (dx, dy) = ToastEntranceAnimation::SlideFromRight.initial_offset(width, height);
        assert_eq!(dx, width as i16);
        assert_eq!(dy, 0);
    }

    #[test]
    fn test_exit_animation_offsets() {
        let width = 30u16;
        let height = 5u16;
        let entrance = ToastEntranceAnimation::SlideFromRight;

        // SlideOut reverses entrance direction
        let (dx, dy) = ToastExitAnimation::SlideOut.final_offset(width, height, entrance);
        assert_eq!(dx, -(width as i16)); // Opposite of SlideFromRight
        assert_eq!(dy, 0);

        // At progress 0.0, should be at (0, 0)
        let (dx, dy) =
            ToastExitAnimation::SlideOut.offset_at_progress(0.0, width, height, entrance);
        assert_eq!(dx, 0);
        assert_eq!(dy, 0);

        // At progress 1.0, should be at final offset
        let (dx, dy) =
            ToastExitAnimation::SlideOut.offset_at_progress(1.0, width, height, entrance);
        assert_eq!(dx, -(width as i16));
        assert_eq!(dy, 0);
    }

    #[test]
    fn test_easing_apply() {
        // Linear: t = t
        assert!((ToastEasing::Linear.apply(0.5) - 0.5).abs() < 0.001);

        // EaseOut at 0.5 should be > 0.5 (decelerating)
        assert!(ToastEasing::EaseOut.apply(0.5) > 0.5);

        // EaseIn at 0.5 should be < 0.5 (accelerating)
        assert!(ToastEasing::EaseIn.apply(0.5) < 0.5);

        // All should be 0 at 0 and 1 at 1
        for easing in [
            ToastEasing::Linear,
            ToastEasing::EaseIn,
            ToastEasing::EaseOut,
            ToastEasing::EaseInOut,
            ToastEasing::Bounce,
        ] {
            assert!((easing.apply(0.0) - 0.0).abs() < 0.001, "{:?} at 0", easing);
            assert!((easing.apply(1.0) - 1.0).abs() < 0.001, "{:?} at 1", easing);
        }
    }

    #[test]
    fn test_animation_state_progress() {
        let state = ToastAnimationState::new();
        // Just created, progress should be very small
        let progress = state.progress(Duration::from_millis(200));
        assert!(
            progress < 0.1,
            "Progress should be small immediately after creation"
        );
    }

    #[test]
    fn test_animation_state_zero_duration() {
        let state = ToastAnimationState::new();
        // Zero duration should return 1.0 (complete)
        let progress = state.progress(Duration::ZERO);
        assert_eq!(progress, 1.0);
    }

    #[test]
    fn test_dismiss_starts_exit_animation() {
        let mut toast = Toast::new("Test").no_animation();
        // First set to visible phase
        toast.state.animation.phase = ToastAnimationPhase::Visible;
        toast.state.animation.reduced_motion = false;

        toast.dismiss();

        assert!(toast.state.dismissed);
        assert_eq!(toast.state.animation.phase, ToastAnimationPhase::Exiting);
    }

    #[test]
    fn test_dismiss_immediately() {
        let mut toast = Toast::new("Test");
        toast.dismiss_immediately();

        assert!(toast.state.dismissed);
        assert_eq!(toast.state.animation.phase, ToastAnimationPhase::Hidden);
        assert!(!toast.is_visible());
    }

    #[test]
    fn test_is_animating() {
        let toast = Toast::new("Test");
        assert!(toast.is_animating()); // Starts in Entering phase

        let toast_visible = Toast::new("Test").no_animation();
        assert!(!toast_visible.is_animating()); // No animation = Visible phase
    }

    #[test]
    fn test_animation_opacity_fade_in() {
        let config = ToastAnimationConfig {
            entrance: ToastEntranceAnimation::FadeIn,
            exit: ToastExitAnimation::FadeOut,
            entrance_duration: Duration::from_millis(200),
            exit_duration: Duration::from_millis(150),
            entrance_easing: ToastEasing::Linear,
            exit_easing: ToastEasing::Linear,
            respect_reduced_motion: false,
        };

        // At progress 0, opacity should be 0
        let mut state = ToastAnimationState::new();
        let opacity = state.current_opacity(&config);
        assert!(opacity < 0.1, "Should be low opacity at start");

        // At progress 1 (Visible phase), opacity should be 1
        state.phase = ToastAnimationPhase::Visible;
        let opacity = state.current_opacity(&config);
        assert!((opacity - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_animation_config_default() {
        let config = ToastAnimationConfig::default();

        assert_eq!(config.entrance, ToastEntranceAnimation::SlideFromRight);
        assert_eq!(config.exit, ToastExitAnimation::FadeOut);
        assert_eq!(config.entrance_duration, Duration::from_millis(200));
        assert_eq!(config.exit_duration, Duration::from_millis(150));
        assert!(config.respect_reduced_motion);
    }

    #[test]
    fn test_animation_affects_position() {
        assert!(ToastEntranceAnimation::SlideFromTop.affects_position());
        assert!(ToastEntranceAnimation::SlideFromRight.affects_position());
        assert!(!ToastEntranceAnimation::FadeIn.affects_position());
        assert!(!ToastEntranceAnimation::None.affects_position());

        assert!(ToastExitAnimation::SlideOut.affects_position());
        assert!(ToastExitAnimation::SlideToLeft.affects_position());
        assert!(!ToastExitAnimation::FadeOut.affects_position());
        assert!(!ToastExitAnimation::None.affects_position());
    }

    #[test]
    fn test_toast_animation_offset() {
        let toast = Toast::new("Test").entrance_animation(ToastEntranceAnimation::SlideFromRight);
        let (dx, dy) = toast.animation_offset();
        // Should have positive dx (sliding from right)
        assert!(dx > 0, "Should have positive x offset at start");
        assert_eq!(dy, 0);
    }
}
