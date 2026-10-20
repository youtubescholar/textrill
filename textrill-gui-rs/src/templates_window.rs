//! The Template Manager window (roadmap §5.5).
//!
//! One window over the engine's own catalog: every discovered pack and the
//! shipped set, listed, searchable, tag-filterable and sortable, with a live
//! preview and a one-click "Use this template". It shares
//! [`textrill::pack::discover`] with the command line, so the list here is
//! exactly what `--list_templates` prints and `--template_library` selects —
//! there is no second notion of "template" in the GUI.
//!
//! Beyond browsing, it is the authoring surface the roadmap asks for: a pack
//! can be duplicated into the project root, scaffolded afresh, opened in a
//! small in-app editor that validates as you type, revealed in the desktop's
//! file manager, exported to a `.tar`, installed from a directory or archive,
//! or removed (user-installed packs only). All of that is plain filesystem work
//! over the engine's [`textrill::pack`] primitives; only the two decisions that
//! touch application state — applying a template and closing the window — are
//! handed back to the caller as a [`TemplatesOutcome`], the same shape the file
//! browser uses.

use std::path::{Path, PathBuf};

use egui::{Context, RichText, Ui};
use textrill::options::Options;
use textrill::pack::{self, CatalogIndex, Entry, Source, Validated, MANIFEST, SHIPPED_ID};

/// What drawing the window decided this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatesOutcome {
    /// Still open; keep it on screen.
    Pending,
    /// Dismissed by the close button or Escape.
    Closed,
    /// The user chose a template; the caller should select it and reconvert.
    /// The address is `pack-id:name`.
    Apply(String),
}

/// How the list is ordered. Version and pack id are the deterministic
/// tie-breakers, so two templates sharing a name never swap order between
/// frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Pack,
    Source,
}

impl SortKey {
    fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::Pack => "Pack",
            SortKey::Source => "Source",
        }
    }
}

/// Which pane the browse view is showing: the discovered packs, or the
/// registered offline catalogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Packs,
    Catalogs,
}

/// One row of the (filtered, sorted) listing, borrowing into the catalog's
/// entries by index so the window never copies a whole catalog per frame.
#[derive(Clone)]
struct Row {
    /// Index into `catalog.entries`.
    index: usize,
    address: String,
    name: String,
    pack_id: String,
    pack_name: String,
    version: String,
    description: String,
    tags: Vec<String>,
    source: Source,
    model: textrill::library::Model,
    slots: Vec<String>,
    vars: Vec<pack::Var>,
    root: Option<PathBuf>,
}

impl Row {
    fn of(entry: &Entry, index: usize) -> Row {
        Row {
            index,
            address: entry.address(),
            name: entry.name.clone(),
            pack_id: entry.pack_id.clone(),
            pack_name: entry.pack_name.clone(),
            version: entry.version.clone(),
            description: entry.description.clone(),
            tags: entry.tags.clone(),
            source: entry.source,
            model: entry.model,
            slots: entry.slots.clone(),
            vars: entry.vars.clone(),
            root: entry.root.clone(),
        }
    }

    /// Whether the row is a shipped template, which cannot be removed or
    /// edited in place.
    fn is_shipped(&self) -> bool {
        self.source == Source::Shipped
    }
}

/// A short note shown under the toolbar until the next action: a success
/// message, a warning, or a refusal.
#[derive(Debug, Clone, Default)]
struct Notice {
    text: String,
    is_error: bool,
}

impl Notice {
    fn ok(text: impl Into<String>) -> Notice {
        Notice {
            text: text.into(),
            is_error: false,
        }
    }

    fn error(text: impl Into<String>) -> Notice {
        Notice {
            text: text.into(),
            is_error: true,
        }
    }
}

/// A detail-pane action, deferred until after the drawing passes so the
/// closures never mutate `self` while it is borrowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Duplicate,
    Edit,
    Reveal,
    Export,
    Remove,
    NewPack,
    Install,
}

/// The in-app editor: a small form over one template file, validated against
/// the engine's own §5.3 checks with every keystroke, so a pack the window
/// writes is a pack the engine accepts.
struct Editor {
    /// The pack directory this editor writes into.
    dir: PathBuf,
    /// The pack id, for the address line.
    id: String,
    /// The template stem (`book.html` → `book`).
    leaf: String,
    /// The manifest text.
    manifest: String,
    /// The template text.
    html: String,
    /// The last validation result: `Ok` with the template list, or the first
    /// problem, refreshed whenever either buffer changes.
    checked: Result<Validated, String>,
    /// Whether the buffers changed since the last check.
    dirty_check: bool,
    /// Whether this is a brand-new pack, so cancelling removes it.
    fresh: bool,
}

/// The Template Manager's whole state.
pub struct TemplatesWindow {
    /// The discovered catalog and its problems, refreshed on open and on
    /// demand.
    catalog: pack::Catalog,
    /// The search box contents: matched against name, pack, tags and
    /// description, case-insensitively.
    query: String,
    /// The tag to keep, if any. `None` shows every template.
    tag: Option<String>,
    /// How the list is ordered.
    sort: SortKey,
    /// Whether to include the compiled-in shipped set in the list.
    show_shipped: bool,
    /// The catalog entry index currently selected, if any.
    selected: Option<usize>,
    /// The project root `new`/`duplicate` write under (`./.textrill/templates`).
    project_root: PathBuf,
    /// The user data root `install`/`remove` use (`$XDG_DATA_HOME/...`), if one
    /// exists.
    data_home: Option<PathBuf>,
    /// The editor, when one is open; the browse view is shown otherwise.
    editor: Option<Editor>,
    /// The last action's result, shown under the toolbar and cleared by the
    /// next action.
    notice: Option<Notice>,
    /// Set when an action changed the pack directories, so the catalog is
    /// re-discovered at the end of the frame.
    needs_refresh: bool,
    /// A field for an install path typed in the window, and whether the install
    /// prompt is showing.
    install_path: String,
    installing: bool,
    /// A path to re-select after a refresh (the address of a just-saved pack).
    reselect: Option<String>,
    /// Set the frame the window opens so the search box takes the keyboard.
    focus_search: bool,
    /// Which pane is showing: the packs list or the catalogs list.
    pane: Pane,
    /// The registered offline catalogs as last scanned, with any index
    /// problems; refreshed alongside `catalog`.
    indexes: Vec<CatalogIndex>,
    /// The catalog pane's search box contents, matched against pack id,
    /// name, description and tags.
    catalog_query: String,
}

impl Default for TemplatesWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplatesWindow {
    /// A window with nothing discovered yet; [`Self::refresh`] fills it.
    pub fn new() -> Self {
        TemplatesWindow {
            catalog: pack::Catalog::default(),
            query: String::new(),
            tag: None,
            sort: SortKey::default(),
            show_shipped: true,
            selected: None,
            project_root: PathBuf::new(),
            data_home: None,
            editor: None,
            notice: None,
            needs_refresh: false,
            install_path: String::new(),
            installing: false,
            reselect: None,
            focus_search: true,
            pane: Pane::Packs,
            indexes: Vec::new(),
            catalog_query: String::new(),
        }
    }

    /// The registered offline catalogs as last scanned, for tests.
    pub fn indexes(&self) -> &[CatalogIndex] {
        &self.indexes
    }

    /// Which pane is showing, for tests.
    pub fn pane(&self) -> Pane {
        self.pane
    }

    /// The catalog as last discovered, for tests.
    pub fn catalog(&self) -> &pack::Catalog {
        &self.catalog
    }

    /// Override where installs and duplicates are written, for tests that must
    /// not touch the real user data root.
    pub fn set_data_home(&mut self, data_home: impl Into<PathBuf>) {
        self.data_home = Some(data_home.into());
    }

    /// The selected entry's address, for tests.
    pub fn selected_address(&self) -> Option<String> {
        self.selected
            .and_then(|i| self.catalog.entries.get(i))
            .map(Entry::address)
    }

    /// Re-run discovery against the options' search path, and recompute where
    /// this window writes. Keeping the selection by address means a refresh
    /// after an install or remove does not lose the user's place.
    pub fn refresh(&mut self, opts: &Options) {
        let search = opts.search_path();
        self.project_root = search.project.clone();
        self.data_home = search.data_home.clone();
        let keep = self.reselect.take().or_else(|| self.selected_address());
        self.catalog = pack::discover(&search);
        self.indexes = pack::load_catalogs(&search);
        self.selected = keep
            .and_then(|address| {
                self.catalog
                    .entries
                    .iter()
                    .position(|e| e.address() == address)
            })
            .or_else(|| {
                // Fall back to the first row so a fresh window has something
                // selected to preview.
                (!self.catalog.entries.is_empty()).then_some(0)
            });
    }

    /// The tags present across the catalog, sorted and de-duplicated, for the
    /// tag filter's menu.
    fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .catalog
            .entries
            .iter()
            .flat_map(|e| e.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// The rows the filter and sort leave, in order.
    fn rows(&self) -> Vec<Row> {
        let needle = self.query.trim().to_lowercase();
        let mut rows: Vec<Row> = self
            .catalog
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.show_shipped || e.source != Source::Shipped)
            .filter(|(_, e)| {
                self.tag
                    .as_ref()
                    .is_none_or(|t| e.tags.iter().any(|x| x == t))
            })
            .filter(|(_, e)| needle.is_empty() || matches_query(e, &needle))
            .map(|(i, e)| Row::of(e, i))
            .collect();
        let key = self.sort;
        rows.sort_by(|a, b| {
            let primary = match key {
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Pack => a.pack_id.cmp(&b.pack_id),
                SortKey::Source => a.source.label().cmp(b.source.label()),
            };
            primary
                .then_with(|| a.pack_id.cmp(&b.pack_id))
                .then_with(|| a.name.cmp(&b.name))
        });
        rows
    }

    /// Draw the window for one frame.
    pub fn show(&mut self, ctx: &Context, opts: &Options) -> TemplatesOutcome {
        let mut outcome = TemplatesOutcome::Pending;
        let mut open = true;
        // Escape closes the window. The search box takes the keyboard when the
        // window opens, so a plain "close on Escape" is the behaviour that
        // matches every other app (Escape means dismiss), rather than asking the
        // user to click out of the box first.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return TemplatesOutcome::Closed;
        }
        // The editor is taken out for the frame and put back after, so drawing
        // it cannot hold a borrow of `self` while the browse view's closures
        // need `self` mutably.
        let mut editor = self.editor.take();
        let mut keep_editor = true;
        egui::Window::new("Template manager")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([820.0, 560.0])
            .min_width(620.0)
            .min_height(400.0)
            .show(ctx, |ui| {
                if let Some(editor) = editor.as_mut() {
                    keep_editor = self.editor_view(ui, editor, &mut outcome);
                } else {
                    self.browse_view(ui, opts, &mut outcome);
                }
            });
        if keep_editor {
            self.editor = editor;
        }
        if !open || outcome == TemplatesOutcome::Closed {
            return TemplatesOutcome::Closed;
        }
        outcome
    }

    // ------------------------------------------------------------ browse

    fn browse_view(&mut self, ui: &mut Ui, opts: &Options, outcome: &mut TemplatesOutcome) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Show:").small().weak());
            if ui
                .selectable_label(self.pane == Pane::Packs, "Packs")
                .clicked()
            {
                self.pane = Pane::Packs;
            }
            if ui
                .selectable_label(self.pane == Pane::Catalogs, "Catalogs")
                .on_hover_text("Registered offline pack catalogs (fetched from nowhere)")
                .clicked()
            {
                self.pane = Pane::Catalogs;
            }
        });

        let mut catalog_install: Option<(usize, usize)> = None;
        let apply = match self.pane {
            Pane::Packs => self.packs_view(ui),
            Pane::Catalogs => {
                self.catalogs_view(ui, &mut catalog_install);
                None
            }
        };

        if self.installing {
            self.install_prompt(ui);
        }
        self.notice_view(ui);

        if self.needs_refresh {
            self.needs_refresh = false;
            self.refresh(opts);
        }
        if let Some((ci, ei)) = catalog_install {
            self.install_from_catalog(ci, ei);
        }
        if let Some(address) = apply {
            *outcome = TemplatesOutcome::Apply(address);
        }
    }

    /// The packs pane: search, tag filter, list and detail. Returns the
    /// chosen template's address, if Apply was pressed.
    fn packs_view(&mut self, ui: &mut Ui) -> Option<String> {
        ui.horizontal_wrapped(|ui| {
            ui.label("Search");
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("name, pack, tag or description")
                    .desired_width(220.0),
            );
            if self.focus_search {
                field.request_focus();
                self.focus_search = false;
            }
            let _ = field;

            ui.separator();
            ui.label("Sort");
            for key in [SortKey::Name, SortKey::Pack, SortKey::Source] {
                if ui.selectable_label(self.sort == key, key.label()).clicked() {
                    self.sort = key;
                }
            }

            ui.separator();
            let mut show_shipped = self.show_shipped;
            if ui
                .checkbox(&mut show_shipped, "Shipped")
                .on_hover_text("Include the compiled-in templates")
                .changed()
            {
                self.show_shipped = show_shipped;
            }
        });

        let tags = self.all_tags();
        ui.horizontal_wrapped(|ui| {
            ui.label("Tag");
            if ui.selectable_label(self.tag.is_none(), "All").clicked() {
                self.tag = None;
            }
            for tag in &tags {
                let on = self.tag.as_deref() == Some(tag.as_str());
                if ui.selectable_label(on, tag).clicked() {
                    self.tag = if on { None } else { Some(tag.clone()) };
                }
            }
            if tags.is_empty() {
                ui.weak("(no tags)");
            }
        });

        ui.separator();

        // Problems first: discovery never fails, so a broken pack is a warning
        // the user must be able to see, not a silent skip.
        if !self.catalog.problems.is_empty() {
            egui::CollapsingHeader::new(format!(
                "{} problem(s) while discovering packs",
                self.catalog.problems.len()
            ))
            .default_open(false)
            .show(ui, |ui| {
                for problem in &self.catalog.problems {
                    ui.colored_label(ui.visuals().warn_fg_color, problem);
                }
            });
        }

        let rows = self.rows();
        let mut apply: Option<String> = None;

        // Two columns: the list on the left, the preview and actions on the
        // right. The list keeps a fixed share so the preview never vanishes.
        let total = ui.available_width();
        let list_width = (total * 0.46).clamp(240.0, 420.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(list_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.list_view(ui, &rows);
                },
            );
            ui.separator();
            ui.vertical(|ui| {
                self.detail_view(ui, &rows, &mut apply);
            });
        });

        apply
    }

    // ------------------------------------------------------------ catalogs

    /// The catalogs pane: every registered catalog, its index problems, and
    /// its entries filtered by the search box, each installable with one
    /// click through the same hash-verified staged install the CLI uses.
    fn catalogs_view(&mut self, ui: &mut Ui, install: &mut Option<(usize, usize)>) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Search");
            ui.add(
                egui::TextEdit::singleline(&mut self.catalog_query)
                    .hint_text("pack id, name, tag or description")
                    .desired_width(240.0),
            );
            if ui
                .button("Rescan")
                .on_hover_text("Reread every registered catalog from disk (fetches nothing)")
                .clicked()
            {
                self.needs_refresh = true;
            }
        });
        if self.indexes.is_empty() {
            ui.add_space(6.0);
            ui.weak("No catalogs registered. Pass --catalog_dir DIR (it can live in .textrillrc).");
            return;
        }
        let needle = self.catalog_query.trim().to_lowercase();
        let problems = self
            .indexes
            .iter()
            .map(|c| c.problems.clone())
            .collect::<Vec<_>>();
        for (ci, cat) in self.indexes.iter().enumerate() {
            let header = format!(
                "{} — {} pack(s) {}",
                cat.path.display(),
                cat.entries.len(),
                if problems[ci].is_empty() {
                    ""
                } else {
                    "  (with problems)"
                }
            );
            egui::CollapsingHeader::new(header)
                .default_open(true)
                .show(ui, |ui| {
                    for problem in &problems[ci] {
                        ui.colored_label(ui.visuals().warn_fg_color, problem);
                    }
                    for (ei, entry) in cat.entries.iter().enumerate() {
                        let matches = needle.is_empty()
                            || entry.id.to_lowercase().contains(&needle)
                            || entry.name.to_lowercase().contains(&needle)
                            || entry.description.to_lowercase().contains(&needle)
                            || entry
                                .tags
                                .iter()
                                .any(|t| t.to_lowercase().contains(&needle));
                        if !matches {
                            continue;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&entry.name).strong());
                            ui.label(
                                RichText::new(format!("{} v{}", entry.id, entry.version)).weak(),
                            );
                            if !entry.tags.is_empty() {
                                ui.label(RichText::new(entry.tags.join(", ")).weak());
                            }
                            if let Some(min) = entry.min_textrill.as_deref() {
                                if matches!(pack::version_satisfies(pack::VERSION, min), Ok(false))
                                {
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        format!("requires textrill >= {min}"),
                                    );
                                }
                            }
                            let installed =
                                self.catalog.entries.iter().any(|e| e.pack_id == entry.id);
                            if installed {
                                ui.label(RichText::new("installed").small());
                            } else if ui
                                .small_button("Install")
                                .on_hover_text(
                                    "Download-free install from this catalog; the archive's \
                                     sha256 must match the index",
                                )
                                .clicked()
                            {
                                *install = Some((ci, ei));
                            }
                        });
                        if !entry.description.is_empty() {
                            ui.weak(&entry.description);
                        }
                    }
                });
        }
    }

    /// Install one catalog entry through the engine's hash-verified staged
    /// install, then refresh so the new pack shows up.
    fn install_from_catalog(&mut self, ci: usize, ei: usize) {
        let Some(data_home) = self.data_home.clone() else {
            self.notice = Some(Notice::error(
                "no user data directory (set XDG_DATA_HOME or HOME)",
            ));
            return;
        };
        let Some(entry) = self.indexes.get(ci).and_then(|c| c.entries.get(ei)) else {
            return;
        };
        match pack::from_catalog_entry(entry, &data_home) {
            Ok(text) => {
                self.notice = Some(Notice::ok(text.trim_end().to_string()));
                self.needs_refresh = true;
            }
            Err(e) => self.notice = Some(Notice::error(e)),
        }
    }

    fn list_view(&mut self, ui: &mut Ui, rows: &[Row]) {
        if rows.is_empty() {
            ui.add_space(6.0);
            ui.weak("No templates match.");
            return;
        }
        ui.label(RichText::new(format!("{} template(s)", rows.len())).small());
        egui::ScrollArea::vertical()
            .id_salt("tmpl-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for row in rows {
                    let selected = self.selected == Some(row.index);
                    let title = format!("{}  ·  {}", row.name, row.source.label());
                    let response = ui.selectable_label(selected, RichText::new(title));
                    let response = response.on_hover_text(format!(
                        "{}\npack {} v{}",
                        row.address, row.pack_id, row.version
                    ));
                    if response.clicked() {
                        self.selected = Some(row.index);
                    }
                }
            });
    }

    fn detail_view(&mut self, ui: &mut Ui, rows: &[Row], apply: &mut Option<String>) {
        let Some(row) = self.selected_row(rows) else {
            ui.weak("Select a template to preview it.");
            return;
        };
        let row = row.clone();

        let mut action: Option<Action> = None;
        let mut tag_click: Option<String> = None;

        ui.heading(&row.name);
        ui.horizontal_wrapped(|ui| {
            badge(ui, row.source.label(), row.source);
            ui.label(RichText::new(format!("pack {} v{}", row.pack_id, row.version)).weak());
            ui.label(RichText::new(format!("model {}", row.model.name())).weak());
        });
        if !row.pack_name.is_empty() && row.pack_name != "Shipped" {
            ui.label(RichText::new(&row.pack_name).strong());
        }
        if !row.description.is_empty() {
            ui.label(&row.description);
        }
        if !row.tags.is_empty() {
            let current = self.tag.clone();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Tags:").weak());
                for tag in &row.tags {
                    let on = current.as_deref() == Some(tag.as_str());
                    if ui.selectable_label(on, tag).clicked() {
                        tag_click = Some(tag.clone());
                    }
                }
            });
        }
        if !row.slots.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Slots:").weak());
                ui.label(RichText::new(row.slots.join(", ")).monospace());
            });
        }
        if !row.vars.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Variables:").weak());
                let vars: Vec<String> = row
                    .vars
                    .iter()
                    .map(|v| {
                        if v.required {
                            format!("{}*", v.name)
                        } else {
                            v.name.clone()
                        }
                    })
                    .collect();
                ui.label(RichText::new(vars.join(", ")).monospace())
                    .on_hover_text("* required");
            });
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Use this template")
                .on_hover_text("Convert with this template (Ctrl+Enter to convert)")
                .clicked()
            {
                *apply = Some(row.address.clone());
            }
            if ui
                .button("Duplicate")
                .on_hover_text("Copy this template into this project's .textrill/templates")
                .clicked()
            {
                action = Some(Action::Duplicate);
            }
            if ui
                .button("Edit…")
                .on_hover_text("Open this template and its manifest in the editor")
                .clicked()
            {
                action = Some(Action::Edit);
            }
            if ui
                .button("Reveal")
                .on_hover_text("Show the pack's folder in the file manager")
                .clicked()
            {
                action = Some(Action::Reveal);
            }
            if ui
                .button("Export…")
                .on_hover_text("Write this pack as a .tar for sharing")
                .clicked()
            {
                action = Some(Action::Export);
            }
            if !row.is_shipped()
                && ui
                    .button("Remove")
                    .on_hover_text("Remove this user-installed pack")
                    .clicked()
            {
                action = Some(Action::Remove);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("New pack…")
                .on_hover_text("Scaffold a new pack in this project")
                .clicked()
            {
                action = Some(Action::NewPack);
            }
            if ui
                .button("Install…")
                .on_hover_text("Install a pack from a directory or a .tar by path")
                .clicked()
            {
                action = Some(Action::Install);
            }
        });

        ui.separator();
        ui.label(RichText::new("Preview").small());
        let preview = self.preview_text(row.index);
        egui::ScrollArea::vertical()
            .id_salt("tmpl-preview")
            .auto_shrink([false, false])
            .show(ui, |ui| match &preview {
                Ok(text) => {
                    ui.add(
                        egui::TextEdit::multiline(&mut text.as_str())
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });

        if let Some(tag) = tag_click {
            self.tag = if self.tag.as_deref() == Some(tag.as_str()) {
                None
            } else {
                Some(tag)
            };
        }
        if let Some(action) = action {
            self.run_action(action, &row);
        }
    }

    /// Perform a detail-pane action on `row`. Deferred out of the drawing
    /// closures so those never hold a borrow of `self` across a mutation.
    fn run_action(&mut self, action: Action, row: &Row) {
        match action {
            Action::Duplicate => self.duplicate(row),
            Action::Edit => self.open_editor(row),
            Action::Reveal => self.reveal(row),
            Action::Export => self.export_pack(row),
            Action::Remove => self.remove(row),
            Action::NewPack => self.new_pack(),
            Action::Install => {
                self.installing = true;
                self.install_path.clear();
            }
        }
    }

    /// The selected entry, resolved against the filtered rows so a selection
    /// hidden by a new filter falls back to the first visible row.
    fn selected_row<'a>(&self, rows: &'a [Row]) -> Option<&'a Row> {
        rows.iter()
            .find(|r| Some(r.index) == self.selected)
            .or_else(|| rows.first())
    }

    /// The template's own markup, read on demand. A shipped entry reads out of
    /// the library; a discovered one from its file.
    fn preview_text(&self, index: usize) -> Result<String, String> {
        self.catalog
            .entries
            .get(index)
            .ok_or_else(|| "no template selected".to_string())
            .and_then(Entry::html)
    }

    fn notice_view(&mut self, ui: &mut Ui) {
        let Some(notice) = self.notice.clone() else {
            return;
        };
        ui.separator();
        let mut dismiss = false;
        ui.horizontal_wrapped(|ui| {
            let color = if notice.is_error {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().warn_fg_color
            };
            ui.colored_label(color, &notice.text);
            if ui.small_button("Dismiss").clicked() {
                dismiss = true;
            }
        });
        if dismiss {
            self.notice = None;
        }
    }

    /// The small install prompt: a typed path, installed on confirm through the
    /// same staged-then-renamed path the CLI uses.
    fn install_prompt(&mut self, ui: &mut Ui) {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("Install from");
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.install_path)
                    .hint_text("path to a pack directory or .tar")
                    .desired_width(360.0),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let go = ui.button("Install").clicked() || entered;
            if ui.button("Cancel").clicked() {
                self.installing = false;
                self.install_path.clear();
            }
            if go {
                self.install();
            }
        });
    }

    /// Install the typed path into the user data root.
    fn install(&mut self) {
        let path = self.install_path.trim().to_string();
        self.installing = false;
        if path.is_empty() {
            self.notice = Some(Notice::error("install: enter a path"));
            return;
        }
        self.install_from(Path::new(&path));
    }

    /// Install a pack directory or `.tar` into the user data root, staged
    /// beside the destination and renamed into place only after it validates,
    /// so a refused or half-copied pack never appears in the catalog. Public so
    /// the app can route a dropped path here (roadmap §5.5: drag-and-drop
    /// install). Returns whether the pack was installed.
    pub fn install_from(&mut self, source: &Path) -> bool {
        let path = source.display().to_string();
        let Some(data_home) = self.data_home.clone() else {
            self.notice = Some(Notice::error(
                "no user data directory (set XDG_DATA_HOME or HOME)",
            ));
            return false;
        };
        let meta = match std::fs::metadata(source) {
            Ok(m) => m,
            Err(e) => {
                self.notice = Some(Notice::error(format!("install: {path}: {e}")));
                return false;
            }
        };
        let staging = data_home.join(format!(".install-{}-{}", std::process::id(), next_seq()));
        let result = (|| -> Result<String, String> {
            std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
            if meta.is_dir() {
                copy_pack(source, &staging)?;
            } else {
                let bytes =
                    std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
                pack::unpack(&bytes, &staging)?;
            }
            let validated = pack::load(&staging)?;
            let dest = data_home.join(&validated.id);
            if dest.exists() {
                return Err(format!(
                    "install: {} is already installed at {}; remove it first",
                    validated.id,
                    dest.display()
                ));
            }
            std::fs::rename(&staging, &dest)
                .map_err(|e| format!("install: could not move {} into place: {e}", validated.id))?;
            Ok(format!("installed {}", validated.id))
        })();
        let _ = std::fs::remove_dir_all(&staging);
        match result {
            Ok(text) => {
                self.notice = Some(Notice::ok(text));
                self.needs_refresh = true;
                true
            }
            Err(e) => {
                self.notice = Some(Notice::error(e));
                false
            }
        }
    }

    // -------------------------------------------------------- editor mode

    /// Open the editor over `row`'s template (and its whole pack).
    fn open_editor(&mut self, row: &Row) {
        let Some(root) = &row.root else {
            self.notice = Some(Notice::error(
                "the shipped templates are compiled in and cannot be edited here",
            ));
            return;
        };
        let manifest_path = root.join(MANIFEST);
        let manifest = match std::fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(e) => {
                self.notice = Some(Notice::error(format!("{}: {e}", manifest_path.display())));
                return;
            }
        };
        let html_path = root.join(format!("{}.html", row.name));
        let html = match std::fs::read_to_string(&html_path) {
            Ok(text) => text,
            Err(e) => {
                self.notice = Some(Notice::error(format!("{}: {e}", html_path.display())));
                return;
            }
        };
        let mut editor = Editor {
            dir: root.clone(),
            id: row.pack_id.clone(),
            leaf: row.name.clone(),
            manifest,
            html,
            checked: Err("not validated yet".to_string()),
            dirty_check: true,
            fresh: false,
        };
        editor.recheck();
        self.editor = Some(editor);
    }

    /// Scaffold a fresh pack and open it in the editor.
    fn new_pack(&mut self) {
        let word = self.query.trim();
        let word = if word.is_empty() { "my-template" } else { word };
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let Some(planned) = pack::plan_scaffold(&self.project_root, &cwd, word) else {
            self.notice = Some(Notice::error(format!(
                "{word:?} has no usable characters for a pack name"
            )));
            return;
        };
        if planned.dir.exists() {
            self.notice = Some(Notice::error(format!(
                "{} already exists",
                planned.dir.display()
            )));
            return;
        }
        let mut editor = Editor {
            dir: planned.dir.clone(),
            id: planned.id.clone(),
            leaf: planned.leaf.clone(),
            manifest: pack::scaffold::manifest(&planned.id, &planned.leaf),
            html: pack::scaffold::html(&planned.leaf),
            checked: Err("not validated yet".to_string()),
            dirty_check: true,
            fresh: true,
        };
        editor.recheck();
        self.editor = Some(editor);
    }

    /// Draw the editor; returns `false` when the editor should close (Save or
    /// Cancel/Discard), so `show` drops the local editor instead of putting it
    /// back.
    fn editor_view(
        &mut self,
        ui: &mut Ui,
        editor: &mut Editor,
        outcome: &mut TemplatesOutcome,
    ) -> bool {
        if editor.dirty_check {
            editor.recheck();
        }
        let mut keep = true;

        ui.horizontal_wrapped(|ui| {
            ui.heading(format!("Editing {}", editor.id));
            ui.label(RichText::new(format!("({}.html)", editor.leaf)).weak());
        });
        ui.horizontal_wrapped(|ui| {
            match &editor.checked {
                Ok(validated) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(0x2e, 0x7d, 0x32),
                        format!("valid · {} template(s)", validated.templates.len()),
                    );
                }
                Err(problem) => {
                    ui.colored_label(ui.visuals().error_fg_color, format!("invalid · {problem}"));
                }
            }
            ui.weak(editor.dir.display().to_string());
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("template.toml").strong());
            ui.weak("the manifest");
        });
        let manifest_changed = egui::ScrollArea::vertical()
            .id_salt("tmpl-manifest")
            .max_height(180.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut editor.manifest)
                        .code_editor()
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            })
            .inner;
        if manifest_changed {
            editor.dirty_check = true;
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}.html", editor.leaf)).strong());
            ui.weak("the template");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                slot_autocomplete(ui, editor);
            });
        });
        let html_changed = egui::ScrollArea::vertical()
            .id_salt("tmpl-html")
            .max_height(220.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut editor.html)
                        .code_editor()
                        .desired_rows(10)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            })
            .inner;
        if html_changed {
            editor.dirty_check = true;
        }

        ui.separator();
        let valid = editor.checked.is_ok();
        let mut save = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(valid, egui::Button::new("Save"))
                .on_hover_text("Write the manifest and template to the pack directory")
                .clicked()
            {
                save = true;
            }
            let cancel_label = if editor.fresh { "Discard" } else { "Cancel" };
            if ui.button(cancel_label).clicked() {
                cancel = true;
            }
        });

        if save {
            match editor.save() {
                Ok(()) => {
                    let address = format!("{}:{}", editor.id, editor.leaf);
                    self.notice = Some(Notice::ok(format!("saved {}", editor.dir.display())));
                    self.reselect = Some(address.clone());
                    self.needs_refresh = true;
                    *outcome = TemplatesOutcome::Apply(address);
                    keep = false;
                }
                Err(e) => self.notice = Some(Notice::error(e)),
            }
        }
        if cancel {
            if editor.fresh {
                let _ = std::fs::remove_dir_all(&editor.dir);
            }
            keep = false;
        }
        keep
    }

    // -------------------------------------------------------- actions

    /// Copy `row`'s pack into the user templates root under a fresh id, so it
    /// becomes an ordinary editable, discoverable pack (roadmap §5.5: duplicate
    /// into `~/.local/share/textrill/templates/<mine>/`).
    fn duplicate(&mut self, row: &Row) {
        let Some(root) = &row.root else {
            self.notice = Some(Notice::error(
                "the shipped templates are compiled in; scaffold a new pack instead",
            ));
            return;
        };
        let Some(data_home) = self.data_home.clone() else {
            self.notice = Some(Notice::error(
                "no user data directory (set XDG_DATA_HOME or HOME)",
            ));
            return;
        };
        let base = format!("{}-copy", row.pack_id);
        let leaf = pack::sanitize(&base).unwrap_or_else(|| row.pack_id.clone());
        let dest = data_home.join(&leaf);
        if dest.exists() {
            self.notice = Some(Notice::error(format!("{} already exists", dest.display())));
            return;
        }
        match copy_pack(root, &dest) {
            Ok(()) => {
                self.notice = Some(Notice::ok(format!("duplicated into {}", dest.display())));
                self.needs_refresh = true;
            }
            Err(e) => self.notice = Some(Notice::error(e)),
        }
    }

    /// Open the pack's directory in the desktop's file manager. Uses
    /// `xdg-open`, which every XDG desktop provides; a failure is reported, not
    /// fatal.
    fn reveal(&mut self, row: &Row) {
        let Some(root) = &row.root else {
            self.notice = Some(Notice::error("the shipped set has no folder on disk"));
            return;
        };
        match std::process::Command::new("xdg-open").arg(root).spawn() {
            Ok(_) => self.notice = Some(Notice::ok(format!("revealed {}", root.display()))),
            Err(e) => self.notice = Some(Notice::error(format!("xdg-open: {e}"))),
        }
    }

    /// Write the pack as a deterministic `.tar` beside the pack directory.
    fn export_pack(&mut self, row: &Row) {
        let Some(root) = &row.root else {
            self.notice = Some(Notice::error(
                "the shipped templates are compiled in and have no pack to export",
            ));
            return;
        };
        let dest = root.with_extension("tar");
        match pack::pack(root).and_then(|bytes| {
            std::fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))
        }) {
            Ok(()) => self.notice = Some(Notice::ok(format!("wrote {}", dest.display()))),
            Err(e) => self.notice = Some(Notice::error(format!("export: {e}"))),
        }
    }

    /// Remove a user-installed pack. Only the user root is touched, so a
    /// project or system pack is left where it is with an explanatory refusal.
    fn remove(&mut self, row: &Row) {
        if row.is_shipped() {
            self.notice = Some(Notice::error("the shipped templates cannot be removed"));
            return;
        }
        let Some(data_home) = &self.data_home else {
            self.notice = Some(Notice::error(
                "no user data directory (set XDG_DATA_HOME or HOME)",
            ));
            return;
        };
        // The pack id is a single plain component; the engine's validation of
        // every discovered pack guarantees this, but the join is still checked.
        if row.pack_id.contains('/') || row.pack_id.contains('\\') || row.pack_id == SHIPPED_ID {
            self.notice = Some(Notice::error(format!(
                "{} is not a removable id",
                row.pack_id
            )));
            return;
        }
        let dir = data_home.join(&row.pack_id);
        if !dir.is_dir() {
            self.notice = Some(Notice::error(format!(
                "{} is a {} pack; only user-installed packs can be removed here",
                row.pack_id,
                row.source.label()
            )));
            return;
        }
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {
                self.notice = Some(Notice::ok(format!("removed {}", row.pack_id)));
                self.needs_refresh = true;
            }
            Err(e) => self.notice = Some(Notice::error(format!("{}: {e}", dir.display()))),
        }
    }
}

/// A small badge for the source column: the three words the CLI uses, coloured
/// so the list can be scanned at a glance.
fn badge(ui: &mut Ui, text: &str, source: Source) {
    let color = match source {
        Source::Shipped => egui::Color32::from_rgb(0x55, 0x55, 0x55),
        Source::Project | Source::Explicit => egui::Color32::from_rgb(0x2e, 0x7d, 0x32),
        Source::User | Source::System => egui::Color32::from_rgb(0x15, 0x65, 0xc0),
    };
    ui.label(RichText::new(text).small().color(color));
}

/// Whether an entry matches a lower-cased query against its name, pack id,
/// pack name, tags and description.
fn matches_query(entry: &Entry, needle: &str) -> bool {
    let hay = [
        entry.name.to_lowercase(),
        entry.pack_id.to_lowercase(),
        entry.pack_name.to_lowercase(),
        entry.description.to_lowercase(),
    ];
    if hay.iter().any(|h| h.contains(needle)) {
        return true;
    }
    entry.tags.iter().any(|t| t.to_lowercase().contains(needle))
}

/// A tiny autocomplete: a button that offers the fixed and declared slot names
/// to insert at the caret is beyond a one-file editor, so this inserts the
/// `{{textrill:…}}` skeleton so the user need not recall the syntax. It appends
/// to the end of the buffer, which is enough to be useful and cannot corrupt
/// the middle of the file.
fn slot_autocomplete(ui: &mut Ui, editor: &mut Editor) {
    ui.menu_button("Insert slot", |ui| {
        for (label, token) in [
            ("content", "{{textrill:content}}"),
            ("title", "{{textrill:title}}"),
            ("toc", "{{textrill:toc}}"),
            ("footer", "{{textrill:footer}}"),
        ] {
            if ui.button(label).clicked() {
                if !editor.html.ends_with('\n') && !editor.html.is_empty() {
                    editor.html.push('\n');
                }
                editor.html.push_str(token);
                editor.html.push('\n');
                editor.dirty_check = true;
                ui.close();
            }
        }
        if !editor
            .checked
            .as_ref()
            .is_ok_and(|v| v.templates.iter().any(|t| !t.slots.is_empty()))
        {
            ui.separator();
            ui.weak("Packs may declare their own slots in template.toml.");
        }
        for slot in declared_slots(editor) {
            if ui.button(&slot).clicked() {
                editor.html.push_str(&format!("{{{{textrill:{slot}}}}}\n"));
                editor.dirty_check = true;
                ui.close();
            }
        }
    });
}

/// The slots the manifest currently declares, for the autocomplete's second
/// group.
fn declared_slots(editor: &Editor) -> Vec<String> {
    editor
        .checked
        .as_ref()
        .ok()
        .map(|v| {
            let mut slots: Vec<String> = v
                .templates
                .iter()
                .flat_map(|t| t.slots.iter().cloned())
                .collect();
            slots.sort();
            slots.dedup();
            slots
        })
        .unwrap_or_default()
}

impl Editor {
    /// Re-run the engine's §5.3 checks over the current buffers, without
    /// touching the disk: the manifest is parsed in memory and the template is
    /// checked by writing to a throwaway directory only when the manifest is
    /// sound. A cheaper, sufficient proxy is to write into the pack's own
    /// directory only on Save; here the check is done against a temp copy so an
    /// in-progress edit never clobbers the file on disk.
    fn recheck(&mut self) {
        self.dirty_check = false;
        self.checked = self.validate_in_temp();
    }

    /// Validate the buffers by staging them in a temporary directory and
    /// running [`pack::load`]. The temp directory is always removed.
    fn validate_in_temp(&self) -> Result<Validated, String> {
        let staging = std::env::temp_dir().join(format!(
            "textrill-tmed-{}-{}",
            std::process::id(),
            next_seq()
        ));
        let result = (|| {
            std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
            std::fs::write(staging.join(MANIFEST), &self.manifest)
                .map_err(|e| format!("{}: {e}", staging.join(MANIFEST).display()))?;
            std::fs::write(staging.join(format!("{}.html", self.leaf)), &self.html)
                .map_err(|e| format!("{}.html: {e}", self.leaf))?;
            pack::load(&staging)
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result
    }

    /// Write the buffers into the real pack directory, keeping the manifest and
    /// template names the editor was opened with.
    fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let manifest_path = self.dir.join(MANIFEST);
        let html_path = self.dir.join(format!("{}.html", self.leaf));
        std::fs::write(&manifest_path, &self.manifest)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        std::fs::write(&html_path, &self.html).map_err(|e| format!("{}: {e}", html_path.display()))
    }
}

/// Copy a pack directory, refusing symlinks and special files, exactly as the
/// engine's own install copy does.
fn copy_pack(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", src.display()))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let meta =
            std::fs::symlink_metadata(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(format!(
                "{} is a symlink; a pack is regular files",
                from.display()
            ));
        }
        if ft.is_dir() {
            copy_pack(&from, &to)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        } else {
            return Err(format!("{} is not a regular file", from.display()));
        }
    }
    Ok(())
}

/// A process-wide sequence for staging directory names.
fn next_seq() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use textrill::pack::SearchPath;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "textrill-tmw-{}-{tag}-{}",
            std::process::id(),
            next_seq()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    fn pack_into(root: &Path, id: &str, stem: &str, tags: &[&str]) {
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
        write(
            &root.join(id),
            MANIFEST,
            &format!(
                "schema = 1\nid = {id:?}\nname = {id:?}\nversion = \"1.0.0\"\n\
                 license = \"GPL-3.0-or-later\"\n{tags}\n\
                 [[templates]]\nfile = \"{stem}.html\"\nmodel = \"document\"\n"
            ),
        );
        write(
            &root.join(id),
            &format!("{stem}.html"),
            "{{textrill:content}}",
        );
    }

    /// A window whose catalog is discovered from explicit roots only, by
    /// building a `SearchPath` and calling discover through a helper so the
    /// test never touches the process environment.
    fn window_with(search: &SearchPath) -> TemplatesWindow {
        let mut window = TemplatesWindow::new();
        window.project_root = search.project.clone();
        window.data_home = search.data_home.clone();
        window.catalog = pack::discover(search);
        if !window.catalog.entries.is_empty() {
            window.selected = Some(0);
        }
        window
    }

    fn search_for(explicit: Option<&Path>, project: &Path, data_home: Option<&Path>) -> SearchPath {
        SearchPath {
            catalog_dirs: Vec::new(),
            explicit: explicit.map(|p| vec![p.to_path_buf()]).unwrap_or_default(),
            project: project.to_path_buf(),
            data_home: data_home.map(Path::to_path_buf),
            data_dirs: Vec::new(),
        }
    }

    #[test]
    fn the_list_shows_discovered_packs_and_the_shipped_set() {
        let base = scratch("list");
        let root = base.join("templates");
        pack_into(&root, "org.example.book", "book", &["prose"]);
        let search = search_for(Some(&root), &base.join("none"), None);
        let window = window_with(&search);
        let rows = window.rows();
        assert!(rows.iter().any(|r| r.name == "book"));
        assert!(rows.iter().any(|r| r.is_shipped()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn hiding_shipped_drops_those_rows() {
        let base = scratch("shipped");
        let root = base.join("templates");
        pack_into(&root, "org.example.book", "book", &[]);
        let search = search_for(Some(&root), &base.join("none"), None);
        let mut window = window_with(&search);
        window.show_shipped = false;
        assert!(window.rows().iter().all(|r| !r.is_shipped()));
        assert!(window.rows().iter().any(|r| r.name == "book"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_tag_filter_keeps_only_matching_packs() {
        let base = scratch("tag");
        let root = base.join("templates");
        pack_into(&root, "org.example.a", "a", &["prose"]);
        pack_into(&root, "org.example.b", "b", &["poetry"]);
        let search = search_for(Some(&root), &base.join("none"), None);
        let mut window = window_with(&search);
        window.tag = Some("prose".to_string());
        let rows = window.rows();
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a"]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_query_matches_name_pack_tag_and_description() {
        let base = scratch("query");
        let root = base.join("templates");
        pack_into(&root, "org.example.novel", "story", &["fiction"]);
        let search = search_for(Some(&root), &base.join("none"), None);
        let mut window = window_with(&search);
        for needle in ["story", "novel", "fiction", "org.example"] {
            window.query = needle.to_string();
            assert!(
                window.rows().iter().any(|r| r.name == "story"),
                "query {needle:?} should match"
            );
        }
        window.query = "nothing-like-this".to_string();
        assert!(window.rows().is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn sorting_by_pack_orders_by_pack_id() {
        let base = scratch("sort");
        let root = base.join("templates");
        pack_into(&root, "org.example.zulu", "z", &[]);
        pack_into(&root, "org.example.alpha", "a", &[]);
        let search = search_for(Some(&root), &base.join("none"), None);
        let mut window = window_with(&search);
        window.show_shipped = false;
        window.sort = SortKey::Pack;
        let rows = window.rows();
        let ids: Vec<&str> = rows.iter().map(|r| r.pack_id.as_str()).collect();
        assert_eq!(ids, vec!["org.example.alpha", "org.example.zulu"]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_preview_reads_a_discovered_template_and_a_shipped_one() {
        let base = scratch("preview");
        let root = base.join("templates");
        pack_into(&root, "org.example.book", "book", &[]);
        let search = search_for(Some(&root), &base.join("none"), None);
        let window = window_with(&search);
        let discovered = window
            .catalog
            .entries
            .iter()
            .position(|e| e.name == "book" && e.source != Source::Shipped)
            .unwrap();
        assert_eq!(
            window.preview_text(discovered).unwrap(),
            "{{textrill:content}}"
        );
        let shipped = window
            .catalog
            .entries
            .iter()
            .position(|e| e.source == Source::Shipped)
            .unwrap();
        assert!(window.preview_text(shipped).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_editor_validates_a_scaffold_and_refuses_a_broken_manifest() {
        let mut editor = Editor {
            dir: PathBuf::from("/nonexistent"),
            id: "org.example.x".to_string(),
            leaf: "x".to_string(),
            manifest: pack::scaffold::manifest("org.example.x", "x"),
            html: pack::scaffold::html("x"),
            checked: Err("not validated yet".to_string()),
            dirty_check: true,
            fresh: true,
        };
        editor.recheck();
        assert!(editor.checked.is_ok(), "{:?}", editor.checked);
        editor.manifest = "schema = 2\n".to_string();
        editor.recheck();
        assert!(editor.checked.is_err());
    }

    #[test]
    fn a_broken_pack_surfaces_as_a_problem_not_a_missing_row() {
        let base = scratch("problem");
        let root = base.join("templates");
        write(&root.join("org.example.bad"), MANIFEST, "schema = 1\n");
        let search = search_for(Some(&root), &base.join("none"), None);
        let window = window_with(&search);
        assert_eq!(window.catalog.problems.len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }
}
