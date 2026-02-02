#![forbid(unsafe_code)]

//! File Browser screen — file system navigation and preview.
//!
//! Demonstrates:
//! - `FilePicker` with simulated directory entries
//! - `Tree` widget for directory structure
//! - `SyntaxHighlighter` for file preview
//! - `filesize::decimal()` for human-readable sizes

use ftui_core::event::{Event, KeyCode, KeyEvent, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_extras::filepicker::{FileEntry, FileKind, FilePicker, FilePickerFilter, FilePickerStyle};
use ftui_extras::filesize;
use ftui_extras::syntax::SyntaxHighlighter;
use ftui_layout::{Constraint, Flex};
use ftui_render::cell::PackedRgba;
use ftui_render::frame::Frame;
use ftui_runtime::Cmd;
use ftui_style::Style;
use ftui_widgets::Widget;
use ftui_widgets::block::{Alignment, Block};
use ftui_widgets::borders::{BorderType, Borders};
use ftui_widgets::paragraph::Paragraph;
use ftui_widgets::tree::{Tree, TreeGuides, TreeNode};

use super::{HelpEntry, Screen};
use crate::theme;

/// Which panel has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    FileTree,
    FilePicker,
    Preview,
}

impl Panel {
    fn next(self) -> Self {
        match self {
            Self::FileTree => Self::FilePicker,
            Self::FilePicker => Self::Preview,
            Self::Preview => Self::FileTree,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::FileTree => Self::Preview,
            Self::FilePicker => Self::FileTree,
            Self::Preview => Self::FilePicker,
        }
    }
}

/// Sample file content for preview.
const SAMPLE_RUST: &str = r#"use std::io;

fn main() -> io::Result<()> {
    let names = vec!["Alice", "Bob", "Charlie"];
    for name in &names {
        println!("Hello, {name}!");
    }
    Ok(())
}
"#;

const SAMPLE_PYTHON: &str = r#"#!/usr/bin/env python3
"""A simple Python script."""

def greet(name: str) -> str:
    return f"Hello, {name}!"

if __name__ == "__main__":
    for name in ["Alice", "Bob", "Charlie"]:
        print(greet(name))
"#;

const SAMPLE_TOML: &str = r#"[package]
name = "my-project"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
"#;

const SAMPLE_JSON: &str = r#"{
  "name": "ftui-demo",
  "version": "0.1.0",
  "description": "TUI demo showcase",
  "keywords": ["terminal", "ui", "tui"]
}
"#;

pub struct FileBrowser {
    focus: Panel,
    picker: FilePicker,
    highlighter: SyntaxHighlighter,
    preview_scroll: usize,
    show_hidden: bool,
}

impl FileBrowser {
    pub fn new() -> Self {
        let entries = simulated_entries();
        let mut picker = FilePicker::new(entries);
        picker.set_path("/home/user/projects/my-app");
        picker.set_style(FilePickerStyle {
            selected: Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::HIGHLIGHT),
            directory: Style::new().fg(PackedRgba::rgb(100, 180, 255)),
            file: Style::new().fg(theme::fg::PRIMARY),
            symlink: Style::new().fg(PackedRgba::rgb(180, 130, 255)),
            path: Style::new().fg(theme::fg::SECONDARY),
        });
        picker.set_filter(FilePickerFilter {
            allowed_extensions: vec![],
            show_hidden: false,
        });

        Self {
            focus: Panel::FilePicker,
            picker,
            highlighter: SyntaxHighlighter::new(),
            preview_scroll: 0,
            show_hidden: false,
        }
    }

    fn current_preview(&self) -> (&str, &str) {
        let entry = self.picker.selected_entry();
        match entry.map(|e| e.name.as_str()) {
            Some("main.rs") | Some("lib.rs") => (SAMPLE_RUST, "rust"),
            Some("app.py") | Some("test_app.py") => (SAMPLE_PYTHON, "python"),
            Some("Cargo.toml") | Some("config.toml") => (SAMPLE_TOML, "toml"),
            Some("package.json") | Some("data.json") => (SAMPLE_JSON, "json"),
            _ => ("(no preview available)", "plain"),
        }
    }

    fn render_tree_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Panel::FileTree;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::FILE_BROWSER)
        } else {
            theme::content_border()
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Project Tree")
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let tree = build_project_tree();
        let tree_widget = Tree::new(tree)
            .with_guides(TreeGuides::Unicode)
            .with_show_root(true)
            .with_label_style(Style::new().fg(theme::fg::PRIMARY))
            .with_root_style(Style::new().fg(PackedRgba::rgb(100, 180, 255)))
            .with_guide_style(Style::new().fg(theme::fg::MUTED));

        tree_widget.render(inner, frame);
    }

    fn render_picker_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Panel::FilePicker;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::FILE_BROWSER)
        } else {
            theme::content_border()
        };

        let hidden_label = if self.show_hidden { "+hidden" } else { "" };
        let title = format!(
            "Files ({} items{})",
            self.picker.filtered_count(),
            hidden_label,
        );
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(&title)
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        // FilePicker::render needs &mut self, but view() has &self.
        // We work around by re-rendering manually with Paragraph lines.
        let selected_idx = self.picker.selected_index();
        let count = self.picker.filtered_count();

        // We'll render a simple list view since FilePicker::render needs &mut.
        let visible = inner.height as usize;
        let scroll = if selected_idx >= visible {
            selected_idx - visible + 1
        } else {
            0
        };

        // Render path header
        let path_area = Rect::new(inner.x, inner.y, inner.width, 1);
        Paragraph::new(self.picker.path())
            .style(Style::new().fg(theme::fg::SECONDARY))
            .render(path_area, frame);

        // Render entries
        let list_area = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
        let entries = simulated_entries();
        let filter = FilePickerFilter {
            allowed_extensions: vec![],
            show_hidden: self.show_hidden,
        };

        for (row, i) in (scroll..count.min(scroll + list_area.height as usize)).enumerate() {
            if i >= entries.len() {
                break;
            }
            let entry = &entries[i];
            if !filter.matches(entry) {
                continue;
            }
            let y = list_area.y.saturating_add(row as u16);
            if y >= list_area.bottom() {
                break;
            }

            let icon = entry.icon();
            let size_str = entry.size.map(filesize::decimal).unwrap_or_default();
            let line = format!("{icon}{:<30} {}", entry.name, size_str);

            let style = if i == selected_idx {
                Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::HIGHLIGHT)
            } else if entry.is_dir() {
                Style::new().fg(PackedRgba::rgb(100, 180, 255))
            } else {
                Style::new().fg(theme::fg::PRIMARY)
            };

            let row_area = Rect::new(list_area.x, y, list_area.width, 1);
            Paragraph::new(&*line).style(style).render(row_area, frame);
        }
    }

    fn render_preview_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Panel::Preview;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::FILE_BROWSER)
        } else {
            theme::content_border()
        };

        let entry_name = self
            .picker
            .selected_entry()
            .map(|e| e.name.as_str())
            .unwrap_or("(none)");
        let title = format!("Preview: {entry_name}");
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(&title)
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let (content, lang) = self.current_preview();
        let highlighted = self.highlighter.highlight(content, lang);
        Paragraph::new(highlighted)
            .style(Style::new().fg(theme::fg::PRIMARY))
            .render(inner, frame);
    }
}

fn simulated_entries() -> Vec<FileEntry> {
    vec![
        FileEntry::new("src", FileKind::Directory),
        FileEntry::new("tests", FileKind::Directory),
        FileEntry::new("docs", FileKind::Directory),
        FileEntry::new(".git", FileKind::Directory),
        FileEntry::new("target", FileKind::Directory),
        FileEntry::new("main.rs", FileKind::File).with_size(2048),
        FileEntry::new("lib.rs", FileKind::File).with_size(4096),
        FileEntry::new("app.py", FileKind::File).with_size(1536),
        FileEntry::new("test_app.py", FileKind::File).with_size(892),
        FileEntry::new("Cargo.toml", FileKind::File).with_size(512),
        FileEntry::new("config.toml", FileKind::File).with_size(256),
        FileEntry::new("package.json", FileKind::File).with_size(384),
        FileEntry::new("data.json", FileKind::File).with_size(10240),
        FileEntry::new("README.md", FileKind::File).with_size(3072),
        FileEntry::new(".gitignore", FileKind::File).with_size(128),
        FileEntry::new(".env", FileKind::File).with_size(64),
        FileEntry::new("build.sh", FileKind::Symlink).with_size(48),
    ]
}

fn build_project_tree() -> TreeNode {
    TreeNode::new("my-app").with_children(vec![
        TreeNode::new("src").with_children(vec![
            TreeNode::new("main.rs"),
            TreeNode::new("lib.rs"),
            TreeNode::new("models")
                .with_children(vec![TreeNode::new("user.rs"), TreeNode::new("post.rs")]),
        ]),
        TreeNode::new("tests").with_children(vec![
            TreeNode::new("test_app.py"),
            TreeNode::new("integration.rs"),
        ]),
        TreeNode::new("docs")
            .with_children(vec![TreeNode::new("README.md"), TreeNode::new("API.md")]),
        TreeNode::new("Cargo.toml"),
        TreeNode::new("package.json"),
        TreeNode::new(".gitignore"),
    ])
}

impl Screen for FileBrowser {
    type Message = Event;

    fn update(&mut self, event: &Event) -> Cmd<Self::Message> {
        // Panel switching
        if let Event::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
            && modifiers.contains(Modifiers::CTRL)
        {
            self.focus = self.focus.next();
            return Cmd::None;
        }
        if let Event::Key(KeyEvent {
            code: KeyCode::Left,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
            && modifiers.contains(Modifiers::CTRL)
        {
            self.focus = self.focus.prev();
            return Cmd::None;
        }

        match self.focus {
            Panel::FilePicker => {
                if let Event::Key(KeyEvent {
                    kind: KeyEventKind::Press,
                    ..
                }) = event
                {
                    match event {
                        Event::Key(KeyEvent {
                            code: KeyCode::Up, ..
                        }) => self.picker.move_up(),
                        Event::Key(KeyEvent {
                            code: KeyCode::Down,
                            ..
                        }) => self.picker.move_down(),
                        Event::Key(KeyEvent {
                            code: KeyCode::Home,
                            ..
                        }) => self.picker.move_to_first(),
                        Event::Key(KeyEvent {
                            code: KeyCode::End, ..
                        }) => self.picker.move_to_last(),
                        Event::Key(KeyEvent {
                            code: KeyCode::PageUp,
                            ..
                        }) => self.picker.page_up(),
                        Event::Key(KeyEvent {
                            code: KeyCode::PageDown,
                            ..
                        }) => self.picker.page_down(),
                        Event::Key(KeyEvent {
                            code: KeyCode::Char('h'),
                            ..
                        }) => {
                            self.show_hidden = !self.show_hidden;
                            self.picker.set_filter(FilePickerFilter {
                                allowed_extensions: vec![],
                                show_hidden: self.show_hidden,
                            });
                        }
                        _ => {}
                    }
                }
            }
            Panel::Preview => {
                if let Event::Key(KeyEvent {
                    code: KeyCode::Up,
                    kind: KeyEventKind::Press,
                    ..
                }) = event
                {
                    self.preview_scroll = self.preview_scroll.saturating_sub(1);
                }
                if let Event::Key(KeyEvent {
                    code: KeyCode::Down,
                    kind: KeyEventKind::Press,
                    ..
                }) = event
                {
                    self.preview_scroll = self.preview_scroll.saturating_add(1);
                }
            }
            Panel::FileTree => {}
        }

        Cmd::None
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }

        let main = Flex::vertical()
            .constraints([Constraint::Min(1), Constraint::Fixed(1)])
            .split(area);

        let cols = Flex::horizontal()
            .constraints([
                Constraint::Fixed(25),
                Constraint::Percentage(40.0),
                Constraint::Min(20),
            ])
            .split(main[0]);

        self.render_tree_panel(frame, cols[0]);
        self.render_picker_panel(frame, cols[1]);
        self.render_preview_panel(frame, cols[2]);

        // Status bar
        let entry_info = self
            .picker
            .selected_entry()
            .map(|e| {
                let size = e.size.map(filesize::decimal).unwrap_or_default();
                format!("{} {}", e.name, size)
            })
            .unwrap_or_else(|| "(no selection)".into());
        let status = format!(
            "{} | Ctrl+\u{2190}/\u{2192}: panels | \u{2191}/\u{2193}: navigate | h: toggle hidden",
            entry_info
        );
        Paragraph::new(&*status)
            .style(Style::new().fg(theme::fg::MUTED).bg(theme::bg::SURFACE))
            .render(main[1], frame);
    }

    fn keybindings(&self) -> Vec<HelpEntry> {
        vec![
            HelpEntry {
                key: "Ctrl+\u{2190}/\u{2192}",
                action: "Switch panel",
            },
            HelpEntry {
                key: "\u{2191}/\u{2193}",
                action: "Navigate files",
            },
            HelpEntry {
                key: "Home/End",
                action: "First/last",
            },
            HelpEntry {
                key: "PgUp/PgDn",
                action: "Page scroll",
            },
            HelpEntry {
                key: "h",
                action: "Toggle hidden files",
            },
        ]
    }

    fn title(&self) -> &'static str {
        "File Browser"
    }

    fn tab_label(&self) -> &'static str {
        "Files"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        })
    }

    fn ctrl_press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::CTRL,
            kind: KeyEventKind::Press,
        })
    }

    #[test]
    fn initial_state() {
        let screen = FileBrowser::new();
        assert_eq!(screen.focus, Panel::FilePicker);
        assert_eq!(screen.title(), "File Browser");
        assert_eq!(screen.tab_label(), "Files");
    }

    #[test]
    fn panel_navigation() {
        let mut screen = FileBrowser::new();
        screen.update(&ctrl_press(KeyCode::Right));
        assert_eq!(screen.focus, Panel::Preview);
        screen.update(&ctrl_press(KeyCode::Right));
        assert_eq!(screen.focus, Panel::FileTree);
        screen.update(&ctrl_press(KeyCode::Left));
        assert_eq!(screen.focus, Panel::Preview);
    }

    #[test]
    fn file_selection() {
        let mut screen = FileBrowser::new();
        assert_eq!(screen.picker.selected_index(), 0);
        screen.update(&press(KeyCode::Down));
        assert_eq!(screen.picker.selected_index(), 1);
        screen.update(&press(KeyCode::Up));
        assert_eq!(screen.picker.selected_index(), 0);
    }

    #[test]
    fn toggle_hidden() {
        let mut screen = FileBrowser::new();
        assert!(!screen.show_hidden);
        screen.update(&press(KeyCode::Char('h')));
        assert!(screen.show_hidden);
    }

    #[test]
    fn simulated_entries_not_empty() {
        let entries = simulated_entries();
        assert!(entries.len() > 10);
        assert!(entries.iter().any(|e| e.is_dir()));
        assert!(entries.iter().any(|e| e.kind == FileKind::Symlink));
    }
}
