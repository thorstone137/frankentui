#![forbid(unsafe_code)]

//! Internationalization (i18n) demo screen (bd-ic6i.5).
//!
//! Demonstrates the i18n foundation:
//! - [`StringCatalog`] with multi-locale lookup and fallback
//! - Pluralization rules (English, Russian, Arabic, French, CJK)
//! - `{name}` interpolation
//! - RTL layout mirroring via [`FlowDirection`]
//! - Locale switching at runtime

use ftui_core::event::{Event, KeyCode, KeyEvent, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_i18n::catalog::{LocaleStrings, StringCatalog};
use ftui_i18n::plural::PluralForms;
use ftui_layout::{Constraint, Flex, FlowDirection};
use ftui_render::frame::Frame;
use ftui_runtime::Cmd;
use ftui_style::Style;
use ftui_widgets::Widget;
use ftui_widgets::block::{Alignment, Block};
use ftui_widgets::borders::{BorderType, Borders};
use ftui_widgets::paragraph::Paragraph;

use super::{HelpEntry, Screen};
use crate::theme;

// ---------------------------------------------------------------------------
// Locale descriptors
// ---------------------------------------------------------------------------

/// Supported demo locales in display order.
const LOCALES: &[LocaleInfo] = &[
    LocaleInfo {
        tag: "en",
        name: "English",
        native: "English",
        rtl: false,
    },
    LocaleInfo {
        tag: "es",
        name: "Spanish",
        native: "Espa\u{f1}ol",
        rtl: false,
    },
    LocaleInfo {
        tag: "fr",
        name: "French",
        native: "Fran\u{e7}ais",
        rtl: false,
    },
    LocaleInfo {
        tag: "ru",
        name: "Russian",
        native: "\u{420}\u{443}\u{441}\u{441}\u{43a}\u{438}\u{439}",
        rtl: false,
    },
    LocaleInfo {
        tag: "ar",
        name: "Arabic",
        native: "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}",
        rtl: true,
    },
    LocaleInfo {
        tag: "ja",
        name: "Japanese",
        native: "\u{65e5}\u{672c}\u{8a9e}",
        rtl: false,
    },
];

struct LocaleInfo {
    tag: &'static str,
    name: &'static str,
    native: &'static str,
    rtl: bool,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// i18n demo screen.
pub struct I18nDemo {
    /// Index into LOCALES.
    locale_idx: usize,
    /// The string catalog with all demo strings.
    catalog: StringCatalog,
    /// Sample count for pluralization demo (adjustable).
    plural_count: i64,
    /// Interpolation user name.
    interp_name: &'static str,
    /// Width of terminal for layout.
    width: u16,
    /// Height of terminal.
    height: u16,
    /// Active panel (0=overview, 1=plurals, 2=RTL).
    panel: usize,
    /// Tick counter for subtle indicator animation.
    tick_count: u64,
}

impl Default for I18nDemo {
    fn default() -> Self {
        Self::new()
    }
}

impl I18nDemo {
    /// Create a new i18n demo screen.
    pub fn new() -> Self {
        Self {
            locale_idx: 0,
            catalog: build_catalog(),
            plural_count: 1,
            interp_name: "Alice",
            width: 80,
            height: 24,
            panel: 0,
            tick_count: 0,
        }
    }

    fn current_locale(&self) -> &'static str {
        LOCALES[self.locale_idx].tag
    }

    fn current_info(&self) -> &'static LocaleInfo {
        &LOCALES[self.locale_idx]
    }

    fn flow(&self) -> FlowDirection {
        if self.current_info().rtl {
            FlowDirection::Rtl
        } else {
            FlowDirection::Ltr
        }
    }

    fn next_locale(&mut self) {
        self.locale_idx = (self.locale_idx + 1) % LOCALES.len();
    }

    fn prev_locale(&mut self) {
        self.locale_idx = (self.locale_idx + LOCALES.len() - 1) % LOCALES.len();
    }

    // -- Rendering ---------------------------------------------------------

    fn render_locale_bar(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }

        let items: Vec<String> = LOCALES
            .iter()
            .enumerate()
            .map(|(i, loc)| {
                if i == self.locale_idx {
                    format!("[{}]", loc.native)
                } else {
                    loc.native.to_string()
                }
            })
            .collect();

        let bar_text = items.join("  ");

        let paragraph = Paragraph::new(bar_text)
            .style(Style::new().fg(theme::fg::PRIMARY).bg(theme::alpha::SURFACE))
            .alignment(Alignment::Center)
            .block(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(theme::fg::MUTED)),
            );
        paragraph.render(area, frame);
    }

    fn render_overview_panel(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }
        let colors = theme::current();
        let locale = self.current_locale();
        let info = self.current_info();

        let flow = self.flow();

        // Split into two columns for LTR/RTL demonstration.
        let cols = Flex::horizontal()
            .constraints([Constraint::Percentage(50.0), Constraint::Percentage(50.0)])
            .flow_direction(flow)
            .gap(1)
            .split(area);

        // Left panel (or right in RTL): basic lookups.
        {
            let title = self
                .catalog
                .get(locale, "demo.title")
                .unwrap_or("i18n Demo");
            let greeting = self
                .catalog
                .get(locale, "greeting")
                .unwrap_or("Hello");
            let welcome = self
                .catalog
                .format(locale, "welcome", &[("name", self.interp_name)])
                .unwrap_or_else(|| format!("Welcome, {}!", self.interp_name));
            let direction_label = self
                .catalog
                .get(locale, "direction")
                .unwrap_or(if info.rtl { "RTL" } else { "LTR" });

            let lines = vec![
                format!("--- {} ---", title),
                String::new(),
                format!("  {}", greeting),
                format!("  {}", welcome),
                String::new(),
                format!("  Locale: {} ({})", info.name, info.native),
                format!("  Direction: {}", direction_label),
                format!("  Flow: {:?}", flow),
            ];

            let text = lines.join("\n");
            let paragraph = Paragraph::new(text)
                .style(Style::new().fg(colors.text))
                .block(
                    Block::new()
                        .title("String Lookup")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::new().fg(colors.accent)),
                );
            paragraph.render(cols[0], frame);
        }

        // Right panel (or left in RTL): catalog stats / extraction info.
        {
            let locales = self.catalog.locales();
            let mut locale_list: Vec<&str> = locales.clone();
            locale_list.sort();

            let mut lines = vec![
                "--- String Extraction ---".to_string(),
                String::new(),
                format!("  Registered locales: {}", locales.len()),
            ];
            for tag in &locale_list {
                let marker = if *tag == locale { " <--" } else { "" };
                lines.push(format!("    - {}{}", tag, marker));
            }
            lines.push(String::new());
            lines.push(format!("  Fallback chain: en"));
            lines.push(String::new());
            lines.push("  Keys used on this screen:".to_string());
            for key in &[
                "demo.title",
                "greeting",
                "welcome",
                "direction",
                "items",
                "files",
            ] {
                let found = self.catalog.get(locale, key).is_some();
                let status = if found { "\u{2713}" } else { "\u{2717}" };
                lines.push(format!("    {} {}", status, key));
            }

            let text = lines.join("\n");
            let paragraph = Paragraph::new(text)
                .style(Style::new().fg(colors.text))
                .block(
                    Block::new()
                        .title("Catalog Info")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::new().fg(colors.secondary)),
                );
            paragraph.render(cols[1], frame);
        }
    }

    fn render_plural_panel(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }
        let colors = theme::current();
        let locale = self.current_locale();

        let mut lines = vec![
            format!("--- Pluralization Demo (count = {}) ---", self.plural_count),
            String::new(),
        ];

        // Show pluralization for each locale to compare.
        for loc in LOCALES {
            let items = self
                .catalog
                .format_plural(loc.tag, "items", self.plural_count, &[])
                .unwrap_or_else(|| "(missing)".to_string());
            let files = self
                .catalog
                .format_plural(loc.tag, "files", self.plural_count, &[])
                .unwrap_or_else(|| "(missing)".to_string());

            let marker = if loc.tag == locale { " <--" } else { "" };
            lines.push(format!("  {} ({}){}:", loc.name, loc.tag, marker));
            lines.push(format!("    items: {}", items));
            lines.push(format!("    files: {}", files));
            lines.push(String::new());
        }

        lines.push("  Use Up/Down to change count".to_string());
        lines.push(format!(
            "  Counts to try: 0, 1, 2, 3, 5, 11, 21, 100, 101"
        ));

        let text = lines.join("\n");
        let paragraph = Paragraph::new(text)
            .style(Style::new().fg(colors.text))
            .block(
                Block::new()
                    .title("Pluralization Rules")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(colors.accent)),
            );
        paragraph.render(area, frame);
    }

    fn render_rtl_panel(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }
        let colors = theme::current();

        // Show side-by-side LTR vs RTL layout.
        let rows = Flex::vertical()
            .constraints([
                Constraint::Fixed(3),
                Constraint::Fill,
                Constraint::Fill,
            ])
            .gap(0)
            .split(area);

        // Header.
        {
            let paragraph = Paragraph::new("RTL Layout Mirroring — Flex children reverse in RTL")
                .style(Style::new().fg(colors.text))
                .alignment(Alignment::Center)
                .block(
                    Block::new()
                        .borders(Borders::BOTTOM)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::new().fg(colors.border)),
                );
            paragraph.render(rows[0], frame);
        }

        // LTR layout.
        self.render_direction_sample(frame, rows[1], FlowDirection::Ltr, &colors);

        // RTL layout.
        self.render_direction_sample(frame, rows[2], FlowDirection::Rtl, &colors);
    }

    fn render_direction_sample(
        &self,
        frame: &mut Frame,
        area: Rect,
        flow: FlowDirection,
        colors: &theme::ThemeColors,
    ) {
        let label = if flow.is_rtl() { "RTL" } else { "LTR" };

        let outer = Block::new()
            .title(format!("{} Layout", label))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(if flow.is_rtl() {
                colors.accent
            } else {
                colors.secondary
            }));
        let inner = outer.inner(area);
        outer.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let cols = Flex::horizontal()
            .constraints([
                Constraint::Percentage(30.0),
                Constraint::Percentage(40.0),
                Constraint::Percentage(30.0),
            ])
            .flow_direction(flow)
            .gap(1)
            .split(inner);

        let labels = ["Sidebar", "Content", "Panel"];
        let label_colors = [colors.accent, colors.text, colors.secondary];

        for (i, (&col, &lbl)) in cols.iter().zip(labels.iter()).enumerate() {
            if col.is_empty() {
                continue;
            }
            let p = Paragraph::new(format!("{} ({})", lbl, i + 1))
                .style(Style::new().fg(label_colors[i]))
                .alignment(Alignment::Center)
                .block(
                    Block::new()
                        .borders(Borders::ALL)
                        .border_style(Style::new().fg(colors.border)),
                );
            p.render(col, frame);
        }
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }
        let colors = theme::current();
        let info = self.current_info();

        let panel_names = ["Overview", "Plurals", "RTL Layout"];
        let panel_label = panel_names.get(self.panel).unwrap_or(&"?");

        let status = format!(
            " Tab/1-3: panels ({})  L/R: locale  Up/Down: count  Current: {} ({})  Dir: {} ",
            panel_label,
            info.name,
            info.tag,
            if info.rtl { "RTL" } else { "LTR" },
        );

        let paragraph = Paragraph::new(status)
            .style(Style::new().fg(colors.surface).bg(colors.accent));
        paragraph.render(area, frame);
    }
}

// ---------------------------------------------------------------------------
// Screen trait
// ---------------------------------------------------------------------------

impl Screen for I18nDemo {
    type Message = ();

    fn update(&mut self, event: &Event) -> Cmd<Self::Message> {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            modifiers,
            ..
        }) = event
        {
            let shift = modifiers.contains(Modifiers::SHIFT);
            match code {
                KeyCode::Right if !shift => self.next_locale(),
                KeyCode::Left if !shift => self.prev_locale(),
                KeyCode::Up => {
                    self.plural_count = self.plural_count.saturating_add(1);
                }
                KeyCode::Down => {
                    self.plural_count = (self.plural_count - 1).max(0);
                }
                KeyCode::Tab => {
                    self.panel = (self.panel + 1) % 3;
                }
                KeyCode::BackTab => {
                    self.panel = (self.panel + 2) % 3;
                }
                KeyCode::Char('1') => self.panel = 0,
                KeyCode::Char('2') => self.panel = 1,
                KeyCode::Char('3') => self.panel = 2,
                _ => {}
            }
        }
        if let Event::Resize { width, height } = event {
            self.width = *width;
            self.height = *height;
        }
        Cmd::None
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }

        let rows = Flex::vertical()
            .constraints([
                Constraint::Fixed(3), // locale bar
                Constraint::Fill,  // main content
                Constraint::Fixed(1), // status bar
            ])
            .split(area);

        self.render_locale_bar(frame, rows[0]);

        match self.panel {
            0 => self.render_overview_panel(frame, rows[1]),
            1 => self.render_plural_panel(frame, rows[1]),
            2 => self.render_rtl_panel(frame, rows[1]),
            _ => {}
        }

        self.render_status_bar(frame, rows[2]);
    }

    fn keybindings(&self) -> Vec<HelpEntry> {
        vec![
            HelpEntry {
                key: "Left/Right",
                action: "Switch locale",
            },
            HelpEntry {
                key: "Up/Down",
                action: "Change plural count",
            },
            HelpEntry {
                key: "Tab/1-3",
                action: "Switch panel",
            },
        ]
    }

    fn tick(&mut self, tick_count: u64) {
        self.tick_count = tick_count;
    }

    fn title(&self) -> &'static str {
        "i18n Demo"
    }

    fn tab_label(&self) -> &'static str {
        "i18n"
    }
}

// ---------------------------------------------------------------------------
// Catalog builder
// ---------------------------------------------------------------------------

fn build_catalog() -> StringCatalog {
    let mut catalog = StringCatalog::new();

    // English
    {
        let mut s = LocaleStrings::new();
        s.insert("demo.title", "Internationalization");
        s.insert("greeting", "Hello!");
        s.insert("welcome", "Welcome, {name}!");
        s.insert("direction", "Left-to-Right");
        s.insert_plural(
            "items",
            PluralForms {
                one: "{count} item".into(),
                other: "{count} items".into(),
                ..Default::default()
            },
        );
        s.insert_plural(
            "files",
            PluralForms {
                one: "{count} file".into(),
                other: "{count} files".into(),
                ..Default::default()
            },
        );
        catalog.add_locale("en", s);
    }

    // Spanish
    {
        let mut s = LocaleStrings::new();
        s.insert("demo.title", "Internacionalizaci\u{f3}n");
        s.insert("greeting", "\u{a1}Hola!");
        s.insert("welcome", "\u{a1}Bienvenido, {name}!");
        s.insert("direction", "Izquierda a derecha");
        s.insert_plural(
            "items",
            PluralForms {
                one: "{count} elemento".into(),
                other: "{count} elementos".into(),
                ..Default::default()
            },
        );
        s.insert_plural(
            "files",
            PluralForms {
                one: "{count} archivo".into(),
                other: "{count} archivos".into(),
                ..Default::default()
            },
        );
        catalog.add_locale("es", s);
    }

    // French
    {
        let mut s = LocaleStrings::new();
        s.insert("demo.title", "Internationalisation");
        s.insert("greeting", "Bonjour\u{a0}!");
        s.insert("welcome", "Bienvenue, {name}\u{a0}!");
        s.insert("direction", "Gauche \u{e0} droite");
        s.insert_plural(
            "items",
            PluralForms {
                one: "{count} \u{e9}l\u{e9}ment".into(),
                other: "{count} \u{e9}l\u{e9}ments".into(),
                ..Default::default()
            },
        );
        s.insert_plural(
            "files",
            PluralForms {
                one: "{count} fichier".into(),
                other: "{count} fichiers".into(),
                ..Default::default()
            },
        );
        catalog.add_locale("fr", s);
    }

    // Russian
    {
        let mut s = LocaleStrings::new();
        s.insert(
            "demo.title",
            "\u{418}\u{43d}\u{442}\u{435}\u{440}\u{43d}\u{430}\u{446}\u{438}\u{43e}\u{43d}\u{430}\u{43b}\u{438}\u{437}\u{430}\u{446}\u{438}\u{44f}",
        );
        s.insert(
            "greeting",
            "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}!",
        );
        s.insert(
            "welcome",
            "\u{414}\u{43e}\u{431}\u{440}\u{43e} \u{43f}\u{43e}\u{436}\u{430}\u{43b}\u{43e}\u{432}\u{430}\u{442}\u{44c}, {name}!",
        );
        s.insert(
            "direction",
            "\u{421}\u{43b}\u{435}\u{432}\u{430} \u{43d}\u{430}\u{43f}\u{440}\u{430}\u{432}\u{43e}",
        );
        s.insert_plural(
            "items",
            PluralForms {
                one: "{count} \u{44d}\u{43b}\u{435}\u{43c}\u{435}\u{43d}\u{442}".into(),
                few: Some(
                    "{count} \u{44d}\u{43b}\u{435}\u{43c}\u{435}\u{43d}\u{442}\u{430}".into(),
                ),
                many: Some(
                    "{count} \u{44d}\u{43b}\u{435}\u{43c}\u{435}\u{43d}\u{442}\u{43e}\u{432}"
                        .into(),
                ),
                other: "{count} \u{44d}\u{43b}\u{435}\u{43c}\u{435}\u{43d}\u{442}\u{43e}\u{432}"
                    .into(),
                ..Default::default()
            },
        );
        s.insert_plural(
            "files",
            PluralForms {
                one: "{count} \u{444}\u{430}\u{439}\u{43b}".into(),
                few: Some("{count} \u{444}\u{430}\u{439}\u{43b}\u{430}".into()),
                many: Some("{count} \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}".into()),
                other: "{count} \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}".into(),
                ..Default::default()
            },
        );
        catalog.add_locale("ru", s);
    }

    // Arabic
    {
        let mut s = LocaleStrings::new();
        s.insert(
            "demo.title",
            "\u{627}\u{644}\u{62a}\u{62f}\u{648}\u{64a}\u{644}",
        );
        s.insert(
            "greeting",
            "\u{645}\u{631}\u{62d}\u{628}\u{627}\u{64b}!",
        );
        s.insert(
            "welcome",
            "\u{623}\u{647}\u{644}\u{627}\u{64b} {name}!",
        );
        s.insert(
            "direction",
            "\u{645}\u{646} \u{627}\u{644}\u{64a}\u{645}\u{64a}\u{646} \u{625}\u{644}\u{649} \u{627}\u{644}\u{64a}\u{633}\u{627}\u{631}",
        );
        s.insert_plural(
            "items",
            PluralForms {
                zero: Some("{count} \u{639}\u{646}\u{627}\u{635}\u{631}".into()),
                one: "\u{639}\u{646}\u{635}\u{631} \u{648}\u{627}\u{62d}\u{62f}".into(),
                two: Some("\u{639}\u{646}\u{635}\u{631}\u{627}\u{646}".into()),
                few: Some("{count} \u{639}\u{646}\u{627}\u{635}\u{631}".into()),
                many: Some("{count} \u{639}\u{646}\u{635}\u{631}\u{627}\u{64b}".into()),
                other: "{count} \u{639}\u{646}\u{635}\u{631}".into(),
            },
        );
        s.insert_plural(
            "files",
            PluralForms {
                zero: Some("{count} \u{645}\u{644}\u{641}\u{627}\u{62a}".into()),
                one: "\u{645}\u{644}\u{641} \u{648}\u{627}\u{62d}\u{62f}".into(),
                two: Some("\u{645}\u{644}\u{641}\u{627}\u{646}".into()),
                few: Some("{count} \u{645}\u{644}\u{641}\u{627}\u{62a}".into()),
                many: Some("{count} \u{645}\u{644}\u{641}\u{64b}\u{627}".into()),
                other: "{count} \u{645}\u{644}\u{641}".into(),
            },
        );
        catalog.add_locale("ar", s);
    }

    // Japanese
    {
        let mut s = LocaleStrings::new();
        s.insert(
            "demo.title",
            "\u{56fd}\u{969b}\u{5316}",
        );
        s.insert(
            "greeting",
            "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{ff01}",
        );
        s.insert(
            "welcome",
            "\u{3088}\u{3046}\u{3053}\u{305d}\u{3001}{name}\u{3055}\u{3093}\u{ff01}",
        );
        s.insert(
            "direction",
            "\u{5de6}\u{304b}\u{3089}\u{53f3}",
        );
        // CJK: no plural distinction.
        s.insert_plural(
            "items",
            PluralForms {
                one: "{count}\u{500b}\u{306e}\u{30a2}\u{30a4}\u{30c6}\u{30e0}".into(),
                other: "{count}\u{500b}\u{306e}\u{30a2}\u{30a4}\u{30c6}\u{30e0}".into(),
                ..Default::default()
            },
        );
        s.insert_plural(
            "files",
            PluralForms {
                one: "{count}\u{500b}\u{306e}\u{30d5}\u{30a1}\u{30a4}\u{30eb}".into(),
                other: "{count}\u{500b}\u{306e}\u{30d5}\u{30a1}\u{30a4}\u{30eb}".into(),
                ..Default::default()
            },
        );
        catalog.add_locale("ja", s);
    }

    catalog.set_fallback_chain(vec!["en".into()]);
    catalog
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::grapheme_pool::GraphemePool;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::NONE,
            kind: KeyEventKind::Press,
        })
    }

    fn render_hash(screen: &I18nDemo, w: u16, h: u16) -> u64 {
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(w, h, &mut pool);
        screen.view(&mut frame, Rect::new(0, 0, w, h));
        let mut hasher = DefaultHasher::new();
        for y in 0..h {
            for x in 0..w {
                if let Some(cell) = frame.buffer.get(x, y) {
                    if let Some(ch) = cell.content.as_char() {
                        ch.hash(&mut hasher);
                    }
                }
            }
        }
        hasher.finish()
    }

    #[test]
    fn default_locale_is_english() {
        let demo = I18nDemo::new();
        assert_eq!(demo.current_locale(), "en");
    }

    #[test]
    fn cycle_locales() {
        let mut demo = I18nDemo::new();
        assert_eq!(demo.current_locale(), "en");
        demo.next_locale();
        assert_eq!(demo.current_locale(), "es");
        demo.next_locale();
        assert_eq!(demo.current_locale(), "fr");
        demo.next_locale();
        assert_eq!(demo.current_locale(), "ru");
        demo.next_locale();
        assert_eq!(demo.current_locale(), "ar");
        demo.next_locale();
        assert_eq!(demo.current_locale(), "ja");
        demo.next_locale();
        assert_eq!(demo.current_locale(), "en"); // wraps
    }

    #[test]
    fn prev_locale_wraps() {
        let mut demo = I18nDemo::new();
        demo.prev_locale();
        assert_eq!(demo.current_locale(), "ja");
    }

    #[test]
    fn arabic_is_rtl() {
        let mut demo = I18nDemo::new();
        // Navigate to Arabic.
        while demo.current_locale() != "ar" {
            demo.next_locale();
        }
        assert!(demo.current_info().rtl);
        assert_eq!(demo.flow(), FlowDirection::Rtl);
    }

    #[test]
    fn catalog_has_all_locales() {
        let catalog = build_catalog();
        let locales = catalog.locales();
        for loc in LOCALES {
            assert!(
                locales.contains(&loc.tag),
                "missing locale: {}",
                loc.tag
            );
        }
    }

    #[test]
    fn catalog_greeting_all_locales() {
        let catalog = build_catalog();
        for loc in LOCALES {
            assert!(
                catalog.get(loc.tag, "greeting").is_some(),
                "missing greeting for {}",
                loc.tag
            );
        }
    }

    #[test]
    fn catalog_plurals_english() {
        let catalog = build_catalog();
        assert_eq!(
            catalog.format_plural("en", "items", 1, &[]),
            Some("1 item".into())
        );
        assert_eq!(
            catalog.format_plural("en", "items", 5, &[]),
            Some("5 items".into())
        );
    }

    #[test]
    fn catalog_plurals_russian() {
        let catalog = build_catalog();
        assert_eq!(
            catalog.format_plural("ru", "files", 1, &[]),
            Some("1 \u{444}\u{430}\u{439}\u{43b}".into())
        );
        assert_eq!(
            catalog.format_plural("ru", "files", 3, &[]),
            Some("3 \u{444}\u{430}\u{439}\u{43b}\u{430}".into())
        );
        assert_eq!(
            catalog.format_plural("ru", "files", 5, &[]),
            Some("5 \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}".into())
        );
    }

    #[test]
    fn catalog_interpolation() {
        let catalog = build_catalog();
        assert_eq!(
            catalog.format("en", "welcome", &[("name", "Bob")]),
            Some("Welcome, Bob!".into())
        );
    }

    #[test]
    fn catalog_fallback() {
        let catalog = build_catalog();
        // Japanese doesn't have "nonexistent", should fall back to English.
        assert_eq!(catalog.get("ja", "greeting").is_some(), true);
        // Try a key that only English has... well all have our keys. Let's
        // test with a totally unknown locale.
        assert_eq!(catalog.get("xx", "greeting"), Some("Hello!"));
    }

    #[test]
    fn render_produces_output() {
        let demo = I18nDemo::new();
        let hash = render_hash(&demo, 120, 40);
        assert_ne!(hash, 0, "render must produce visible output");
    }

    #[test]
    fn render_deterministic() {
        let demo = I18nDemo::new();
        let h1 = render_hash(&demo, 80, 24);
        let h2 = render_hash(&demo, 80, 24);
        assert_eq!(h1, h2, "same state must produce identical render");
    }

    #[test]
    fn panel_switching() {
        let mut demo = I18nDemo::new();
        assert_eq!(demo.panel, 0);
        demo.update(&press(KeyCode::Tab));
        assert_eq!(demo.panel, 1);
        demo.update(&press(KeyCode::Tab));
        assert_eq!(demo.panel, 2);
        demo.update(&press(KeyCode::Tab));
        assert_eq!(demo.panel, 0);
    }

    #[test]
    fn number_keys_select_panel() {
        let mut demo = I18nDemo::new();
        demo.update(&press(KeyCode::Char('3')));
        assert_eq!(demo.panel, 2);
        demo.update(&press(KeyCode::Char('1')));
        assert_eq!(demo.panel, 0);
    }

    #[test]
    fn plural_count_adjustable() {
        let mut demo = I18nDemo::new();
        assert_eq!(demo.plural_count, 1);
        demo.update(&press(KeyCode::Up));
        assert_eq!(demo.plural_count, 2);
        demo.update(&press(KeyCode::Down));
        assert_eq!(demo.plural_count, 1);
        demo.update(&press(KeyCode::Down));
        assert_eq!(demo.plural_count, 0);
        demo.update(&press(KeyCode::Down));
        assert_eq!(demo.plural_count, 0); // clamped at 0
    }

    #[test]
    fn all_panels_render_each_locale() {
        let mut demo = I18nDemo::new();
        for panel in 0..3 {
            demo.panel = panel;
            for loc_idx in 0..LOCALES.len() {
                demo.locale_idx = loc_idx;
                let hash = render_hash(&demo, 100, 30);
                assert_ne!(
                    hash, 0,
                    "panel={} locale={} must render",
                    panel,
                    LOCALES[loc_idx].tag
                );
            }
        }
    }

    #[test]
    fn locale_key_events() {
        let mut demo = I18nDemo::new();
        demo.update(&press(KeyCode::Right));
        assert_eq!(demo.current_locale(), "es");
        demo.update(&press(KeyCode::Left));
        assert_eq!(demo.current_locale(), "en");
    }

    #[test]
    fn small_terminal_no_panic() {
        let demo = I18nDemo::new();
        // Very small terminal should not panic.
        let hash = render_hash(&demo, 30, 8);
        assert_ne!(hash, 0);
    }
}
