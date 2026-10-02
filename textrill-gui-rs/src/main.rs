// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Window entry point.

#![forbid(unsafe_code)]

fn main() -> eframe::Result<()> {
    // A window big enough for the input, the preview and the option panel
    // without scrolling on a typical screen. The minimum keeps the controls
    // reachable on something small; the user can scale the whole UI from the
    // Display row, and the OS scale factor is applied on top of both.
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([900.0, 720.0])
        .with_min_inner_size([360.0, 300.0]);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "textrill",
        options,
        Box::new(|_cc| Ok(Box::new(textrill_gui::TextrillApp::default()))),
    )
}
