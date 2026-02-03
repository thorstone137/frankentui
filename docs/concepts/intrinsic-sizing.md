# Intrinsic Sizing

FrankenTUI supports **intrinsic sizing**: widgets can report their natural dimensions based on content, enabling content-aware layouts with `Constraint::FitContent`.

## Overview

Traditional TUI layouts use fixed or percentage-based sizing. Intrinsic sizing adds content-awareness:

| Approach | Example | Use Case |
|----------|---------|----------|
| Fixed | `Constraint::Fixed(20)` | Static chrome, borders |
| Percentage | `Constraint::Percentage(50.0)` | Proportional splits |
| Fill | `Constraint::Fill` | Remaining space |
| **FitContent** | `Constraint::FitContent` | Content-aware sizing |

## Core Types

### `SizeConstraints`

Captures the full sizing semantics for a widget:

```rust
pub struct SizeConstraints {
    pub min: Size,           // Minimum usable size
    pub preferred: Size,     // Ideal size for content
    pub max: Option<Size>,   // Maximum useful size (None = unbounded)
}
```

**Invariants**:
- `min.width <= preferred.width <= max.map_or(∞, |m| m.width)`
- `min.height <= preferred.height <= max.map_or(∞, |m| m.height)`

**Constructors**:
- `SizeConstraints::ZERO` — Default for fill-available widgets
- `SizeConstraints::exact(size)` — Fixed size (min = preferred = max)
- `SizeConstraints::at_least(min, preferred)` — Minimum with unbounded max

### `MeasurableWidget` Trait

Widgets implement this to report intrinsic dimensions:

```rust
pub trait MeasurableWidget {
    /// Measure the widget given available space.
    fn measure(&self, available: Size) -> SizeConstraints {
        SizeConstraints::ZERO // Default: fill available
    }

    /// Does this widget have content-dependent sizing?
    fn has_intrinsic_size(&self) -> bool {
        false // Default: no
    }
}
```

## Widget Author Guide

### When to Implement `MeasurableWidget`

Implement intrinsic sizing when your widget has natural dimensions:

| Widget Type | Has Intrinsic Size? | Example |
|-------------|---------------------|---------|
| Label/Text | Yes | Width = text length |
| Button | Yes | Width = label + padding |
| Icon | Yes | Usually 1x1 or 2x1 |
| Paragraph | Yes | Wrapped text dimensions |
| Table | Yes | Column widths from content |
| Container | Maybe | Sum of children |
| Canvas/Chart | No | Fills available space |

### Implementation Checklist

1. **Calculate min**: Smallest size before content clips
2. **Calculate preferred**: Size that best displays content
3. **Calculate max**: Maximum useful size (None if unbounded)
4. **Return true for `has_intrinsic_size()`**

### Example: Simple Label

```rust
use ftui_core::geometry::Size;
use ftui_widgets::{MeasurableWidget, SizeConstraints};

struct Label {
    text: String,
}

impl MeasurableWidget for Label {
    fn measure(&self, _available: Size) -> SizeConstraints {
        let width = self.text.len() as u16;
        SizeConstraints {
            min: Size::new(1, 1),           // At least show something
            preferred: Size::new(width, 1), // Full text on one line
            max: Some(Size::new(width, 1)), // No benefit from extra space
        }
    }

    fn has_intrinsic_size(&self) -> bool {
        true
    }
}
```

### Example: Wrapping Text

```rust
impl MeasurableWidget for Paragraph {
    fn measure(&self, available: Size) -> SizeConstraints {
        // Calculate wrapped line count at available width
        let lines = self.wrap_lines(available.width);
        let max_line_width = lines.iter().map(|l| l.len()).max().unwrap_or(0) as u16;
        let height = lines.len() as u16;

        SizeConstraints {
            min: Size::new(1, 1),
            preferred: Size::new(max_line_width, height),
            max: None, // Can use extra space for padding
        }
    }

    fn has_intrinsic_size(&self) -> bool {
        true
    }
}
```

### Example: Container with Children

```rust
impl MeasurableWidget for HorizontalStack {
    fn measure(&self, available: Size) -> SizeConstraints {
        let mut total_width = 0u16;
        let mut max_height = 0u16;
        let mut min_width = 0u16;
        let mut min_height = 0u16;

        for child in &self.children {
            let c = child.measure(available);
            total_width = total_width.saturating_add(c.preferred.width);
            max_height = max_height.max(c.preferred.height);
            min_width = min_width.saturating_add(c.min.width);
            min_height = min_height.max(c.min.height);
        }

        SizeConstraints {
            min: Size::new(min_width, min_height),
            preferred: Size::new(total_width, max_height),
            max: None,
        }
    }

    fn has_intrinsic_size(&self) -> bool {
        self.children.iter().any(|c| c.has_intrinsic_size())
    }
}
```

### Implementation Requirements

1. **Monotonicity**: `min <= preferred <= max`
2. **Purity**: Same inputs → same outputs (no side effects)
3. **Performance**: O(content_length) worst case
4. **Min Constancy**: `min` should not depend on `available`

## Layout Migration Guide

### Before: Static Layout

```rust
// Old approach: fixed widths
let chunks = Flex::horizontal()
    .constraints([
        Constraint::Fixed(20),  // Sidebar always 20 cols
        Constraint::Fill,       // Content fills rest
    ])
    .split(area);
```

### After: Content-Aware Layout

```rust
// New approach: fit to content
let chunks = Flex::horizontal()
    .constraints([
        Constraint::FitContent, // Sidebar fits its content
        Constraint::Fill,       // Content fills rest
    ])
    .split(area);

// The layout system calls sidebar.measure() to determine width
```

### Responsive Patterns

#### Adaptive Sidebar

Sidebar collapses to icons when space is tight:

```rust
impl MeasurableWidget for AdaptiveSidebar {
    fn measure(&self, available: Size) -> SizeConstraints {
        if available.width < 60 {
            // Icon-only mode
            SizeConstraints::exact(Size::new(4, available.height))
        } else {
            // Full labels
            SizeConstraints {
                min: Size::new(4, 1),
                preferred: Size::new(20, available.height),
                max: Some(Size::new(30, available.height)),
            }
        }
    }
}
```

#### Flexible Cards

Cards switch between horizontal and vertical layout:

```rust
fn layout_cards(area: Rect) -> Vec<Rect> {
    if area.width >= 60 {
        // Side-by-side
        Flex::horizontal()
            .constraints([Constraint::FitContent, Constraint::FitContent])
            .split(area)
    } else {
        // Stacked
        Flex::vertical()
            .constraints([Constraint::FitContent, Constraint::FitContent])
            .split(area)
    }
}
```

## Constraint Types Reference

| Constraint | Behavior | When to Use |
|------------|----------|-------------|
| `Fixed(n)` | Exactly n cells | Borders, separators |
| `Percentage(p)` | p% of available | Proportional splits |
| `Ratio(n, d)` | n/d of available | Precise ratios |
| `Fill` | All remaining space | Main content areas |
| `FitContent` | Widget's preferred size | Content-aware sizing |

## Testing Intrinsic Sizing

### Unit Test Template

```rust
#[test]
fn widget_measure_invariants() {
    let widget = MyWidget::new("content");
    let available = Size::new(100, 50);
    let c = widget.measure(available);

    // Invariant: min <= preferred <= max
    assert!(c.min.width <= c.preferred.width);
    assert!(c.min.height <= c.preferred.height);
    if let Some(max) = c.max {
        assert!(c.preferred.width <= max.width);
        assert!(c.preferred.height <= max.height);
    }
}

#[test]
fn widget_measure_is_pure() {
    let widget = MyWidget::new("content");
    let available = Size::new(100, 50);

    let c1 = widget.measure(available);
    let c2 = widget.measure(available);

    assert_eq!(c1, c2, "measure() must be pure");
}

#[test]
fn widget_min_is_constant() {
    let widget = MyWidget::new("content");

    let c1 = widget.measure(Size::new(100, 50));
    let c2 = widget.measure(Size::new(200, 100));

    assert_eq!(c1.min, c2.min, "min should not depend on available");
}
```

### Property Tests

See `crates/ftui-widgets/src/measurable.rs` for comprehensive property-based tests using proptest.

## Related Work

- **Demo Screen**: See `crates/ftui-demo-showcase/src/screens/intrinsic_sizing.rs` for live examples
- **Paragraph**: `crates/ftui-widgets/src/paragraph.rs` implements `MeasurableWidget`
- **Block**: `crates/ftui-widgets/src/block.rs` calculates chrome dimensions
- **List**: `crates/ftui-widgets/src/list.rs` measures item content

## See Also

- Rustdoc: `ftui_widgets::measurable`
- ADR: (future) Intrinsic Sizing Architecture Decision Record
- Bead: `bd-2dow` (Intrinsic Sizing epic)
