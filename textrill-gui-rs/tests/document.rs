// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Tests for the document model, settings, and the edit/debounce wiring.
//!
//! Ported from `test_gui.py`'s document, file and settings cases. They are
//! pure state tests: no window, no event loop, and no way to reach the user's
//! real `~/.config/textrill-gui/textrill.conf` -- every settings store is
//! written under a temporary directory of this test's own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use textrill_gui::app::AUTO_CONVERT_DELAY;
use textrill_gui::document::{Document, SAMPLE_TEXT};
use textrill_gui::settings::{parse_bool, Settings};
use textrill_gui::TextrillApp;

/// A directory that deletes itself when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "textrill-gui-test-{}-{nanos}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn app_at(dir: &TempDir) -> TextrillApp {
    TextrillApp::with_settings(Settings::with_path(dir.path().join("textrill.conf")))
}

// --------------------------------------------------------------- document

#[test]
fn a_loaded_file_is_not_dirty_and_forgets_any_saved_html() {
    let dir = TempDir::new();
    let src = dir.path().join("note.txt");
    std::fs::write(&src, b"caf\xe9 au lait\n").expect("write source");

    let mut doc = Document::new();
    doc.output_path = Some(dir.path().join("old.html"));
    doc.saved_html = Some("<p>old</p>".to_string());
    doc.load(&src).expect("load");

    assert_eq!(doc.text, "café au lait\n");
    assert_eq!(doc.path.as_deref(), Some(src.as_path()));
    assert!(!doc.dirty, "a freshly opened file is saved");
    assert_eq!(
        doc.output_path, None,
        "the old HTML belonged to another file"
    );
    assert_eq!(doc.saved_html, None);
    assert!(!doc.output_stale);
}

#[test]
fn saving_the_text_preserves_the_source_bytes_and_encoding() {
    let dir = TempDir::new();
    let src = dir.path().join("cafe.txt");
    std::fs::write(&src, b"caf\xe9 au lait\n").expect("write source");

    let mut doc = Document::new();
    doc.load(&src).expect("load");
    let copy = dir.path().join("cafe-copy.txt");
    doc.save_text_to(&copy).expect("save");

    assert_eq!(
        std::fs::read(&copy).expect("read copy"),
        std::fs::read(&src).expect("read source"),
        "the bytes must round-trip so saving cannot transcode the file"
    );
    assert_eq!(doc.path.as_deref(), Some(copy.as_path()));
    assert!(!doc.dirty);
}

#[test]
fn saving_html_never_overwrites_the_source() {
    let dir = TempDir::new();
    let src = dir.path().join("note.txt");
    std::fs::write(&src, b"Hello\n").expect("write source");

    let mut doc = Document::new();
    doc.load(&src).expect("load");
    let error = doc
        .save_html_to(&src, "<p>Hello</p>")
        .expect_err("saving over the source must fail");

    assert!(
        error.contains("is the file you are converting"),
        "unexpected error: {error}"
    );
    assert_eq!(
        std::fs::read(&src).expect("read source"),
        b"Hello\n",
        "the source must be untouched"
    );
}

#[test]
fn saving_html_refuses_the_source_even_through_a_different_spelling() {
    let dir = TempDir::new();
    let src = dir.path().join("note.txt");
    std::fs::write(&src, b"Hello\n").expect("write source");

    let mut doc = Document::new();
    doc.load(&src).expect("load");
    // The dialog may hand back a path that only differs textually.
    let spelled = src.parent().unwrap().join(".").join("note.txt");
    assert!(doc.save_html_to(&spelled, "<p>Hello</p>").is_err());
}

#[test]
fn saving_html_records_the_output_and_clears_staleness() {
    let dir = TempDir::new();
    let src = dir.path().join("note.txt");
    std::fs::write(&src, b"Hello\n").expect("write source");

    let mut doc = Document::new();
    doc.load(&src).expect("load");
    doc.note_converted("<p>Hello</p>");
    assert!(doc.output_stale, "unsaved HTML is stale");

    let out = dir.path().join("note.html");
    doc.save_html_to(&out, "<p>Hello</p>").expect("save html");
    assert!(!doc.output_stale);
    assert_eq!(doc.output_path.as_deref(), Some(out.as_path()));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "<p>Hello</p>");

    // Re-converting the same text is not new unsaved output...
    doc.note_converted("<p>Hello</p>");
    assert!(!doc.output_stale);
    // ...but a real change is.
    doc.note_converted("<p>Goodbye</p>");
    assert!(doc.output_stale);
}

#[test]
fn the_title_marks_unsaved_text_and_unsaved_html_separately() {
    let mut doc = Document::new();
    assert_eq!(doc.title(), "untitled \u{2014} textrill");

    doc.dirty = true;
    assert_eq!(doc.title(), "untitled* \u{2014} textrill");

    doc.output_stale = true;
    assert_eq!(doc.title(), "untitled*+ \u{2014} textrill");

    doc.path = Some(PathBuf::from("/tmp/example.txt"));
    doc.dirty = false;
    assert_eq!(doc.title(), "example.txt+ \u{2014} textrill");
}

#[test]
fn save_as_proposes_an_html_name_not_the_source_name() {
    let mut doc = Document::new();
    doc.path = Some(PathBuf::from("/tmp/report.txt"));
    assert_eq!(
        doc.suggested_output_name(),
        PathBuf::from("/tmp/report.html")
    );

    doc.output_path = Some(PathBuf::from("/tmp/other.html"));
    assert_eq!(
        doc.suggested_output_name(),
        PathBuf::from("/tmp/other.html")
    );
}

#[test]
fn reset_clears_everything() {
    let dir = TempDir::new();
    let src = dir.path().join("note.txt");
    std::fs::write(&src, b"Hello\n").expect("write source");

    let mut doc = Document::new();
    doc.load(&src).expect("load");
    doc.dirty = true;
    doc.output_stale = true;
    doc.reset();

    assert_eq!(doc.text, "");
    assert_eq!(doc.path, None);
    assert_eq!(doc.output_path, None);
    assert_eq!(doc.saved_html, None);
    assert!(!doc.dirty);
    assert!(!doc.output_stale);
}

#[test]
fn the_sample_is_utf8_and_starts_unsaved() {
    let mut doc = Document::new();
    doc.load_sample();
    assert_eq!(doc.text, SAMPLE_TEXT);
    assert!(doc.dirty, "the sample has not been saved anywhere");
    assert_eq!(doc.path, None);
    assert!(!doc.output_stale);
}

// --------------------------------------------------------------- settings

#[test]
fn the_auto_flag_round_trips_through_the_file() {
    let dir = TempDir::new();
    let path = dir.path().join("nested/textrill.conf");

    let mut settings = Settings::with_path(&path);
    assert_eq!(settings.auto(), None, "no file means no choice yet");
    settings.set_auto(false).expect("store");

    let reopened = Settings::with_path(&path);
    assert_eq!(reopened.auto(), Some(false));
}

#[test]
fn a_hand_edited_config_is_still_understood() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    std::fs::write(&path, "[General]\nauto=true\n").expect("write config");

    let settings = Settings::with_path(&path);
    assert_eq!(settings.auto(), Some(true));
}

#[test]
fn bools_accept_the_spellings_a_text_editor_might_produce() {
    for value in ["true", "TRUE", "True", "1", "yes", "on", " true "] {
        assert!(parse_bool(value), "{value:?} should be true");
    }
    for value in ["false", "0", "no", "off", "", "maybe"] {
        assert!(!parse_bool(value), "{value:?} should be false");
    }
}

#[test]
fn store_settings_writes_the_choice_out() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    assert!(app.auto, "auto is on by default");

    app.auto = false;
    app.store_settings();

    assert_eq!(Settings::with_path(&path).auto(), Some(false));
}

/// Every value `get_value` can read back, for comparing a full option set.
fn snapshot(opts: &textrill::options::Options) -> Vec<(String, String)> {
    textrill::cli::SPECS
        .iter()
        .map(|spec| {
            (
                spec.names[0].to_string(),
                textrill::cli::get_value(opts, spec.names[0]).unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn the_whole_option_set_round_trips_through_the_file() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    for (name, value) in [
        ("demoronize", "false"),
        ("hrule_min", "7"),
        ("title", "a, b"),
        ("infile", "one.txt"),
        ("infile", "two.txt"),
        ("table_type", "ALIGN=0"),
        ("meta_charset", "true"),
    ] {
        textrill::cli::set_value(&mut app.opts, name, value).expect("set");
    }
    app.store_settings();

    let reopened = TextrillApp::with_settings(Settings::with_path(&path));
    assert_eq!(snapshot(&app.opts), snapshot(&reopened.opts));
}

#[test]
fn a_hand_written_python_option_blob_is_understood() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    // Exactly the shape `QSettings` wrote for `json.dumps(values)`: the blob is
    // a quoted INI value, quoting is backslash-escaped, and `json.dumps` emits
    // non-ASCII as `\uXXXX`.
    std::fs::write(
        &path,
        r#"[General]
auto=true
options="{\"hrule_min\": 5, \"title\": \"caf\u00e9\", \"demoronize\": false, \"infile\": [\"a.txt\", \"b.txt\"], \"table_type\": {\"ALIGN\": false}}"
"#,
    )
    .expect("write config");

    let settings = Settings::with_path(&path);
    assert_eq!(settings.auto(), Some(true));
    let app = TextrillApp::with_settings(Settings::with_path(&path));
    assert_eq!(
        textrill::cli::get_value(&app.opts, "hrule_min").unwrap(),
        "5"
    );
    assert_eq!(
        textrill::cli::get_value(&app.opts, "title").unwrap(),
        "café"
    );
    assert!(!app.opts.demoronize);
    assert_eq!(
        app.opts.infile,
        vec!["a.txt".to_string(), "b.txt".to_string()]
    );
    assert!(!app.opts.table_type.align);
}

// ----------------------------------------------------------- window geometry

#[test]
fn window_geometry_round_trips_through_the_file() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");

    let mut settings = Settings::with_path(&path);
    assert_eq!(settings.window_size(), None, "no file means no size yet");
    settings
        .set_window_state([800.0, 600.0], true, 1.5)
        .expect("store");

    let reopened = Settings::with_path(&path);
    assert_eq!(reopened.window_size(), Some([800.0, 600.0]));
    assert_eq!(reopened.maximized(), Some(true));
    assert_eq!(reopened.zoom(), Some(1.5));
}

#[test]
fn a_corrupt_geometry_is_ignored_not_trusted() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    std::fs::write(
        &path,
        "[General]\nwindow-size=wide\nmaximized=maybe\nzoom=lots\n",
    )
    .expect("write config");

    let settings = Settings::with_path(&path);
    assert_eq!(
        settings.window_size(),
        None,
        "an unreadable size falls back"
    );
    assert_eq!(settings.zoom(), None, "an unreadable zoom falls back");
    // `maximized` is a bool, so an unrecognised spelling simply reads as false.
    assert_eq!(settings.maximized(), Some(false));
    assert_eq!(
        settings
            .window_size()
            .unwrap_or(textrill_gui::window_state::DEFAULT_SIZE),
        textrill_gui::window_state::DEFAULT_SIZE,
        "the caller's fallback is the default size"
    );
}

#[test]
fn a_stored_size_is_clamped_so_a_bad_file_cannot_strand_the_window() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    std::fs::write(&path, "[General]\nwindow-size=10x10\n").expect("write config");
    assert_eq!(
        Settings::with_path(&path).window_size(),
        Some(textrill_gui::window_state::MIN_SIZE)
    );
}

#[test]
fn the_window_state_is_written_atomically() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    let mut settings = Settings::with_path(&path);
    settings
        .set_window_state([800.0, 600.0], false, 2.0)
        .expect("store");

    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.contains("tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "a temporary file was left behind: {leftovers:?}"
    );
}

#[test]
fn the_app_remembers_and_restores_its_geometry() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    app.window_size = [1024.0, 768.0];
    app.window_maximized = true;
    app.zoom = 1.25;
    app.store_window_state();

    let reopened = TextrillApp::with_settings(Settings::with_path(&path));
    assert_eq!(
        reopened.window_size,
        [1024.0, 768.0],
        "the last normal size is kept even while maximized"
    );
    assert!(reopened.window_maximized);
    assert_eq!(reopened.zoom, 1.25);
}

#[test]
fn resetting_the_window_geometry_forgets_it() {
    let dir = TempDir::new();
    let path = dir.path().join("textrill.conf");
    let mut app = TextrillApp::with_settings(Settings::with_path(&path));
    app.window_size = [1024.0, 768.0];
    app.window_maximized = true;
    app.zoom = 1.25;
    app.store_window_state();

    app.reset_window_state(&egui::Context::default());

    assert_eq!(
        Settings::with_path(&path).window_size(),
        None,
        "the stored geometry was not cleared"
    );
    assert_eq!(app.window_size, textrill_gui::window_state::DEFAULT_SIZE);
    assert!(!app.window_maximized);
    assert_eq!(app.zoom, 1.0);
}

// ------------------------------------------------------------------- app

#[test]
fn a_first_change_is_debounced_even_with_auto_off() {
    let dir = TempDir::new();
    let mut app = app_at(&dir);
    app.auto = false;
    let t0 = Instant::now();

    // `not latest` keeps the timer running, so an empty preview is still
    // filled -- just after the delay, matching the Python `schedule_convert`.
    app.schedule_convert(t0);
    assert_eq!(app.latest, 0, "the timer should run first");

    app.tick(t0 + AUTO_CONVERT_DELAY);
    assert_eq!(app.latest, 1);
}

#[test]
fn with_auto_off_a_later_option_change_converts_at_once() {
    let dir = TempDir::new();
    let mut app = app_at(&dir);
    app.auto = false;
    app.convert_now();
    let after_first = app.latest;

    app.schedule_convert(Instant::now());

    assert_eq!(
        app.latest,
        after_first + 1,
        "with a preview already up, an option change must not wait"
    );
}

#[test]
fn edits_are_debounced_until_the_deadline() {
    let dir = TempDir::new();
    let mut app = app_at(&dir);
    let t0 = Instant::now();

    app.on_edit(t0);
    assert_eq!(app.latest, 0, "the edit should wait out the delay");

    app.tick(t0 + AUTO_CONVERT_DELAY - Duration::from_millis(1));
    assert_eq!(app.latest, 0, "one millisecond early is still too early");

    app.tick(t0 + AUTO_CONVERT_DELAY);
    assert_eq!(app.latest, 1, "the deadline fires exactly once");
    app.tick(t0 + AUTO_CONVERT_DELAY + Duration::from_secs(1));
    assert_eq!(app.latest, 1, "a fired deadline does not fire again");
}

#[test]
fn with_auto_off_a_text_edit_does_not_convert() {
    let dir = TempDir::new();
    let mut app = app_at(&dir);
    app.auto = false;
    app.convert_now(); // establish a first conversion
    let after_first = app.latest;

    app.on_edit(Instant::now());
    app.tick(Instant::now() + Duration::from_secs(10));

    assert_eq!(
        app.latest, after_first,
        "auto off must not convert on edits"
    );
}

#[test]
fn a_conversion_declares_its_encoding_even_though_the_engine_defaults_off() {
    let dir = TempDir::new();
    let app = app_at(&dir);

    assert!(!app.opts.meta_charset, "the engine default is unchanged");
    assert!(
        app.conversion_options().meta_charset,
        "the GUI must declare UTF-8 in the HTML it writes"
    );
}

#[test]
fn save_reports_the_name_to_propose_when_there_is_no_path_yet() {
    let dir = TempDir::new();
    let mut app = app_at(&dir);
    app.doc.text = "Hello".to_string();

    match app.save_text().expect("save text") {
        textrill_gui::app::SaveRequest::NeedsName(name) => {
            assert_eq!(
                name.file_name().and_then(|n| n.to_str()),
                Some("untitled.txt")
            );
        }
        other => panic!("expected a name request, got {other:?}"),
    }

    match app.save_document().expect("save document") {
        textrill_gui::app::SaveRequest::NeedsName(name) => {
            assert_eq!(
                name.file_name().and_then(|n| n.to_str()),
                Some("converted.html")
            );
        }
        other => panic!("expected a name request, got {other:?}"),
    }
}

#[test]
fn saving_text_to_an_explicit_path_updates_the_title() {
    let dir = TempDir::new();
    let mut app = app_at(&dir);
    app.doc.text = "Hello".to_string();
    let out = dir.path().join("hello.txt");

    app.save_text_to(&out).expect("save text");

    assert_eq!(app.doc.path.as_deref(), Some(out.as_path()));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "Hello");
    assert_eq!(app.doc.title(), "hello.txt \u{2014} textrill");
}

#[test]
fn loading_a_file_fills_the_document_and_converts() {
    let dir = TempDir::new();
    let src = dir.path().join("greeting.txt");
    std::fs::write(&src, b"Hello\n").expect("write source");
    let mut app = app_at(&dir);

    app.load_file(&src).expect("load");
    wait_for_conversion(&mut app, Duration::from_secs(10));

    assert_eq!(app.doc.text, "Hello\n");
    assert!(
        app.output.contains("<p>Hello</p>"),
        "output: {}",
        app.output
    );
}

fn wait_for_conversion(app: &mut TextrillApp, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while app.completed < app.latest && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        app.drain();
    }
}
