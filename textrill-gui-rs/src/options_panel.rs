// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The settings form, generated from the engine's option table.
//!
//! The Python front end carried a 503-line `optionspanel.py` that existed only
//! to turn option metadata into widgets. Here the metadata is read directly
//! from [`textrill::cli::SPECS`], so a new engine option appears in the panel
//! without a second edit, and the panel cannot offer a value the engine
//! rejects: the numeric bounds come from [`textrill::options::numeric_range`],
//! the same table the engine validates against.
//!
//! The panel carries the filter text as state (as the Python `OptionsPanel`
//! did), so it is a struct rather than a free function.

use egui::Ui;
use textrill::cli::{self, Kind, Opt};
use textrill::options::{numeric_range, Encoding, Options};

/// The options form and its filter.
#[derive(Debug, Default)]
pub struct OptionsPanel {
    /// The filter box contents.
    pub filter: String,
    /// Show only the options whose value differs from the engine default.
    pub only_changed: bool,
    /// Currently selected category when not filtering.
    selected_category: usize,
    /// The template source last chosen in the panel; see [`TemplateSource`].
    template_source: TemplateSource,
    /// The title source last chosen in the panel; see [`TitleSource`].
    title_source: TitleSource,
}

/// Which template source the panel is editing.
///
/// The engine records the choice as whichever of `--body_template`,
/// `--document_template` and `--template_library` is non-empty, so a source
/// chosen before its field is filled in has no option to live in. The panel
/// remembers it here, or the radio would snap back to "None" on the next frame
/// and the field could never be typed into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum TemplateSource {
    #[default]
    None,
    Body,
    Document,
    Library,
    Discovered,
}

/// Where the document `<title>` comes from.
///
/// The engine's precedence is explicit `--title` first, then `--titlefirst`
/// (the document's own first line). The panel presents that as one choice and
/// remembers an "Explicit title" with an empty field, which the options alone
/// cannot tell from "no title".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum TitleSource {
    #[default]
    None,
    FirstLine,
    Explicit,
}

const CATEGORIES: &[&str] = &[
    "All",
    "Input & Output",
    "Document & HTML",
    "Text Formatting",
    "Headings & TOC",
    "Links & URLs",
    "References",
    "Lists",
    "Mail & Headers",
    "Paragraphs",
    "Preformatting",
    "Tables",
    "Layout & Spacing",
];

/// Which category an option belongs to. Exhaustive over [`Opt`] (no catch-all),
/// so a new option must be placed in a category to compile — the same guarantee
/// the acceptance test checks, but at build time.
fn category(opt: Opt) -> &'static str {
    match opt {
        Opt::Infile
        | Opt::Instring
        | Opt::Outfile
        | Opt::Encoding
        | Opt::Utf8
        | Opt::EightBitClean
        | Opt::Stream
        | Opt::Report
        | Opt::Preset
        | Opt::ListTemplates => "Input & Output",
        Opt::Doctype
        | Opt::Html5
        | Opt::Xhtml
        | Opt::CloseOptionalTags
        | Opt::MetaCharset
        | Opt::Extract
        | Opt::Chunk
        | Opt::ChunkChars
        | Opt::Section
        | Opt::StyleUrl
        | Opt::DocumentTemplate
        | Opt::BodyTemplate
        | Opt::TemplateLibrary
        | Opt::TemplateDir
        | Opt::PrependFile
        | Opt::AppendFile
        | Opt::AppendHead
        | Opt::BodyDeco
        | Opt::Var => "Document & HTML",
        Opt::Demoronize
        | Opt::BoldDelimiter
        | Opt::ItalicDelimiter
        | Opt::UnderlineDelimiter
        | Opt::UnderlineLengthTolerance
        | Opt::UnderlineOffsetTolerance
        | Opt::Unhyphenation
        | Opt::CapsTag
        | Opt::MinCapsLength
        | Opt::Lang
        | Opt::LangRuns
        | Opt::EscapeHtmlChars
        | Opt::LowerCaseTags => "Text Formatting",
        Opt::CustomHeadingRegexp
        | Opt::ExplicitHeadings
        | Opt::NumberHeadings
        | Opt::Toc
        | Opt::MakeAnchors
        | Opt::Title
        | Opt::Titlefirst => "Headings & TOC",
        Opt::MakeLinks
        | Opt::LinkOnly
        | Opt::AllowedUrlSchemes
        | Opt::DefaultLinkDict
        | Opt::LinksDictionaries
        | Opt::NoReferrer => "Links & URLs",
        Opt::Citations | Opt::Glossary => "References",
        Opt::Bullets | Opt::BulletsOrdered | Opt::PreserveIndent => "Lists",
        Opt::Mailmode | Opt::UseMosaicHeader => "Mail & Headers",
        Opt::ParIndent | Opt::IndentParBreak | Opt::IndentWidth | Opt::ShortLineLength => {
            "Paragraphs"
        }
        Opt::PreformatTriggerLines
        | Opt::EndpreformatTriggerLines
        | Opt::PreformatStartMarker
        | Opt::PreformatEndMarker
        | Opt::PreformatWhitespaceMin
        | Opt::UsePreformatMarker => "Preformatting",
        Opt::MakeTables | Opt::TableType => "Tables",
        Opt::TabWidth | Opt::HruleMin | Opt::CatalogDir => "Layout & Spacing",
    }
}

impl OptionsPanel {
    /// Draw the options panel: a filter row, a category list down the left, and
    /// the selected category's widgets in a scrolling column on the right.
    ///
    /// The qBittorrent "Options" shape, rather than a single strip: the list is
    /// how you move between groups, and the page scrolls so a long category is
    /// not clipped. Returns whether any widget was edited, so the caller can
    /// re-convert.
    pub fn draw(&mut self, ui: &mut Ui, opts: &mut Options) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter options…")
                    .desired_width(160.0),
            );
            ui.checkbox(&mut self.only_changed, "Show only changed");
            if ui
                .button("Reset all options")
                .on_hover_text("Put every option back to its default")
                .clicked()
            {
                *opts = Options::default();
                self.template_source = TemplateSource::None;
                self.title_source = TitleSource::None;
                changed = true;
            }
        });

        let needle = self.filter.trim().to_lowercase();
        let filtering = !needle.is_empty();
        let height = ui.available_height().max(120.0);
        let mut template_source_choice = self.template_source;
        let mut title_source_choice = self.title_source;

        ui.horizontal_top(|ui| {
            // Left: the categories. While a filter is active the list gives way
            // to a heading, so the results on the right are not competing with a
            // category choice.
            ui.allocate_ui_with_layout(
                egui::vec2(172.0, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_width(172.0);
                    ui.add_space(4.0);
                    if filtering {
                        ui.label(egui::RichText::new("Search results").strong());
                        ui.label(
                            egui::RichText::new("Clear the filter to pick a category.")
                                .small()
                                .weak(),
                        );
                    } else {
                        for (idx, category) in CATEGORIES.iter().enumerate() {
                            if ui
                                .selectable_label(self.selected_category == idx, *category)
                                .clicked()
                            {
                                self.selected_category = idx;
                            }
                        }
                    }
                },
            );

            ui.separator();

            // Right: the selected category's options, scrolling in full height.
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let selected = CATEGORIES[self.selected_category];
                            let mut shown = 0usize;
                            // Some engine options are not independent: two
                            // `html5`/`xhtml` flags describe one document type,
                            // the three template sources are mutually exclusive,
                            // and the two link flags describe one handling mode.
                            // Each group is drawn once, at whichever option is
                            // reached first; the rest are then skipped.
                            let mut document_type_drawn = false;
                            let mut template_source_drawn = false;
                            let mut link_mode_drawn = false;
                            let mut title_source_drawn = false;
                            for &opt in Opt::ALL {
                                let keep = if filtering {
                                    opt_matches(opt, &needle, opts)
                                } else if selected == "All" {
                                    true
                                } else {
                                    category(opt) == selected
                                };
                                let keep = keep && (!self.only_changed || any_changed(opt, opts));
                                if !keep {
                                    continue;
                                }
                                shown += 1;
                                if matches!(opt, Opt::Html5 | Opt::Xhtml) {
                                    if !document_type_drawn {
                                        document_type_drawn = true;
                                        changed |= document_type(ui, opts);
                                    }
                                    continue;
                                }
                                if matches!(
                                    opt,
                                    Opt::BodyTemplate
                                        | Opt::DocumentTemplate
                                        | Opt::TemplateLibrary
                                ) {
                                    if !template_source_drawn {
                                        template_source_drawn = true;
                                        changed |= template_source(
                                            ui,
                                            opts,
                                            &mut template_source_choice,
                                            !opts.extract,
                                        );
                                    }
                                    continue;
                                }
                                if matches!(opt, Opt::MakeLinks | Opt::LinkOnly) {
                                    if !link_mode_drawn {
                                        link_mode_drawn = true;
                                        changed |= link_mode(ui, opts);
                                    }
                                    continue;
                                }
                                if matches!(opt, Opt::Title | Opt::Titlefirst) {
                                    if !title_source_drawn {
                                        title_source_drawn = true;
                                        changed |= title_source(ui, opts, &mut title_source_choice);
                                    }
                                    continue;
                                }
                                if opt == Opt::Encoding {
                                    changed |= encoding_choice(ui, opts);
                                    continue;
                                }
                                let on = enabled(opt, opts);
                                changed |= match opt.kind() {
                                    Kind::Flag => flag(ui, opts, opt, on),
                                    Kind::Int => int(ui, opts, opt, on),
                                    Kind::Str => string(ui, opts, opt, on),
                                    Kind::StrArray => string_array(ui, opts, opt, on),
                                    Kind::TableType => table_type(ui, opts, opt.help(), on),
                                };
                            }
                            if shown == 0 {
                                ui.label("No options match that filter.");
                            }
                        });
                },
            );
        });

        self.template_source = template_source_choice;
        self.title_source = title_source_choice;

        changed
    }

    /// Clear the filter, as the Python implementation did.
    pub fn clear_filter(&mut self) {
        self.filter.clear();
    }

    /// Test whether the option named `name` matches the current filter.
    pub fn matches(&self, name: &str) -> bool {
        let needle = self.filter.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        match cli::lookup(name) {
            Some(opt) => opt_matches_name(opt, &needle),
            None => false,
        }
    }
}

/// Render the document type as one three-way choice rather than the two
/// independent `html5`/`xhtml` flags, which a user can leave both off — an
/// undocumented "HTML 4" state that no single widget owns. The flags' own help
/// text is still shown, so the panel keeps naming every engine option.
fn document_type(ui: &mut Ui, opts: &mut Options) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum DocType {
        Html5,
        Html4,
        Xhtml,
    }

    let html5 = cli::get_opt(opts, Opt::Html5) == "true";
    let xhtml = cli::get_opt(opts, Opt::Xhtml) == "true";
    let current = if xhtml {
        DocType::Xhtml
    } else if html5 {
        DocType::Html5
    } else {
        DocType::Html4
    };
    let mut choice = current;

    ui.label("Document type");
    ui.horizontal(|ui| {
        ui.radio_value(&mut choice, DocType::Html5, "HTML5");
        ui.radio_value(&mut choice, DocType::Html4, "HTML4");
        ui.radio_value(&mut choice, DocType::Xhtml, "XHTML");
    });
    ui.label(egui::RichText::new(Opt::Html5.help()).small().weak());
    ui.label(egui::RichText::new(Opt::Xhtml.help()).small().weak());
    let reset = group_reset(ui, opts, &[Opt::Html5, Opt::Xhtml]);

    if choice == current {
        return reset;
    }
    // `html5` and `xhtml` are mutually exclusive in the engine; selecting the
    // pair off gives the reference's HTML 4.01.
    match choice {
        DocType::Html5 => {
            let _ = cli::set_opt(opts, Opt::Html5, "true");
        }
        DocType::Xhtml => {
            let _ = cli::set_opt(opts, Opt::Xhtml, "true");
        }
        DocType::Html4 => {
            let _ = cli::set_opt(opts, Opt::Html5, "false");
            let _ = cli::set_opt(opts, Opt::Xhtml, "false");
        }
    }
    true
}

/// A help string as the small, weak caption the panel uses for an option whose
/// widget is drawn by a group rather than by the generic loop.
fn help_label(ui: &mut Ui, opt: Opt) {
    ui.label(egui::RichText::new(opt.help()).small().weak());
}

/// Render the template sources as one choice. The engine refuses a document
/// with two of `--body_template`, `--document_template` and
/// `--template_library` set, so the panel offers a single radio with one field
/// instead of three checkboxes that can collide. Only the selected source's
/// own field is drawn; the others keep their help text visible so the panel
/// still names every engine option.
///
/// `--template_library` holds either a shipped name (`article`, `book`, …) or
/// a `pack-id:name` address from a discovered pack (roadmap §5.4). Those are
/// two different mental models — a fixed built-in set versus whatever packs are
/// installed — so they get two radios: "Shipped" takes a name, "Discovered"
/// offers the discovered addresses as a menu.
///
/// `enabled` is `false` when `--extract` (Body only) is on, because the engine
/// refuses a template together with body-only output.
fn template_source(
    ui: &mut Ui,
    opts: &mut Options,
    remembered: &mut TemplateSource,
    enabled: bool,
) -> bool {
    let discovered = discovered_addresses(opts);
    let current = if !opts.template_library.is_empty() {
        if discovered.iter().any(|a| a == &opts.template_library) {
            TemplateSource::Discovered
        } else {
            TemplateSource::Library
        }
    } else if !opts.document_template.is_empty() {
        TemplateSource::Document
    } else if !opts.template.is_empty() {
        TemplateSource::Body
    } else {
        *remembered
    };

    ui.add_enabled_ui(enabled, |ui| {
        let mut choice = current;

        let label = ui.label("Template source");
        if !enabled {
            label.on_hover_text("A template cannot be used with \"Body only\" output.");
        }
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(&mut choice, TemplateSource::None, "No template");
            ui.radio_value(&mut choice, TemplateSource::Body, "Body wrapper");
            ui.radio_value(&mut choice, TemplateSource::Document, "Whole document");
            ui.radio_value(&mut choice, TemplateSource::Library, "Shipped");
            ui.radio_value(&mut choice, TemplateSource::Discovered, "Discovered");
        });

        let mut changed = false;
        if choice != current {
            // Mutually exclusive: clear the others before the new one is entered.
            let _ = cli::set_opt(opts, Opt::BodyTemplate, "");
            let _ = cli::set_opt(opts, Opt::DocumentTemplate, "");
            let _ = cli::set_opt(opts, Opt::TemplateLibrary, "");
            changed = true;
        }
        // A `--template_library` chosen as Shipped but not one of the five
        // shipped names may still be a valid address the user typed; a move
        // between the two library radios keeps the field, so only a deliberate
        // change to a non-library source clears it.
        *remembered = choice;

        match choice {
            TemplateSource::None => {
                help_label(ui, Opt::BodyTemplate);
                help_label(ui, Opt::DocumentTemplate);
                help_label(ui, Opt::TemplateLibrary);
            }
            TemplateSource::Body => {
                changed |= string(ui, opts, Opt::BodyTemplate, true);
                help_label(ui, Opt::DocumentTemplate);
                help_label(ui, Opt::TemplateLibrary);
            }
            TemplateSource::Document => {
                changed |= string(ui, opts, Opt::DocumentTemplate, true);
                help_label(ui, Opt::BodyTemplate);
                help_label(ui, Opt::TemplateLibrary);
            }
            TemplateSource::Library => {
                changed |= string(ui, opts, Opt::TemplateLibrary, true);
                help_label(ui, Opt::BodyTemplate);
                help_label(ui, Opt::DocumentTemplate);
            }
            TemplateSource::Discovered => {
                changed |= discovered_choice(ui, opts, &discovered);
                help_label(ui, Opt::BodyTemplate);
                help_label(ui, Opt::DocumentTemplate);
            }
        }
        changed
    })
    .inner
}

/// The `pack-id:name` addresses of every discovered (non-shipped) template,
/// discovered through the engine's own catalog so the menu is exactly what
/// `--list_templates` prints.
fn discovered_addresses(opts: &Options) -> Vec<String> {
    let catalog = textrill::pack::discover(&opts.search_path());
    let mut addresses: Vec<String> = catalog
        .entries
        .iter()
        .filter(|e| e.source != textrill::pack::Source::Shipped)
        .map(textrill::pack::Entry::address)
        .collect();
    addresses.sort();
    addresses
}

/// The "Discovered" source's field: a menu of the discovered addresses plus a
/// free-text box, so a pack installed since the window opened can still be
/// typed. The menu lists nothing when no packs are discovered, which is itself
/// the answer to "why can I not pick one".
fn discovered_choice(ui: &mut Ui, opts: &mut Options, discovered: &[String]) -> bool {
    let current = opts.template_library.clone();
    let mut chosen = current.clone();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Template");
        let selected = if current.is_empty() {
            "(pick a discovered template)"
        } else {
            current.as_str()
        };
        egui::ComboBox::from_id_salt("discovered-template")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                if discovered.is_empty() {
                    ui.weak("No discovered packs. Add one under .textrill/templates.");
                }
                for address in discovered {
                    if ui.selectable_label(current == *address, address).clicked() {
                        chosen = address.clone();
                    }
                }
            });
        ui.label(
            egui::RichText::new(Opt::TemplateLibrary.help())
                .small()
                .weak(),
        );
    });
    if chosen != current {
        if let Err(message) = cli::set_opt(opts, Opt::TemplateLibrary, &chosen) {
            ui.colored_label(ui.visuals().error_fg_color, message);
        } else {
            changed = true;
        }
    }
    changed
}

/// Render the document title as one choice over the engine's precedence:
/// an explicit `--title` wins, `--titlefirst` (the document's own first line)
/// is the fallback, and neither means no title. When "First line" is chosen the
/// explicit title is cleared, so the main window's Title box and this radio
/// cannot disagree; "Explicit title" reveals the field.
fn title_source(ui: &mut Ui, opts: &mut Options, remembered: &mut TitleSource) -> bool {
    let current = if !opts.title.is_empty() {
        TitleSource::Explicit
    } else if opts.titlefirst {
        TitleSource::FirstLine
    } else {
        *remembered
    };
    let mut choice = current;

    ui.label("Title source");
    ui.horizontal(|ui| {
        ui.radio_value(&mut choice, TitleSource::None, "No title");
        ui.radio_value(&mut choice, TitleSource::FirstLine, "First line");
        ui.radio_value(&mut choice, TitleSource::Explicit, "Explicit title");
    });

    let mut changed = false;
    if choice != current {
        changed = true;
        match choice {
            TitleSource::None => {
                let _ = cli::set_opt(opts, Opt::Titlefirst, "false");
                let _ = cli::set_opt(opts, Opt::Title, "");
            }
            TitleSource::FirstLine => {
                let _ = cli::set_opt(opts, Opt::Titlefirst, "true");
                let _ = cli::set_opt(opts, Opt::Title, "");
            }
            TitleSource::Explicit => {
                let _ = cli::set_opt(opts, Opt::Titlefirst, "false");
            }
        }
    }
    *remembered = choice;

    let reset = group_reset(ui, opts, &[Opt::Title, Opt::Titlefirst]);
    match choice {
        TitleSource::Explicit => {
            changed |= string(ui, opts, Opt::Title, true);
            help_label(ui, Opt::Titlefirst);
        }
        TitleSource::None | TitleSource::FirstLine => {
            help_label(ui, Opt::Title);
            help_label(ui, Opt::Titlefirst);
        }
    }
    changed = changed || reset;
    changed
}

/// Render the two link flags as one mode. `--link_only` keeps only the links,
/// `--make_links` off leaves them as plain text, and the engine clears the
/// link dictionaries when links are off; one radio keeps the states coherent.
fn link_mode(ui: &mut Ui, opts: &mut Options) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Off,
        Links,
        Only,
    }

    let current = if opts.link_only {
        Mode::Only
    } else if opts.make_links {
        Mode::Links
    } else {
        Mode::Off
    };
    let mut choice = current;

    ui.label("Link handling");
    ui.horizontal(|ui| {
        ui.radio_value(&mut choice, Mode::Off, "No links");
        ui.radio_value(&mut choice, Mode::Links, "Make links");
        ui.radio_value(&mut choice, Mode::Only, "Links only");
    });
    help_label(ui, Opt::MakeLinks);
    help_label(ui, Opt::LinkOnly);
    let reset = group_reset(ui, opts, &[Opt::MakeLinks, Opt::LinkOnly]);

    if choice == current {
        return reset;
    }
    match choice {
        Mode::Off => {
            let _ = cli::set_opt(opts, Opt::MakeLinks, "false");
            let _ = cli::set_opt(opts, Opt::LinkOnly, "false");
        }
        Mode::Links => {
            let _ = cli::set_opt(opts, Opt::MakeLinks, "true");
            let _ = cli::set_opt(opts, Opt::LinkOnly, "false");
        }
        Mode::Only => {
            let _ = cli::set_opt(opts, Opt::MakeLinks, "true");
            let _ = cli::set_opt(opts, Opt::LinkOnly, "true");
        }
    }
    true
}

/// Render the input encoding as a radio rather than asking the user to spell an
/// alias correctly. The Unicode choices come first; the single-byte legacy
/// encodings follow.
fn encoding_choice(ui: &mut Ui, opts: &mut Options) -> bool {
    const UNICODE: &[(Encoding, &str)] = &[
        (Encoding::Auto, "Auto"),
        (Encoding::Utf8, "UTF-8"),
        (Encoding::Utf16Le, "UTF-16 LE"),
        (Encoding::Utf16Be, "UTF-16 BE"),
        (Encoding::Utf32Le, "UTF-32 LE"),
        (Encoding::Utf32Be, "UTF-32 BE"),
    ];
    const LEGACY: &[(Encoding, &str)] = &[
        (Encoding::Cp1252, "Windows-1252"),
        (Encoding::Latin1, "Latin-1"),
        (Encoding::Cp1251, "Windows-1251"),
        (Encoding::Cp1253, "Windows-1253"),
        (Encoding::Koi8R, "KOI8-R"),
    ];

    let mut choice = opts.encoding;
    ui.label("Input encoding");
    help_label(ui, Opt::Encoding);
    ui.horizontal_wrapped(|ui| {
        for &(encoding, label) in UNICODE {
            ui.radio_value(&mut choice, encoding, label);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for &(encoding, label) in LEGACY {
            ui.radio_value(&mut choice, encoding, label);
        }
    });
    let mut reset = group_reset(ui, opts, &[Opt::Encoding]);

    if choice != opts.encoding {
        opts.encoding = choice;
        reset = true;
    }
    reset
}

/// Whether an option's *name or help* matches the (already lowercased) needle.
fn opt_matches_name(opt: Opt, needle: &str) -> bool {
    for name in opt.names() {
        if name.to_lowercase().contains(needle) {
            return true;
        }
        // Also match underscores as spaces to be forgiving.
        if name.replace('_', " ").to_lowercase().contains(needle) {
            return true;
        }
    }
    opt.help().to_lowercase().contains(needle)
}

/// Whether an option matches the needle: its names, its help, and its current
/// *value*, so a search for `zh-Hans` or `true` finds the option carrying that
/// value rather than only the option named for it.
fn opt_matches(opt: Opt, needle: &str, opts: &Options) -> bool {
    opt_matches_name(opt, needle) || cli::get_opt(opts, opt).to_lowercase().contains(needle)
}

/// Whether `opt` differs from the engine default, exactly as the panel's
/// changed badge defines it: the canonical spelling of the live value against
/// the canonical spelling of [`Options::default`].
pub fn changed_from_default(opt: Opt, opts: &Options) -> bool {
    cli::get_opt(opts, opt) != cli::get_opt(&Options::default(), opt)
}

/// [`changed_from_default`] for the option a drawn group stands for: the two
/// document-type flags, the three template sources and the two link modes are
/// drawn once, so the "Show only changed" filter keeps the row when any member
/// has moved off the default.
fn any_changed(opt: Opt, opts: &Options) -> bool {
    let same = |a: Opt, b: Opt| changed_from_default(a, opts) || changed_from_default(b, opts);
    match opt {
        Opt::Html5 | Opt::Xhtml => same(Opt::Html5, Opt::Xhtml),
        Opt::BodyTemplate | Opt::DocumentTemplate | Opt::TemplateLibrary => {
            changed_from_default(Opt::BodyTemplate, opts)
                || changed_from_default(Opt::DocumentTemplate, opts)
                || changed_from_default(Opt::TemplateLibrary, opts)
        }
        Opt::MakeLinks | Opt::LinkOnly => same(Opt::MakeLinks, Opt::LinkOnly),
        Opt::Title | Opt::Titlefirst => same(Opt::Title, Opt::Titlefirst),
        other => changed_from_default(other, opts),
    }
}

/// Put one option back to its engine default, the row-local counterpart of
/// "Reset all options". Returns `true` (an edit), since the value moved.
fn reset_opt(opts: &mut Options, opt: Opt) -> bool {
    if opt.kind() == Kind::StrArray {
        clear_array(opts, opt);
        return true;
    }
    let default = cli::get_opt(&Options::default(), opt);
    cli::set_opt(opts, opt, &default).is_ok()
}

/// The changed badge plus a one-option reset, drawn only while the option is
/// off its default, so the panel's answer to "what did I change?" sits next to
/// the control itself.
fn option_reset(ui: &mut Ui, opts: &mut Options, opt: Opt) -> bool {
    if !changed_from_default(opt, opts) {
        return false;
    }
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("changed")
                .small()
                .strong()
                .color(ui.visuals().warn_fg_color),
        )
        .on_hover_text("This option differs from the engine default.");
        if ui
            .small_button("\u{21ba}")
            .on_hover_text("Reset this option to its default")
            .clicked()
        {
            clicked = true;
        }
    });
    if clicked {
        reset_opt(opts, opt)
    } else {
        false
    }
}

/// [`option_reset`] for a widget that draws several options as one control: the
/// badge appears once, from the first member that has changed, and resets every
/// member when clicked.
fn group_reset(ui: &mut Ui, opts: &mut Options, members: &[Opt]) -> bool {
    if !members.iter().any(|o| changed_from_default(*o, opts)) {
        return false;
    }
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("changed")
                .small()
                .strong()
                .color(ui.visuals().warn_fg_color),
        )
        .on_hover_text("These options differ from the engine default.");
        if ui
            .small_button("\u{21ba}")
            .on_hover_text("Reset them to their defaults")
            .clicked()
        {
            clicked = true;
        }
    });
    if clicked {
        let mut moved = false;
        for opt in members {
            moved |= reset_opt(opts, *opt);
        }
        moved
    } else {
        false
    }
}

/// Options the GUI structurally cannot use. The worker converts the document
/// already on screen with [`EngineConverter::convert_text`](crate::worker), so
/// a separate input (`--infile`/`--instring`), a second output (`--outfile`),
/// paragraph-at-a-time streaming (`--stream`), chunked files (`--chunk`), the
/// stderr summary (`--report`) and the print-and-exit catalog listing
/// (`--list_templates`) have nothing to act on.
///
/// They are drawn disabled (never hidden) so the panel still names every engine
/// option, but [`crate::options_store`] leaves them out of the saved set: a
/// stored value could never be acted on, so keeping it would only resurface as
/// a stale edge.
pub(crate) fn gui_ignored(opt: Opt) -> bool {
    matches!(
        opt,
        Opt::Stream
            | Opt::Chunk
            | Opt::ChunkChars
            | Opt::Report
            | Opt::Preset
            | Opt::ListTemplates
            | Opt::Infile
            | Opt::Instring
            | Opt::Outfile
    )
}

/// Whether `opt` can change the current conversion. An option whose effect is
/// governed by another is drawn disabled rather than hidden: the panel still
/// names every engine option (so nothing silently disappears) and the
/// dependency is on screen. A table's column types need tables on;
/// `--explicit_headings` numbers the custom heading regexps, so it needs at
/// least one; the link dictionaries feed the linkifier, which the engine clears
/// when links are off.
fn enabled(opt: Opt, opts: &Options) -> bool {
    if gui_ignored(opt) {
        return false;
    }
    match opt {
        Opt::TableType => opts.make_tables,
        Opt::ExplicitHeadings => !opts.custom_heading_regexp.is_empty(),
        Opt::DefaultLinkDict | Opt::LinksDictionaries => opts.make_links,
        _ => true,
    }
}

/// Why a disabled option is disabled, shown as a hover note so the panel
/// explains itself instead of leaving a dead widget. `None` when the option is
/// enabled or has no note worth showing.
fn disabled_reason(opt: Opt, opts: &Options) -> Option<&'static str> {
    if gui_ignored(opt) {
        return Some("Command-line only; this window converts the document it has open.");
    }
    if enabled(opt, opts) {
        return None;
    }
    Some(match opt {
        Opt::TableType => "Turn on table processing to choose the column types.",
        Opt::ExplicitHeadings => "Add a custom heading regexp to use this.",
        Opt::DefaultLinkDict | Opt::LinksDictionaries => {
            "Turn on link creation to use link dictionaries."
        }
        _ => return None,
    })
}

fn flag(ui: &mut Ui, opts: &mut Options, opt: Opt, enabled: bool) -> bool {
    let help = opt.help();
    let mut on = cli::get_opt(opts, opt) == "true";
    let mut changed = false;
    ui.horizontal(|ui| {
        let response = ui.add_enabled(enabled, egui::Checkbox::new(&mut on, help));
        let response = match disabled_reason(opt, opts) {
            Some(reason) => response.on_hover_text(reason),
            None => response,
        };
        if response.changed() {
            let _ = cli::set_opt(opts, opt, if on { "true" } else { "false" });
            changed = true;
        }
        changed |= option_reset(ui, opts, opt);
    });
    changed
}

fn int(ui: &mut Ui, opts: &mut Options, opt: Opt, enabled: bool) -> bool {
    let help = opt.help();
    let current = cli::get_opt(opts, opt).parse::<i64>().unwrap_or(0);
    let mut value = current;
    let mut reset = false;
    ui.horizontal(|ui| {
        let mut widget = egui::DragValue::new(&mut value).speed(1.0);
        if let Some((low, high)) = numeric_range(opt.canonical()) {
            widget = widget.range(low as i64..=high as i64);
        }
        let response = ui.add_enabled(enabled, widget);
        let label = ui.label(help);
        response.labelled_by(label.id);
        reset = option_reset(ui, opts, opt);
    });
    let moved = value != current;
    if moved {
        let _ = cli::set_opt(opts, opt, &value.to_string());
    }
    moved || reset
}

fn string(ui: &mut Ui, opts: &mut Options, opt: Opt, enabled: bool) -> bool {
    let help = opt.help();
    let mut text = cli::get_opt(opts, opt);
    let changed = ui
        .horizontal(|ui| {
            let response = ui.add_enabled(enabled, egui::TextEdit::singleline(&mut text));
            let changed = response.changed();
            let label = ui.label(help);
            let response = response.labelled_by(label.id);
            if let Some(reason) = disabled_reason(opt, opts) {
                response.on_hover_text(reason);
            }
            if let Some(reason) = disabled_reason(opt, opts) {
                label.on_hover_text(reason);
            }
            changed
        })
        .inner;
    if changed {
        if let Err(message) = cli::set_opt(opts, opt, &text) {
            ui.colored_label(ui.visuals().error_fg_color, message);
        }
    }
    let mut reset = false;
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        reset = option_reset(ui, opts, opt);
    });
    changed || reset
}

fn string_array(ui: &mut Ui, opts: &mut Options, opt: Opt, enabled: bool) -> bool {
    let help = opt.help();
    let mut text = cli::get_opt(opts, opt);
    let label = ui.label(help);
    let label_id = label.id;
    if let Some(reason) = disabled_reason(opt, opts) {
        label.on_hover_text(reason);
    }
    let response = ui.add_enabled(
        enabled,
        egui::TextEdit::multiline(&mut text).desired_rows(3),
    );
    let response = response.labelled_by(label_id);
    let changed = response.changed();
    if let Some(reason) = disabled_reason(opt, opts) {
        response.on_hover_text(reason);
    }
    if !changed {
        let mut reset = false;
        ui.horizontal(|ui| {
            reset = option_reset(ui, opts, opt);
        });
        return reset;
    }
    // `set_opt` appends to a `StrArray`; the panel edits the whole list, so
    // clear it and re-add. `var` is a `Vec<(String, String)>` rather than a
    // `Vec<String>`, but `set_opt` parses its `name=value` lines, so the
    // replace-and-re-add loop is the same for every array option and the engine
    // keeps the validation. Reading the list back (rather than assigning the
    // widgets directly) is what stops a new array option from being silently
    // dropped, as `var` was.
    clear_array(opts, opt);
    let mut error = None;
    for line in text.lines() {
        if let Err(message) = cli::set_opt(opts, opt, line) {
            error.get_or_insert(message);
        }
    }
    if let Some(message) = error {
        ui.colored_label(ui.visuals().error_fg_color, message);
    }
    ui.horizontal(|ui| {
        option_reset(ui, opts, opt);
    });
    true
}

/// Empty the `StrArray` `opt` names, so [`string_array`] can re-add its lines.
///
/// Every [`Kind::StrArray`] option needs an arm: a miss would leave the old list
/// in place and the re-add would double it. The unit test
/// `every_string_array_option_is_cleared` walks every `StrArray` option, so one
/// added without its arm fails there.
fn clear_array(opts: &mut Options, opt: Opt) {
    match opt {
        Opt::CustomHeadingRegexp => opts.custom_heading_regexp.clear(),
        Opt::Infile => opts.infile.clear(),
        Opt::Instring => opts.instring.clear(),
        Opt::LinksDictionaries => opts.links_dictionaries.clear(),
        Opt::TemplateDir => opts.template_dir.clear(),
        Opt::CatalogDir => opts.catalog_dir.clear(),
        Opt::Var => opts.vars.clear(),
        _ => {}
    }
}

fn table_type(ui: &mut Ui, opts: &mut Options, help: &str, enabled: bool) -> bool {
    ui.label(help);
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= option_reset(ui, opts, Opt::TableType);
    });
    ui.horizontal(|ui| {
        changed |= ui
            .add_enabled(
                enabled,
                egui::Checkbox::new(&mut opts.table_type.align, "ALIGN"),
            )
            .changed();
        changed |= ui
            .add_enabled(
                enabled,
                egui::Checkbox::new(&mut opts.table_type.pgsql, "PGSQL"),
            )
            .changed();
        changed |= ui
            .add_enabled(
                enabled,
                egui::Checkbox::new(&mut opts.table_type.border, "BORDER"),
            )
            .changed();
        changed |= ui
            .add_enabled(
                enabled,
                egui::Checkbox::new(&mut opts.table_type.delim, "DELIM"),
            )
            .changed();
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value that `set_value` accepts for every array option.
    fn probe(key: &str) -> &'static str {
        // `var` is the one `name=value` array; the rest take any text.
        if key == "var" {
            "probe=1"
        } else {
            "probe"
        }
    }

    #[test]
    fn every_string_array_option_is_cleared() {
        for &opt in Opt::ALL {
            if opt.kind() != Kind::StrArray {
                continue;
            }
            let key = opt.canonical();
            let mut opts = Options::default();
            cli::set_opt(&mut opts, opt, probe(key)).expect("set");
            assert!(
                !cli::get_opt(&opts, opt).is_empty(),
                "{key}: probe did not populate the list"
            );
            clear_array(&mut opts, opt);
            assert!(
                cli::get_opt(&opts, opt).is_empty(),
                "{key}: clear_array has no arm for this StrArray option"
            );
        }
    }

    /// The changed badge is exactly "the live value differs from the default",
    /// read through `cli::get_opt` on both sides — no second opinion.
    #[test]
    fn the_changed_badge_matches_get_opt_versus_defaults() {
        let defaults = Options::default();
        let mut opts = Options::default();
        for &opt in Opt::ALL {
            assert!(
                !changed_from_default(opt, &defaults),
                "{}: a default must not read as changed",
                opt.canonical()
            );
        }
        for (key, value) in [
            ("tab_width", "9"),
            ("lang", "ja"),
            ("section", "true"),
            ("title", "Doc"),
            ("template_library", "book"),
        ] {
            let opt = cli::lookup(key).expect("known option");
            cli::set_opt(&mut opts, opt, value).expect("set");
            assert!(
                changed_from_default(opt, &opts),
                "{key}: a moved value must read as changed"
            );
            assert!(
                changed_from_default(opt, &opts)
                    == (cli::get_opt(&opts, opt) != cli::get_opt(&defaults, opt)),
                "{key}: the badge must be exactly the get_opt comparison"
            );
            reset_opt(&mut opts, opt);
            assert!(!changed_from_default(opt, &opts), "{key}: reset restores");
        }
    }

    /// The filter matches the current value, not only the option's name and
    /// help, so searching for a value finds the option carrying it.
    #[test]
    fn the_filter_matches_option_values() {
        let opts = Options {
            lang: "zh-Hans".to_string(),
            ..Options::default()
        };
        assert!(opt_matches(Opt::Lang, "zh-hans", &opts));
        assert!(!opt_matches(Opt::TabWidth, "zh-hans", &opts));
        let tab = cli::get_opt(&opts, Opt::TabWidth);
        assert!(opt_matches(Opt::TabWidth, &tab, &opts));
    }

    /// An option is enabled exactly when the option it depends on is in the
    /// state that gives it an effect.
    #[test]
    fn dependent_options_follow_their_parent() {
        let mut opts = Options::default();
        assert!(opts.make_links, "links are on by default");
        assert!(enabled(Opt::DefaultLinkDict, &opts));
        assert!(!enabled(Opt::TableType, &opts), "tables are off by default");
        assert!(
            !enabled(Opt::ExplicitHeadings, &opts),
            "no custom heading regexp by default"
        );
        assert!(
            enabled(Opt::MakeTables, &opts),
            "a parent is never its own child"
        );

        // Turning off the parent disables its children.
        opts.make_links = false;
        assert!(!enabled(Opt::DefaultLinkDict, &opts));
        assert!(!enabled(Opt::LinksDictionaries, &opts));

        // Turning on the parent enables them.
        opts.make_tables = true;
        opts.custom_heading_regexp.push("^H".to_string());
        assert!(enabled(Opt::TableType, &opts));
        assert!(enabled(Opt::ExplicitHeadings, &opts));
    }

    /// The command-line pipeline options cannot affect the document the GUI
    /// converts, so they are disabled and carry a hover note explaining why.
    #[test]
    fn command_line_pipeline_options_are_disabled() {
        let opts = Options::default();
        for opt in [
            Opt::Stream,
            Opt::Chunk,
            Opt::ChunkChars,
            Opt::Report,
            Opt::Infile,
            Opt::Instring,
            Opt::Outfile,
        ] {
            assert!(
                !enabled(opt, &opts),
                "{} should be disabled",
                opt.canonical()
            );
            assert!(
                disabled_reason(opt, &opts).is_some(),
                "{} should explain itself",
                opt.canonical()
            );
        }
        // An option that is enabled needs no note.
        assert!(disabled_reason(Opt::MakeTables, &opts).is_none());
    }
}
