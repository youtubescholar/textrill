// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Serialise the whole option set to the JSON blob the Python GUI stored.
//!
//! `mainwindow.py` persisted `optionspanel.values()`, keyed by each option's
//! canonical name and typed the way the widget was (bool, int, str, list of
//! str, or a `table_type` object). This reproduces that shape from `cli::SPECS`,
//! so a settings file written by either front end restores in the other.

use textrill::cli::{self, Kind};
use textrill::options::Options;

use crate::json::{self, Json};
use crate::settings::parse_bool;

/// The option set as a JSON object, in `cli::SPECS` order.
pub fn encode(opts: &Options) -> String {
    let mut entries: Vec<(&str, Json)> = Vec::new();
    for spec in cli::SPECS {
        let name = spec.names[0];
        let value = match spec.kind {
            Kind::Flag => Json::Bool(parse_bool(&cli::get_value(opts, name).unwrap_or_default())),
            Kind::Int => Json::Number(
                cli::get_value(opts, name)
                    .ok()
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0),
            ),
            Kind::Str => Json::Str(cli::get_value(opts, name).unwrap_or_default()),
            Kind::StrArray => Json::Array(array_of(opts, name)),
            Kind::TableType => Json::Object(vec![
                ("ALIGN".to_string(), opts.table_type.align),
                ("PGSQL".to_string(), opts.table_type.pgsql),
                ("BORDER".to_string(), opts.table_type.border),
                ("DELIM".to_string(), opts.table_type.delim),
            ]),
        };
        // The Python panel forced this on when it serialised, because the GUI
        // always writes UTF-8 and must declare it. Match that, or a later
        // restore would show the checkbox the engine defaults it to.
        let value = if name == "meta_charset" {
            Json::Bool(true)
        } else {
            value
        };
        entries.push((name, value));
    }
    json::write_object(&entries)
}

fn array_of(opts: &Options, name: &str) -> Vec<String> {
    match name {
        "custom_heading_regexp" => opts.custom_heading_regexp.clone(),
        "infile" => opts.infile.clone(),
        "instring" => opts.instring.clone(),
        "links_dictionaries" => opts.links_dictionaries.clone(),
        _ => Vec::new(),
    }
}

/// Apply a stored blob to `opts`. Unknown names and unreadable values are
/// skipped, which is what Python's `set_values` did with a bad blob.
pub fn decode(text: &str, opts: &mut Options) {
    let Some(entries) = json::parse_object(text) else {
        return;
    };
    for (name, value) in entries {
        apply(opts, &name, &value);
    }
}

fn apply(opts: &mut Options, name: &str, value: &Json) {
    let mut set = |v: &str| {
        let _ = cli::set_value(opts, name, v);
    };
    match value {
        Json::Bool(on) => set(if *on { "true" } else { "false" }),
        Json::Number(n) => set(&n.to_string()),
        Json::Str(s) => set(s),
        Json::Array(items) => {
            for item in items {
                set(item);
            }
        }
        Json::Object(pairs) => {
            let text = pairs
                .iter()
                .map(|(key, on)| format!("{key}={}", u8::from(*on)))
                .collect::<Vec<_>>()
                .join(" ");
            set(&text);
        }
    }
}
