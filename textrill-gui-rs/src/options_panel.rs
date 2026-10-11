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
use textrill::cli::{self, Kind, Spec};
use textrill::options::{numeric_range, Options};

/// The options form and its filter.
#[derive(Debug, Default)]
pub struct OptionsPanel {
    /// The filter box contents.
    pub filter: String,
    /// Currently selected category when not filtering.
    selected_category: usize,
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

fn spec_category(spec: &Spec) -> &'static str {
    let key = spec.names[0];
    match key {
        "infile" | "instring" | "outfile" | "encoding" | "utf8" | "eight_bit_clean" | "stream"
        | "report" => "Input & Output",
        "doctype" | "html5" | "xhtml" | "meta_charset" | "extract" | "chunk" | "section"
        | "style_url" | "document_template" | "body_template" | "template_library"
        | "prepend_file" | "append_file" | "append_head" | "body_deco" | "var" => "Document & HTML",
        "demoronize"
        | "bold_delimiter"
        | "italic_delimiter"
        | "underline_delimiter"
        | "underline_length_tolerance"
        | "underline_offset_tolerance"
        | "unhyphenation"
        | "caps_tag"
        | "min_caps_length"
        | "lang"
        | "lang_runs"
        | "escape_HTML_chars"
        | "lower_case_tags" => "Text Formatting",
        "custom_heading_regexp"
        | "explicit_headings"
        | "number_headings"
        | "toc"
        | "make_anchors"
        | "title"
        | "titlefirst" => "Headings & TOC",
        "make_links"
        | "link_only"
        | "allowed_url_schemes"
        | "default_link_dict"
        | "links_dictionaries"
        | "no_referrer" => "Links & URLs",
        "citations" | "glossary" => "References",
        "bullets" | "bullets_ordered" | "preserve_indent" => "Lists",
        "mailmode" | "use_mosaic_header" => "Mail & Headers",
        "par_indent" | "indent_par_break" | "indent_width" | "short_line_length" => "Paragraphs",
        "preformat_trigger_lines"
        | "endpreformat_trigger_lines"
        | "preformat_start_marker"
        | "preformat_end_marker"
        | "preformat_whitespace_min"
        | "use_preformat_marker" => "Preformatting",
        "make_tables" | "table_type" => "Tables",
        "tab_width" | "hrule_min" => "Layout & Spacing",
        _ => "Other",
    }
}

impl OptionsPanel {
    /// Draw the filter row and every option it lets through.
    ///
    /// Returns whether any widget was edited, so the caller can re-convert.
    pub fn draw(&mut self, ui: &mut Ui, opts: &mut Options) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter options…")
                    .desired_width(160.0),
            );
            if ui
                .button("Reset all options")
                .on_hover_text("Put every option back to its default")
                .clicked()
            {
                *opts = Options::default();
                changed = true;
            }
        });

        let needle = self.filter.trim().to_lowercase();
        let filtering = !needle.is_empty();

        if !filtering {
            ui.horizontal(|ui| {
                ui.label("Category:");
                egui::ComboBox::from_id_salt("options_category")
                    .width(200.0)
                    .selected_text(CATEGORIES[self.selected_category])
                    .show_ui(ui, |ui| {
                        for (idx, cat) in CATEGORIES.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_category, idx, *cat);
                        }
                    });
            });
        }

        let selected_cat = CATEGORIES[self.selected_category];
        for spec in cli::SPECS {
            if filtering {
                if !spec_matches(spec, &needle) {
                    continue;
                }
            } else if selected_cat != "All" && spec_category(spec) != selected_cat {
                continue;
            }
            let key = spec.names[0];
            changed |= match spec.kind {
                Kind::Flag => flag(ui, opts, key, spec.help),
                Kind::Int => int(ui, opts, key, spec.help),
                Kind::Str => string(ui, opts, key, spec.help),
                Kind::StrArray => string_array(ui, opts, key, spec.help),
                Kind::TableType => table_type(ui, opts, spec.help),
            };
        }
        changed
    }

    /// Clear the filter, as the Python implementation did.
    pub fn clear_filter(&mut self) {
        self.filter.clear();
    }

    /// Test whether an option specification matches the given needle.
    pub fn matches(&self, name: &str) -> bool {
        let needle = self.filter.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        for spec in cli::SPECS {
            if spec.names.contains(&name) {
                return spec_matches(spec, &needle);
            }
        }
        false
    }
}

/// Whether an option matches the (already lowercased) needle.
fn spec_matches(spec: &cli::Spec, needle: &str) -> bool {
    for name in spec.names {
        if name.to_lowercase().contains(needle) {
            return true;
        }
        // Also match underscores as spaces to be forgiving.
        if name.replace('_', " ").to_lowercase().contains(needle) {
            return true;
        }
    }
    if spec.help.to_lowercase().contains(needle) {
        return true;
    }
    false
}

fn flag(ui: &mut Ui, opts: &mut Options, key: &str, help: &str) -> bool {
    let mut on = cli::get_value(opts, key).unwrap_or_default() == "true";
    if ui.checkbox(&mut on, help).changed() {
        let _ = cli::set_value(opts, key, if on { "true" } else { "false" });
        true
    } else {
        false
    }
}

fn int(ui: &mut Ui, opts: &mut Options, key: &str, help: &str) -> bool {
    let current = cli::get_value(opts, key)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let mut value = current;
    ui.horizontal(|ui| {
        let mut widget = egui::DragValue::new(&mut value).speed(1.0);
        if let Some((low, high)) = numeric_range(key) {
            widget = widget.range(low as i64..=high as i64);
        }
        ui.add(widget);
        ui.label(help);
    });
    if value != current {
        let _ = cli::set_value(opts, key, &value.to_string());
        true
    } else {
        false
    }
}

fn string(ui: &mut Ui, opts: &mut Options, key: &str, help: &str) -> bool {
    let mut text = cli::get_value(opts, key).unwrap_or_default();
    let changed = ui
        .horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut text);
            ui.label(help);
            response.changed()
        })
        .inner;
    if changed {
        if let Err(message) = cli::set_value(opts, key, &text) {
            ui.colored_label(egui::Color32::RED, message);
        }
    }
    changed
}

fn string_array(ui: &mut Ui, opts: &mut Options, key: &str, help: &str) -> bool {
    let mut text = cli::get_value(opts, key).unwrap_or_default();
    ui.label(help);
    if !ui
        .add(egui::TextEdit::multiline(&mut text).desired_rows(3))
        .changed()
    {
        return false;
    }
    // `set_value` appends to a `StrArray`; the panel edits the whole list, so
    // clear it and re-add. `var` is a `Vec<(String, String)>` rather than a
    // `Vec<String>`, but `set_value` parses its `name=value` lines, so the
    // replace-and-re-add loop is the same for every array option and the engine
    // keeps the validation. Reading the list back (rather than assigning the
    // widgets directly) is what stops a new array option from being silently
    // dropped, as `var` was.
    clear_array(opts, key);
    let mut error = None;
    for line in text.lines() {
        if let Err(message) = cli::set_value(opts, key, line) {
            error.get_or_insert(message);
        }
    }
    if let Some(message) = error {
        ui.colored_label(egui::Color32::RED, message);
    }
    true
}

/// Empty the `StrArray` `key` names, so [`string_array`] can re-add its lines.
fn clear_array(opts: &mut Options, key: &str) {
    match key {
        "custom_heading_regexp" => opts.custom_heading_regexp.clear(),
        "infile" => opts.infile.clear(),
        "instring" => opts.instring.clear(),
        "links_dictionaries" => opts.links_dictionaries.clear(),
        "var" => opts.vars.clear(),
        _ => {}
    }
}

fn table_type(ui: &mut Ui, opts: &mut Options, help: &str) -> bool {
    ui.label(help);
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.checkbox(&mut opts.table_type.align, "ALIGN").changed();
        changed |= ui.checkbox(&mut opts.table_type.pgsql, "PGSQL").changed();
        changed |= ui.checkbox(&mut opts.table_type.border, "BORDER").changed();
        changed |= ui.checkbox(&mut opts.table_type.delim, "DELIM").changed();
    });
    changed
}
