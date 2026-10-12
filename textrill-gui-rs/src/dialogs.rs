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

/// Whether the desktop provides a native file dialog this build can use.
///
/// The dialog goes through `rfd`'s `xdg-portal` backend: the D-Bus
/// `org.freedesktop.portal.FileChooser`. That needs either a portal backend
/// that implements `FileChooser` or `zenity` (which `rfd` falls back to).
/// `xdg-desktop-portal-wlr` (Sway/wlroots) and `xdg-desktop-portal-hyprland`
/// do **not** implement it, so those desktops report `false` unless a backend
/// such as `xdg-desktop-portal-gtk` is installed and routed.
///
/// This is a heuristic, not a live D-Bus probe: it reads the portal's own
/// backend declarations (`*.portal` files under `xdg-desktop-portal/portals`)
/// for a backend that offers `org.freedesktop.impl.portal.FileChooser`, and
/// checks `PATH` for `zenity`. A wrong answer is harmless because the app can
/// always be told to use its built-in browser instead.
pub fn system_dialog_available() -> bool {
    portal_file_chooser_available() || zenity_available()
}

/// Whether some installed portal backend declares `FileChooser`.
fn portal_file_chooser_available() -> bool {
    portal_file_chooser_available_in(&portal_data_dirs())
}

/// [`portal_file_chooser_available`] against an explicit list of data
/// directories, so it can be tested without touching the host's `/usr`.
fn portal_file_chooser_available_in(data_dirs: &[PathBuf]) -> bool {
    const INTERFACE: &str = "org.freedesktop.impl.portal.FileChooser";
    for dir in data_dirs {
        let entries = match std::fs::read_dir(dir.join("xdg-desktop-portal/portals")) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) != Some("portal") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                if portal_declares_file_chooser(&text, INTERFACE) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether a `*.portal` file's `Interfaces` line offers `interface`.
fn portal_declares_file_chooser(text: &str, interface: &str) -> bool {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| key.trim() == "Interfaces")
        .any(|(_, value)| value.split(';').any(|item| item.trim() == interface))
}

/// The data directories the portal searches, in order: `$XDG_DATA_HOME`, then
/// `$XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`).
fn portal_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        if !data_home.is_empty() {
            dirs.push(PathBuf::from(data_home));
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }
    match std::env::var_os("XDG_DATA_DIRS") {
        Some(dirs_var) if !dirs_var.is_empty() => {
            dirs.extend(std::env::split_paths(&dirs_var));
        }
        _ => dirs.extend([
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ]),
    }
    dirs
}

/// Whether `zenity` is on `PATH`; `rfd` uses it when the portal call fails.
fn zenity_available() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join("zenity");
        candidate.is_file()
    })
}

/// The file choosers the app asks for paths.
///
/// Tests swap in a stub so the command logic can run without a desktop
/// portal; the real implementation is [`NativeChooser`].
pub trait Chooser {
    /// `File → Open…`. `from` is the file or directory to open in, if known,
    /// so the chooser starts where the user last worked.
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
        if let Some(start) = from {
            let directory = if start.is_dir() {
                start
            } else {
                start.parent().unwrap_or(start)
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const INTERFACE: &str = "org.freedesktop.impl.portal.FileChooser";

    #[test]
    fn interface_line_is_parsed_from_a_portal_file() {
        let text = "[portal]\nDBusName=org.freedesktop.impl.portal.desktop.gtk\n\
                    Interfaces=org.freedesktop.impl.portal.FileChooser;\
                    org.freedesktop.impl.portal.OpenURI;\n";
        assert!(portal_declares_file_chooser(text, INTERFACE));
    }

    #[test]
    fn a_wlr_portal_does_not_claim_the_file_chooser() {
        // xdg-desktop-portal-wlr offers screencopy, not FileChooser.
        let text = "[portal]\nInterfaces=org.freedesktop.impl.portal.Screenshot;\
                    org.freedesktop.impl.portal.ScreenCast;\n";
        assert!(!portal_declares_file_chooser(text, INTERFACE));
    }

    #[test]
    fn a_backend_in_a_data_dir_is_found() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root =
            std::env::temp_dir().join(format!("textrill-portal-{}-{nanos}", std::process::id()));
        let portals = root.join("xdg-desktop-portal/portals");
        std::fs::create_dir_all(&portals).unwrap();
        std::fs::write(
            portals.join("gtk.portal"),
            "[portal]\nInterfaces=org.freedesktop.impl.portal.FileChooser;\n",
        )
        .unwrap();

        assert!(portal_file_chooser_available_in(std::slice::from_ref(
            &root
        )));

        // A directory with no portals does not claim availability.
        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(!portal_file_chooser_available_in(std::slice::from_ref(
            &empty
        )));

        let _ = std::fs::remove_dir_all(&root);
    }
}
