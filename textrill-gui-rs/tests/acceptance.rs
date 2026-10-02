// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Acceptance tests for the native front end.
//!
//! These run headlessly: `egui_kittest` builds an AccessKit tree from the same
//! `draw` code the window uses, so a widget is queried by the label a user
//! sees. No display, no windowing system.

use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use textrill::options::Options;
use textrill_gui::{options_panel, TextrillApp};

/// `Options.demoronize` is on by default, so a click must turn it off.
const DEMORONIZE: &str = "Convert Microsoft character codes into sensible HTML.";

/// The help string is the panel's label for every kind, so a duplicated help
/// string would make a by-label query ambiguous. Enforce that first.
#[test]
fn option_help_strings_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for spec in textrill::cli::SPECS {
        assert!(
            seen.insert(spec.help),
            "duplicate help string: {}",
            spec.help
        );
    }
    assert_eq!(seen.len(), 54, "the engine's option count changed");
}

/// Every option in `cli::SPECS` is reachable as a labelled widget.
#[test]
fn the_panel_exposes_every_option() {
    let mut harness = Harness::new_ui_state(
        |ui, opts: &mut Options| {
            options_panel::draw(ui, opts);
        },
        Options::default(),
    );
    // The panel is taller than a default test window; give it room so nothing
    // is clipped out of the access tree.
    harness.set_size(egui::vec2(1000.0, 4000.0));
    harness.run();

    for spec in textrill::cli::SPECS {
        harness.get_by_label(spec.help);
    }
}

/// A checkbox click reaches the engine's own option set.
#[test]
fn clicking_a_checkbox_writes_through_to_options() {
    let mut harness = Harness::new_ui_state(
        |ui, opts: &mut Options| {
            options_panel::draw(ui, opts);
        },
        Options::default(),
    );
    assert!(harness.state().demoronize, "default should be on");

    harness.get_by_label(DEMORONIZE).click();
    harness.run();

    assert!(
        !harness.state().demoronize,
        "the click did not reach `Options.demoronize`"
    );
}

/// A queued conversion is applied to the preview when it finishes.
#[test]
#[allow(clippy::field_reassign_with_default)]
fn a_conversion_reaches_the_preview() {
    let mut app = TextrillApp::default();
    app.doc.text = "Hello".to_string();
    app.request_conversion();
    wait_for_conversion(&mut app, Duration::from_secs(10));

    assert!(
        app.output.contains("<p>Hello</p>"),
        "unexpected output: {}",
        app.output
    );
    assert_eq!(app.completed, app.latest, "the preview never settled");
    assert_ne!(app.status, "converting…", "the status never cleared");
}

/// The window has its controls, and the app draws without panicking.
#[test]
fn the_app_exposes_its_controls() {
    let harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.get_by_label("Input");
    harness.get_by_label("Convert");
    harness.get_by_label("HTML");
    harness.get_by_label("Options");
}

/// The whole UI scales at runtime, so a 4K panel and a 96 DPI projector are
/// both readable. egui ships the keyboard shortcuts; this pins that they reach
/// us, because a font/DPI refactor could silently drop them.
#[test]
fn the_ui_zooms_with_the_keyboard() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.run();
    assert_eq!(harness.ctx.zoom_factor(), 1.0, "the UI starts at 100%");

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Plus);
    harness.run();
    let zoomed = harness.ctx.zoom_factor();
    assert!(zoomed > 1.0, "Ctrl+= did not zoom in: {zoomed}");

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num0);
    harness.run();
    assert_eq!(harness.ctx.zoom_factor(), 1.0, "Ctrl+0 did not reset");
}

/// Zoom is also a visible control, not only a shortcut.
#[test]
fn the_display_controls_are_exposed() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.run();

    harness.get_by_label("Zoom out");
    harness.get_by_label("Zoom in");
    harness.get_by_label("Reset zoom");

    harness.get_by_label("Zoom in").click();
    harness.run();
    harness.run();
    assert!(
        harness.ctx.zoom_factor() > 1.0,
        "the Zoom in button did not scale the UI"
    );
}

fn wait_for_conversion(app: &mut TextrillApp, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while app.completed < app.latest && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        app.drain();
    }
}
