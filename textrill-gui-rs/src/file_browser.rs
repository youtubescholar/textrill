// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! A built-in file browser, drawn with egui.
//!
//! The native chooser ([`crate::dialogs::NativeChooser`]) goes through the XDG
//! desktop portal, which is the right thing inside a Flatpak sandbox and on the
//! big desktops. It is not always there: `xdg-desktop-portal-wlr` (Sway) and
//! `xdg-desktop-portal-hyprland` do not implement `FileChooser`, and a minimal
//! install may have neither a backend nor `zenity`. In that case the portal call
//! fails and `rfd` returns `None`, which is indistinguishable from a cancel, so
//! Open/Save silently does nothing.
//!
//! This browser is the fallback: a plain widget that reads directories and
//! returns a path, so a native install on any Linux desktop (or anywhere egui
//! runs) can still open and save. It never writes to disk; the caller does that.

use std::path::{Path, PathBuf};

use egui::{Context, RichText};

/// What is being chosen. Fixes the title, the file-name filter, and whether the
/// confirm button says "Open" or "Save".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileIntent {
    OpenText,
    SaveText,
    SaveHtml,
}

impl FileIntent {
    /// The window heading.
    pub fn title(self) -> &'static str {
        match self {
            FileIntent::OpenText => "Open text file",
            FileIntent::SaveText => "Save text",
            FileIntent::SaveHtml => "Save HTML",
        }
    }

    /// The label on the confirm button.
    fn confirm_label(self) -> &'static str {
        if self.is_save() {
            "Save"
        } else {
            "Open"
        }
    }

    /// Whether this intent writes a file (rather than reading one).
    pub fn is_save(self) -> bool {
        matches!(self, FileIntent::SaveText | FileIntent::SaveHtml)
    }

    /// The extensions the list shows, lower-case and without the dot.
    fn extensions(self) -> &'static [&'static str] {
        match self {
            FileIntent::OpenText | FileIntent::SaveText => &["txt", "text"],
            FileIntent::SaveHtml => &["html", "htm"],
        }
    }
}

/// What drawing the browser decided this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserOutcome {
    /// Still open; keep it on screen.
    Pending,
    /// Dismissed by Cancel, Escape, or a click on the backdrop.
    Cancelled,
    /// Confirmed; the caller should act on this path.
    Chosen(PathBuf),
}

/// One row in the listing.
struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

/// The browser's whole state: where it is, what is typed, what is listed.
pub struct FileBrowser {
    intent: FileIntent,
    directory: PathBuf,
    file_name: String,
    entries: Vec<Entry>,
    selected: Option<usize>,
    show_all: bool,
    error: Option<String>,
}

impl FileBrowser {
    /// A browser for reading a text file, starting where `from` lives.
    pub fn for_open(from: Option<&Path>) -> Self {
        let directory = from
            .and_then(Path::parent)
            .filter(|dir| dir.is_dir())
            .map(Path::to_path_buf)
            .unwrap_or_else(home_dir);
        let file_name = from
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self::new(FileIntent::OpenText, directory, file_name)
    }

    /// A browser for writing, proposing `suggested`'s directory and name.
    pub fn for_save(intent: FileIntent, suggested: &Path) -> Self {
        debug_assert!(intent.is_save());
        let directory = suggested
            .parent()
            .filter(|dir| dir.is_dir())
            .map(Path::to_path_buf)
            .unwrap_or_else(home_dir);
        let file_name = suggested
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self::new(intent, directory, file_name)
    }

    fn new(intent: FileIntent, directory: PathBuf, file_name: String) -> Self {
        let mut browser = Self {
            intent,
            directory,
            file_name,
            entries: Vec::new(),
            selected: None,
            show_all: false,
            error: None,
        };
        browser.refresh();
        browser
    }

    /// The intent this browser was opened for.
    pub fn intent(&self) -> FileIntent {
        self.intent
    }

    /// The directory on screen, for tests and status messages.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Re-read the current directory into the listing.
    fn refresh(&mut self) {
        self.entries.clear();
        self.selected = None;
        match std::fs::read_dir(&self.directory) {
            Ok(read) => {
                for entry in read.flatten() {
                    let path = entry.path();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !is_dir && !self.matches_filter(&name) {
                        continue;
                    }
                    self.entries.push(Entry { path, name, is_dir });
                }
                self.entries.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("{}: {error}", self.directory.display()));
            }
        }
    }

    /// Whether a file name passes the intent's filter (or "all files" is on).
    fn matches_filter(&self, name: &str) -> bool {
        if self.show_all {
            return true;
        }
        let Some(extension) = Path::new(name).extension().and_then(|e| e.to_str()) else {
            return false;
        };
        self.intent
            .extensions()
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(extension))
    }

    /// Change directory and re-list.
    fn enter(&mut self, directory: PathBuf) {
        self.directory = directory;
        self.refresh();
    }

    /// The path the typed name points at, or the reason it is unusable.
    fn confirm(&self) -> Result<PathBuf, String> {
        let name = self.file_name.trim();
        if name.is_empty() {
            return Err("Enter a file name".to_string());
        }
        Ok(self.directory.join(name))
    }

    /// Draw the browser for one frame.
    pub fn show(&mut self, ctx: &Context) -> BrowserOutcome {
        let mut navigate: Option<PathBuf> = None;
        let mut confirm = false;
        let mut cancel = false;

        let response = egui::Modal::new(egui::Id::new("file-browser")).show(ctx, |ui| {
            ui.set_max_width(540.0);
            ui.heading(self.intent.title());

            ui.horizontal_wrapped(|ui| {
                if ui.button("Up").clicked() {
                    navigate = self.directory.parent().map(Path::to_path_buf);
                }
                if ui.button("Home").clicked() {
                    navigate = Some(home_dir());
                }
                ui.label(RichText::new(self.directory.display().to_string()).monospace());
            });

            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.entries.is_empty() && self.error.is_none() {
                        ui.weak("(no matching files)");
                    }
                    for index in 0..self.entries.len() {
                        let (name, is_dir, path) = {
                            let entry = &self.entries[index];
                            (entry.name.clone(), entry.is_dir, entry.path.clone())
                        };
                        let label = if is_dir {
                            RichText::new(format!("{name}/")).strong()
                        } else {
                            RichText::new(name.clone())
                        };
                        if ui
                            .selectable_label(self.selected == Some(index), label)
                            .clicked()
                        {
                            if is_dir {
                                navigate = Some(path);
                            } else {
                                self.selected = Some(index);
                                self.file_name = name;
                            }
                        }
                    }
                });

            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("File name:");
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.file_name)
                        .hint_text(self.intent.extensions().first().map_or("", |_| "name.txt")),
                );
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    confirm = true;
                }
            });
            if ui
                .checkbox(&mut self.show_all, "All files")
                .on_hover_text("Show files that do not match the filter")
                .changed()
            {
                self.refresh();
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(self.intent.confirm_label()).clicked() {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });

        // A directory click wins over everything: staying open is the point.
        if let Some(directory) = navigate {
            if directory.is_dir() {
                self.enter(directory);
            }
            return BrowserOutcome::Pending;
        }
        if confirm {
            return match self.confirm() {
                Ok(path) => BrowserOutcome::Chosen(path),
                Err(message) => {
                    self.error = Some(message);
                    BrowserOutcome::Pending
                }
            };
        }
        if cancel || response.should_close() {
            return BrowserOutcome::Cancelled;
        }
        BrowserOutcome::Pending
    }
}

/// The user's home directory, or the current directory if `HOME` is unset.
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A fresh scratch directory, unique per call.
    fn scratch(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("textrill-fb-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn names(browser: &FileBrowser) -> Vec<String> {
        browser
            .entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    #[test]
    fn open_shows_the_filtered_listing_with_directories_first() {
        let dir = scratch("open");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("notes.text"), "n").unwrap();
        std::fs::write(dir.join("b.md"), "b").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();

        let browser = FileBrowser::for_open(Some(&dir.join("a.txt")));

        assert_eq!(browser.intent(), FileIntent::OpenText);
        assert_eq!(browser.directory(), dir.as_path());
        assert_eq!(browser.file_name, "a.txt");
        // Directories first, then the text files; the markdown file is filtered.
        assert_eq!(names(&browser), vec!["sub", "a.txt", "notes.text"]);
    }

    #[test]
    fn show_all_lifts_the_extension_filter() {
        let dir = scratch("all");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("b.md"), "b").unwrap();

        let mut browser = FileBrowser::for_open(Some(&dir.join("a.txt")));
        assert_eq!(names(&browser), vec!["a.txt"]);

        browser.show_all = true;
        browser.refresh();
        assert_eq!(names(&browser), vec!["a.txt", "b.md"]);
    }

    #[test]
    fn save_uses_the_html_filter_and_proposed_name() {
        let dir = scratch("save");
        std::fs::write(dir.join("page.html"), "h").unwrap();
        std::fs::write(dir.join("note.txt"), "t").unwrap();

        let browser = FileBrowser::for_save(FileIntent::SaveHtml, &dir.join("out.html"));

        assert_eq!(browser.intent(), FileIntent::SaveHtml);
        assert!(browser.intent().is_save());
        assert_eq!(browser.file_name, "out.html");
        assert_eq!(names(&browser), vec!["page.html"]);
    }

    #[test]
    fn entering_a_directory_re_lists_it() {
        let dir = scratch("enter");
        let sub = dir.join("inner");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("deep.txt"), "d").unwrap();

        let mut browser = FileBrowser::for_open(None);
        browser.enter(sub.clone());

        assert_eq!(browser.directory(), sub.as_path());
        assert_eq!(names(&browser), vec!["deep.txt"]);
    }

    #[test]
    fn confirm_joins_the_typed_name_onto_the_directory() {
        let dir = scratch("confirm");
        let browser = FileBrowser::for_save(FileIntent::SaveText, &dir.join("out.txt"));
        assert_eq!(browser.confirm().unwrap(), dir.join("out.txt"));
    }

    #[test]
    fn confirm_refuses_an_empty_name() {
        let dir = scratch("empty");
        let mut browser = FileBrowser::for_save(FileIntent::SaveText, &dir.join("out.txt"));
        browser.file_name.clear();
        assert!(browser.confirm().is_err());
    }

    #[test]
    fn a_missing_directory_is_reported_not_panicked() {
        let dir = scratch("missing");
        let mut browser = FileBrowser::for_open(None);
        browser.enter(dir.join("does-not-exist"));
        assert!(browser.error.is_some());
        assert!(browser.entries.is_empty());
    }
}
