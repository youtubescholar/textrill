// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Application state and layout.

use std::sync::Arc;

use egui::Ui;
use textrill::options::Options;

use crate::options_panel;
use crate::worker::ConversionWorker;

/// The whole front end.
pub struct TextrillApp {
    /// The live option set. The panel writes straight through
    /// `textrill::cli::set_value`, so this is the same model the engine sees.
    pub opts: Options,
    /// Source text.
    pub input: String,
    /// Rendered HTML, or the error text after a failed conversion.
    pub output: String,
    /// Status line: `Ready`, `converting…`, a summary, or `error`.
    pub status: String,
    /// The newest conversion requested.
    pub latest: u64,
    /// The newest conversion applied. `latest > completed` means work is out.
    pub completed: u64,
    worker: Arc<ConversionWorker>,
    waker_installed: bool,
}

impl Default for TextrillApp {
    fn default() -> Self {
        Self {
            opts: Options::default(),
            input: String::new(),
            output: String::new(),
            status: "Ready".to_string(),
            latest: 0,
            completed: 0,
            worker: ConversionWorker::new(2),
            waker_installed: false,
        }
    }
}

impl TextrillApp {
    /// Queue a conversion of the current text and options.
    ///
    /// Waiting work is dropped by the worker; a burst of edits leaves the
    /// newest job waiting, not one copy of the document per keystroke.
    pub fn request_conversion(&mut self) {
        self.latest = self.worker.convert(&self.input, &self.opts);
        self.status = "converting…".to_string();
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
                    self.status = format!("{lines} lines · {:.0} ms", outcome.seconds * 1000.0);
                }
            }
        }
    }

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

        ui.heading("textrill");
        ui.horizontal(|ui| {
            ui.label("Input");
            if ui.button("Convert").clicked() {
                self.request_conversion();
            }
            ui.label(&self.status);
        });

        if ui
            .add(egui::TextEdit::multiline(&mut self.input).desired_rows(6))
            .changed()
        {
            self.request_conversion();
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
            self.request_conversion();
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
