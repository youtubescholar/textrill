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

use egui::Ui;
use textrill::cli::{self, Kind};
use textrill::options::{numeric_range, Options};

/// Draw every option, in the engine table's order.
///
/// Returns whether any widget was edited, so the caller can re-convert.
pub fn draw(ui: &mut Ui, opts: &mut Options) -> bool {
    let mut changed = false;
    for spec in cli::SPECS {
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

fn flag(ui: &mut Ui, opts: &mut Options, key: &str, help: &str) -> bool {
    let mut on = cli::get_value(opts, key)
        .map(|v| v == "true")
        .unwrap_or(false);
    if key == "utf8" {
        // `utf8` is accepted for compatibility and does nothing; `get_value`
        // reports it as "1" and `set_value` ignores writes. Render it disabled
        // rather than as a checkbox that silently springs back.
        ui.add_enabled(false, egui::Checkbox::new(&mut on, help));
        return false;
    }
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
    if ui
        .add(egui::TextEdit::multiline(&mut text).desired_rows(3))
        .changed()
    {
        // `set_value` appends to a `StrArray`; the panel edits the whole list,
        // so replace it. The four array options are the only ones that need
        // this, and naming them here keeps the engine free of a GUI-shaped API.
        let entries: Vec<String> = text.lines().map(str::to_string).collect();
        match key {
            "custom_heading_regexp" => opts.custom_heading_regexp = entries,
            "infile" => opts.infile = entries,
            "instring" => opts.instring = entries,
            "links_dictionaries" => opts.links_dictionaries = entries,
            _ => {}
        }
        true
    } else {
        false
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
