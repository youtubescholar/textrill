// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Application state and layout.

use egui::Ui;
use textrill::convert::Converter;
use textrill::options::Options;

use crate::options_panel;

/// The whole front end.
///
/// This is a skeleton: it converts synchronously and keeps no worker. The
/// generation-tagged, queue-dropping worker contract in `SURFACE.md` §3 lands
/// next and replaces [`TextrillApp::convert_now`] as the preview's source.
pub struct TextrillApp {
    /// The live option set. The panel writes straight through
    /// `textrill::cli::set_value`, so this is the same model the engine sees.
    pub opts: Options,
    /// Source text.
    pub input: String,
    /// Rendered HTML, kept in sync on every edit.
    pub output: String,
    /// Last conversion outcome, shown next to the Convert control.
    pub status: String,
}

impl Default for TextrillApp {
    fn default() -> Self {
        Self {
            opts: Options::default(),
            input: String::new(),
            output: String::new(),
            status: "Ready".to_string(),
        }
    }
}

impl TextrillApp {
    /// Convert `input` with the current options.
    ///
    /// A panic is caught and reported, not allowed to unwind into the event
    /// loop. The engine's panic paths are the inherited-hang class (P5), and
    /// the reference front end treated a converter panic as a user-visible
    /// error for the same reason.
    pub fn convert_now(&mut self) {
        let converted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut converter = Converter::new(self.opts.clone());
            converter.convert_text(&self.input)
        }));
        match converted {
            Ok(html) => {
                self.output = html;
                self.status = "Ready".to_string();
            }
            Err(_) => {
                self.status = "The converter stopped on invalid input".to_string();
            }
        }
    }

    /// Draw the whole UI into `ui`.
    pub fn draw(&mut self, ui: &mut Ui) {
        ui.heading("textrill");
        ui.horizontal(|ui| {
            ui.label("Input");
            if ui.button("Convert").clicked() {
                self.convert_now();
            }
            ui.label(&self.status);
        });

        if ui
            .add(egui::TextEdit::multiline(&mut self.input).desired_rows(6))
            .changed()
        {
            self.convert_now();
        }

        ui.separator();
        egui::CollapsingHeader::new("Options")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| options_panel::draw(ui, &mut self.opts));
            });

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
