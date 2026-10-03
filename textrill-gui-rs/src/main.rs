// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Window entry point.

#![forbid(unsafe_code)]

use textrill_gui::args;

fn main() -> eframe::Result<()> {
    let parsed = match args::parse(std::env::args_os().skip(1)) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("textrill-gui: try `textrill-gui --help` for more information");
            std::process::exit(2);
        }
    };
    if parsed.help {
        print!("{}", args::USAGE);
        return Ok(());
    }
    if parsed.version {
        // The engine and this window are released together and share a version.
        println!("textrill-gui for textrill {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

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
        Box::new(move |_cc| {
            let mut app = textrill_gui::TextrillApp::default();
            app.apply_command_line(&parsed);
            Ok(Box::new(app))
        }),
    )
}
