// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Application state and layout.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::Ui;
use textrill::options::Options;

use crate::args::Args;
use crate::dialogs::{Chooser, NativeChooser};
use crate::document::Document;
use crate::options_panel::OptionsPanel;
use crate::settings::Settings;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAction {
    New,
    Open,
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
    /// Whether the options pane is shown (`View → Show options`).
    pub show_options: bool,
    /// Whether the About window is open.
    pub about_open: bool,
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
    /// The file choosers. Tests swap in a stub so the command logic can run
    /// without a desktop portal.
    chooser: Box<dyn Chooser>,
}

impl Default for TextrillApp {
    fn default() -> Self {
        Self::with_settings(Settings::from_default())
    }
}

impl TextrillApp {
    /// Build an app backed by `settings`, applying the stored `auto` choice.
    ///
    /// Tests pass a [`Settings`] in a temporary directory so a developer's real
    /// configuration cannot change what the tests observe.
    pub fn with_settings(settings: Settings) -> Self {
        let mut app = Self {
            doc: Document::new(),
            opts: Options::default(),
            panel: OptionsPanel::default(),
            show_options: true,
            about_open: false,
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
            chooser: Box::new(NativeChooser),
        };
        app.restore_settings();
        app
    }

    /// Replace the file choosers. Tests use this to answer prompts without a
    /// desktop portal.
    pub fn set_chooser(&mut self, chooser: Box<dyn Chooser>) {
        self.chooser = chooser;
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

    /// Persist everything the window owns: the option set and the geometry.
    fn persist(&mut self) {
        self.store_settings();
        self.store_window_state();
    }

    /// `View → Reset window size and zoom`: return to the size a first run
    /// opens at, at 100%, and forget the stored geometry so the reset survives
    /// a restart too. This is the one action that always undoes a geometry the
    /// user does not want.
    pub fn reset_window_state(&mut self, ctx: &egui::Context) {
        self.window_size = crate::window_state::DEFAULT_SIZE;
        self.window_maximized = false;
        self.zoom = 1.0;
        ctx.set_zoom_factor(1.0);
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
                    self.output = error;
                    self.status = "error".to_string();
                }
                None => {
                    let lines = if outcome.html.is_empty() {
                        0
                    } else {
                        outcome.html.matches('\n').count() + 1
                    };
                    self.output = outcome.html;
                    self.doc.note_converted(&self.output);
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
        self.convert_now();
    }

    /// Open `path`, remembering its encoding. Never overwrites the text on
    /// failure.
    pub fn load_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.doc.load(path)?;
        self.convert_now();
        Ok(())
    }

    /// `File → Save text`. Uses the opened path, or reports the one to propose.
    pub fn save_text(&mut self) -> Result<SaveRequest, String> {
        match self.doc.path.clone() {
            Some(path) => {
                self.doc.save_text_to(&path)?;
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

    /// `File → Open…`: ask about unsaved text, then choose a file.
    pub fn open_document(&mut self) {
        if self.doc.dirty {
            self.pending = Some(PendingAction::Open);
        } else {
            self.do_open();
        }
    }

    fn do_open(&mut self) {
        let from = self.doc.path.clone();
        if let Some(path) = self.chooser.open_text(from.as_deref()) {
            if let Err(error) = self.load_file(&path) {
                self.status = error;
            }
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
        if let Some(path) = self.chooser.save_html(&self.suggested_output_name()) {
            match self.save_html_to(&path) {
                Ok(()) => self.status = format!("saved {}", path.display()),
                Err(error) => self.status = error,
            }
        }
    }

    /// `File → Save text…`: write the text to the opened path, or ask.
    pub fn save_text_command(&mut self) {
        match self.doc.path.clone() {
            Some(path) => match self.save_text_to(&path) {
                Ok(()) => self.status = format!("saved text to {}", path.display()),
                Err(error) => self.status = error,
            },
            None => {
                if let Some(path) = self.chooser.save_text(&self.doc.suggested_text_name()) {
                    match self.save_text_to(&path) {
                        Ok(()) => self.status = format!("saved text to {}", path.display()),
                        Err(error) => self.status = error,
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
        let Some(action) = self.pending else {
            return;
        };
        match answer {
            SaveAnswer::Cancel => self.pending = None,
            SaveAnswer::Discard => self.perform(action, ctx),
            SaveAnswer::Save => {
                self.save_text_command();
                if self.doc.dirty {
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
            PendingAction::LoadSample => self.do_load_sample(),
            PendingAction::Quit => {
                self.persist();
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

    /// Act on the keyboard shortcuts the menu advertises.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (mut new, mut save, mut save_text, mut convert, mut copy, mut quit, mut reset_window) =
            (false, false, false, false, false, false, false);
        ctx.input_mut(|input| {
            new = input.consume_shortcut(&NEW_SHORTCUT);
            save = input.consume_shortcut(&SAVE_SHORTCUT);
            save_text = input.consume_shortcut(&SAVE_TEXT_SHORTCUT);
            convert = input.consume_shortcut(&CONVERT_SHORTCUT);
            copy = input.consume_shortcut(&COPY_HTML_SHORTCUT);
            quit = input.consume_shortcut(&QUIT_SHORTCUT);
            reset_window = input.consume_shortcut(&RESET_WINDOW_SHORTCUT);
        });
        if reset_window {
            self.reset_window_state(ctx);
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
            if let Err(error) = self.load_file(path) {
                self.status = error;
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.new_document();
                    ui.close();
                }
                if ui.button("Open…").clicked() {
                    self.open_document();
                    ui.close();
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
                ui.separator();
                if ui.button("Quit").clicked() {
                    self.quit(ui.ctx());
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
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
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut self.show_options, "Show options");
                ui.separator();
                egui::gui_zoom::zoom_menu_buttons(ui);
                ui.separator();
                // The single way back from a size or zoom the user does not
                // want. It is in the menu bar on purpose: the menu bar is the
                // first, shortest row, so it stays reachable even when the
                // toolbar has folded onto several lines at a high zoom.
                if ui
                    .button("Reset window size and zoom")
                    .on_hover_text("Ctrl+Alt+0")
                    .clicked()
                {
                    self.reset_window_state(ui.ctx());
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    self.about_open = true;
                    ui.close();
                }
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
            ui.checkbox(&mut self.show_options, "Show options");
            ui.separator();

            // Whole-UI zoom, for the same reason a browser has it: a 4K laptop
            // and a 96 DPI desktop should not dictate the same physical text
            // size. The keyboard shortcuts (Ctrl/⌘ + / - / 0) work too; these
            // buttons make the feature discoverable. `zoom_factor` multiplies
            // the OS's own scale factor, so a HiDPI screen is already handled.
            ui.label(egui::RichText::new("Display").small());
            if ui
                .button("Zoom out")
                .on_hover_text("Zoom out (Ctrl+-)")
                .clicked()
            {
                egui::gui_zoom::zoom_out(ui.ctx());
            }
            ui.label(format!("{:.0}%", ui.ctx().zoom_factor() * 100.0));
            if ui
                .button("Zoom in")
                .on_hover_text("Zoom in (Ctrl+=)")
                .clicked()
            {
                egui::gui_zoom::zoom_in(ui.ctx());
            }
            if ui
                .button("Reset zoom")
                .on_hover_text("Reset to 100% (Ctrl+0)")
                .clicked()
            {
                ui.ctx().set_zoom_factor(1.0);
            }
        });
        // The status is its own wrapped row: a long message can then never push
        // a control off the edge, and it stays readable when the buttons fold.
        ui.horizontal_wrapped(|ui| {
            ui.label(&self.status);
        });
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        egui::Window::new("About textrill")
            .collapsible(false)
            .resizable(false)
            .open(&mut self.about_open)
            .show(ctx, |ui| {
                ui.label(format!("textrill {}", env!("CARGO_PKG_VERSION")));
                ui.label("A Rust port of HTML::TextToHTML 3.0, with an egui front end.");
                ui.label("The conversion is byte-identical to the original Perl module.");
                ui.label("Released under the GNU General Public License, version 3 or later.");
            });
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

    /// Draw the whole UI into `ui`.
    pub fn draw(&mut self, ui: &mut Ui) {
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
        // A pending prompt owns the keyboard, so shortcuts cannot stack commands.
        if self.pending.is_none() {
            self.handle_shortcuts(ui.ctx());
            self.accept_dropped_files(ui.ctx());
        }

        self.menu_bar(ui);
        self.toolbar(ui);

        ui.label("Input");
        if ui
            .add(egui::TextEdit::multiline(&mut self.doc.text).desired_rows(6))
            .changed()
        {
            self.on_edit(Instant::now());
        }

        if self.show_options {
            ui.separator();
            ui.label(egui::RichText::new("Options").strong());
            let changed = egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| self.panel.draw(ui, &mut self.opts))
                .inner;
            if changed {
                self.schedule_convert(Instant::now());
            }
        }

        ui.separator();
        ui.label("HTML");
        ui.add(
            egui::TextEdit::multiline(&mut self.output)
                .code_editor()
                .desired_rows(8),
        );

        self.about_window(ui.ctx());
        self.save_prompt_window(ui.ctx());
    }
}

impl eframe::App for TextrillApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}
