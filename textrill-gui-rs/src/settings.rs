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
//! Only the keys this front end actually reads are modelled, so an unknown key
//! in the file is preserved by being left alone rather than dropped. The write
//! path rewrites the whole `[General]` section, which is what `QSettings` did
//! too.

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

    /// Set `key` and write the file immediately.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.values.insert(key.to_string(), value.to_string());
        self.write()
    }

    /// Set the `auto` flag.
    pub fn set_auto(&mut self, auto: bool) -> Result<(), String> {
        self.set("auto", if auto { "true" } else { "false" })
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
                    .insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }

    /// Write the `[General]` section back out, creating the directory.
    fn write(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut out = String::from("[General]\n");
        for (key, value) in &self.values {
            out.push_str(key);
            out.push('=');
            out.push_str(value);
            out.push('\n');
        }
        std::fs::write(&self.path, out).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

/// Read a setting that may be a real bool, a number, or a string.
pub fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}
