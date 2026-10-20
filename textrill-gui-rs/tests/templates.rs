// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Acceptance tests for the Template Manager (roadmap §5.5).
//!
//! Headless, like the rest: `egui_kittest` builds an AccessKit tree from the
//! same `draw` code the window uses. Every app runs on a private settings file
//! under a temporary directory, so the developer's real
//! `~/.config/textrill-gui/textrill.conf` is never touched.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use textrill_gui::{Settings, TextrillApp};
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
            "textrill-gui-tmpl-{}-{nanos}-{n}",
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

/// Write a valid pack `id` with one template `stem` under `root`.
fn write_pack(root: &Path, id: &str, stem: &str, tags: &[&str]) {
    let tags = if tags.is_empty() {
        String::new()
    } else {
        format!(
            "tags = [{}]\n",
            tags.iter()
                .map(|t| format!("{t:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).expect("pack dir");
    std::fs::write(
        dir.join("template.toml"),
        format!(
            "schema = 1\nid = {id:?}\nname = {id:?}\nversion = \"1.0.0\"\n\
             license = \"GPL-3.0-or-later\"\n{tags}\n\
             [[templates]]\nfile = \"{stem}.html\"\nmodel = \"document\"\n"
        ),
    )
    .expect("manifest");
    std::fs::write(dir.join(format!("{stem}.html")), "{{textrill:content}}").expect("template");
}

/// An app whose settings are private, with `templates` as the explicit template
/// root so the window discovers exactly the packs the test wrote.
fn app_with_templates(dir: &TempDir, templates: &Path) -> TextrillApp {
    let mut app = TextrillApp::with_settings(Settings::with_path(dir.path().join("textrill.conf")));
    app.opts.template_dir = vec![templates.to_string_lossy().into_owned()];
    app
}

fn app(dir: &TempDir) -> TextrillApp {
    TextrillApp::with_settings(Settings::with_path(dir.path().join("textrill.conf")))
}

/// The Template Manager opens from the menu and lists what discovery found,
/// including a pack in the explicit template root.
#[test]
fn the_manager_opens_from_the_menu_and_lists_discovered_packs() {
    let dir = TempDir::new();
    let templates = dir.path().join("templates");
    write_pack(&templates, "org.example.book", "book", &["prose"]);

    let app = app_with_templates(&dir, &templates);
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.set_size(egui::vec2(1200.0, 900.0));
    harness.run();

    // The window starts closed.
    assert_eq!(harness.query_all_by_label("Template manager").count(), 0);

    harness.get_by_label("Templates").click();
    harness.run();
    harness.get_by_label("Manage templates…").click();
    harness.run();

    harness.get_by_label("Template manager");
    // The discovered pack and the shipped set are both listed.
    assert!(
        harness.query_all_by_label("book  ·  local").count() >= 1,
        "the discovered pack should be listed with its source badge"
    );
    assert!(
        harness.query_all_by_label("article  ·  shipped").count() >= 1,
        "the shipped set should be listed too"
    );
}

/// Applying a template writes `--template_library` with the full address and
/// clears the other template fields the engine refuses to combine with it.
#[test]
fn applying_a_template_sets_the_library_and_clears_the_others() {
    let dir = TempDir::new();
    let templates = dir.path().join("templates");
    write_pack(&templates, "org.example.book", "book", &[]);

    let mut app = app_with_templates(&dir, &templates);
    // Pre-set the mutually exclusive fields so applying must clear them.
    app.opts.template = "stale-body.html".to_string();
    app.opts.document_template = "stale-doc.html".to_string();

    app.open_templates();
    app.apply_template("org.example.book:book");

    assert_eq!(app.opts.template_library, "org.example.book:book");
    assert!(
        app.opts.template.is_empty(),
        "the body template must be cleared"
    );
    assert!(
        app.opts.document_template.is_empty(),
        "the document template must be cleared"
    );
}

/// A pack dropped into the root after the window opened is seen on the next
/// refresh, so a user can install a pack in another shell and pick it up
/// without restarting.
#[test]
fn a_refresh_sees_a_pack_added_since_the_window_opened() {
    let dir = TempDir::new();
    let templates = dir.path().join("templates");
    std::fs::create_dir_all(&templates).unwrap();

    let mut app = app_with_templates(&dir, &templates);
    app.open_templates();
    let before = app
        .templates
        .as_ref()
        .unwrap()
        .catalog()
        .entries
        .iter()
        .filter(|e| e.name == "late")
        .count();
    assert_eq!(before, 0, "the pack is not there yet");

    write_pack(&templates, "org.example.late", "late", &[]);
    app.open_templates();
    let after = app
        .templates
        .as_ref()
        .unwrap()
        .catalog()
        .entries
        .iter()
        .filter(|e| e.name == "late")
        .count();
    assert_eq!(after, 1, "the refresh should discover the new pack");
}

/// The window closes on Escape, leaving the app with no manager open.
#[test]
fn the_manager_closes_on_escape() {
    let dir = TempDir::new();
    let templates = dir.path().join("templates");
    write_pack(&templates, "org.example.book", "book", &[]);

    let app = app_with_templates(&dir, &templates);
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.set_size(egui::vec2(1200.0, 900.0));
    harness.run();

    harness.get_by_label("Templates").click();
    harness.run();
    harness.get_by_label("Manage templates…").click();
    harness.run();
    harness.get_by_label("Template manager");

    harness.key_press(egui::Key::Escape);
    harness.run();
    assert_eq!(
        harness.query_all_by_label("Template manager").count(),
        0,
        "Escape should close the Template manager"
    );
}

/// The empty-state "Browse templates" button opens the same window, so a first
/// run reaches it without knowing the menu.
#[test]
fn the_empty_state_button_opens_the_manager() {
    let dir = TempDir::new();
    let app = app(&dir);
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.set_size(egui::vec2(1200.0, 900.0));
    harness.run();

    harness.get_by_label("Browse templates").click_accesskit();
    harness.run();
    harness.get_by_label("Template manager");
}

/// The options panel exposes the new "Discovered" template source alongside the
/// existing choices.
#[test]
fn the_panel_offers_the_discovered_template_source() {
    let dir = TempDir::new();
    let app = app(&dir);
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.set_size(egui::vec2(1200.0, 900.0));
    harness.run();

    // Open the options window via the View menu.
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Show options").click();
    harness.run();

    harness.get_by_label("Discovered");
    harness.get_by_label("Shipped");
}

/// Installing a pack directory (as the drop handler does) validates it, lands
/// it in the data root and makes it discoverable on the next refresh.
#[test]
fn installing_a_pack_directory_adds_it_to_the_catalog() {
    let dir = TempDir::new();
    let incoming = dir.path().join("incoming");
    let data_home = dir.path().join("share");
    write_pack(&incoming, "org.example.dropped", "dropped", &[]);

    let mut app = app(&dir);
    app.open_templates();
    let window = app.templates.as_mut().unwrap();
    window.set_data_home(&data_home);
    assert!(
        window.install_from(&incoming.join("org.example.dropped")),
        "a valid pack directory should install"
    );

    // The pack landed under the data root and reloads cleanly.
    let landed = data_home.join("org.example.dropped");
    assert!(landed.join("template.toml").is_file());
    assert!(textrill::pack::load(&landed).is_ok());
}

/// Build an offline catalog under `dir` holding one pack, exactly as
/// `templates pack` plus a hand-written index would: an archive, its sha256
/// and a `catalog.toml`. Returns the catalog directory.
fn make_catalog(dir: &TempDir, id: &str, stem: &str) -> PathBuf {
    let src = dir.path().join("catalog-src");
    write_pack(&src, id, stem, &["catalog"]);
    let bytes = textrill::pack::pack(&src.join(id)).expect("pack the catalog source");
    let cat = dir.path().join("catalog");
    std::fs::create_dir_all(cat.join("packs")).expect("catalog packs dir");
    let archive = format!("packs/{id}-1.0.0.tar");
    std::fs::write(cat.join(&archive), &bytes).expect("catalog archive");
    let sha = textrill::pack::sha256::sha256_hex(&bytes);
    std::fs::write(
        cat.join("catalog.toml"),
        format!(
            "schema = 1\n\n[[packs]]\nid = {id:?}\nname = \"Catalog {id}\"\n\
             version = \"1.0.0\"\ndescription = \"a shared pack\"\n\
             tags = [\"catalog\"]\nsha256 = {sha:?}\narchive = {archive:?}\n"
        ),
    )
    .expect("catalog index");
    cat
}

/// The Catalogs pane lists a registered catalog's entries with their address
/// and an Install button, without touching the packs list.
#[test]
fn the_catalogs_pane_lists_registered_catalog_entries() {
    let dir = TempDir::new();
    let cat = make_catalog(&dir, "org.example.shared", "doc");

    let mut app = app(&dir);
    app.opts.catalog_dir = vec![cat.to_string_lossy().into_owned()];
    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.set_size(egui::vec2(1200.0, 900.0));
    harness.run();

    harness.get_by_label("Templates").click();
    harness.run();
    harness.get_by_label("Manage templates…").click();
    harness.run();

    harness.get_by_label("Catalogs").click();
    harness.run();

    assert!(
        harness
            .query_all_by_label("Catalog org.example.shared")
            .count()
            >= 1,
        "the catalog entry's name should be listed"
    );
    assert!(
        harness
            .query_all_by_label("org.example.shared v1.0.0")
            .count()
            >= 1,
        "the entry should show its id and version"
    );
    assert!(
        harness.query_all_by_label("Install").count() >= 1,
        "an entry that is not installed offers Install"
    );
}

/// Clicking Install in the Catalogs pane runs the engine's hash-verified
/// staged install into the data root, after which the entry reads as
/// installed and the pack is discoverable.
#[test]
fn installing_from_the_catalog_pane_lands_the_pack() {
    let dir = TempDir::new();
    let cat = make_catalog(&dir, "org.example.shared", "doc");
    let data_home = dir.path().join("share");

    let mut app = app(&dir);
    app.opts.catalog_dir = vec![cat.to_string_lossy().into_owned()];
    // The window writes installs to `data_home`; discovery in a test cannot
    // read the real user data root, so the same directory doubles as an
    // explicit template root and the landed pack is discoverable.
    app.opts.template_dir = vec![data_home.to_string_lossy().into_owned()];
    app.open_templates();
    app.templates.as_mut().unwrap().set_data_home(&data_home);

    let mut harness = Harness::new_ui_state(|ui, app: &mut TextrillApp| app.draw(ui), app);
    harness.set_size(egui::vec2(1200.0, 900.0));
    harness.run();

    // The window was already opened above, so it is on screen this frame.
    harness.get_by_label("Template manager");
    harness.get_by_label("Catalogs").click();
    harness.run();

    harness.get_by_label("Install").click();
    harness.run();

    let landed = data_home.join("org.example.shared");
    assert!(
        landed.join("template.toml").is_file(),
        "the install should land the pack in the data root"
    );
    assert!(textrill::pack::load(&landed).is_ok());
    assert!(
        harness.query_all_by_label("installed").count() >= 1,
        "the entry should now read as installed"
    );
}
