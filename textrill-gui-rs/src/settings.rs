// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Persistent settings, in the file the Python GUI already uses.
//!
//! The Python window used `QSettings("textrill-gui", "textrill")`, which on
//! Linux writes an INI file at `~/.config/textrill-gui/textrill.conf` with the
//! keys in a `[General]` section. This is the same file and the same layout, so
//! a user's existing `auto` choice carries over when the native GUI replaces
//! the Python one -- and a hand-edited `auto=true` keeps working, which is what
//! `test_auto_convert_setting_survives_a_hand_edited_config` pins.
//!
//! The `auto` flag and the `options` JSON blob the Python window stored with
//! `json.dumps` are modelled, as is the native window's geometry under the keys
//! `window-size`, `maximized` and `zoom` (see [`crate::window_state`] for why
//! no position is stored). Values are escaped the way `QSettings` escaped them,
//! so the same file round-trips between the two front ends. The write path
//! rewrites the whole `[General]` section, which is what `QSettings` did too,
//! and does so atomically so a crash mid-write cannot corrupt it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A `QSettings`-compatible INI store.
///
/// The path is always explicit so tests can keep their settings in a temporary
/// directory instead of the user's real configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    path: PathBuf,
    values: BTreeMap<String, String>,
}

impl Settings {
    /// The default location: `$XDG_CONFIG_HOME/textrill-gui/textrill.conf`, or
    /// `~/.config/...` when `XDG_CONFIG_HOME` is unset.
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(std::env::temp_dir);
        base.join("textrill-gui").join("textrill.conf")
    }

    /// Open the store at `path`, reading it if it exists.
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        let mut settings = Self {
            path: path.into(),
            values: BTreeMap::new(),
        };
        settings.reload();
        settings
    }

    /// Open the default store.
    pub fn from_default() -> Self {
        Self::with_path(Self::default_path())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The raw value for `key`, if the file or a `set` put one there.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// The `auto` flag, if it is set and readable. Accepts the same spellings
    /// Python's `_as_bool` did, because a hand-edited file may hold a string
    /// where `QSettings` would have stored a real bool.
    pub fn auto(&self) -> Option<bool> {
        self.get("auto").map(parse_bool)
    }

    /// The stored option blob, exactly the JSON text `mainwindow.py` wrote.
    pub fn options(&self) -> Option<String> {
        self.get("options").map(str::to_string)
    }

    /// Set `key` and write the file immediately.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.values.insert(key.to_string(), value.to_string());
        self.write()
    }

    /// Set the `auto` flag.
    pub fn set_auto(&mut self, auto: bool) -> Result<(), String> {
        self.set("auto", if auto { "true" } else { "false" })
    }

    /// Set the option blob (raw JSON; it is escaped for the file on write).
    pub fn set_options(&mut self, json: &str) -> Result<(), String> {
        self.set("options", json)
    }

    /// The saved normal window size, clamped; `None` when unset or unreadable.
    ///
    /// Size is stored in points before zoom, which is the value
    /// `ViewportBuilder::with_inner_size` takes on the next launch.
    pub fn window_size(&self) -> Option<[f32; 2]> {
        self.get("window-size")
            .and_then(crate::window_state::parse_size)
    }

    /// Whether the window was maximized when it was last closed.
    pub fn maximized(&self) -> Option<bool> {
        self.get("maximized").map(parse_bool)
    }

    /// The saved zoom factor, clamped; `None` when unset or unreadable.
    pub fn zoom(&self) -> Option<f32> {
        self.get("zoom").and_then(crate::window_state::parse_zoom)
    }

    /// Persist the whole geometry in a single atomic write: the normal size,
    /// whether the window was maximized, and the zoom. Position is deliberately
    /// absent; see [`crate::window_state`].
    pub fn set_window_state(
        &mut self,
        size: [f32; 2],
        maximized: bool,
        zoom: f32,
    ) -> Result<(), String> {
        self.values.insert(
            "window-size".to_string(),
            crate::window_state::format_size(size),
        );
        self.values.insert(
            "maximized".to_string(),
            if maximized { "true" } else { "false" }.to_string(),
        );
        self.values.insert(
            "zoom".to_string(),
            format!("{:.3}", crate::window_state::clamp_zoom(zoom)),
        );
        self.write()
    }

    /// Forget the stored geometry, leaving every other key alone.
    pub fn clear_window_state(&mut self) -> Result<(), String> {
        self.values.remove("window-size");
        self.values.remove("maximized");
        self.values.remove("zoom");
        self.write()
    }

    /// Remove `key` and write the file immediately.
    pub fn remove(&mut self, key: &str) -> Result<(), String> {
        self.values.remove(key);
        self.write()
    }

    /// Re-read the file, replacing the in-memory copy.
    pub fn reload(&mut self) {
        self.values.clear();
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return;
        };
        let mut in_general = true;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                // Top-level keys (before any section) and `[General]` are the
                // ones `QSettings` uses for `QSettings(org, app)`.
                in_general = section.eq_ignore_ascii_case("General");
                continue;
            }
            if !in_general {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                self.values
                    .insert(key.trim().to_string(), unescape_ini(value));
            }
        }
    }

    /// Write the `[General]` section back out, creating the directory.
    ///
    /// Values are escaped the way `QSettings` escapes them, so the option blob
    /// (`{...}` with `"`, `,` and `=`) comes out quoted and a hand-written file
    /// keeps working.
    ///
    /// The bytes go to a sibling temporary file that is then renamed over the
    /// real one. A rename is atomic, so a crash (or a second instance) can never
    /// leave a truncated file that fails to parse on the next launch.
    fn write(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut out = String::from("[General]\n");
        for (key, value) in &self.values {
            out.push_str(key);
            out.push('=');
            out.push_str(&escape_ini(value));
            out.push('\n');
        }
        let mut temporary = self.path.clone().into_os_string();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        std::fs::write(&temporary, out).map_err(|e| format!("{}: {e}", temporary.display()))?;
        std::fs::rename(&temporary, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            format!("{}: {e}", self.path.display())
        })
    }
}

/// Escape a value the way `QSettings`' INI writer does: always escape the
/// backslash and control characters, and quote when the value could otherwise
/// be mistaken for a list or a key/value separator.
fn escape_ini(value: &str) -> String {
    let mut escaped = String::new();
    for c in value.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c => escaped.push(c),
        }
    }
    let quoted = value.starts_with(' ') || value.ends_with(' ') || value.contains([',', ';', '=']);
    if quoted {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

/// The inverse of [`escape_ini`]: strip the surrounding quotes if any, then
/// resolve the backslash escapes. Unquoted values are trimmed, so a
/// hand-written `auto = true` still reads as `true`.
fn unescape_ini(value: &str) -> String {
    let trimmed = value.trim();
    let (inner, quoted) = match trimmed.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(inner) => (inner, true),
        None => (trimmed, false),
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    if quoted {
        out
    } else {
        out.trim().to_string()
    }
}

/// Read a setting that may be a real bool, a number, or a string.
pub fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}
