// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Window entry point.

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "textrill",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(textrill_gui::TextrillApp::default()))),
    )
}
