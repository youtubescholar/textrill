// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The open document and everything the window knows about the file behind it.
//!
//! Ported from `MainWindow`'s file state in `mainwindow.py`, with the same
//! distinctions because each one is a bug that was paid for:
//!
//! - `path` is the text file that was opened, `output_path` is where the HTML
//!   was last written. They are deliberately different files.
//! - `encoding` is the encoding the *source* was decoded with, so "Save text"
//!   writes the same bytes back rather than transcoding the file behind the
//!   user's back. It resets with each load: a new file's encoding says nothing
//!   about the old one.
//! - `dirty` tracks edits to the text; `output_stale` tracks HTML that has not
//!   been written to `output_path` yet. Saving the HTML must never make unsaved
//!   *text* edits look saved, so the two flags are separate.
//! - `saved_html` is the exact HTML last written, so re-converting the same
//!   text does not keep claiming there is unsaved output.

use std::path::{Path, PathBuf};

use textrill::convert::decode_bytes_with;
use textrill::encode::write_with;
use textrill::options::Encoding;

/// The example text `File → Load example` (and the Python `SAMPLE_TEXT`) uses.
pub const SAMPLE_TEXT: &str = "\
EXAMPLE HEADER
=============

This is a paragraph with *emphasis*, #strong# text and a link to
https://example.org/ in it.  Paragraphs, lists, tables and preformatted
text are all recognised:

1. first item
2. second item
    - a nested bullet
    - another one

+--------+--------+
| Food   | Qty    |
+--------+--------+
| Bread  | 1      |
| Milk   | 1      |
| Oranges| 3      |
| Apples | 6      |
+--------+--------+

Turn on \"make tables\" in the options panel to convert that block into a
real table.
";

/// The text, plus the file state around it.
#[derive(Debug, Clone)]
pub struct Document {
    /// The current text.
    pub text: String,
    /// The source file, if the text came from one.
    pub path: Option<PathBuf>,
    /// Where the HTML was last written, if it has been.
    pub output_path: Option<PathBuf>,
    /// The encoding the source was decoded with; always concrete, never `Auto`.
    pub encoding: Encoding,
    /// Unsaved edits to the text.
    pub dirty: bool,
    /// HTML that differs from what is on disk at `output_path`.
    pub output_stale: bool,
    /// The HTML last written to `output_path`.
    pub saved_html: Option<String>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            text: String::new(),
            path: None,
            output_path: None,
            encoding: Encoding::Utf8,
            dirty: false,
            output_stale: false,
            saved_html: None,
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the document, as `File → New` does.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Replace the text with the example, flagged as an unsaved edit.
    pub fn load_sample(&mut self) {
        self.text = SAMPLE_TEXT.to_string();
        self.path = None;
        self.output_path = None;
        self.saved_html = None;
        self.output_stale = false;
        // Like any other edit, it has not been saved anywhere yet.
        self.dirty = true;
        // The sample is UTF-8; a previous file's encoding must not carry over,
        // or a later "Save text" would try to write the sample as CP1252.
        self.encoding = Encoding::Utf8;
    }

    /// Read `path` the way the converter would, and remember its encoding.
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        // Read the bytes here rather than through `convert::read_with`, which
        // returns `None` on any I/O error: a save dialog is where a mistyped
        // path happens, and the user needs the reason, not a blank failure.
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (text, resolved) = decode_bytes_with(&bytes, Encoding::Auto);
        self.text = text;
        self.path = Some(path.to_path_buf());
        // A newly opened file has no saved HTML yet.
        self.output_path = None;
        self.saved_html = None;
        self.output_stale = false;
        self.dirty = false;
        self.encoding = resolved.encoding();
        Ok(())
    }

    /// Write the text back, in the encoding it was read with.
    pub fn save_text_to(&mut self, path: &Path) -> Result<(), String> {
        write_with(path, &self.text, self.encoding).map_err(|e| e.to_string())?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    /// Record the HTML a finished conversion produced.
    ///
    /// Sets `output_stale` only when the HTML differs from what was last saved,
    /// so re-converting an unchanged document does not re-flag it.
    pub fn note_converted(&mut self, html: &str) {
        if self.saved_html.as_deref() != Some(html) {
            self.output_stale = true;
        }
    }

    /// Write the generated HTML, never over the text file being converted.
    pub fn save_html_to(&mut self, path: &Path, html: &str) -> Result<(), String> {
        // The guard runs *before* touching the disk: the source must stay
        // intact even when a path was typed by hand into the dialog.
        if let Some(source) = &self.path {
            if same_target(path, source) {
                return Err(format!(
                    "{} is the file you are converting.\n\
                     Choose a different name, or use File \u{2192} Save As.",
                    path.display()
                ));
            }
        }
        // The generated HTML is UTF-8 whatever the source was: it is text this
        // program produced, not a transcription of the input.
        write_with(path, html, Encoding::Utf8).map_err(|e| e.to_string())?;
        self.output_path = Some(path.to_path_buf());
        self.saved_html = Some(html.to_string());
        // Saving the HTML does not make edits to the *source* go away, so
        // `dirty` is untouched.
        self.output_stale = false;
        Ok(())
    }

    /// Where `Save As` should propose a name.
    ///
    /// The suggestion is the *output* name; proposing the source `.txt` back
    /// would invite overwriting the very file being converted.
    pub fn suggested_output_name(&self) -> PathBuf {
        if let Some(base) = self.output_path.as_ref().or(self.path.as_ref()) {
            base.with_extension("html")
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("converted.html")
        }
    }

    /// Where `Save text` should propose a name when the text has no file yet.
    pub fn suggested_text_name(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("untitled.txt")
        })
    }

    /// The window title: the file name, `*` for unsaved text, `+` for unsaved
    /// HTML, then the program name.
    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled");
        let mark = format!(
            "{}{}",
            if self.dirty { "*" } else { "" },
            if self.output_stale { "+" } else { "" }
        );
        format!("{name}{mark} \u{2014} textrill")
    }
}

/// Whether two paths name the same file.
///
/// Canonicalises where it can, so a relative and an absolute spelling of the
/// same existing file compare equal, and falls back to a lexical absolute path
/// for a target that does not exist yet (which is the common case for a save).
fn same_target(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => match (std::path::absolute(a), std::path::absolute(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        },
    }
}
