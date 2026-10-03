//! Native file choosers, kept behind a tiny safe wrapper.
//!
//! The rest of the app never sees `rfd`: it asks a [`Chooser`], which either
//! returns the chosen path or `None` when the user cancels. None of the
//! choosers write to disk; opening and saving stay in `document`.
//! `can_create_directories` is off so a mistyped name cannot silently grow a
//! tree of empty folders.

use std::path::{Path, PathBuf};

const TEXT_EXTENSIONS: &[&str] = &["txt", "text"];
const HTML_EXTENSIONS: &[&str] = &["html", "htm"];
const ALL_EXTENSIONS: &[&str] = &["*"];

/// The file choosers the app asks for paths.
///
/// Tests swap in a stub so the command logic can run without a desktop
/// portal; the real implementation is [`NativeChooser`].
pub trait Chooser {
    /// `File → Open…`. `from` is the current document, if any, so the chooser
    /// opens where the user last worked.
    fn open_text(&self, from: Option<&Path>) -> Option<PathBuf>;

    /// `File → Save text…`. `suggested` is the full path to propose.
    fn save_text(&self, suggested: &Path) -> Option<PathBuf>;

    /// `File → Save As…`. `suggested` is the full path to propose.
    fn save_html(&self, suggested: &Path) -> Option<PathBuf>;
}

/// The desktop's own chooser, through the XDG desktop portal.
pub struct NativeChooser;

impl Chooser for NativeChooser {
    fn open_text(&self, from: Option<&Path>) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Open text file")
            .add_filter("Text files", TEXT_EXTENSIONS)
            .add_filter("All files", ALL_EXTENSIONS);
        if let Some(directory) = from.and_then(Path::parent) {
            dialog = dialog.set_directory(directory);
        }
        dialog.pick_file()
    }

    fn save_text(&self, suggested: &Path) -> Option<PathBuf> {
        save_dialog("Save text", suggested, TEXT_EXTENSIONS)
    }

    fn save_html(&self, suggested: &Path) -> Option<PathBuf> {
        save_dialog("Save HTML", suggested, HTML_EXTENSIONS)
    }
}

fn save_dialog(title: &str, suggested: &Path, extensions: &[&str]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title(title)
        .set_can_create_directories(false)
        .add_filter(title, extensions)
        .add_filter("All files", ALL_EXTENSIONS);
    if let Some(directory) = suggested.parent() {
        dialog = dialog.set_directory(directory);
    }
    if let Some(name) = suggested.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    dialog.save_file()
}
