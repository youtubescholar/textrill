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

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use textrill::options::Options;
use textrill_gui::options_panel::OptionsPanel;
use textrill_gui::{Chooser, SaveAnswer, TextrillApp};

/// A chooser that answers `None`, as if the user closed the dialog.
struct CancellingChooser;

impl Chooser for CancellingChooser {
    fn open_text(&self, _from: Option<&Path>) -> Option<PathBuf> {
        None
    }
    fn save_text(&self, _suggested: &Path) -> Option<PathBuf> {
        None
    }
    fn save_html(&self, _suggested: &Path) -> Option<PathBuf> {
        None
    }
}

/// A chooser that always picks `path`, so a save can be tested with no portal.
struct PathChooser(PathBuf);

impl Chooser for PathChooser {
    fn open_text(&self, _from: Option<&Path>) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn save_text(&self, _suggested: &Path) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn save_html(&self, _suggested: &Path) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

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
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
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
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    assert!(harness.state().1.demoronize, "default should be on");

    harness.get_by_label(DEMORONIZE).click();
    harness.run();

    assert!(
        !harness.state().1.demoronize,
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
    harness.get_by_label("File");
    harness.get_by_label("Edit");
    harness.get_by_label("View");
    harness.get_by_label("Help");
}

/// The filter box hides the options it does not match, and clearing it brings
/// them back. Ported from `test_filter_hides_rows`.
#[test]
fn the_filter_hides_rows() {
    let mut panel = OptionsPanel::default();
    assert!(panel.matches("unhyphenation"));
    panel.filter = "unhyphenation".to_string();
    assert!(panel.matches("unhyphenation"));
    assert!(
        !panel.matches("tab_width"),
        "the filter let a stranger through"
    );
    panel.clear_filter();
    assert!(panel.matches("tab_width"), "clearing the filter lost a row");
}

/// A filter that matches nothing hides everything, and clearing it shows
/// everything again. Ported from `test_filter_never_hides_everything_at_once`.
#[test]
fn the_filter_hides_everything_and_restores_it() {
    let mut panel = OptionsPanel {
        filter: "zzzz-no-such-option".to_string(),
    };
    for spec in textrill::cli::SPECS {
        assert!(!panel.matches(spec.names[0]), "{}", spec.names[0]);
    }
    panel.clear_filter();
    for spec in textrill::cli::SPECS {
        assert!(panel.matches(spec.names[0]), "{}", spec.names[0]);
    }
}

/// `Reset all options` puts every option back to its default. Ported from
/// `test_reset_restores_defaults`.
#[test]
fn reset_options_restores_defaults() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    textrill::cli::set_value(&mut harness.state_mut().1, "bold_delimiter", "^").expect("set");
    assert_eq!(
        textrill::cli::get_value(&harness.state().1, "bold_delimiter").unwrap(),
        "^"
    );

    harness.run();
    harness.get_by_label("Reset all options").click();
    harness.run();

    assert_eq!(
        textrill::cli::get_value(&harness.state().1, "bold_delimiter").unwrap(),
        "#",
        "reset did not restore the default"
    );
}

/// The example text exercises emphasis, strong, links, tables and lists.
/// Ported from `test_sample_text_exercises_the_converter`.
#[test]
fn the_sample_text_exercises_the_converter() {
    let mut app = TextrillApp::default();
    app.load_sample();
    textrill::cli::set_value(&mut app.opts, "make_tables", "true").expect("set");
    app.request_conversion();
    wait_for_conversion(&mut app, Duration::from_secs(10));

    for (what, fragment) in [
        ("emphasis", "<em>emphasis</em>"),
        ("strong", "<strong>strong</strong>"),
        ("link", "example.org"),
        ("table", "<table"),
        ("heading", "EXAMPLE"),
        ("list", "<ol"),
    ] {
        assert!(app.output.contains(fragment), "the sample lost its {what}");
    }
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

// --------------------------------------------------------- unsaved changes

/// A clean document runs `New` at once: there is nothing to lose.
#[test]
fn a_clean_document_does_not_ask_before_new() {
    let mut app = TextrillApp::default();
    app.new_document();
    assert!(!app.is_prompting());
    assert!(app.doc.text.is_empty());
}

/// An edited document holds `New` behind the prompt until it is answered.
#[test]
fn a_dirty_document_asks_before_new() {
    let mut app = TextrillApp::default();
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.new_document();
    assert!(app.is_prompting(), "the prompt did not open");
    assert_eq!(app.doc.text, "edited", "New ran before it was answered");
}

/// `Cancel` leaves the edits and drops the waiting command.
#[test]
fn cancelling_the_prompt_keeps_the_edits() {
    let mut app = TextrillApp::default();
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.new_document();
    app.resolve_save_prompt(SaveAnswer::Cancel, &egui::Context::default());
    assert!(!app.is_prompting());
    assert_eq!(app.doc.text, "edited");
}

/// `Discard` throws the edits away and runs the command.
#[test]
fn discarding_the_prompt_runs_the_command() {
    let mut app = TextrillApp::default();
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.new_document();
    app.resolve_save_prompt(SaveAnswer::Discard, &egui::Context::default());
    assert!(!app.is_prompting());
    assert!(
        app.doc.text.is_empty(),
        "Discard did not clear the document"
    );
}

/// `Save` writes the text to the opened file, then runs the command.
#[test]
fn saving_the_prompt_writes_the_text_then_runs_the_command() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let src = std::env::temp_dir().join(format!(
        "textrill-prompt-{}-{nanos}.txt",
        std::process::id()
    ));
    std::fs::write(&src, "original").expect("write source");

    let mut app = TextrillApp::default();
    app.load_file(&src).expect("load");
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.new_document();
    assert!(app.is_prompting());

    app.resolve_save_prompt(SaveAnswer::Save, &egui::Context::default());

    assert!(!app.is_prompting());
    assert!(
        app.doc.text.is_empty(),
        "the command did not run after saving"
    );
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "edited");
    let _ = std::fs::remove_file(&src);
}

/// With nowhere to write, `Save` asks the chooser; a cancelled chooser drops
/// the command and keeps the edits -- the same result as cancelling Qt's dialog.
#[test]
fn saving_with_nowhere_to_write_drops_the_command() {
    let mut app = TextrillApp::default();
    app.set_chooser(Box::new(CancellingChooser));
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.new_document();
    app.resolve_save_prompt(SaveAnswer::Save, &egui::Context::default());
    assert!(!app.is_prompting());
    assert_eq!(app.doc.text, "edited", "the edits were lost without a save");
    assert!(
        !app.status.contains("saved"),
        "a cancelled save must not report success: {}",
        app.status
    );
}

/// When the chooser does name a file, `Save` writes the text and runs the
/// waiting command.
#[test]
fn saving_through_the_chooser_writes_then_runs_the_command() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let target = std::env::temp_dir().join(format!(
        "textrill-chooser-{}-{nanos}.txt",
        std::process::id()
    ));

    let mut app = TextrillApp::default();
    app.set_chooser(Box::new(PathChooser(target.clone())));
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.new_document();
    assert!(app.is_prompting());

    app.resolve_save_prompt(SaveAnswer::Save, &egui::Context::default());

    assert!(!app.is_prompting());
    assert!(
        app.doc.text.is_empty(),
        "the command did not run after saving"
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "edited");
    let _ = std::fs::remove_file(&target);
}

/// The prompt is a real widget: it draws, and its buttons answer it.
#[test]
fn the_prompt_is_drawn_and_discard_works() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.state_mut().doc.text = "edited".to_string();
    harness.state_mut().doc.dirty = true;
    harness.state_mut().new_document();
    harness.run();

    harness.get_by_label("Unsaved changes");
    harness.get_by_label("Discard").click();
    harness.run();

    assert!(!harness.state().is_prompting());
    assert!(harness.state().doc.text.is_empty());
}

fn wait_for_conversion(app: &mut TextrillApp, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while app.completed < app.latest && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        app.drain();
    }
}
