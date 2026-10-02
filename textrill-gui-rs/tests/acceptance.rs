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

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use textrill::options::Options;
use textrill_gui::{options_panel, TextrillApp};

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
        |ui, opts: &mut Options| options_panel::draw(ui, opts),
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
    const DEMORONIZE: &str = "Convert Microsoft character codes into sensible HTML.";
    let mut harness = Harness::new_ui_state(
        |ui, opts: &mut Options| options_panel::draw(ui, opts),
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

/// The conversion path really calls the engine.
#[test]
fn convert_now_uses_the_engine() {
    let mut app = TextrillApp {
        input: "Hello".to_string(),
        ..Default::default()
    };
    app.convert_now();
    assert!(
        app.output.contains("<p>Hello</p>"),
        "unexpected output: {}",
        app.output
    );
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
