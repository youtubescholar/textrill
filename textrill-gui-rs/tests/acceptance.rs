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

use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use textrill::options::Options;
use textrill_gui::options_panel::OptionsPanel;
use textrill_gui::theme::Theme;
use textrill_gui::{Chooser, SaveAnswer, Settings, TextrillApp};

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

/// The panel's label for an option, looked up by a long name so a test does not
/// hard-code the help text.
fn help_for(name: &str) -> &'static str {
    textrill::cli::SPECS
        .iter()
        .find(|spec| spec.names.contains(&name))
        .map(|spec| spec.help)
        .unwrap_or_else(|| panic!("no engine option named {name}"))
}

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
    // Adding an engine option means adding a panel widget. The count is here
    // to make an addition a deliberate act rather than something that happens
    // when the engine grows an option.
    assert_eq!(seen.len(), 72, "the engine's option count changed");
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

/// The Document type choice is one control over the engine's mutually
/// exclusive `html5`/`xhtml` flags, including the HTML 4 state neither flag
/// names on its own.
#[test]
fn the_document_type_choice_sets_the_html_flags() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.set_size(egui::vec2(1000.0, 4000.0));
    harness.run();

    let (html5, xhtml) = {
        let opts = &harness.state().1;
        (opts.html5, opts.xhtml)
    };
    assert!(html5 && !xhtml, "the default should be HTML5");

    harness.get_by_label("XHTML").click();
    harness.run();
    let opts = &harness.state().1;
    assert!(opts.xhtml && !opts.html5, "XHTML did not set the flags");

    harness.get_by_label("HTML4").click();
    harness.run();
    let opts = &harness.state().1;
    assert!(!opts.html5 && !opts.xhtml, "HTML4 did not clear both flags");

    harness.get_by_label("HTML5").click();
    harness.run();
    let opts = &harness.state().1;
    assert!(opts.html5 && !opts.xhtml, "HTML5 did not set the flags");
}

/// The template source is one radio over the engine's three mutually exclusive
/// template options: picking a source reveals its own field and clears the
/// others, and a source with an empty field stays selected across frames.
#[test]
fn the_template_source_choice_clears_the_others() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.set_size(egui::vec2(1000.0, 4000.0));
    harness.run();

    // A source with an empty field has no option to live in, so the panel must
    // remember it; otherwise the field would vanish on the next frame.
    let before = harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .count();
    harness.get_by_label("Body wrapper").click();
    harness.run();
    harness.run();
    let after = harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .count();
    assert_eq!(after, before + 1, "the body-template field did not persist");

    // Picking another source clears the first rather than leaving both set.
    harness.state_mut().1.template = "body.html".to_string();
    harness.run();
    harness.get_by_label("Shipped").click();
    harness.run();

    let opts = &harness.state().1;
    assert_eq!(opts.template, "", "the body template survived the switch");
    assert_eq!(opts.document_template, "");
}

/// The link handling is one radio over the two link flags, including the
/// "links only" mode that both flags on describe.
#[test]
fn the_link_mode_choice_sets_the_link_flags() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.set_size(egui::vec2(1000.0, 4000.0));

    {
        let opts = &harness.state().1;
        assert!(
            opts.make_links && !opts.link_only,
            "the default should link"
        );
    }

    harness.get_by_label("No links").click();
    harness.run();
    {
        let opts = &harness.state().1;
        assert!(!opts.make_links && !opts.link_only, "No links set a flag");
    }

    harness.get_by_label("Links only").click();
    harness.run();
    {
        let opts = &harness.state().1;
        assert!(
            opts.make_links && opts.link_only,
            "Links only did not set both"
        );
    }

    harness.get_by_label("Make links").click();
    harness.run();
    {
        let opts = &harness.state().1;
        assert!(
            opts.make_links && !opts.link_only,
            "Make links left link_only on"
        );
    }
}

/// The input encoding is one radio over the engine's encoding values.
#[test]
fn the_encoding_choice_sets_the_encoding() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.set_size(egui::vec2(1000.0, 4000.0));
    harness.run();

    assert_eq!(harness.state().1.encoding.name(), "auto", "the default");

    harness.get_by_label("UTF-16 LE").click();
    harness.run();
    assert_eq!(harness.state().1.encoding.name(), "utf-16le");

    harness.get_by_label("Windows-1251").click();
    harness.run();
    assert_eq!(harness.state().1.encoding.name(), "cp1251");
}

/// The title radio is one control over the engine's `--title`/`--titlefirst`
/// precedence: "First line" turns `titlefirst` on and clears any explicit
/// title, "Explicit title" turns it off and reveals the field, and "No title"
/// clears both.
#[test]
fn the_title_source_choice_sets_title_and_titlefirst() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.set_size(egui::vec2(1000.0, 4000.0));
    harness.run();

    harness.get_by_label("First line").click();
    harness.run();
    let opts = &harness.state().1;
    assert!(
        opts.titlefirst && opts.title.is_empty(),
        "First line did not turn on titlefirst"
    );

    harness.get_by_label("Explicit title").click();
    harness.run();
    assert!(
        !harness.state().1.titlefirst,
        "Explicit title did not turn off titlefirst"
    );

    harness.get_by_label("No title").click();
    harness.run();
    let opts = &harness.state().1;
    assert!(
        !opts.titlefirst && opts.title.is_empty(),
        "No title did not clear both"
    );
}

/// A dependent option is drawn disabled, not hidden: it is still in the
/// accessibility tree (so the panel names it), but it cannot be clicked until
/// its parent gives it an effect.
#[test]
fn a_dependent_option_is_disabled_until_its_parent_is_on() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.set_size(egui::vec2(1000.0, 4000.0));
    harness.run();

    assert!(
        harness.get_by_label("ALIGN").accesskit_node().is_disabled(),
        "the table column options should be disabled while tables are off"
    );

    harness.state_mut().1.make_tables = true;
    harness.run();
    assert!(
        !harness.get_by_label("ALIGN").accesskit_node().is_disabled(),
        "the table column options should enable once tables are on"
    );
}

/// The command-line pipeline options do nothing to the document the GUI
/// converts (the worker calls `convert_text`), so they are drawn disabled — and
/// still named, so the panel keeps exposing every engine option.
#[test]
fn the_command_line_pipeline_options_are_disabled() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.set_size(egui::vec2(1000.0, 4000.0));
    harness.run();

    // The flags are queryable by their own label. The `--infile`/`--instring`/
    // `--outfile` fields draw their help as a plain caption, so the unit test
    // `command_line_pipeline_options_are_disabled` covers those.
    for name in ["stream", "chunk", "report"] {
        assert!(
            harness
                .get_by_label(help_for(name))
                .accesskit_node()
                .is_disabled(),
            "{name} should be disabled in the GUI"
        );
    }
    // A GUI-relevant option stays live.
    assert!(
        !harness
            .get_by_label(help_for("extract"))
            .accesskit_node()
            .is_disabled(),
        "body-only output is meaningful in the GUI and must stay live"
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
        app.output.contains("<p>Hello"),
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
    let mut panel = OptionsPanel::default();
    panel.filter = "zzzz-no-such-option".to_string();
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

/// `var` is the one `StrArray` whose elements are `name=value` pairs rather
/// than bare strings. Editing it must reach `Options.vars`. Regression: the
/// widget used to fall through to a no-op arm and drop the edit.
#[test]
fn the_var_list_edits_through_to_options() {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut (OptionsPanel, Options)| {
            state.0.draw(ui, &mut state.1);
        },
        (OptionsPanel::default(), Options::default()),
    );
    harness.state_mut().0.filter = "var".to_string();
    harness.set_size(egui::vec2(1000.0, 600.0));
    harness.run();

    let field = harness
        .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
        .next()
        .expect("the var editor");
    field.focus();
    field.type_text("greeting=hi");
    harness.run();

    assert_eq!(
        harness.state().1.vars,
        vec![("greeting".to_string(), "hi".to_string())],
        "the var edit did not reach Options.vars"
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

/// Zoom is also a visible control in the View menu, not only a shortcut.
#[test]
fn the_display_controls_are_exposed() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.run();

    harness.get_by_label("View").click();
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

/// At a narrow width the toolbar folds instead of running off the edge, so no
/// control becomes clipped and unreachable. This is the reflow half of the
/// promise that the UI stays usable at 200% zoom (WCAG 1.4.10).
#[test]
fn the_toolbar_reflows_at_a_narrow_width() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.set_size(egui::vec2(360.0, 640.0));
    harness.run();

    for label in ["New", "Save", "Example", "Convert", "Options", "Focus"] {
        harness.get_by_label(label);
    }

    // The display controls now live in the View menu; they must stay reachable
    // at the same narrow width, and the menu bar is the first row, so opening
    // it never folds it off the edge.
    harness.get_by_label("View").click();
    harness.run();
    for label in ["Zoom out", "Zoom in", "Reset zoom"] {
        harness.get_by_label(label);
    }
}

/// The escape hatch is always in the menu, so a window size or zoom the user
/// does not want is one action away from the default.
#[test]
fn the_reset_geometry_command_is_in_the_view_menu() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.run();

    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Reset window size and zoom");
}

/// The same escape hatch has a keyboard route, so it still works when the menu
/// bar itself has been folded off the edge by a high zoom.
#[test]
fn the_reset_geometry_shortcut_works() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "textrill-reset-{}-{nanos}.conf",
        std::process::id()
    ));

    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    app.window_size = [1200.0, 900.0];
    app.window_maximized = true;
    app.zoom = 2.0;
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    harness.key_press_modifiers(
        egui::Modifiers {
            command: true,
            alt: true,
            ..egui::Modifiers::NONE
        },
        egui::Key::Num0,
    );
    harness.run();

    assert_eq!(harness.ctx.zoom_factor(), 1.0, "zoom was not reset");
    assert!(
        !harness.state().window_maximized,
        "maximized was not cleared"
    );
    assert_eq!(
        Settings::with_path(&path).zoom(),
        None,
        "the stored geometry was not cleared"
    );
    let _ = std::fs::remove_file(&path);
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

/// Opening while there are edits asks first, like New.
#[test]
fn a_dirty_document_asks_before_open() {
    let mut app = TextrillApp::default();
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.open_document();
    assert!(app.is_prompting());
}

/// With nothing to lose, `Open` goes straight to the chooser.
#[test]
fn a_clean_document_opens_the_chosen_file() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let src =
        std::env::temp_dir().join(format!("textrill-open-{}-{nanos}.txt", std::process::id()));
    std::fs::write(&src, "hello").expect("write source");

    let mut app = TextrillApp::default();
    app.set_chooser(Box::new(PathChooser(src.clone())));
    app.open_document();

    assert!(!app.is_prompting());
    assert_eq!(app.doc.path.as_deref(), Some(src.as_path()));
    assert_eq!(app.doc.text, "hello");
    let _ = std::fs::remove_file(&src);
}

/// Discarding the prompt runs the waiting `Open`, which loads the chosen file.
#[test]
fn discarding_the_prompt_opens_the_chosen_file() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let src =
        std::env::temp_dir().join(format!("textrill-open-{}-{nanos}.txt", std::process::id()));
    std::fs::write(&src, "hello").expect("write source");

    let mut app = TextrillApp::default();
    app.set_chooser(Box::new(PathChooser(src.clone())));
    app.doc.text = "edited".to_string();
    app.doc.dirty = true;
    app.open_document();
    assert!(app.is_prompting());

    app.resolve_save_prompt(SaveAnswer::Discard, &egui::Context::default());

    assert!(!app.is_prompting());
    assert_eq!(app.doc.path.as_deref(), Some(src.as_path()));
    assert_eq!(app.doc.text, "hello");
    let _ = std::fs::remove_file(&src);
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

/// The startup flags set the options the menu would, and open the file.
#[test]
fn the_command_line_sets_options_and_opens_a_file() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let src =
        std::env::temp_dir().join(format!("textrill-args-{}-{nanos}.txt", std::process::id()));
    std::fs::write(&src, "hello").expect("write source");

    let args = textrill_gui::args::parse(
        ["--no-xhtml", "--tables", src.to_str().unwrap()]
            .iter()
            .map(std::ffi::OsString::from),
    )
    .expect("parse args");

    let mut app = TextrillApp::default();
    app.apply_command_line(&args);

    assert!(!app.opts.xhtml, "--no-xhtml did not reach the options");
    assert!(app.opts.make_tables, "--tables did not reach the options");
    assert_eq!(app.doc.path.as_deref(), Some(src.as_path()));
    assert_eq!(app.doc.text, "hello");
    let _ = std::fs::remove_file(&src);
}

// ------------------------------------------------------ theme and type size

/// A unique settings path that cannot touch a developer's real configuration.
fn temp_settings_path(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "textrill-{tag}-{}-{nanos}.conf",
        std::process::id()
    ))
}

/// The theme rows reach the app's theme and its style, and the choice is
/// remembered in the settings file the window reads on the next launch.
#[test]
fn the_theme_picker_switches_and_remembers_the_theme() {
    let path = temp_settings_path("theme");
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    assert_eq!(harness.state().theme, Theme::Light, "stem from the default");

    // The theme rows are in the View menu (the toolbar stays short).
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label(Theme::Dark.name()).click();
    harness.run();

    assert_eq!(
        harness.state().theme,
        Theme::Dark,
        "the picker did not switch"
    );
    let style = harness.ctx.style_of(harness.ctx.theme());
    assert!(style.visuals.dark_mode, "the dark theme is not dark");
    assert_eq!(
        Settings::with_path(&path).theme(),
        Some(Theme::Dark),
        "the choice was not persisted"
    );
    let _ = std::fs::remove_file(&path);
}

/// The high-contrast theme asks for the AAA target size (44 px on a clickable
/// control), not the AA 24 px of the ordinary themes.
#[test]
fn high_contrast_ups_the_clickable_height() {
    let mut app = TextrillApp::default();
    app.theme = Theme::HighContrast;
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    let style = harness.ctx.style_of(harness.ctx.theme());
    assert_eq!(
        style.spacing.interact_size.y, 44.0,
        "High contrast should use the AAA target size"
    );

    let ctx = harness.ctx.clone();
    harness.state_mut().theme = Theme::Light;
    harness.state_mut().apply_ui_style(&ctx);
    let style = harness.ctx.style_of(harness.ctx.theme());
    assert_eq!(
        style.spacing.interact_size.y, 24.0,
        "an ordinary theme should keep the AA target size"
    );
}

/// The text-size buttons step the plain-text scale up and down around 100%,
/// and the multiplier is remembered.
#[test]
fn the_text_size_buttons_scale_the_type_and_are_remembered() {
    let path = temp_settings_path("text-size");
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    // The text-size buttons are in the View menu.
    harness.get_by_label("View").click();
    harness.run();

    assert_eq!(harness.state().text_size, 1.0);
    harness.get_by_label("A+").click();
    harness.run();
    // A click on a menu row closes the menu, so reopen it for the second step.
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("A+").click();
    harness.run();

    assert_eq!(harness.state().text_size, 1.5, "two A+ steps from 100%");
    let style = harness.ctx.style_of(harness.ctx.theme());
    let body = style.text_styles[&egui::TextStyle::Body].size;
    assert!(
        (body - 13.0 * 1.5).abs() < 1e-3,
        "the body face did not scale to 150%: {body}"
    );
    assert_eq!(
        Settings::with_path(&path).text_size(),
        Some(1.5),
        "the text size was not persisted"
    );

    let _ = std::fs::remove_file(&path);
}

/// Reduce motion is a real, remembered choice: it zeroes egui's animation
/// time, so nothing slides or fades.
#[test]
fn reduce_motion_zeroes_the_animation_time_and_is_remembered() {
    let path = temp_settings_path("motion");
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    // Reduce motion is in the View menu.
    harness.get_by_label("View").click();
    harness.run();

    harness.get_by_label("Reduce motion").click();
    harness.run();

    assert!(harness.state().reduce_motion);
    let style = harness.ctx.style_of(harness.ctx.theme());
    assert_eq!(style.animation_time, 0.0, "reduced motion still animates");
    assert_eq!(
        Settings::with_path(&path).reduce_motion(),
        Some(true),
        "the motion choice was not persisted"
    );
    let _ = std::fs::remove_file(&path);
}

/// The View menu's built-in-browser checkbox is a real, remembered choice:
/// ticking it routes file commands to the in-app browser, and the choice is
/// written to settings.
#[test]
fn the_view_menu_toggles_the_builtin_file_browser() {
    let path = temp_settings_path("builtin-browser");
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    let initial = harness.state().uses_builtin_browser();

    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Use built-in file browser").click();
    harness.run();

    assert_eq!(
        harness.state().uses_builtin_browser(),
        !initial,
        "the checkbox did not flip the choice"
    );
    assert_eq!(
        Settings::with_path(&path).builtin_browser(),
        Some(!initial),
        "the choice was not persisted"
    );
    let _ = std::fs::remove_file(&path);
}

/// Inside a Flatpak the built-in browser is not offered: with no home access to
/// browse it could only show an empty tree, and the FileChooser portal is always
/// reachable instead. A remembered choice cannot turn it back on.
#[test]
fn a_flatpak_uses_the_portal_and_withholds_the_builtin_browser() {
    let path = temp_settings_path("flatpak-portal");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    app.set_builtin_browser(true);
    assert!(
        app.uses_builtin_browser(),
        "the built-in browser did not turn on outside a Flatpak"
    );

    app.set_in_flatpak(true);

    assert!(
        !app.builtin_browser_available(),
        "a Flatpak still offered the built-in browser"
    );
    assert!(
        !app.uses_builtin_browser(),
        "a Flatpak still routed file commands through the filesystem browser"
    );
    let _ = std::fs::remove_file(&path);
}

/// The View menu drops the built-in-browser toggle inside a Flatpak, so the
/// unavailable choice is never shown as a dead end. On a native install it is
/// there.
#[test]
fn the_view_menu_hides_the_builtin_browser_in_a_flatpak() {
    let app = TextrillApp::default();
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    assert!(
        harness
            .query_by_label("Use built-in file browser")
            .is_some(),
        "the toggle is missing on a native install"
    );

    let mut app = TextrillApp::default();
    app.set_in_flatpak(true);
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    assert!(
        harness
            .query_by_label("Use built-in file browser")
            .is_none(),
        "the toggle is still offered in a Flatpak"
    );
}

/// A chooser that returns nothing on a desktop with no dialog is explained, so
/// a portal-less install does not look like a broken button. A normal cancel on
/// a working desktop stays silent (see
/// `saving_with_nowhere_to_write_drops_the_command`).
#[test]
fn a_missing_file_dialog_is_explained() {
    let path = temp_settings_path("no-dialog");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    app.set_chooser(Box::new(CancellingChooser));
    app.set_native_dialog_available(false);

    app.open_document();

    assert!(
        app.status.contains("built-in file browser"),
        "the status did not point at the built-in browser: {}",
        app.status
    );
    let _ = std::fs::remove_file(&path);
}

/// Focus mode hides the menu bar and toolbar, but keeps the work reachable:
/// the two action buttons stay, so a manual conversion or a save never needs a
/// trip back to the chrome. The visible "Exit focus" (or `Ctrl+Shift+F`) is
/// the way back.
#[test]
fn focus_mode_hides_the_chrome_and_offers_a_way_back() {
    let app = TextrillApp::default();
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    harness.get_by_label("Focus").click();
    harness.run();

    assert!(harness.state().focus_mode, "Focus did not engage");
    assert!(
        harness.query_by_label("New").is_none(),
        "the toolbar is still on screen in focus mode"
    );
    assert!(
        harness.query_by_label("File").is_none(),
        "the menu bar is still on screen in focus mode"
    );
    // The action bar survives: the work does not leave with the chrome.
    assert!(
        harness.query_by_label("Convert").is_some(),
        "Convert left the focus view"
    );
    assert!(
        harness.query_by_label("Save HTML").is_some(),
        "Save HTML left the focus view"
    );
    harness.get_by_label("Exit focus").click();
    harness.run();

    assert!(!harness.state().focus_mode, "Exit focus did not disengage");
    assert!(
        harness.query_by_label("New").is_some(),
        "the toolbar did not come back"
    );
    assert!(
        harness.query_by_label("File").is_some(),
        "the menu bar did not come back"
    );
}

/// The focus action bar is not decoration: with auto off, Convert still asks
/// the worker for a conversion, and Save HTML still opens the save flow.
#[test]
fn focus_mode_action_bar_converts_and_saves() {
    let conf = temp_settings_path("focus-action");
    let mut app = TextrillApp::with_settings(Settings::with_path(&conf));
    app.set_builtin_browser(true);
    app.focus_mode = true;
    app.auto = false;

    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    let before = harness.state().latest;
    harness.get_by_label("Convert").click();
    harness.run();
    assert!(
        harness.state().latest > before,
        "Convert in focus mode did not request a conversion"
    );

    harness.get_by_label("Save HTML").click();
    harness.run();
    assert!(
        harness.state().is_browsing(),
        "Save HTML in focus mode did not open the save flow"
    );

    let _ = std::fs::remove_file(&conf);
}

/// The Title field feeds `--title`, and the preview leads with the header
/// landmark the engine now emits for a titled document.
#[test]
fn the_title_field_reaches_the_preview_as_a_heading() {
    let mut app = TextrillApp::default();
    app.doc.text = "Hello.\n".to_string();
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    // The Title field is the first text input egui draws (it precedes Input).
    let title_field = harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .expect("a title field");
    title_field.focus();
    title_field.type_text("The Book");
    harness.run();

    assert_eq!(
        harness.state().opts.title,
        "The Book",
        "the field missed the options"
    );

    let ctx = harness.ctx.clone();
    harness.state_mut().convert_now();
    wait_for_conversion(harness.state_mut(), Duration::from_secs(10));
    let _ = ctx;

    assert!(
        harness
            .state()
            .output
            .contains("<header class=\"document-header\">\n<h1>The Book</h1>\n</header>"),
        "unexpected preview: {}",
        harness.state().output
    );
}

fn wait_for_conversion(app: &mut TextrillApp, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while app.completed < app.latest && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        app.drain();
    }
}

// ------------------------------------------------- built-in file browser

/// A fresh scratch directory, unique per call.
fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("textrill-gui-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The View menu offers the built-in browser as an explicit choice, which is
/// the escape hatch when the desktop has no working file dialog.
#[test]
fn the_view_menu_offers_the_builtin_browser() {
    let app = TextrillApp::default();
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Use built-in file browser");
}

/// Turning the built-in browser on is remembered across launches.
#[test]
fn the_builtin_browser_choice_is_remembered() {
    let path = temp_settings_path("browser-remember");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));

    app.set_builtin_browser(true);

    assert!(app.uses_builtin_browser());
    assert_eq!(
        Settings::with_path(&path).builtin_browser(),
        Some(true),
        "the browser choice was not persisted"
    );
    let _ = std::fs::remove_file(&path);
}

/// With the built-in browser on, `Save As` opens it instead of the desktop
/// chooser.
#[test]
fn save_as_opens_the_builtin_browser_when_chosen() {
    let path = temp_settings_path("browser-dispatch");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    app.set_builtin_browser(true);

    app.save_document_as_command();

    assert!(app.is_browsing(), "Save As did not open the browser");
    let _ = std::fs::remove_file(&path);
}

/// A remembered directory is where an untitled conversion's Save As starts, so
/// the sample cannot drop its output in whatever folder the app was launched
/// from (the old behaviour wrote beside the input, `/tmp/opencode` and all).
#[test]
fn save_as_starts_in_the_remembered_directory() {
    let dir = scratch_dir("remembered-dir");
    let conf = temp_settings_path("remembered-dir");
    let mut settings = Settings::with_path(&conf);
    settings.set_last_dir(&dir).expect("remember the directory");

    let mut app = TextrillApp::with_settings(settings);
    app.set_builtin_browser(true);

    app.save_document_as_command();

    assert!(app.is_browsing(), "the browser did not open");
    assert_eq!(app.browser_directory(), Some(dir.as_path()));

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&conf);
}

/// `Save As` returns to where the HTML last went, not the source text's folder:
/// after writing the output somewhere else, a second `Save As` proposes that
/// place again.
#[test]
fn save_as_starts_beside_the_last_output() {
    let src_dir = scratch_dir("save-as-source");
    let out_dir = scratch_dir("save-as-output");
    let src = src_dir.join("doc.txt");
    std::fs::write(&src, "hello").expect("write source");

    let conf = temp_settings_path("save-as-output");
    let mut app = TextrillApp::with_settings(Settings::with_path(&conf));
    app.set_builtin_browser(true);
    app.doc.path = Some(src);
    app.doc.output_path = Some(out_dir.join("doc.html"));

    app.save_document_as_command();

    assert_eq!(app.browser_directory(), Some(out_dir.as_path()));

    let _ = std::fs::remove_dir_all(&src_dir);
    let _ = std::fs::remove_dir_all(&out_dir);
    let _ = std::fs::remove_file(&conf);
}

/// The browser completes a save end to end: it opens on the proposed file, and
/// confirming writes the HTML there. Focus mode is on so the toolbar's own
/// "Save" button cannot shadow the modal's in the accessibility tree.
#[test]
fn the_builtin_browser_saves_the_html() {
    let dir = scratch_dir("browser-save");
    let src = dir.join("doc.txt");
    std::fs::write(&src, "hello").expect("write source");

    let conf = temp_settings_path("browser-save");
    let mut app = TextrillApp::with_settings(Settings::with_path(&conf));
    app.set_builtin_browser(true);
    app.focus_mode = true;
    app.load_file(&src).expect("load");
    // Let the load's conversion finish before pinning the output; otherwise a
    // result landing during a later `run()` overwrites it and the file gets the
    // engine's HTML instead of the fixed string this test saves.
    wait_for_conversion(&mut app, Duration::from_secs(10));
    app.output = "<p>hello</p>".to_string();

    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();
    harness.state_mut().save_document_as_command();
    harness.run();

    assert!(harness.state().is_browsing(), "the browser did not open");
    harness.get_by_label("Save").click();
    harness.run();

    assert!(!harness.state().is_browsing(), "the browser stayed open");
    let out = dir.join("doc.html");
    assert!(out.exists(), "the HTML was not written");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "<p>hello</p>");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&conf);
}

/// Cancelling the browser writes nothing and closes it.
#[test]
fn cancelling_the_builtin_browser_writes_nothing() {
    let dir = scratch_dir("browser-cancel");
    let src = dir.join("doc.txt");
    std::fs::write(&src, "hello").expect("write source");

    let conf = temp_settings_path("browser-cancel");
    let mut app = TextrillApp::with_settings(Settings::with_path(&conf));
    app.set_builtin_browser(true);
    app.focus_mode = true;
    app.load_file(&src).expect("load");

    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();
    harness.state_mut().save_document_as_command();
    harness.run();
    assert!(harness.state().is_browsing());

    harness.get_by_label("Cancel").click();
    harness.run();

    assert!(!harness.state().is_browsing(), "Cancel did not close it");
    assert!(
        !dir.join("doc.html").exists(),
        "a cancelled save wrote a file"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&conf);
}

// ------------------------------------------------------------------ presets

/// A preset is a starting point, not a mode: applying one writes its option
/// bundle and remembers the choice in settings.
#[test]
fn applying_a_preset_sets_the_options_and_is_remembered() {
    let path = temp_settings_path("preset-apply");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    assert_eq!(app.preset_key(), "plain");
    assert!(!app.preset_is_modified(), "Plain should match the defaults");

    app.apply_preset(textrill_gui::presets::get("book").expect("book preset"));

    assert_eq!(app.preset_key(), "book");
    assert!(!app.preset_is_modified(), "a fresh preset is not modified");
    assert_eq!(app.conversion_options().template_library, "book");
    assert!(
        app.conversion_options().toc,
        "Book carries a table of contents"
    );
    assert_eq!(
        Settings::with_path(&path).preset().as_deref(),
        Some("book"),
        "the choice was not remembered"
    );
    let _ = std::fs::remove_file(&path);
}

/// Editing an option away from the chosen preset raises the "Modified" marker;
/// applying the preset again clears it.
#[test]
fn editing_away_from_a_preset_marks_it_modified() {
    let path = temp_settings_path("preset-modified");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    app.apply_preset(textrill_gui::presets::get("article").expect("article preset"));
    assert!(!app.preset_is_modified());

    app.opts.toc = false;
    assert!(
        app.preset_is_modified(),
        "editing `toc` should mark Modified"
    );

    app.apply_preset(textrill_gui::presets::get("article").expect("article preset"));
    assert!(
        !app.preset_is_modified(),
        "re-applying the preset should clear Modified"
    );
    let _ = std::fs::remove_file(&path);
}

/// An unreadable stored key falls back to Plain rather than presetting to
/// nothing.
#[test]
fn an_unknown_stored_preset_falls_back_to_plain() {
    let path = temp_settings_path("preset-unknown");
    {
        let mut settings = Settings::with_path(&path);
        settings.set_preset("not-a-preset").expect("write");
    }
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    assert_eq!(app.preset_key(), "plain");
    assert!(!app.preset_is_modified());
    let _ = std::fs::remove_file(&path);
}

/// The preset strip is on screen with the rest of the chrome, and clicking a
/// preset flows through the app and raises the Modified marker when the options
/// are then edited.
#[test]
fn the_preset_bar_applies_presets_and_shows_modified() {
    let path = temp_settings_path("preset-ui");
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    assert!(harness.query_by_label("Plain").is_some(), "no preset strip");

    harness.get_by_label("Book").click();
    harness.run();

    assert_eq!(harness.state().preset_key(), "book");
    assert!(!harness.state().preset_is_modified());
    assert!(
        harness.query_by_label("Modified").is_none(),
        "a freshly applied preset should not read Modified"
    );

    harness.state_mut().opts.toc = false;
    harness.run();
    assert!(
        harness.query_by_label("Modified").is_some(),
        "editing away from the preset should show Modified"
    );

    let _ = std::fs::remove_file(&path);
}

// ------------------------------------------------------------------ summary

/// The landing view is on screen and reads the live settings; applying a preset
/// updates it.
#[test]
fn the_summary_landing_view_reads_the_settings() {
    let path = temp_settings_path("summary");
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    assert!(
        harness.query_by_label("Summary").is_some(),
        "no Summary heading"
    );
    assert!(
        harness.query_by_label_contains("no template").is_some(),
        "the default summary should say there is no template"
    );

    harness.get_by_label("Book").click();
    harness.run();

    assert!(
        harness
            .query_by_label_contains("shipped \"book\" template")
            .is_some(),
        "the summary did not follow the preset"
    );
    assert!(
        harness
            .query_by_label_contains("table of contents")
            .is_some(),
        "the summary should mention the table of contents"
    );

    let _ = std::fs::remove_file(&path);
}

/// The source-and-destination strip names the file being converted and where
/// the HTML goes, and drops the `+` once it is written.
#[test]
fn the_file_strip_shows_source_and_destination() {
    let dir = scratch_dir("strip");
    let src = dir.join("doc.txt");
    std::fs::write(&src, "hello").expect("write source");
    let out = dir.join("doc.html");

    let conf = temp_settings_path("strip");
    let app = TextrillApp::with_settings(Settings::with_path(&conf));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();

    assert!(
        harness
            .query_by_label_contains("Output: not saved")
            .is_some(),
        "a fresh document should read Output: not saved"
    );

    harness.state_mut().load_file(&src).expect("load");
    harness.run();
    assert!(
        harness.query_by_label_contains("Input: doc.txt").is_some(),
        "the strip did not show the loaded source"
    );

    harness.state_mut().save_html_to(&out).expect("save");
    harness.run();
    assert!(
        harness
            .query_by_label_contains("Output: doc.html")
            .is_some(),
        "the strip did not show the saved output"
    );
    assert!(
        harness
            .query_by_label_contains("Output: doc.html+")
            .is_none(),
        "a just-saved HTML should not carry the stale mark"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&conf);
}

/// The three view modes switch with Ctrl+1/2/3, not only from the View menu.
/// It uses a temporary settings file: switching a mode persists the choice, and
/// a test must never write to the developer's real configuration.
#[test]
fn the_view_modes_switch_with_the_keyboard() {
    use textrill_gui::settings::ViewMode;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "textrill-viewmode-{}-{nanos}.conf",
        std::process::id()
    ));

    let app = TextrillApp::with_settings(Settings::with_path(&path));
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.run();
    assert_eq!(harness.state().view_mode(), ViewMode::Split);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num2);
    harness.run();
    assert_eq!(harness.state().view_mode(), ViewMode::Source);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num3);
    harness.run();
    assert_eq!(harness.state().view_mode(), ViewMode::Html);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    harness.run();
    assert_eq!(harness.state().view_mode(), ViewMode::Split);

    let _ = std::fs::remove_file(&path);
}

/// With an empty document the quick-start actions are offered, and "Try the
/// example" loads the sample. They vanish once there is text.
#[test]
fn the_empty_state_offers_working_shortcuts() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.set_size(egui::vec2(1000.0, 800.0));
    harness.run();

    // The pointer-based `click()` mis-reads the rect under the Split pane's
    // nested layout; the AccessKit action exercises the same wired handler.
    harness.get_by_label("Try the example").click_accesskit();
    harness.run();
    assert!(
        !harness.state().doc.text.is_empty(),
        "Try the example did not load the sample"
    );

    // Once there is text the quick-start row is gone.
    assert!(
        harness.query_by_label_contains("Try the example").is_none(),
        "the quick-start row should disappear once the document has text"
    );
}

/// The Ctrl/Cmd+K palette starts closed, lists its commands when opened, and
/// closes again on Escape. The command rows are probed by the label a user
/// would read, which is also how the palette is distinguished from the File
/// menu (its other home) — that menu is closed here.
#[test]
fn the_command_palette_opens_and_closes_with_the_shortcut() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.run();
    assert_eq!(
        harness.query_all_by_label("Load example").count(),
        0,
        "the palette should start closed"
    );

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();
    assert_eq!(
        harness.query_all_by_label("Load example").count(),
        1,
        "Ctrl+K should open the palette and list its commands"
    );

    harness.key_press(egui::Key::Escape);
    harness.run();
    assert_eq!(
        harness.query_all_by_label("Load example").count(),
        0,
        "Escape should close the palette"
    );
}

/// Typing narrows the list to what matches, and Enter runs the highlighted
/// row. This is the palette's whole reason to exist: name a command in a few
/// letters and fire it without hunting through a menu.
#[test]
fn the_command_palette_filters_and_runs_what_you_pick() {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut TextrillApp| app.draw(ui),
        TextrillApp::default(),
    );
    harness.run();
    assert!(harness.state().doc.text.is_empty());

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();

    // The search box takes focus the moment the palette opens, so the very
    // first keystroke lands in it rather than in the source editor.
    let search = harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|node| node.is_focused())
        .expect("the palette search box should hold focus when it opens");
    search.type_text("example");
    harness.run();

    assert_eq!(
        harness.query_all_by_label("Load example").count(),
        1,
        "the matching command should stay listed"
    );
    assert_eq!(
        harness.query_all_by_label("Copy HTML").count(),
        0,
        "a non-matching command should be filtered out"
    );

    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        !harness.state().doc.text.is_empty(),
        "Enter should run the highlighted, filtered command"
    );
}
