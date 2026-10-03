// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Tests for the window's command line.

use std::ffi::OsString;
use std::path::PathBuf;

use textrill_gui::args::{self, Args};

fn parse(items: &[&str]) -> Result<Args, String> {
    args::parse(items.iter().map(OsString::from))
}

#[test]
fn no_arguments_starts_the_default_window() {
    assert_eq!(parse(&[]).unwrap(), Args::default());
}

#[test]
fn a_positional_file_is_kept() {
    assert_eq!(
        parse(&["notes.txt"]).unwrap().file,
        Some(PathBuf::from("notes.txt"))
    );
}

#[test]
fn the_dialect_flags_choose_xhtml_or_html4() {
    assert_eq!(parse(&["--xhtml"]).unwrap().xhtml, Some(true));
    assert_eq!(parse(&["--no-xhtml"]).unwrap().xhtml, Some(false));
    assert_eq!(parse(&[]).unwrap().xhtml, None);
}

#[test]
fn tables_turns_on_table_recognition() {
    assert!(parse(&["--tables"]).unwrap().tables);
}

#[test]
fn version_and_help_are_recognized() {
    assert!(parse(&["--version"]).unwrap().version);
    assert!(parse(&["-h"]).unwrap().help);
    assert!(parse(&["--help"]).unwrap().help);
}

#[test]
fn a_second_file_is_an_error() {
    assert!(parse(&["one.txt", "two.txt"]).is_err());
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(parse(&["--nope"]).is_err());
}
