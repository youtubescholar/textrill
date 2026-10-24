// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Application state and layout.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::Ui;
use textrill::options::Options;

use crate::args::Args;
use crate::command_palette::Command;
use crate::dialogs::{self, Chooser, NativeChooser};
use crate::document::Document;
use crate::file_browser::{BrowserOutcome, FileBrowser, FileIntent};
use crate::options_panel::OptionsPanel;
use crate::presets::{self, Preset};
use crate::settings::Settings;
use crate::summary;
use crate::theme::{self, Theme};
use crate::worker::ConversionWorker;

/// How long to wait after the last change before converting.
pub const AUTO_CONVERT_DELAY: Duration = Duration::from_millis(300);

const NEW_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
const SAVE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
const SAVE_TEXT_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    },
    egui::Key::S,
);
const CONVERT_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter);
const COPY_HTML_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    },
    egui::Key::C,
);
const QUIT_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Q);
/// Toggle the immersive "focus" view that hides the menus and the toolbar,
/// leaving the text and a slim Convert/Save HTML action bar.
/// `Ctrl/cmd+Shift+F` is the convention browsers give fullscreen; the *same*
/// shortcut is a second, keyboard-only way back alongside the visible
/// "Exit focus" button.
const FOCUS_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    },
    egui::Key::F,
);
/// Pick which view the working area shows: `Ctrl+1` side by side, `Ctrl+2`
/// source only, `Ctrl+3` converted HTML only. The View menu offers the same
/// three choices by name.
const VIEW_SPLIT_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num1);
const VIEW_SOURCE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num2);
const VIEW_HTML_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num3);
/// `Ctrl+4` shows the native rendered preview (roadmap §7), the view that
/// answers "what does this look like?" without leaving the app.
const VIEW_PREVIEW_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num4);
/// `Ctrl+5` shows the outline (roadmap §7, workstream 2).
const VIEW_OUTLINE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num5);
/// `Ctrl+6` shows the rendered current/previous comparison (roadmap §7,
/// workstream 3).
const VIEW_DIFF_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num6);
/// The reset command needs a route that does not depend on any widget being
/// on screen, because the whole point of it is recovering a layout that is
/// off-screen or folded away. `Ctrl/Cmd+Alt+0` is outside the set egui's own
/// zoom shortcuts (`Ctrl/Cmd` with `+`, `-`, `0`) consume.
const RESET_WINDOW_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers {
        command: true,
        alt: true,
        ..egui::Modifiers::NONE
    },
    egui::Key::Num0,
);
/// Open the command palette. `Ctrl/Cmd+K` is the chord editors and browsers
/// have settled on, and it is the one key that reaches every action from a
/// single, typeable list when a menu or the toolbar is not on screen.
const PALETTE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K);

/// What a save command wants next.
///
/// There is no dialog yet, so a save with nowhere to write reports the name it
/// would have proposed. The chrome slice wires that to a real file chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveRequest {
    /// Written to this path.
    Saved(PathBuf),
    /// The caller must choose a name; this is what `Save As` would propose.
    NeedsName(PathBuf),
}

/// The user's answer to the unsaved-changes prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveAnswer {
    /// Write the text back first, then carry out the waiting command.
    Save,
    /// Throw the edits away and carry out the waiting command.
    Discard,
    /// Do nothing.
    Cancel,
}

/// The command waiting behind the unsaved-changes prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    New,
    Open,
    OpenRecent(PathBuf),
    LoadSample,
    Quit,
}

/// The whole front end.
pub struct TextrillApp {
    /// The open document and its file state.
    pub doc: Document,
    /// The live option set. The panel writes straight through
    /// `textrill::cli::set_value`, so this is the same model the engine sees.
    pub opts: Options,
    /// The options form and its filter.
    pub panel: OptionsPanel,
    /// Whether the options dialog is shown (`View → Show options`).
    pub show_options: bool,
    /// Whether the About window is open.
    pub about_open: bool,
    /// The Template Manager, when it is open (roadmap §5.5). Kept in an
    /// `Option` so a test that never opens it pays nothing and sees no change.
    pub templates: Option<crate::templates_window::TemplatesWindow>,
    /// The command held back until the unsaved-changes prompt is answered.
    pending: Option<PendingAction>,
    /// Set once the window has been told to close, so an in-flight
    /// `close_requested` cannot re-open the prompt.
    closing: bool,
    /// Rendered HTML, or the error text after a failed conversion.
    pub output: String,
    /// Status line: `Ready`, `converting…`, a summary, or `error`.
    pub status: String,
    /// The newest conversion requested.
    pub latest: u64,
    /// The newest conversion applied. `latest > completed` means work is out.
    pub completed: u64,
    /// Convert as the text changes, after [`AUTO_CONVERT_DELAY`].
    pub auto: bool,
    worker: Arc<ConversionWorker>,
    waker_installed: bool,
    /// When the debounced conversion is due, if one is pending.
    deadline: Option<Instant>,
    /// The window title last sent to the platform, so it is only sent on
    /// change. Sending it every frame requests a repaint every frame, which
    /// never lets the UI go to sleep.
    title_sent: Option<String>,
    settings: Settings,
    /// The last normal (non-maximized) window size, in points *before* zoom, so
    /// it is unaffected by a change of zoom and can be handed straight back to
    /// `ViewportBuilder::with_inner_size`. Never the maximized size; see
    /// [`crate::window_state`].
    pub window_size: [f32; 2],
    /// Whether the window was maximized the last time it was observed.
    pub window_maximized: bool,
    /// The UI zoom the user last chose.
    pub zoom: f32,
    /// The colour theme, from the window's own [`Theme`] set.
    pub theme: Theme,
    /// The plain-text size multiplier over the built-in type scale.
    pub text_size: f32,
    /// Whether animation is switched off (WCAG `prefers-reduced-motion`).
    pub reduce_motion: bool,
    /// Immersive view: menus and toolbar hidden, leaving the text, a slim
    /// Convert/Save HTML action bar, and the way back.
    pub focus_mode: bool,
    /// Whether the chosen theme/text size/motion has been pushed into the egui
    /// context yet. `draw` does it on the first frame, so the look is applied
    /// from the very first paint both in the window and under `egui_kittest`.
    applied_style: bool,
    /// The file choosers. Tests swap in a stub so the command logic can run
    /// without a desktop portal.
    chooser: Box<dyn Chooser>,
    /// Use the in-app [`FileBrowser`] instead of the desktop's own chooser.
    ///
    /// Defaults on when no usable native dialog is present (see
    /// [`dialogs::system_dialog_available`]), which is what makes a native
    /// install on a portal-less desktop work. The user can flip it either way
    /// from the View menu, and the choice is remembered.
    use_builtin_browser: bool,
    /// Whether [`dialogs::system_dialog_available`] reported a usable desktop
    /// dialog when the app started. Cached so the status line can explain a
    /// chooser that never appeared without probing the filesystem per frame.
    native_dialog_available: bool,
    /// Whether this process runs inside a Flatpak sandbox
    /// ([`dialogs::in_flatpak`]), decided once at startup. In a Flatpak the
    /// portal is always present, so the app always uses it and never offers the
    /// built-in browser, which the sandbox's lack of home access would leave
    /// staring at an empty tree.
    in_flatpak: bool,
    /// The built-in browser, when one is open.
    file_browser: Option<FileBrowser>,
    /// The directory the last open or save used, so the next dialog starts
    /// there (or in the user's Documents) instead of wherever the input was
    /// launched from. `None` until one is recorded or loaded from settings.
    last_dir: Option<PathBuf>,
    /// The key of the chosen preset ([`crate::presets`]). A starting point, not
    /// a mode: the options it set are the live ones, and the window shows a
    /// "Modified" marker as soon as they are edited away from the preset's
    /// bundle. Owned because a user preset's key comes from a file.
    preset: String,
    /// The user's own presets, read from `presets.toml` (roadmap §8.1) and
    /// offered in the strip beside the shipped starting points. The engine
    /// reads the same file for `--preset`, so both paths apply the same
    /// bundles.
    user_presets: Vec<textrill::presets::Preset>,
    /// The source/preview split ratio: the fraction of the working area given
    /// to the source pane, the rest to the preview. Clamped in the UI.
    pub split_ratio: f32,
    /// Which view the main working area shows (split, source-only, …).
    pub view_mode: crate::settings::ViewMode,
    /// How tightly the interface is packed.
    pub density: crate::settings::Density,
    /// While the split handle is dragged: the pointer x and split ratio where
    /// the drag began, so the ratio tracks the pointer without running away.
    drag_start: Option<(f32, f32)>,
    /// The `--report` counts of the last produced HTML, cached when a conversion
    /// lands so the status bar never rescans a large document every frame.
    counts: textrill::report::Counts,
    /// The source editor's caret, as 1-based (line, column); `None` when the
    /// editor has never been focused this run.
    cursor: Option<(usize, usize)>,
    /// The output before the most recent successful conversion, for the
    /// rendered diff. Reset when a different document is loaded.
    previous_output: Option<String>,
    /// The problems from the last failed conversion (roadmap §7, workstream
    /// 4). Empty after a successful one.
    problems: Vec<crate::problems::Problem>,
    /// Recently opened files, most recent first (roadmap §7, workstream 5).
    recent: Vec<PathBuf>,
    /// Whether the last document is reopened on launch. Opt-in.
    restore_session: bool,
    /// A caret position requested by the outline, applied the next time the
    /// source editor draws (an outline click switches to that view).
    pending_caret: Option<usize>,
    /// Whether the Ctrl/Cmd+K command palette is open.
    palette_open: bool,
    /// What has been typed into the palette's search box.
    palette_query: String,
    /// Which row of the filtered palette list is highlighted.
    palette_selected: usize,
    /// Set the frame the palette opens so its search box takes the keyboard;
    /// cleared once it has. Without this the palette opens but the first
    /// keystroke goes to whatever had focus before it.
    palette_focus: bool,
    /// Whether the search box held focus at the end of the previous frame.
    /// Enter hands focus back on the very frame it is pressed, so the current
    /// frame's focus would always read `false` and swallow the key; last
    /// frame's is what actually says the user was typing in the palette.
    palette_search_focused: bool,
}

impl Default for TextrillApp {
    fn default() -> Self {
        Self::with_settings(Settings::from_default())
    }
}

/// Lay out a menu's contents inside a viewport-height scroll area.
///
/// Menus used to size themselves to their contents. At a high zoom or in a
/// short window the lower entries (the whole display section in View) were
/// placed below the screen edge, with no way to reach them. A menu is a popup
/// at a known screen position, so the remaining space below it is the honest
/// limit: fit the items that fit, and scroll the rest.
fn scrollable_menu<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let screen = ui.ctx().content_rect();
    let max_height = (screen.bottom() - ui.cursor().top() - 8.0).max(80.0);
    // A popup's Ui starts with egui's default area size (400 points tall), and
    // a ScrollArea cannot grow past its parent. Expand the menu to the real
    // screen room first, or the menu is simply clipped at that default.
    ui.set_max_height(max_height);
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .show(ui, add_contents)
        .inner
}

impl TextrillApp {
    /// Build an app backed by `settings`, applying the stored `auto` choice.
    ///
    /// Tests pass a [`Settings`] in a temporary directory so a developer's real
    /// configuration cannot change what the tests observe.
    pub fn with_settings(settings: Settings) -> Self {
        let in_flatpak = dialogs::in_flatpak();
        // In a Flatpak the FileChooser portal is always reachable, so treat a
        // dialog as available even when the filesystem probe cannot see the
        // host's `.portal` files.
        let native_dialog_available = dialogs::system_dialog_available() || in_flatpak;
        let mut app = Self {
            doc: Document::new(),
            opts: Options::default(),
            panel: OptionsPanel::default(),
            show_options: false,
            about_open: false,
            templates: None,
            pending: None,
            closing: false,
            output: String::new(),
            status: "Ready".to_string(),
            latest: 0,
            completed: 0,
            auto: true,
            worker: ConversionWorker::new(2),
            waker_installed: false,
            deadline: None,
            title_sent: None,
            settings,
            window_size: crate::window_state::DEFAULT_SIZE,
            window_maximized: false,
            zoom: 1.0,
            theme: Theme::default(),
            text_size: theme::DEFAULT_TEXT_SIZE,
            reduce_motion: false,
            focus_mode: false,
            applied_style: false,
            chooser: Box::new(NativeChooser),
            use_builtin_browser: !native_dialog_available,
            native_dialog_available,
            in_flatpak,
            preset: presets::default_preset().key.to_string(),
            user_presets: Vec::new(),
            file_browser: None,
            last_dir: None,
            split_ratio: 0.5,
            view_mode: crate::settings::ViewMode::default(),
            density: crate::settings::Density::default(),
            drag_start: None,
            counts: textrill::report::Counts::default(),
            cursor: None,
            previous_output: None,
            problems: Vec::new(),
            recent: Vec::new(),
            restore_session: false,
            pending_caret: None,
            palette_open: false,
            palette_query: String::new(),
            palette_selected: 0,
            palette_focus: false,
            palette_search_focused: false,
        };
        // Presets load before the stored key resolves: a stored user preset
        // key must find its entry rather than falling back to Plain.
        app.load_user_presets_default();
        app.restore_settings();
        app.restore_last_document();
        app
    }

    /// Replace the file choosers. Tests use this to answer prompts without a
    /// desktop portal. It also routes file commands to the stub (rather than
    /// the built-in browser), so a stub always answers them.
    pub fn set_chooser(&mut self, chooser: Box<dyn Chooser>) {
        self.chooser = chooser;
        self.use_builtin_browser = false;
        // A stub always answers, so no "no dialog" note applies to it.
        self.native_dialog_available = true;
    }

    /// Whether file commands use the built-in browser rather than the desktop
    /// chooser.
    pub fn uses_builtin_browser(&self) -> bool {
        self.use_builtin_browser
    }

    /// Override the cached result of the startup dialog probe. Tests use this to
    /// exercise the "no dialog" status note on a host that does have one.
    pub fn set_native_dialog_available(&mut self, available: bool) {
        self.native_dialog_available = available;
    }

    /// Whether the built-in browser may be offered here. It is not in a
    /// Flatpak: the manifest grants no home access to browse, and the portal is
    /// always present instead, so the app uses the portal and this choice is
    /// withheld rather than shown as a dead end.
    pub fn builtin_browser_available(&self) -> bool {
        !self.in_flatpak
    }

    /// Override the Flatpak detection. Tests use this to exercise the
    /// portal-only path without an actual sandbox.
    pub fn set_in_flatpak(&mut self, in_flatpak: bool) {
        self.in_flatpak = in_flatpak;
        if in_flatpak {
            self.use_builtin_browser = false;
        }
    }

    /// Whether the built-in browser is currently on screen.
    pub fn is_browsing(&self) -> bool {
        self.file_browser.is_some()
    }

    /// The directory the built-in browser is showing, if one is open. For
    /// tests that pin where a file dialog starts.
    pub fn browser_directory(&self) -> Option<&std::path::Path> {
        self.file_browser.as_ref().map(FileBrowser::directory)
    }

    /// Choose which chrome file commands use: `true` for the built-in browser,
    /// `false` for the desktop's own chooser. The choice is remembered.
    pub fn set_builtin_browser(&mut self, builtin: bool) {
        self.use_builtin_browser = builtin;
        if let Err(error) = self.settings.set_builtin_browser(builtin) {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// Show `mode` in the main working area and remember the choice.
    pub fn set_view_mode(&mut self, mode: crate::settings::ViewMode) {
        if self.view_mode != mode {
            self.view_mode = mode;
            self.store_layout_settings();
        }
    }

    /// How tightly the interface is packed, for the layout code.
    pub fn view_mode(&self) -> crate::settings::ViewMode {
        self.view_mode
    }

    /// The chosen preset's key, for tests and the settings store.
    pub fn preset_key(&self) -> &str {
        &self.preset
    }

    /// The user presets read from `presets.toml` (empty when the file is
    /// missing or refused, in which case the refusal is the window's status).
    pub fn user_presets(&self) -> &[textrill::presets::Preset] {
        &self.user_presets
    }

    /// Read the user presets file from its config location. A broken file is
    /// reported in the status line and yields no presets rather than a
    /// half-applied strip.
    pub fn load_user_presets_default(&mut self) {
        match textrill::presets::load() {
            Ok(list) => self.user_presets = list,
            Err(error) => {
                self.user_presets.clear();
                self.status = format!("presets: {error}");
            }
        }
    }

    /// Read `path` as the user presets file, replacing the current list.
    /// Tests use this to point at a scratch file.
    pub fn load_user_presets(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.user_presets = textrill::presets::load_from(path)?;
        Ok(())
    }

    /// The chosen preset's name: the built-in one, the user preset's own name,
    /// or Plain when the key resolves to neither (a deleted file, a
    /// hand-edited settings line).
    pub fn current_preset_name(&self) -> String {
        if let Some(builtin) = presets::get(&self.preset) {
            return builtin.name.to_string();
        }
        self.user_presets
            .iter()
            .find(|p| p.key == self.preset)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| presets::default_preset().name.to_string())
    }

    /// The option bundle the chosen preset stands for: the built-in's, the
    /// user preset's, or Plain's, so the Modified marker compares against
    /// whatever the key actually names.
    pub fn current_preset_options(&self) -> Options {
        if let Some(builtin) = presets::get(&self.preset) {
            return builtin.options();
        }
        self.user_presets
            .iter()
            .find(|p| p.key == self.preset)
            .map(|p| p.options.clone())
            .unwrap_or_else(|| presets::default_preset().options())
    }

    /// Whether the live options differ from the chosen preset's bundle. This is
    /// the "Modified" marker; a preset is a starting point, so editing any
    /// option away from it is normal, not an error.
    pub fn preset_is_modified(&self) -> bool {
        !presets::same_options(&self.opts, &self.current_preset_options())
    }

    /// Adopt `preset`: write its option bundle and remember the choice. The
    /// options it set stay editable, so this is a starting point rather than a
    /// lock.
    pub fn apply_preset(&mut self, preset: &Preset) {
        self.adopt_preset(preset.key, preset.options());
    }

    /// Adopt a user preset by key, exactly as `--preset` would: the bundle
    /// replaces the option state, so the GUI and the command line agree.
    pub fn apply_user_preset(&mut self, key: &str) -> Result<(), String> {
        let options = self
            .user_presets
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.options.clone())
            .ok_or_else(|| {
                format!(
                    "no preset `{key}`; the file has: {}",
                    self.user_presets
                        .iter()
                        .map(|p| p.key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        self.adopt_preset(key, options);
        Ok(())
    }

    /// The shared adopt behind both preset kinds.
    fn adopt_preset(&mut self, key: &str, options: Options) {
        self.opts = options;
        self.preset = key.to_string();
        if let Err(error) = self.settings.set_preset(key) {
            self.status = format!("could not save settings: {error}");
        }
        self.schedule_convert(Instant::now());
    }

    /// Open the Template Manager (roadmap §5.5), discovering the catalog fresh
    /// so a pack dropped into the project since the window was last open shows
    /// up immediately.
    pub fn open_templates(&mut self) {
        match &mut self.templates {
            Some(window) => window.refresh(&self.opts),
            None => {
                let mut window = crate::templates_window::TemplatesWindow::new();
                window.refresh(&self.opts);
                self.templates = Some(window);
            }
        }
    }

    /// Select `address` (`pack-id:name`) as the conversion's template: write
    /// `--template_library`, clear the other two template fields the engine
    /// refuses to combine with it, and convert.
    pub fn apply_template(&mut self, address: &str) {
        self.opts.template_library = address.to_string();
        self.opts.template.clear();
        self.opts.document_template.clear();
        self.schedule_convert(Instant::now());
    }

    /// The options a conversion actually uses.
    ///
    /// `meta_charset` is forced on: the engine defaults it off so no golden
    /// moves, which is right for a byte-compatible CLI, but the GUI always
    /// writes UTF-8 and hands the file to a browser, which guesses wrong
    /// without a declaration.
    pub fn conversion_options(&self) -> Options {
        let mut opts = self.opts.clone();
        opts.meta_charset = true;
        opts
    }

    /// Apply the persisted `auto` choice, option set and window geometry, if
    /// there are any. Everything read back is clamped or defaulted, so a
    /// hand-edited file can never stop the window from opening.
    pub fn restore_settings(&mut self) {
        if let Some(auto) = self.settings.auto() {
            self.auto = auto;
        }
        if let Some(json) = self.settings.options() {
            crate::options_store::decode(&json, &mut self.opts);
        }
        self.window_size = self
            .settings
            .window_size()
            .unwrap_or(crate::window_state::DEFAULT_SIZE);
        self.window_maximized = self.settings.maximized().unwrap_or(false);
        self.zoom = self.settings.zoom().unwrap_or(1.0);
        if let Some(theme) = self.settings.theme() {
            self.theme = theme;
        }
        if let Some(text_size) = self.settings.text_size() {
            self.text_size = text_size;
        }
        if let Some(reduce_motion) = self.settings.reduce_motion() {
            self.reduce_motion = reduce_motion;
        }
        if let Some(builtin) = self.settings.builtin_browser() {
            // A remembered choice cannot re-enable the built-in browser inside
            // a Flatpak, where it has no home access to browse.
            self.use_builtin_browser = builtin && !self.in_flatpak;
        }
        self.last_dir = self.settings.last_dir();
        if let Some(key) = self.settings.preset() {
            // An unreadable key is Plain, never a dangling choice: unknown
            // here means neither a built-in nor a user preset's key.
            let known =
                presets::get(&key).is_some() || self.user_presets.iter().any(|p| p.key == key);
            self.preset = if known {
                key
            } else {
                presets::default_preset().key.to_string()
            };
        }
        if let Some(ratio) = self.settings.split_ratio() {
            self.split_ratio = ratio;
        }
        if let Some(mode) = self.settings.view_mode() {
            self.view_mode = mode;
        }
        if let Some(density) = self.settings.density() {
            self.density = density;
        }
        self.recent = self.settings.recent_files();
        self.restore_session = self.settings.restore_session().unwrap_or(false);
    }

    /// Reopen the document from the previous run, when the user opted in.
    ///
    /// This runs once, on an empty fresh buffer: there is no unsaved text to
    /// clobber, and a missing file is a status note rather than a failure.
    fn restore_last_document(&mut self) {
        if !self.restore_session || !self.doc.text.is_empty() || self.doc.dirty {
            return;
        }
        let Some(path) = self.settings.session_document() else {
            return;
        };
        if !path.is_file() {
            self.status = format!("the last document is gone: {}", path.display());
            return;
        }
        match self.load_file(&path) {
            Ok(()) => self.status = format!("restored {}", path.display()),
            Err(error) => self.status = error,
        }
    }

    /// Persist the layout choices the main panes own (split ratio, view mode,
    /// density).
    fn store_layout_settings(&mut self) {
        if let Err(error) =
            self.settings
                .store_layout(self.split_ratio, self.view_mode, self.density)
        {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// Persist the display choices the user made (theme, text size, motion).
    fn store_display_settings(&mut self) {
        if let Err(error) =
            self.settings
                .store_display(self.theme, self.text_size, self.reduce_motion)
        {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// Put the chosen theme, text size and motion into effect on `ctx`.
    ///
    /// Called once at startup (so the window never flashes at the stock look)
    /// and again whenever a control changes one of the three.
    pub fn apply_ui_style(&self, ctx: &egui::Context) {
        theme::apply(ctx, self.theme, self.text_size, self.reduce_motion);
    }

    /// Persist the current `auto` choice and option set.
    pub fn store_settings(&mut self) {
        let json = crate::options_store::encode(&self.opts);
        let stored = self
            .settings
            .set_auto(self.auto)
            .and_then(|()| self.settings.set_options(&json));
        if let Err(error) = stored {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// Persist the window size, maximized flag and zoom in one atomic write.
    ///
    /// The stored size is always the last *normal* size, never the size a
    /// maximized window happens to have, so un-maximizing on the next launch
    /// gives back a window the user can still grab.
    pub fn store_window_state(&mut self) {
        let stored =
            self.settings
                .set_window_state(self.window_size, self.window_maximized, self.zoom);
        if let Err(error) = stored {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// Persist everything the window owns: the option set, the layout and the
    /// geometry.
    fn persist(&mut self) {
        self.store_settings();
        self.store_layout_settings();
        self.store_window_state();
    }

    /// Set the whole-UI zoom, clamped and rounded to the 10% steps the
    /// shortcuts and menu use. Centralised so the menu, keyboard and persisted
    /// value all speak the same range ([`crate::window_state::MIN_ZOOM`] to
    /// [`crate::window_state::MAX_ZOOM`]) rather than egui's wider defaults.
    pub fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        let zoom = crate::window_state::clamp_zoom((zoom * 10.0).round() / 10.0);
        self.zoom = zoom;
        ctx.set_zoom_factor(zoom);
    }

    /// One step larger, stopping at [`crate::window_state::MAX_ZOOM`].
    pub fn zoom_in(&mut self, ctx: &egui::Context) {
        self.set_zoom(ctx, ctx.zoom_factor() + crate::window_state::ZOOM_STEP);
    }

    /// One step smaller, stopping at [`crate::window_state::MIN_ZOOM`].
    pub fn zoom_out(&mut self, ctx: &egui::Context) {
        self.set_zoom(ctx, ctx.zoom_factor() - crate::window_state::ZOOM_STEP);
    }

    /// `View → Reset window size and zoom`: return to the size a first run
    /// opens at, at 100%, and forget the stored geometry so the reset survives
    /// a restart too. This is the one action that always undoes a geometry the
    /// user does not want.
    pub fn reset_window_state(&mut self, ctx: &egui::Context) {
        self.window_size = crate::window_state::DEFAULT_SIZE;
        self.window_maximized = false;
        self.set_zoom(ctx, 1.0);
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            crate::window_state::DEFAULT_SIZE[0],
            crate::window_state::DEFAULT_SIZE[1],
        )));
        if let Err(error) = self.settings.clear_window_state() {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// Apply the startup choices from the command line, in the order `app.py`
    /// did: open the file, set the dialect/table options, then reconvert if an
    /// option was given so the preview matches straight away.
    pub fn apply_command_line(&mut self, args: &Args) {
        if let Some(file) = &args.file {
            if let Err(error) = self.load_file(file) {
                self.status = error;
            }
        }
        if let Some(xhtml) = args.xhtml {
            let value = if xhtml { "true" } else { "false" };
            let _ = textrill::cli::set_value(&mut self.opts, "xhtml", value);
        }
        if args.tables {
            let _ = textrill::cli::set_value(&mut self.opts, "make_tables", "true");
        }
        if args.xhtml.is_some() || args.tables {
            self.convert_now();
        }
    }

    // ----------------------------------------------------------- converting

    /// Queue a conversion of the current text and options.
    ///
    /// Waiting work is dropped by the worker; a burst of edits leaves the
    /// newest job waiting, not one copy of the document per keystroke.
    pub fn request_conversion(&mut self) {
        let opts = self.conversion_options();
        self.latest = self.worker.convert(&self.doc.text, &opts);
        self.status = "converting…".to_string();
    }

    /// A text edit: mark the document dirty and, when auto is on, debounce.
    pub fn on_edit(&mut self, now: Instant) {
        self.doc.dirty = true;
        if self.auto {
            self.schedule_convert(now);
        }
    }

    /// Queue a conversion, coalescing a burst of changes.
    ///
    /// When auto is off an *option* change still converts immediately, but a
    /// never-converted document does too, so the preview is never empty by
    /// default.
    pub fn schedule_convert(&mut self, now: Instant) {
        if self.auto || self.latest == 0 {
            self.deadline = Some(now + AUTO_CONVERT_DELAY);
        } else {
            self.convert_now();
        }
    }

    /// Convert immediately, cancelling any pending debounce.
    pub fn convert_now(&mut self) {
        self.deadline = None;
        self.request_conversion();
    }

    /// Fire the debounced conversion if its deadline has passed.
    pub fn tick(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline {
            if now >= deadline {
                self.convert_now();
            }
        }
    }

    /// Apply every finished conversion, dropping results older than the newest
    /// request.
    pub fn drain(&mut self) {
        while let Some(outcome) = self.worker.poll() {
            if outcome.generation < self.latest {
                continue; // a newer conversion is already under way
            }
            self.completed = outcome.generation;
            match outcome.error {
                Some(error) => {
                    self.problems = crate::problems::parse(&error);
                    self.output = error;
                    self.status = "error".to_string();
                }
                None => {
                    if let Some(problem) = outcome.problem {
                        // The engine refused the document: no output is a
                        // result. Keep the last good output out of the way and
                        // let the problems panel explain why.
                        self.problems = crate::problems::parse(&problem);
                        self.output.clear();
                        self.status = "error".to_string();
                        continue;
                    }
                    self.problems.clear();
                    let lines = if outcome.html.is_empty() {
                        0
                    } else {
                        outcome.html.matches('\n').count() + 1
                    };
                    if !self.output.is_empty() && self.output != outcome.html {
                        self.previous_output =
                            Some(std::mem::replace(&mut self.output, outcome.html));
                    } else {
                        self.output = outcome.html;
                    }
                    self.doc.note_converted(&self.output);
                    self.counts = textrill::report::Counts::of(&self.output);
                    self.status = format!("{lines} lines · {:.0} ms", outcome.seconds * 1000.0);
                }
            }
        }
    }

    // ----------------------------------------------------------- documents

    /// `File → New`, after the unsaved-changes prompt when there are edits.
    pub fn new_document(&mut self) {
        if self.doc.dirty {
            self.pending = Some(PendingAction::New);
        } else {
            self.do_new();
        }
    }

    fn do_new(&mut self) {
        self.doc.reset();
        self.previous_output = None;
        self.convert_now();
    }

    /// `File → Load example`, after the prompt when there are edits.
    pub fn load_sample(&mut self) {
        if self.doc.dirty {
            self.pending = Some(PendingAction::LoadSample);
        } else {
            self.do_load_sample();
        }
    }

    fn do_load_sample(&mut self) {
        self.doc.load_sample();
        self.previous_output = None;
        self.convert_now();
    }

    /// Open `path`, remembering its encoding. Never overwrites the text on
    /// failure.
    pub fn load_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.doc.load(path)?;
        self.previous_output = None;
        self.remember_recent(path);
        self.convert_now();
        Ok(())
    }

    /// `File → Save text`. Uses the opened path, or reports the one to propose.
    pub fn save_text(&mut self) -> Result<SaveRequest, String> {
        match self.doc.path.clone() {
            Some(path) => {
                self.doc.save_text_to(&path)?;
                self.remember_recent(&path);
                Ok(SaveRequest::Saved(path))
            }
            None => Ok(SaveRequest::NeedsName(self.doc.suggested_text_name())),
        }
    }

    /// `File → Save text`, to an explicit path.
    pub fn save_text_to(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.doc.save_text_to(path)
    }

    /// `File → Save`: write the HTML, never over the text file being converted.
    pub fn save_document(&mut self) -> Result<SaveRequest, String> {
        match self.doc.output_path.clone() {
            Some(path) => {
                self.save_html_to(&path)?;
                Ok(SaveRequest::Saved(path))
            }
            None => Ok(SaveRequest::NeedsName(self.doc.suggested_output_name())),
        }
    }

    /// `File → Save As`, to an explicit path.
    pub fn save_html_to(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.doc.save_html_to(path, &self.output)
    }

    /// The name `Save As` should propose.
    pub fn suggested_output_name(&self) -> PathBuf {
        self.doc.suggested_output_name()
    }

    // ------------------------------------------------------------- commands

    /// `Edit → Reset options`.
    pub fn reset_options(&mut self) {
        self.opts = Options::default();
        self.schedule_convert(Instant::now());
    }

    /// `Edit → Copy HTML`.
    pub fn copy_html(&mut self, ctx: &egui::Context) {
        ctx.copy_text(self.output.clone());
        self.status = "HTML copied to the clipboard".to_string();
    }

    /// The recent-files list, most recent first.
    pub fn recent_files(&self) -> &[PathBuf] {
        &self.recent
    }

    /// Open the `index`th recent file, asking about unsaved text first.
    pub fn open_recent(&mut self, index: usize) {
        let Some(path) = self.recent.get(index).cloned() else {
            return;
        };
        if self.doc.dirty {
            self.pending = Some(PendingAction::OpenRecent(path));
        } else {
            self.load_or_report(path);
        }
    }

    /// Move `path` to the front of the recent list (bounded) and remember it as
    /// the session document. Settings failures are status notes, not errors:
    /// the document itself opened or saved fine.
    fn remember_recent(&mut self, path: &std::path::Path) {
        let path = path.to_path_buf();
        self.recent.retain(|recent| recent != &path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(10);
        let mut stored = self.settings.set_recent_files(&self.recent);
        if stored.is_ok() {
            stored = self.settings.set_session_document(&path);
        }
        if let Err(error) = stored {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// `File → Open…`: ask about unsaved text, then choose a file.
    pub fn open_document(&mut self) {
        if self.doc.dirty {
            self.pending = Some(PendingAction::Open);
        } else {
            self.do_open();
        }
    }

    /// The directory a file dialog should open in. A freshly opened document
    /// wins, so saving stays beside its source; otherwise the last directory
    /// used is remembered, and failing that the user's Documents. This is what
    /// stops an untitled conversion (the sample, a paste) from dropping its
    /// output in whatever transient folder the app happened to start in.
    fn file_dialog_dir(&self) -> PathBuf {
        if let Some(dir) = self
            .doc
            .path
            .as_deref()
            .and_then(|path| path.parent())
            .filter(|dir| dir.is_dir())
        {
            return dir.to_path_buf();
        }
        if let Some(dir) = self.last_dir.as_ref().filter(|dir| dir.is_dir()) {
            return dir.clone();
        }
        documents_dir()
    }

    /// The directory `Save As` should open in.
    ///
    /// The already-written HTML's directory wins (a second `Save As` returns to
    /// where the HTML last went), then the source file's, then the same
    /// recent/Documents place an open starts in. [`Self::file_dialog_dir`] alone
    /// would always use the source's directory and forget an output location
    /// the user chose.
    fn save_html_dir(&self) -> PathBuf {
        self.doc
            .output_path
            .as_deref()
            .or(self.doc.path.as_deref())
            .and_then(std::path::Path::parent)
            .filter(|dir| dir.is_dir())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| self.file_dialog_dir())
    }

    /// Remember `path`'s directory for the next file dialog, and persist it.
    fn remember_dir(&mut self, path: &std::path::Path) {
        let Some(dir) = path.parent().filter(|dir| dir.is_dir()) else {
            return;
        };
        self.last_dir = Some(dir.to_path_buf());
        if let Err(error) = self.settings.set_last_dir(dir) {
            self.status = format!("could not save settings: {error}");
        }
    }

    /// Explain a `None` from the native chooser when no dialog was expected to
    /// appear at all.
    ///
    /// A `None` is normally the user cancelling, and stays silent. But when the
    /// built-in browser is off on a desktop [`dialogs::system_dialog_available`]
    /// reported as having no file dialog, a `None` almost certainly means the
    /// dialog never appeared — a missing portal daemon is indistinguishable from
    /// a cancel to `rfd` — so say how to get a chooser back. This is a
    /// best-effort heuristic, not a live probe.
    fn note_if_no_dialog(&mut self) {
        if !self.use_builtin_browser && !self.native_dialog_available {
            self.status =
                "No file dialog is available here; turn on View → Use built-in file browser."
                    .to_string();
        }
    }

    fn do_open(&mut self) {
        let dir = self.file_dialog_dir();
        if self.use_builtin_browser {
            self.file_browser = Some(FileBrowser::for_open_in(dir));
        } else if let Some(path) = self.chooser.open_text(Some(&dir)) {
            self.load_or_report(path);
        } else {
            self.note_if_no_dialog();
        }
    }

    /// Load `path`, reporting any failure on the status line.
    fn load_or_report(&mut self, path: PathBuf) {
        match self.load_file(&path) {
            Ok(()) => self.remember_dir(&path),
            Err(error) => self.status = error,
        }
    }

    /// Write the HTML to `path`, reporting the result on the status line.
    fn save_html_or_report(&mut self, path: PathBuf) {
        match self.save_html_to(&path) {
            Ok(()) => {
                self.status = format!("saved {}", path.display());
                self.remember_dir(&path);
            }
            Err(error) => self.status = error,
        }
    }

    /// Write the text to `path`, reporting the result on the status line.
    fn save_text_or_report(&mut self, path: PathBuf) {
        match self.save_text_to(&path) {
            Ok(()) => {
                self.status = format!("saved text to {}", path.display());
                self.remember_dir(&path);
            }
            Err(error) => self.status = error,
        }
    }

    /// `File → Save`: write the HTML where it is already going, or ask.
    pub fn save_document_command(&mut self) {
        if self.doc.output_path.is_some() {
            self.report_document();
        } else {
            self.save_document_as_command();
        }
    }

    /// `File → Save As…`: always ask where the HTML should go.
    pub fn save_document_as_command(&mut self) {
        let dir = self.save_html_dir();
        let name = self
            .suggested_output_name()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output.html".to_string());
        if self.use_builtin_browser {
            self.file_browser = Some(FileBrowser::for_save_in(FileIntent::SaveHtml, dir, name));
        } else {
            let suggested = dir.join(name);
            if let Some(path) = self.chooser.save_html(&suggested) {
                self.save_html_or_report(path);
            } else {
                self.note_if_no_dialog();
            }
        }
    }

    /// `File → Save text…`: write the text to the opened path, or ask.
    pub fn save_text_command(&mut self) {
        match self.doc.path.clone() {
            Some(path) => self.save_text_or_report(path),
            None => {
                let dir = self.file_dialog_dir();
                let name = self
                    .doc
                    .suggested_text_name()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "untitled.txt".to_string());
                if self.use_builtin_browser {
                    self.file_browser =
                        Some(FileBrowser::for_save_in(FileIntent::SaveText, dir, name));
                } else {
                    let suggested = dir.join(name);
                    if let Some(path) = self.chooser.save_text(&suggested) {
                        self.save_text_or_report(path);
                    } else {
                        self.note_if_no_dialog();
                    }
                }
            }
        }
    }

    fn report_document(&mut self) {
        match self.save_document() {
            Ok(SaveRequest::Saved(path)) => self.status = format!("saved {}", path.display()),
            Ok(SaveRequest::NeedsName(path)) => {
                self.status = format!("no output path for {}", path.display());
            }
            Err(error) => self.status = error,
        }
    }

    /// `File → Quit`. Asks about unsaved text first, like any editor.
    pub fn quit(&mut self, ctx: &egui::Context) {
        if self.doc.dirty {
            self.pending = Some(PendingAction::Quit);
        } else {
            self.closing = true;
            self.persist();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Whether the unsaved-changes prompt is on screen.
    pub fn is_prompting(&self) -> bool {
        self.pending.is_some()
    }

    /// Answer the unsaved-changes prompt.
    ///
    /// `Save` writes the *text* (what the prompt is about), never the HTML, and
    /// only carries out the waiting command if the write actually happened. A
    /// `Save` that has nowhere to write is the same as the Python dialog being
    /// cancelled: the command is dropped, not the edits.
    pub fn resolve_save_prompt(&mut self, answer: SaveAnswer, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else {
            return;
        };
        match answer {
            SaveAnswer::Cancel => self.pending = None,
            SaveAnswer::Discard => self.perform(action, ctx),
            SaveAnswer::Save => {
                self.save_text_command();
                if self.file_browser.is_some() {
                    // A built-in browser is asking for a name. It will carry
                    // the waiting command out once it has one, so `pending`
                    // stays set and the prompt is hidden behind the browser.
                } else if self.doc.dirty {
                    // Cancelled or failed: drop the waiting command, keep edits.
                    self.pending = None;
                } else {
                    self.perform(action, ctx);
                }
            }
        }
    }

    /// Carry out a command the prompt has cleared.
    fn perform(&mut self, action: PendingAction, ctx: &egui::Context) {
        self.pending = None;
        match action {
            PendingAction::New => self.do_new(),
            PendingAction::Open => self.do_open(),
            PendingAction::OpenRecent(path) => self.load_or_report(path),
            PendingAction::LoadSample => self.do_load_sample(),
            PendingAction::Quit => {
                self.persist();
                self.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    // ------------------------------------------------------------------ ui

    /// Remember the current window geometry so it can be saved on exit.
    ///
    /// The size is recorded in points *before* zoom (`viewport * zoom`), which
    /// is the value `ViewportBuilder::with_inner_size` takes on the next
    /// launch. While maximized the last normal size is deliberately kept:
    /// storing the screen-sized rectangle would restore a window with no
    /// resize grips. `viewport_rect` is used rather than the viewport's
    /// `inner_rect` because the latter is `None` on Wayland.
    fn note_window_state(&mut self, ctx: &egui::Context) {
        let zoom = ctx.zoom_factor();
        self.zoom = zoom;
        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
        self.window_maximized = maximized;
        if !maximized {
            let size = ctx.viewport_rect().size() * zoom;
            if size.x >= 1.0 && size.y >= 1.0 {
                self.window_size = [size.x, size.y];
            }
        }
    }

    /// Send the window title, but only when it changes.
    fn set_title(&mut self, ctx: &egui::Context) {
        let title = self.doc.title();
        if self.title_sent.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_sent = Some(title);
        }
    }

    /// Act on the browser-style zoom chords. Kept apart from the other
    /// shortcuts because zoom stays available while a prompt owns the document
    /// commands, and because egui's built-in handler must not see the keys: it
    /// steps to its own wider 20%-500% range, which this app would clamp on
    /// save and reload.
    fn handle_zoom_shortcuts(&mut self, ctx: &egui::Context) {
        let (zoom_in, zoom_in_secondary, zoom_out, zoom_reset) = ctx.input_mut(|input| {
            (
                input.consume_shortcut(&egui::gui_zoom::kb_shortcuts::ZOOM_IN),
                input.consume_shortcut(&egui::gui_zoom::kb_shortcuts::ZOOM_IN_SECONDARY),
                input.consume_shortcut(&egui::gui_zoom::kb_shortcuts::ZOOM_OUT),
                input.consume_shortcut(&egui::gui_zoom::kb_shortcuts::ZOOM_RESET),
            )
        });
        // Ctrl/Cmd+0 resets only the zoom; the window geometry has its own
        // Ctrl+Alt+0 escape hatch in `RESET_WINDOW_SHORTCUT`.
        if zoom_reset {
            self.set_zoom(ctx, 1.0);
        } else {
            if zoom_in || zoom_in_secondary {
                self.zoom_in(ctx);
            }
            if zoom_out {
                self.zoom_out(ctx);
            }
        }
    }

    /// Act on the keyboard shortcuts the menu advertises.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (
            mut new,
            mut save,
            mut save_text,
            mut convert,
            mut copy,
            mut quit,
            mut reset_window,
            mut focus,
            mut view_split,
            mut view_source,
            mut view_html,
            mut view_preview,
            mut view_outline,
            mut view_diff,
            mut palette,
        ) = (
            false, false, false, false, false, false, false, false, false, false, false, false,
            false, false, false,
        );
        ctx.input_mut(|input| {
            new = input.consume_shortcut(&NEW_SHORTCUT);
            save_text = input.consume_shortcut(&SAVE_TEXT_SHORTCUT);
            save = input.consume_shortcut(&SAVE_SHORTCUT);
            convert = input.consume_shortcut(&CONVERT_SHORTCUT);
            copy = input.consume_shortcut(&COPY_HTML_SHORTCUT);
            quit = input.consume_shortcut(&QUIT_SHORTCUT);
            reset_window = input.consume_shortcut(&RESET_WINDOW_SHORTCUT);
            focus = input.consume_shortcut(&FOCUS_SHORTCUT);
            view_split = input.consume_shortcut(&VIEW_SPLIT_SHORTCUT);
            view_source = input.consume_shortcut(&VIEW_SOURCE_SHORTCUT);
            view_html = input.consume_shortcut(&VIEW_HTML_SHORTCUT);
            view_preview = input.consume_shortcut(&VIEW_PREVIEW_SHORTCUT);
            view_outline = input.consume_shortcut(&VIEW_OUTLINE_SHORTCUT);
            view_diff = input.consume_shortcut(&VIEW_DIFF_SHORTCUT);
            palette = input.consume_shortcut(&PALETTE_SHORTCUT);
        });
        if palette {
            self.toggle_palette();
        }
        if reset_window {
            self.reset_window_state(ctx);
        }
        if focus {
            self.focus_mode = !self.focus_mode;
        }
        if view_split {
            self.set_view_mode(crate::settings::ViewMode::Split);
        }
        if view_source {
            self.set_view_mode(crate::settings::ViewMode::Source);
        }
        if view_html {
            self.set_view_mode(crate::settings::ViewMode::Html);
        }
        if view_preview {
            self.set_view_mode(crate::settings::ViewMode::Preview);
        }
        if view_outline {
            self.set_view_mode(crate::settings::ViewMode::Outline);
        }
        if view_diff {
            self.set_view_mode(crate::settings::ViewMode::Diff);
        }
        if new {
            self.new_document();
        }
        if save {
            self.save_document_command();
        }
        if save_text {
            self.save_text_command();
        }
        if convert {
            self.convert_now();
        }
        if copy {
            self.copy_html(ctx);
        }
        if quit {
            self.quit(ctx);
        }
    }

    /// Open the command palette fresh, or close an open one. Resetting the
    /// query and row on open is what makes the chord feel like a stateless
    /// "type a command" box rather than a window that remembers last week's
    /// filter.
    pub fn toggle_palette(&mut self) {
        self.palette_open = !self.palette_open;
        if self.palette_open {
            self.palette_query.clear();
            self.palette_selected = 0;
            self.palette_focus = true;
            self.palette_search_focused = false;
        }
    }

    /// Open the first file dropped onto the window, as the Python window did.
    fn accept_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        // Clear them, or the drop fires again on every frame.
        ctx.input_mut(|input| input.raw.dropped_files.clear());
        if let Some(path) = dropped.first() {
            // A pack folder or `.tar` dropped while the Template Manager is open
            // installs there (roadmap §5.5); anything else loads as a document.
            let is_pack = path.is_dir()
                || path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("tar"));
            if is_pack {
                if let Some(window) = self.templates.as_mut() {
                    window.install_from(path);
                    return;
                }
            }
            if let Err(error) = self.load_file(path) {
                self.status = error;
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                scrollable_menu(ui, |ui| {
                    if ui.button("New").clicked() {
                        self.new_document();
                        ui.close();
                    }
                    if ui.button("Open…").clicked() {
                        self.open_document();
                        ui.close();
                    }
                    let mut chosen = None;
                    ui.menu_button("Recent files", |ui| {
                        scrollable_menu(ui, |ui| {
                            if self.recent.is_empty() {
                                ui.weak("No recent files yet.");
                            }
                            for (index, path) in self.recent.iter().enumerate() {
                                let label = format!("{}", path.display());
                                if ui.button(label).clicked() {
                                    chosen = Some(index);
                                    ui.close();
                                }
                            }
                            if !self.recent.is_empty() {
                                ui.separator();
                                if ui.button("Clear recent files").clicked() {
                                    self.recent.clear();
                                    if let Err(error) = self.settings.set_recent_files(&self.recent) {
                                        self.status = format!("could not save settings: {error}");
                                    }
                                    ui.close();
                                }
                            }
                        });
                    });
                    if let Some(index) = chosen {
                        self.open_recent(index);
                        ui.close();
                    }
                    let mut restore = self.restore_session;
                    if ui
                        .checkbox(&mut restore, "Restore the last document on launch")
                        .on_hover_text(
                            "Opt-in. On the next launch the last opened file is reopened                          before there is any unsaved text to overwrite.",
                        )
                        .changed()
                    {
                        self.restore_session = restore;
                        if let Err(error) = self.settings.set_restore_session(restore) {
                            self.status = format!("could not save settings: {error}");
                        }
                    }
                    if ui.button("Save").clicked() {
                        self.save_document_command();
                        ui.close();
                    }
                    if ui.button("Save As…").clicked() {
                        self.save_document_as_command();
                        ui.close();
                    }
                    if ui.button("Save text…").clicked() {
                        self.save_text_command();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Load example").clicked() {
                        self.load_sample();
                        ui.close();
                    }
                    if ui.button("Manage templates…").clicked() {
                        self.open_templates();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        self.quit(ui.ctx());
                        ui.close();
                    }
                });
            });
            ui.menu_button("Edit", |ui| {
                scrollable_menu(ui, |ui| {
                    if ui
                        .button("Command palette…")
                        .on_hover_text("Ctrl+K")
                        .clicked()
                    {
                        self.toggle_palette();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Copy HTML").clicked() {
                        self.copy_html(ui.ctx());
                        ui.close();
                    }
                    if ui.button("Convert now").clicked() {
                        self.convert_now();
                        ui.close();
                    }
                    if ui.button("Reset options").clicked() {
                        self.reset_options();
                        ui.close();
                    }
                });
            });
            ui.menu_button("Templates", |ui| {
                scrollable_menu(ui, |ui| {
                    if ui
                        .button("Manage templates…")
                        .on_hover_text("Browse, filter, preview and apply template packs")
                        .clicked()
                    {
                        self.open_templates();
                        ui.close();
                    }
                    if ui
                        .button("Refresh")
                        .on_hover_text("Re-discover packs under the search path")
                        .clicked()
                    {
                        self.open_templates();
                        ui.close();
                    }
                });
            });
            ui.menu_button("View", |ui| {
                scrollable_menu(ui, |ui| {
                    ui.checkbox(&mut self.show_options, "Show options");
                    ui.separator();
                    // Which view the working area shows: side by side, or the
                    // whole area given to one pane. Ctrl+1/2/3 reach the same
                    // choices without the menu.
                    ui.label("Layout");
                    let mode = self.view_mode;
                    for (label, want) in [
                        ("Source and HTML", crate::settings::ViewMode::Split),
                        ("Source only", crate::settings::ViewMode::Source),
                        ("HTML only", crate::settings::ViewMode::Html),
                        ("Preview (rendered)", crate::settings::ViewMode::Preview),
                        ("Outline", crate::settings::ViewMode::Outline),
                        ("Diff (rendered vs. previous)", crate::settings::ViewMode::Diff),
                    ] {
                        if ui
                            .selectable_label(mode == want, label)
                            .clicked()
                        {
                            self.set_view_mode(want);
                            ui.close();
                        }
                    }
                    ui.separator();
                    // The built-in browser is not offered in a Flatpak: with no
                    // home filesystem access it could show only an empty tree, and
                    // the FileChooser portal is always present there instead.
                    if self.builtin_browser_available() {
                        let mut builtin = self.use_builtin_browser;
                        if ui
                            .checkbox(&mut builtin, "Use built-in file browser")
                            .on_hover_text(
                                "Pick files with the in-app browser instead of the \
                                 desktop's dialog. Useful where no file dialog exists \
                                 (for example Sway without xdg-desktop-portal-gtk).",
                            )
                            .changed()
                        {
                            self.set_builtin_browser(builtin);
                        }
                        ui.separator();
                    }
                    ui.checkbox(&mut self.focus_mode, "Focus mode")
                        .on_hover_text("Hide the menus and toolbar, keep the text and a slim action bar (Ctrl+Shift+F)");
                    if ui
                        .checkbox(&mut self.reduce_motion, "Reduce motion")
                        .changed()
                    {
                        self.apply_ui_style(ui.ctx());
                        self.store_display_settings();
                    }
                    // How tightly the controls are packed. Compact gives a denser
                    // layout for a small screen; Comfortable is the default.
                    ui.separator();
                    ui.label(egui::RichText::new("Density").small());
                    let density = self.density;
                    for (label, want) in [
                        ("Comfortable", crate::settings::Density::Comfortable),
                        ("Compact", crate::settings::Density::Compact),
                    ] {
                        if ui.selectable_label(density == want, label).clicked() {
                            self.density = want;
                            self.store_layout_settings();
                        }
                    }
                    // Whole-UI zoom, for the same reason a browser has it: a 4K
                    // laptop and a 96 DPI desktop should not dictate the same
                    // physical text size. The keyboard shortcuts (Ctrl/⌘ + / - / 0)
                    // work too; these buttons make the feature discoverable. They
                    // live in the menu rather than the toolbar, which keeps the
                    // toolbar short. `zoom_factor` multiplies the OS's own scale
                    // factor, so a HiDPI screen is already handled.
                    ui.separator();
                    ui.label(egui::RichText::new("Display").small());
                    ui.horizontal(|ui| {
                        if ui
                            .button("Zoom out")
                            .on_hover_text("Zoom out (Ctrl+-)")
                            .clicked()
                        {
                            self.zoom_out(ui.ctx());
                        }
                        ui.label(format!("{:.0}%", ui.ctx().zoom_factor() * 100.0));
                        if ui
                            .button("Zoom in")
                            .on_hover_text("Zoom in (Ctrl+=)")
                            .clicked()
                        {
                            self.zoom_in(ui.ctx());
                        }
                        if ui
                            .button("Reset zoom")
                            .on_hover_text("Reset to 100% (Ctrl+0)")
                            .clicked()
                        {
                            self.set_zoom(ui.ctx(), 1.0);
                        }
                    });

                    // Plain-text size, in the spirit of a browser's font-size
                    // control: it scales the type and therefore the resting size of
                    // controls with it, but leaves the whole-window zoom above
                    // alone.
                    ui.separator();
                    ui.label(egui::RichText::new("Text").small());
                    let sizes = theme::TEXT_SIZES;
                    let step = sizes
                        .iter()
                        .enumerate()
                        .min_by(|(_, (a, _)), (_, (b, _))| {
                            (a - self.text_size)
                                .abs()
                                .partial_cmp(&(b - self.text_size).abs())
                                .unwrap()
                        })
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(step > 0, egui::Button::new("A−"))
                            .on_hover_text("Smaller text")
                            .clicked()
                        {
                            self.text_size = sizes[step - 1].0;
                            self.apply_ui_style(ui.ctx());
                            self.store_display_settings();
                        }
                        ui.label(format!("{:.0}%", self.text_size * 100.0));
                        if ui
                            .add_enabled(step + 1 < sizes.len(), egui::Button::new("A+"))
                            .on_hover_text("Larger text")
                            .clicked()
                        {
                            self.text_size = sizes[step + 1].0;
                            self.apply_ui_style(ui.ctx());
                            self.store_display_settings();
                        }
                        if ui
                            .button("Reset text size")
                            .on_hover_text("Back to the default size (100%)")
                            .clicked()
                        {
                            self.text_size = theme::DEFAULT_TEXT_SIZE;
                            self.apply_ui_style(ui.ctx());
                            self.store_display_settings();
                        }
                    });

                    // The theme picker. Both of egui's internal themes were
                    // replaced with this theme's full style, so switching is one
                    // row. Rows rather than a combo: an interactive popup cannot
                    // live inside a menu, which closes on the click.
                    ui.separator();
                    ui.label(egui::RichText::new("Theme").small());
                    for theme in Theme::ALL {
                        if ui
                            .selectable_label(self.theme == theme, theme.name())
                            .clicked()
                        {
                            self.theme = theme;
                            self.apply_ui_style(ui.ctx());
                            self.store_display_settings();
                        }
                    }

                    ui.separator();
                    if ui
                        .button("Reset window size and zoom")
                        .on_hover_text("Ctrl+Alt+0")
                        .clicked()
                    {
                        self.reset_window_state(ui.ctx());
                        ui.close();
                    }
                });
            });
            ui.menu_button("Help", |ui| {
                scrollable_menu(ui, |ui| {
                    if ui.button("About…").clicked() {
                        self.about_open = true;
                        ui.close();
                    }
                });
            });
        });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        // `horizontal_wrapped`, not `horizontal`: at a narrow width or a high
        // zoom the row must fold onto the next line rather than run off the
        // edge, where the trailing controls would be clipped and unreachable.
        // This is the Reflow half of staying usable at 200% (WCAG 1.4.10).
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {
                self.new_document();
            }
            if ui.button("Save").clicked() {
                self.save_document_command();
            }
            if ui.button("Example").clicked() {
                self.load_sample();
            }
            ui.separator();
            if ui.button("Convert").clicked() {
                self.convert_now();
            }
            if ui.checkbox(&mut self.auto, "auto").changed() {
                self.store_settings();
                if self.auto {
                    self.schedule_convert(Instant::now());
                } else {
                    self.deadline = None;
                }
            }
            ui.separator();
            if ui
                .button("Options")
                .on_hover_text("Open the options panel")
                .clicked()
            {
                self.show_options = true;
            }
            ui.separator();
            if ui
                .button("Focus")
                .on_hover_text("Immersive view: hide the menus and toolbar, keep the text and a slim action bar (Ctrl+Shift+F)")
                .clicked()
            {
                self.focus_mode = true;
            }
        });
    }

    /// The preset strip: one button per starting point, plus a "Modified"
    /// marker once the live options have been edited away from the chosen one.
    /// A preset seeds the options; diverging from it is visible rather than
    /// silently wrong.
    fn preset_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Preset");
            for preset in presets::PRESETS {
                let chosen = self.preset == preset.key;
                if ui
                    .selectable_label(chosen, preset.name)
                    .on_hover_text(preset.description())
                    .clicked()
                {
                    self.apply_preset(preset);
                }
            }
            // The user's own presets, from presets.toml, beside the shipped
            // ones: the same file --preset reads.
            let user: Vec<(String, String, String)> = self
                .user_presets
                .iter()
                .map(|p| (p.key.clone(), p.name.clone(), p.description.clone()))
                .collect();
            for (key, name, description) in &user {
                let chosen = self.preset == *key;
                if ui
                    .selectable_label(chosen, name)
                    .on_hover_text(description)
                    .clicked()
                {
                    if let Err(error) = self.apply_user_preset(key) {
                        self.status = error;
                    }
                }
            }
            if self.preset_is_modified() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Modified").small().weak())
                    .on_hover_text(
                        "These options have been edited away from the chosen \
                         preset. Pick the preset again to restore them.",
                    );
            }
        });
    }

    /// The source-and-destination strip: the file being converted and where the
    /// HTML will go, with the same `*` and `+` marks the window title uses, so
    /// the stripes of the window and the title never disagree. The full paths
    /// are one hover away.
    fn file_strip(&mut self, ui: &mut Ui) {
        let input = format!(
            "{}{}",
            self.doc.source_name(),
            if self.doc.dirty { "*" } else { "" }
        );
        let output = format!(
            "{}{}",
            self.doc.output_name(),
            if self.doc.output_stale { "+" } else { "" }
        );
        let line = format!("Input: {input}  \u{00b7}  Output: {output}");
        let detail = match (&self.doc.path, &self.doc.output_path) {
            (Some(src), Some(out)) => format!("{}\n\u{2192} {}", src.display(), out.display()),
            (Some(src), None) => {
                format!(
                    "{}\n\u{2192} not saved yet (Ctrl+S writes it)",
                    src.display()
                )
            }
            (None, Some(out)) => format!("new text\n\u{2192} {}", out.display()),
            (None, None) => "Nothing has been loaded or saved yet.".to_string(),
        };
        let detail = format!("{detail}\n\n* unsaved text \u{00b7} + unsaved HTML");
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(line).weak())
                .on_hover_text(detail);
        });
    }

    /// The options dialog, qBittorrent-style: a category list beside a
    /// scrolling page. Opened from the toolbar's `Options` button or the View
    /// menu, and closed with its own title-bar `x`.
    ///
    /// On desktop this is a real OS window, not a panel pinned inside the main
    /// one, so it can be moved beside the text it is configuring. egui's
    /// immediate viewports render it through the same backend; integrations
    /// without multi-viewport support (egui_kittest, web) fall back to an
    /// embedded window automatically.
    fn options_window(&mut self, ctx: &egui::Context) {
        if !self.show_options {
            return;
        }
        let mut open = self.show_options;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("textrill-options"),
            egui::ViewportBuilder::default()
                .with_title("Textrill options")
                .with_inner_size([780.0, 560.0])
                .with_min_inner_size([560.0, 360.0])
                .with_resizable(true),
            |ui, class| {
                if ui.input(|input| input.viewport().close_requested()) {
                    open = false;
                }
                let mut draw = |ui: &mut egui::Ui| {
                    ui.set_min_width(540.0);
                    if self.panel.draw(ui, &mut self.opts) {
                        self.schedule_convert(Instant::now());
                    }
                };
                if class == egui::ViewportClass::EmbeddedWindow {
                    // The fallback Window has no `.open`, so it gets no
                    // title-bar close button. Give the backend without
                    // multi-viewport support an explicit way out.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close options").clicked() {
                            open = false;
                        }
                    });
                    draw(ui);
                } else {
                    // A native viewport is a bare canvas, like eframe's root
                    // window. Give it the theme's paper and the same breathing
                    // room the old in-window dialog had.
                    egui::Frame::central_panel(ui.style()).show(ui, draw);
                }
            },
        );
        self.show_options = open;
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        egui::Window::new("About")
            .collapsible(false)
            .resizable(false)
            .open(&mut self.about_open)
            .show(ctx, |ui| {
                ui.label(format!("textrill {}", env!("CARGO_PKG_VERSION")));
                ui.label("Plain text to HTML.");
                ui.label("GPLv3+");
            });
    }

    /// Draw the Template Manager (roadmap §5.5) when it is open. The window owns
    /// its own discovery and filesystem actions; the two decisions that touch
    /// this app — applying a template and closing — come back as an outcome.
    fn templates_window(&mut self, ctx: &egui::Context) {
        let Some(window) = self.templates.as_mut() else {
            return;
        };
        let opts = self.opts.clone();
        match window.show(ctx, &opts) {
            crate::templates_window::TemplatesOutcome::Pending => {}
            crate::templates_window::TemplatesOutcome::Closed => self.templates = None,
            crate::templates_window::TemplatesOutcome::Apply(address) => {
                self.apply_template(&address);
                self.status = format!("template {address}");
                // Applying a template does not close the window: the user may
                // want to try another without reopening it.
            }
        }
    }

    /// The Ctrl/Cmd+K palette: one search box over every command, with the
    /// matching rows listed under it.
    ///
    /// Follows the [`Self::about_window`] shape — an `egui::Window` gated on a
    /// bool — so a test that never opens it sees no change at all. The rows
    /// carry the command's own label as their accessible name (the category
    /// sits beside it as a separate, inert label) so a query for
    /// `"Load example"` resolves to exactly one node.
    fn command_palette(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            return;
        }
        // `open` is the window's own close button and the post-draw truth;
        // `escape` and `run` are what the body reports back. Kept apart
        // because the builder borrows `open` for the whole `.show` call.
        let mut open = self.palette_open;
        let mut escape = false;
        let mut run: Option<Command> = None;
        egui::Window::new("Command palette")
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .id(egui::Id::new("palette-search"))
                        .hint_text("Search commands…")
                        .desired_width(f32::INFINITY),
                );
                // Take the keyboard the first frame the palette is up, so the
                // first keystroke lands in the search box and not wherever
                // focus was before it opened.
                if self.palette_focus {
                    search.request_focus();
                    self.palette_focus = false;
                }

                let mut all = Command::all();
                all.extend((0..self.recent.len()).map(Command::OpenRecent));
                let commands: Vec<Command> = all
                    .into_iter()
                    .filter(|command| command.matches_with(&self.palette_query, &self.recent))
                    .collect();
                if self.palette_selected >= commands.len() {
                    self.palette_selected = commands.len().saturating_sub(1);
                }

                // Arrow keys move the selection and Enter runs it, but only
                // while the search box had the keyboard: the palette is not
                // modal, so the source editor keeps its own keys once the user
                // clicks back into it. "Had" not "has" — Enter releases the
                // box's focus on the same frame it arrives, so this frame's
                // focus would always read false and swallow the key.
                let typed_here = search.has_focus() || self.palette_search_focused;
                if typed_here {
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        self.palette_selected =
                            (self.palette_selected + 1).min(commands.len().saturating_sub(1));
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        self.palette_selected = self.palette_selected.saturating_sub(1);
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        run = commands.get(self.palette_selected).copied();
                    }
                }
                self.palette_search_focused = search.has_focus();
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    escape = true;
                }

                ui.separator();
                // The floating scroll bar is drawn over the content, so keep a
                // gutter on the right. Without it the weak category labels sit
                // under the bar and read as struck through.
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .content_margin(egui::Margin {
                        right: 12,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        for (index, command) in commands.iter().enumerate() {
                            let selected = index == self.palette_selected;
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(selected, command.label_with(&self.recent))
                                    .clicked()
                                {
                                    run = Some(*command);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(egui::RichText::new(command.category()).weak());
                                    },
                                );
                            });
                        }
                        if commands.is_empty() {
                            ui.label(egui::RichText::new("No matching commands.").weak());
                        }
                    });
            });
        if escape {
            open = false;
        }
        self.palette_open = open;
        if let Some(command) = run {
            self.palette_open = false;
            self.run_command(command, ctx);
        }
    }

    /// What each [`Command`] does. Kept as one `match`, apart from the
    /// drawing, so the palette's vocabulary and the app's behaviour are
    /// visibly the same set.
    fn run_command(&mut self, command: Command, ctx: &egui::Context) {
        match command {
            Command::New => self.new_document(),
            Command::Open => self.open_document(),
            Command::Save => self.save_document_command(),
            Command::SaveText => self.save_text_command(),
            Command::Convert => self.convert_now(),
            Command::CopyHtml => self.copy_html(ctx),
            Command::Example => self.load_sample(),
            Command::ShowOptions => self.show_options = true,
            Command::FocusMode => self.focus_mode = !self.focus_mode,
            Command::ViewSplit => self.set_view_mode(crate::settings::ViewMode::Split),
            Command::ViewSource => self.set_view_mode(crate::settings::ViewMode::Source),
            Command::ViewHtml => self.set_view_mode(crate::settings::ViewMode::Html),
            Command::ViewPreview => self.set_view_mode(crate::settings::ViewMode::Preview),
            Command::ViewOutline => self.set_view_mode(crate::settings::ViewMode::Outline),
            Command::ViewDiff => self.set_view_mode(crate::settings::ViewMode::Diff),
            Command::ResetWindow => self.reset_window_state(ctx),
            Command::OpenRecent(index) => self.open_recent(index),
            Command::Preset(index) => {
                if let Some(preset) = presets::PRESETS.get(index) {
                    self.apply_preset(preset);
                }
            }
        }
    }

    /// The modal three-way prompt Qt showed before a command could discard text.
    fn save_prompt_window(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let mut answer = None;
        let response = egui::Modal::new(egui::Id::new("unsaved-changes")).show(ctx, |ui| {
            ui.set_max_width(340.0);
            ui.heading("Unsaved changes");
            ui.label("The text has unsaved changes.  Save them?");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(SaveAnswer::Save);
                }
                if ui.button("Discard").clicked() {
                    answer = Some(SaveAnswer::Discard);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(SaveAnswer::Cancel);
                }
            });
        });
        let answer = match answer {
            Some(answer) => answer,
            // Escape or a click on the backdrop is the same as Cancel.
            None if response.should_close() => SaveAnswer::Cancel,
            None => return,
        };
        self.resolve_save_prompt(answer, ctx);
    }

    /// Drive the built-in file browser, if one is open, and act on its answer.
    fn file_browser_window(&mut self, ctx: &egui::Context) {
        let Some(mut browser) = self.file_browser.take() else {
            return;
        };
        match browser.show(ctx) {
            BrowserOutcome::Pending => self.file_browser = Some(browser),
            BrowserOutcome::Cancelled => {
                // A browser opened for the unsaved-changes prompt drops the
                // waiting command, exactly as cancelling the native dialog did.
                self.pending = None;
            }
            BrowserOutcome::Chosen(path) => match browser.intent() {
                FileIntent::OpenText => self.load_or_report(path),
                FileIntent::SaveHtml => self.save_html_or_report(path),
                FileIntent::SaveText => {
                    self.save_text_or_report(path);
                    // The save may have been the one the unsaved-changes prompt
                    // was waiting for; carry its command out now.
                    if !self.doc.dirty {
                        if let Some(action) = self.pending.take() {
                            self.perform(action, ctx);
                        }
                    }
                }
            },
        }
    }

    /// Draw the whole UI into `ui`.
    pub fn draw(&mut self, ui: &mut Ui) {
        // Apply the chosen look on the first frame. `main` could do it once at
        // startup, but that would leave a test harness (and any other driver)
        // drawing at the wrong appearance unless it knew to call this first.
        if !self.applied_style {
            self.apply_ui_style(ui.ctx());
            // The app owns the zoom chords (see `handle_zoom_shortcuts`), so
            // egui's wider-range built-in handler must stay out of the way.
            ui.ctx()
                .options_mut(|options| options.zoom_with_keyboard = false);
            self.applied_style = true;
        }
        // Pack the whole interface according to the chosen density. The three
        // tokens (gap, padding, corner) all move together, so a compact
        // density is compact in both axes rather than only between rows.
        // `Comfortable` reproduces egui's defaults exactly, so the default
        // look is untouched; `build_style` sets neither, so this cannot fight
        // the theme.
        let [space_x, space_y] = self.density.item_spacing();
        let [pad_x, pad_y] = self.density.button_padding();
        let spacing = ui.spacing_mut();
        spacing.item_spacing = egui::vec2(space_x, space_y);
        spacing.button_padding = egui::vec2(pad_x, pad_y);
        // The worker asks for a redraw when a result lands, so the preview does
        // not wait for the next input event to appear.
        if !self.waker_installed {
            let ctx = ui.ctx().clone();
            self.worker.set_waker(move || ctx.request_repaint());
            self.waker_installed = true;
        }
        self.drain();
        self.tick(Instant::now());
        if self.deadline.is_some() {
            ui.ctx().request_repaint_after(AUTO_CONVERT_DELAY);
        }
        self.set_title(ui.ctx());
        self.note_window_state(ui.ctx());
        self.handle_zoom_shortcuts(ui.ctx());

        // The window's own close button goes through the unsaved-changes prompt
        // too, unless we are already closing.
        if !self.closing && ui.ctx().input(|i| i.viewport().close_requested()) {
            // Persist before the window goes away, then hold the close only if
            // there is unsaved text to ask about.
            self.persist();
            if self.doc.dirty {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending = Some(PendingAction::Quit);
            }
        }
        // A pending prompt or an open browser owns the keyboard, so shortcuts
        // cannot stack commands behind it.
        if self.pending.is_none() && self.file_browser.is_none() {
            self.handle_shortcuts(ui.ctx());
            self.accept_dropped_files(ui.ctx());
        }

        if self.focus_mode {
            // Focus mode drops the menu bar and the toolbar, but not the work:
            // Convert and Save HTML stay within reach, so a manual conversion
            // or a save never forces a trip back to the chrome. Immersion is
            // still one click deep — the first control is the one visible way
            // back (and `Ctrl+Shift+F` works too).
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Exit focus")
                    .on_hover_text("Back to the window chrome (Ctrl+Shift+F)")
                    .clicked()
                {
                    self.focus_mode = false;
                }
                ui.separator();
                if ui
                    .button("Convert")
                    .on_hover_text("Convert now (Ctrl+Enter)")
                    .clicked()
                {
                    self.convert_now();
                }
                if ui
                    .button("Save HTML")
                    .on_hover_text("Write the HTML output (Ctrl+S)")
                    .clicked()
                {
                    self.save_document_command();
                }
                ui.separator();
                ui.label(&self.status);
            });
        } else {
            self.menu_bar(ui);
            self.toolbar(ui);
            self.preset_bar(ui);
            self.file_strip(ui);
            // The landing view: a plain-language reading of what the current
            // settings will do. Open by default so a first-time user sees the
            // answers without opening the panel, and collapsible so it costs
            // nothing once they know their way around.
            egui::CollapsingHeader::new("Summary")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(summary::describe(
                        &self.opts,
                        &self.current_preset_name(),
                        self.preset_is_modified(),
                    ));
                });
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Title");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.opts.title)
                        .hint_text("Document title: shown as a heading in the HTML"),
                )
                .changed()
            {
                self.schedule_convert(Instant::now());
            }
        });

        if self.show_options {
            self.options_window(ui.ctx());
        }

        self.working_area(ui);
        self.problems_panel(ui);
        self.status_bar(ui);

        self.command_palette(ui.ctx());
        self.about_window(ui.ctx());
        self.templates_window(ui.ctx());
        if self.file_browser.is_some() {
            // The browser covers the prompt it may have been opened from.
            self.file_browser_window(ui.ctx());
        } else {
            self.save_prompt_window(ui.ctx());
        }
    }

    /// The main working area: the source pane, the converted HTML, or the two
    /// side by side with a drag-to-resize handle between them. `Split` gives
    /// the source `split_ratio` of the width; the single-view modes give the
    /// whole area to one pane.
    fn working_area(&mut self, ui: &mut Ui) {
        match self.view_mode {
            crate::settings::ViewMode::Split => {
                let handle_width = 6.0;
                let total = (ui.available_width() - handle_width).max(160.0);
                let left = (total * self.split_ratio).max(120.0);
                let right = (total - left).max(120.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            // In Split the HTML sits beside us, so the source
                            // pane takes the full height of its column.
                            let height = ui.available_height();
                            self.source_pane(ui, height);
                        },
                    );
                    let handle = ui
                        .allocate_exact_size(
                            egui::vec2(handle_width, ui.available_height()),
                            egui::Sense::click_and_drag(),
                        )
                        .1;
                    if handle.drag_started() {
                        let x = ui.input(|i| i.pointer.interact_pos().map(|p| p.x).unwrap_or(0.0));
                        self.drag_start = Some((x, self.split_ratio));
                    }
                    if handle.dragged() {
                        if let Some((start_x, start_ratio)) = self.drag_start {
                            let now_x = ui.input(|i| {
                                i.pointer.interact_pos().map(|p| p.x).unwrap_or(start_x)
                            });
                            self.split_ratio = crate::settings::clamp_split_ratio(
                                start_ratio + (now_x - start_x) / total,
                            );
                        }
                    }
                    if handle.drag_stopped() {
                        self.drag_start = None;
                        self.store_layout_settings();
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(right, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            self.html_pane(ui);
                        },
                    );
                });
            }
            crate::settings::ViewMode::Source => {
                let height = ui.available_height();
                self.source_pane(ui, height);
            }
            crate::settings::ViewMode::Html => {
                self.html_pane(ui);
            }
            crate::settings::ViewMode::Preview => {
                self.preview_pane(ui);
            }
            crate::settings::ViewMode::Outline => {
                self.outline_pane(ui);
            }
            crate::settings::ViewMode::Diff => {
                self.diff_pane(ui);
            }
        }
    }

    /// The source pane: its label, the bounded scroll area, and the edit hook.
    /// `max_height` bounds the editor so a long document scrolls inside the
    /// pane instead of pushing the rest of the window off the screen.
    fn source_pane(&mut self, ui: &mut Ui, max_height: f32) {
        ui.heading("Input");
        // The hint plus a quick-start row is the empty state: with no text it
        // points a first run at the ways in; once there is text both disappear
        // without the editor ever going away.
        let empty = self.doc.text.is_empty();
        egui::ScrollArea::vertical()
            .id_salt("input-pane")
            .max_height(max_height.max(80.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut editor = egui::TextEdit::multiline(&mut self.doc.text).desired_rows(6);
                editor = editor.id(egui::Id::new("source-editor"));
                if empty {
                    editor = editor.hint_text("Type here, or use the shortcuts below to start.");
                }
                let mut output = editor.show(ui);
                if let Some(index) = self.pending_caret.take() {
                    let cursor = egui::text::CCursor::new(index);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
                    output.state.store(ui.ctx(), egui::Id::new("source-editor"));
                    output.response.request_focus();
                }
                if output.response.changed() {
                    self.on_edit(Instant::now());
                }
                // Remember the caret for the status bar's line:column readout.
                if let Some(range) = output.cursor_range {
                    let index: usize = range.primary.index.into();
                    let (line, col) = line_col(&self.doc.text, index);
                    self.cursor = Some((line, col));
                }
                if empty {
                    self.quick_start(ui);
                }
            });
    }

    /// The empty state's quick-start actions. Each reaches a real command; the
    /// template entry is honest about arriving with Phase 2.
    fn quick_start(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Open file…")
                .on_hover_text("Open a plain-text file (Ctrl+O)")
                .clicked()
            {
                self.open_document();
            }
            if ui
                .button("Paste")
                .on_hover_text("Focus the editor, then paste with Ctrl+V")
                .clicked()
            {
                ui.ctx()
                    .memory_mut(|m| m.request_focus(egui::Id::new("source-editor")));
            }
            if ui
                .button("Try the example")
                .on_hover_text("Load a sample document that exercises the converter")
                .clicked()
            {
                self.load_sample();
            }
            if ui
                .button("Browse templates")
                .on_hover_text("Open the Template Manager to pick a template pack")
                .clicked()
            {
                self.open_templates();
            }
        });
    }

    /// The converted-HTML pane: its label and the read-only editor over
    /// `self.output`.
    fn html_pane(&mut self, ui: &mut Ui) {
        ui.heading("HTML");
        egui::ScrollArea::vertical()
            .id_salt("html-pane")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.output)
                        .code_editor()
                        .desired_rows(8),
                );
            });
    }

    /// The native rendered preview (roadmap §7, workstream 1): the produced
    /// HTML drawn with egui's own text and layout, plus the local escape hatch
    /// that hands the exact bytes to the desktop's browser.
    fn preview_pane(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("Preview");
            ui.label(
                egui::RichText::new(
                    "native subset — faithful to structure, not pixel-identical to a browser",
                )
                .small()
                .weak(),
            );
            if ui
                .button("Open in browser")
                .on_hover_text(
                    "Write the produced HTML to a temporary file and open it with the \
                     desktop's handler. This is a local program launch; textrill itself \
                     makes no network request.",
                )
                .clicked()
            {
                self.open_in_browser();
            }
        });
        ui.separator();
        if self.status == "error" {
            ui.colored_label(ui.visuals().error_fg_color, "No output was produced.");
            ui.add_space(2.0);
        }
        egui::ScrollArea::vertical()
            .id_salt("preview-pane")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(crate::preview::Action::Open(link)) =
                    crate::preview::show(ui, &self.output)
                {
                    self.open_link(&link);
                }
            });
    }

    /// Write the exact bytes `Save HTML` would write into `dir`, returning the
    /// path. Kept separate from the launch so a test can compare the bytes
    /// without starting a browser (roadmap §7 acceptance).
    pub fn write_preview_file(&self, dir: &Path) -> Result<PathBuf, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = dir.join(format!(
            "textrill-preview-{}-{nanos}.html",
            std::process::id()
        ));
        std::fs::write(&path, self.output.as_bytes())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    /// Hand the produced HTML to the desktop's default handler. No network
    /// path exists in this process; the browser is a separate local program.
    pub fn open_in_browser(&mut self) {
        match self.write_preview_file(&std::env::temp_dir()) {
            Ok(path) => match crate::opener::open_target(&path) {
                Ok(_) => self.status = format!("opened {} in the browser", path.display()),
                Err(e) => self.status = format!("open in browser: {e}"),
            },
            Err(e) => self.status = format!("open in browser: {e}"),
        }
    }

    /// A link clicked in the preview. Internal anchors are left alone (the
    /// outline is the navigation surface); anything else is handed to the
    /// desktop, which is the same local-launch rule as `Open in browser`.
    fn open_link(&mut self, href: &str) {
        if href.starts_with('#') {
            return;
        }
        match crate::opener::open_target(href) {
            Ok(_) => self.status = format!("opened {href}"),
            Err(e) => self.status = format!("open link: {e}"),
        }
    }

    /// The output before the most recent successful conversion, if any.
    pub fn previous_output(&self) -> Option<&str> {
        self.previous_output.as_deref()
    }

    /// The problems from the last failed conversion, if any.
    pub fn problems(&self) -> &[crate::problems::Problem] {
        &self.problems
    }

    /// The source caret as 1-based `(line, column)`, once the editor has drawn.
    pub fn cursor(&self) -> Option<(usize, usize)> {
        self.cursor
    }

    /// The problems panel (roadmap §7, workstream 4). A failure is a list of
    /// sentences, not one long line; a sentence that names a line is a jump.
    fn problems_panel(&mut self, ui: &mut Ui) {
        if self.problems.is_empty() {
            return;
        }
        let mut jump = None;
        egui::Frame::NONE
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("Problems ({})", self.problems.len()),
                    );
                    ui.label(
                        egui::RichText::new(if self.output.is_empty() {
                            "no output was written because:"
                        } else {
                            "the conversion stopped because:"
                        })
                        .small()
                        .weak(),
                    );
                });
                egui::ScrollArea::vertical()
                    .id_salt("problems-panel")
                    .max_height(140.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for problem in &self.problems {
                            match problem.line {
                                Some(line) => {
                                    let text = format!("line {line}: {}", problem.message);
                                    if ui
                                        .add(
                                            egui::Label::new(text)
                                                .sense(egui::Sense::click())
                                                .wrap(),
                                        )
                                        .on_hover_text("Jump to this line in the source")
                                        .clicked()
                                    {
                                        jump = Some(line);
                                    }
                                }
                                None => {
                                    ui.add(
                                        egui::Label::new(&problem.message)
                                            .wrap()
                                            .halign(egui::Align::Min),
                                    );
                                }
                            }
                        }
                    });
            });
        if let Some(line) = jump {
            self.jump_to_line(line);
        }
    }

    /// Put the source caret on 1-based `line` and show the source view.
    pub fn jump_to_line(&mut self, line: usize) {
        let line = line.max(1);
        let mut index = 0usize;
        for (current, part) in self.doc.text.split_inclusive('\n').enumerate() {
            if current + 1 == line {
                break;
            }
            index += part.chars().count();
        }
        self.pending_caret = Some(index);
        self.set_view_mode(crate::settings::ViewMode::Source);
        self.status = format!("problem at line {line}");
    }

    /// The rendered diff (roadmap §7, workstream 3): the current output beside
    /// the previous one, both drawn by the same subset renderer, so a settings
    /// change can be judged by reading rather than by diffing HTML.
    fn diff_pane(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("Diff");
            ui.label(
                egui::RichText::new(
                    "current output beside the previous conversion — rendered, not an HTML diff",
                )
                .small()
                .weak(),
            );
        });
        ui.separator();
        let Some(previous) = self.previous_output.clone() else {
            ui.weak(
                "No previous output yet. Change the text or an option, convert again, \
                 and the two results appear here side by side.",
            );
            return;
        };
        let gap = 12.0;
        let column = ((ui.available_width() - gap) / 2.0).max(160.0);
        let height = ui.available_height();
        let mut action = None;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(column, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.strong("Current");
                    egui::ScrollArea::vertical()
                        .id_salt("diff-current")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            action = crate::preview::show(ui, &self.output);
                        });
                },
            );
            ui.add_space(gap);
            ui.allocate_ui_with_layout(
                egui::vec2(column, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.strong("Previous");
                    egui::ScrollArea::vertical()
                        .id_salt("diff-previous")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let _ = crate::preview::show(ui, &previous);
                        });
                },
            );
        });
        if let Some(crate::preview::Action::Open(link)) = action {
            self.open_link(&link);
        }
    }

    /// The outline (roadmap §7, workstream 2): the headings of the produced
    /// document, indented by level. The engine exposes no source line map, so
    /// a click finds the heading's text in the source and puts the caret
    /// there, switching to the source view where the jump is visible.
    fn outline_pane(&mut self, ui: &mut Ui) {
        let headings = crate::preview::outline(&self.output);
        ui.horizontal_wrapped(|ui| {
            ui.heading("Outline");
            ui.label(
                egui::RichText::new(
                    "headings in the produced document — click to jump to the source",
                )
                .small()
                .weak(),
            );
        });
        ui.separator();
        if headings.is_empty() {
            ui.weak(
                "No headings yet. The outline is built from the converted HTML, \
                 so it appears as soon as the document has one.",
            );
            return;
        }
        let mut jump = None;
        egui::ScrollArea::vertical()
            .id_salt("outline-pane")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for heading in &headings {
                    ui.horizontal(|ui| {
                        ui.add_space(f32::from(heading.level.saturating_sub(1)) * 14.0);
                        let text = if heading.level == 1 {
                            egui::RichText::new(&heading.text).strong()
                        } else {
                            egui::RichText::new(&heading.text)
                        };
                        let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                        if response.clicked() {
                            jump = Some(heading.text.clone());
                        }
                        if let Some(id) = &heading.id {
                            ui.label(egui::RichText::new(format!("#{id}")).small().weak());
                        }
                    });
                }
            });
        if let Some(text) = jump {
            self.jump_to_heading(&text);
        }
    }

    /// Put the source caret on `text` and switch to the source view. The caret
    /// is applied by `source_pane` on the next frame: requesting focus on a
    /// widget that is not in this frame's tree would confuse AccessKit.
    fn jump_to_heading(&mut self, text: &str) {
        let Some(byte) = find_in_source(&self.doc.text, text) else {
            self.status = format!("outline: no source line matches {text:?}");
            return;
        };
        let index = self.doc.text[..byte].chars().count();
        self.pending_caret = Some(index);
        self.set_view_mode(crate::settings::ViewMode::Source);
        let (line, column) = line_col(&self.doc.text, index);
        self.status = format!("outline: {text} is at line {line}, column {column}");
    }

    /// The status bar, pinned to the bottom of the working area. Left: the live
    /// message. Right: the caret position, the source encoding, the `--report`
    /// structure counts, the document size, and the unsaved marks.
    fn status_bar(&mut self, ui: &mut Ui) {
        let words = self.doc.text.split_whitespace().count();
        let chars = self.doc.text.chars().count();
        let marks = format!(
            "{}{}",
            if self.doc.dirty { "*" } else { "" },
            if self.doc.output_stale { "+" } else { "" }
        );
        let caret = self
            .cursor
            .map(|(l, c)| format!("{l}:{c}"))
            .unwrap_or_default();
        let counts = &self.counts;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&self.status).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tail = format!(
                    "{marks}  ·  {}  ·  {}w {}c  ·  {} ¶ {} H {} S {} BR  ·  {caret}",
                    self.doc.encoding.name(),
                    words,
                    chars,
                    counts.paragraphs,
                    counts.headings,
                    counts.strong,
                    counts.br,
                );
                ui.label(egui::RichText::new(tail).weak());
            });
        });
    }
}

/// Find `needle` in the source, case-insensitively as a fallback. Only accept
/// the lowercase mapping when it preserves byte length (some Unicode case
/// mappings do not), so character offsets stay valid.
fn find_in_source(source: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    if let Some(byte) = source.find(needle) {
        return Some(byte);
    }
    let lower = source.to_lowercase();
    if lower.len() != source.len() {
        return None;
    }
    lower.find(&needle.to_lowercase())
}

/// The 1-based (line, column) of a character index in `text`, counting columns
/// in characters. Used for the status bar's caret readout.
fn line_col(text: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (n, ch) in text.chars().enumerate() {
        if n >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl eframe::App for TextrillApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // eframe's `ui()` is a bare canvas: no margin, no background, and the
        // framebuffer clears to the window's default clear colour (near-black)
        // until something paints over it. A central-panel frame gives the whole
        // surface the theme's `panel_fill`, so the ink always sits on the
        // theme's paper rather than on that dark canvas. `draw` itself stays a
        // plain widget pass, which is what the test harness drives.
        egui::Frame::central_panel(ui.style()).show(ui, |ui| self.draw(ui));
    }

    /// What the framebuffer is cleared to before egui paints. eframe's default
    /// is a translucent near-black, which would flash against every theme; this
    /// clears to the theme's own panel colour, so there is never a dark frame —
    /// not on the first paint and not between theme switches.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}

/// Where a first file dialog should start when nothing has been remembered yet:
/// the user's Documents directory, their home, or the working directory.
fn documents_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home.as_ref() {
        let documents = home.join("Documents");
        if documents.is_dir() {
            return documents;
        }
    }
    home.filter(|home| home.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}
