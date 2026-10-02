// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Application state and layout.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::Ui;
use textrill::options::Options;

use crate::document::Document;
use crate::options_panel;
use crate::settings::Settings;
use crate::worker::ConversionWorker;

/// How long to wait after the last change before converting.
pub const AUTO_CONVERT_DELAY: Duration = Duration::from_millis(300);

/// What a save command wants next.
///
/// There is no dialog yet, so a save with nowhere to write reports the name it
/// would have proposed. The chrome slice wires that to a real file chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveRequest {
    /// Written to this path.
    Saved(PathBuf),
    /// The caller must choose a name; this is what `Save As` would propose.
    NeedsName(PathBuf),
}

/// The whole front end.
pub struct TextrillApp {
    /// The open document and its file state.
    pub doc: Document,
    /// The live option set. The panel writes straight through
    /// `textrill::cli::set_value`, so this is the same model the engine sees.
    pub opts: Options,
    /// Rendered HTML, or the error text after a failed conversion.
    pub output: String,
    /// Status line: `Ready`, `converting…`, a summary, or `error`.
    pub status: String,
    /// The newest conversion requested.
    pub latest: u64,
    /// The newest conversion applied. `latest > completed` means work is out.
    pub completed: u64,
    /// Convert as the text changes, after [`AUTO_CONVERT_DELAY`].
    pub auto: bool,
    worker: Arc<ConversionWorker>,
    waker_installed: bool,
    /// When the debounced conversion is due, if one is pending.
    deadline: Option<Instant>,
    /// The window title last sent to the platform, so it is only sent on
    /// change. Sending it every frame requests a repaint every frame, which
    /// never lets the UI go to sleep.
    title_sent: Option<String>,
    settings: Settings,
}

impl Default for TextrillApp {
    fn default() -> Self {
        Self::with_settings(Settings::from_default())
    }
}

impl TextrillApp {
    /// Build an app backed by `settings`, applying the stored `auto` choice.
    ///
    /// Tests pass a [`Settings`] in a temporary directory so a developer's real
    /// configuration cannot change what the tests observe.
    pub fn with_settings(settings: Settings) -> Self {
        let mut app = Self {
            doc: Document::new(),
            opts: Options::default(),
            output: String::new(),
            status: "Ready".to_string(),
            latest: 0,
            completed: 0,
            auto: true,
            worker: ConversionWorker::new(2),
            waker_installed: false,
            deadline: None,
            title_sent: None,
            settings,
        };
        app.restore_settings();
        app
    }

    /// The options a conversion actually uses.
    ///
    /// `meta_charset` is forced on: the engine defaults it off so no golden
    /// moves, which is right for a byte-compatible CLI, but the GUI always
    /// writes UTF-8 and hands the file to a browser, which guesses wrong
    /// without a declaration.
    pub fn conversion_options(&self) -> Options {
        let mut opts = self.opts.clone();
        opts.meta_charset = true;
        opts
    }

    /// Apply the persisted `auto` choice, if there is one.
    pub fn restore_settings(&mut self) {
        if let Some(auto) = self.settings.auto() {
            self.auto = auto;
        }
    }

    /// Persist the current `auto` choice.
    pub fn store_settings(&mut self) {
        if let Err(error) = self.settings.set_auto(self.auto) {
            self.status = format!("could not save settings: {error}");
        }
    }

    // ----------------------------------------------------------- converting

    /// Queue a conversion of the current text and options.
    ///
    /// Waiting work is dropped by the worker; a burst of edits leaves the
    /// newest job waiting, not one copy of the document per keystroke.
    pub fn request_conversion(&mut self) {
        let opts = self.conversion_options();
        self.latest = self.worker.convert(&self.doc.text, &opts);
        self.status = "converting…".to_string();
    }

    /// A text edit: mark the document dirty and, when auto is on, debounce.
    pub fn on_edit(&mut self, now: Instant) {
        self.doc.dirty = true;
        if self.auto {
            self.schedule_convert(now);
        }
    }

    /// Queue a conversion, coalescing a burst of changes.
    ///
    /// When auto is off an *option* change still converts immediately, but a
    /// never-converted document does too, so the preview is never empty by
    /// default.
    pub fn schedule_convert(&mut self, now: Instant) {
        if self.auto || self.latest == 0 {
            self.deadline = Some(now + AUTO_CONVERT_DELAY);
        } else {
            self.convert_now();
        }
    }

    /// Convert immediately, cancelling any pending debounce.
    pub fn convert_now(&mut self) {
        self.deadline = None;
        self.request_conversion();
    }

    /// Fire the debounced conversion if its deadline has passed.
    pub fn tick(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline {
            if now >= deadline {
                self.convert_now();
            }
        }
    }

    /// Apply every finished conversion, dropping results older than the newest
    /// request.
    pub fn drain(&mut self) {
        while let Some(outcome) = self.worker.poll() {
            if outcome.generation < self.latest {
                continue; // a newer conversion is already under way
            }
            self.completed = outcome.generation;
            match outcome.error {
                Some(error) => {
                    self.output = error;
                    self.status = "error".to_string();
                }
                None => {
                    let lines = if outcome.html.is_empty() {
                        0
                    } else {
                        outcome.html.matches('\n').count() + 1
                    };
                    self.output = outcome.html;
                    self.doc.note_converted(&self.output);
                    self.status = format!("{lines} lines · {:.0} ms", outcome.seconds * 1000.0);
                }
            }
        }
    }

    // ----------------------------------------------------------- documents

    /// `File → New`.
    pub fn new_document(&mut self) {
        self.doc.reset();
        self.convert_now();
    }

    /// `File → Load example`.
    pub fn load_sample(&mut self) {
        self.doc.load_sample();
        self.convert_now();
    }

    /// Open `path`, remembering its encoding. Never overwrites the text on
    /// failure.
    pub fn load_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.doc.load(path)?;
        self.convert_now();
        Ok(())
    }

    /// `File → Save text`. Uses the opened path, or reports the one to propose.
    pub fn save_text(&mut self) -> Result<SaveRequest, String> {
        match self.doc.path.clone() {
            Some(path) => {
                self.doc.save_text_to(&path)?;
                Ok(SaveRequest::Saved(path))
            }
            None => Ok(SaveRequest::NeedsName(self.doc.suggested_text_name())),
        }
    }

    /// `File → Save text`, to an explicit path.
    pub fn save_text_to(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.doc.save_text_to(path)
    }

    /// `File → Save`: write the HTML, never over the text file being converted.
    pub fn save_document(&mut self) -> Result<SaveRequest, String> {
        match self.doc.output_path.clone() {
            Some(path) => {
                self.save_html_to(&path)?;
                Ok(SaveRequest::Saved(path))
            }
            None => Ok(SaveRequest::NeedsName(self.doc.suggested_output_name())),
        }
    }

    /// `File → Save As`, to an explicit path.
    pub fn save_html_to(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.doc.save_html_to(path, &self.output)
    }

    /// The name `Save As` should propose.
    pub fn suggested_output_name(&self) -> PathBuf {
        self.doc.suggested_output_name()
    }

    // ------------------------------------------------------------------ ui

    /// Draw the whole UI into `ui`.
    pub fn draw(&mut self, ui: &mut Ui) {
        // The worker asks for a redraw when a result lands, so the preview does
        // not wait for the next input event to appear.
        if !self.waker_installed {
            let ctx = ui.ctx().clone();
            self.worker.set_waker(move || ctx.request_repaint());
            self.waker_installed = true;
        }
        self.drain();
        self.tick(Instant::now());
        if self.deadline.is_some() {
            ui.ctx().request_repaint_after(AUTO_CONVERT_DELAY);
        }
        let title = self.doc.title();
        if self.title_sent.as_deref() != Some(title.as_str()) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_sent = Some(title);
        }
        ui.heading("textrill");
        ui.horizontal(|ui| {
            ui.label("Input");
            if ui.button("Convert").clicked() {
                self.convert_now();
            }
            if ui.checkbox(&mut self.auto, "auto").changed() {
                self.store_settings();
                if self.auto {
                    self.schedule_convert(Instant::now());
                } else {
                    self.deadline = None;
                }
            }
            ui.label(&self.status);
        });

        // Whole-UI zoom, for the same reason a browser has it: a 4K laptop and
        // a 96 DPI desktop should not dictate the same physical text size. The
        // keyboard shortcuts (Ctrl/⌘ + / - / 0) work too; these buttons exist
        // so the feature is discoverable. `zoom_factor` multiplies the OS's own
        // scale factor, so a HiDPI screen is already handled before this.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Display").small());
            if ui
                .button("Zoom out")
                .on_hover_text("Zoom out (Ctrl+-)")
                .clicked()
            {
                egui::gui_zoom::zoom_out(ui.ctx());
            }
            ui.label(format!("{:.0}%", ui.ctx().zoom_factor() * 100.0));
            if ui
                .button("Zoom in")
                .on_hover_text("Zoom in (Ctrl+=)")
                .clicked()
            {
                egui::gui_zoom::zoom_in(ui.ctx());
            }
            if ui
                .button("Reset zoom")
                .on_hover_text("Reset to 100% (Ctrl+0)")
                .clicked()
            {
                ui.ctx().set_zoom_factor(1.0);
            }
        });

        if ui
            .add(egui::TextEdit::multiline(&mut self.doc.text).desired_rows(6))
            .changed()
        {
            let now = Instant::now();
            self.on_edit(now);
        }

        ui.separator();
        let mut options_changed = false;
        egui::CollapsingHeader::new("Options")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        options_changed = options_panel::draw(ui, &mut self.opts);
                    });
            });
        if options_changed {
            self.schedule_convert(Instant::now());
        }

        ui.separator();
        ui.label("HTML");
        ui.add(
            egui::TextEdit::multiline(&mut self.output)
                .code_editor()
                .desired_rows(8),
        );
    }
}

impl eframe::App for TextrillApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}
