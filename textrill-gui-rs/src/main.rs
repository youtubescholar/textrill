// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Window entry point.

#![forbid(unsafe_code)]

use textrill_gui::args;
use textrill_gui::window_state;
use textrill_gui::Settings;

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

    // Restore the geometry the window remembers: its size, whether it was
    // maximized and its zoom -- never its position, because a stale position is
    // how a window reopens on a monitor that is no longer there (and Wayland
    // will not let us set one at all). The size has already been clamped on
    // read; `eframe` clamps it again to the monitor, and a corrupt file simply
    // falls back to the default.
    let settings = Settings::from_default();
    let size = settings.window_size().unwrap_or(window_state::DEFAULT_SIZE);
    let maximized = settings.maximized().unwrap_or(false);
    let zoom = settings.zoom().unwrap_or(1.0);

    // A window big enough for the input, the preview and the option panel
    // without scrolling on a typical screen. The minimum keeps the controls
    // reachable on something small; the user can scale the whole UI from the
    // Display row, and the OS scale factor is applied on top of both.
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size(window_state::MIN_SIZE)
        .with_clamp_size_to_monitor_size(true)
        .with_maximized(maximized);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "textrill",
        options,
        Box::new(move |cc| {
            textrill_gui::fonts::install(&cc.egui_ctx);
            // Apply the zoom before the first frame, so the UI never appears at
            // 100% and then jumps.
            cc.egui_ctx.set_zoom_factor(zoom);
            let mut app = textrill_gui::TextrillApp::with_settings(settings);
            app.apply_command_line(&parsed);
            Ok(Box::new(app))
        }),
    )
}
