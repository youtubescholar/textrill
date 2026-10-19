// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The Ctrl/Cmd+K command palette: one searchable list over the actions the
//! menus already offer, plus the presets.
//!
//! This module holds only the *vocabulary* — what the commands are called, how
//! they are grouped, and how a typed query matches one. Running a command
//! needs the whole app, so [`crate::app::TextrillApp`] owns the dispatch and
//! the drawing; keeping this half free of that dependency is what lets the
//! fuzzy matching and labels be unit-tested on their own.

use crate::presets;

/// One entry the palette can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    Save,
    SaveText,
    Convert,
    CopyHtml,
    Example,
    ShowOptions,
    FocusMode,
    ViewSplit,
    ViewSource,
    ViewHtml,
    ResetWindow,
    /// A preset, as an index into [`presets::PRESETS`].
    Preset(usize),
}

impl Command {
    /// Every command, presets included, in a stable display order.
    pub fn all() -> Vec<Command> {
        let mut commands = vec![
            Command::New,
            Command::Open,
            Command::Save,
            Command::SaveText,
            Command::Convert,
            Command::CopyHtml,
            Command::Example,
            Command::ShowOptions,
            Command::FocusMode,
            Command::ViewSplit,
            Command::ViewSource,
            Command::ViewHtml,
            Command::ResetWindow,
        ];
        commands.extend((0..presets::PRESETS.len()).map(Command::Preset));
        commands
    }

    /// The text shown in the list, and the text [`Command::matches`] searches.
    pub fn label(&self) -> String {
        match self {
            Command::Preset(index) => format!("Preset: {}", presets::PRESETS[*index].name),
            Command::New => String::from("New document"),
            Command::Open => String::from("Open file"),
            Command::Save => String::from("Save"),
            Command::SaveText => String::from("Save text"),
            Command::Convert => String::from("Convert now"),
            Command::CopyHtml => String::from("Copy HTML"),
            Command::Example => String::from("Load example"),
            Command::ShowOptions => String::from("Show options"),
            Command::FocusMode => String::from("Toggle focus mode"),
            Command::ViewSplit => String::from("View: source and HTML"),
            Command::ViewSource => String::from("View: source only"),
            Command::ViewHtml => String::from("View: HTML only"),
            Command::ResetWindow => String::from("Reset window size and zoom"),
        }
    }

    /// The group shown beside each row, so results read as
    /// `Load example · file`.
    pub fn category(&self) -> &'static str {
        match self {
            Command::Preset(_) => "preset",
            Command::ViewSplit | Command::ViewSource | Command::ViewHtml => "view",
            Command::New | Command::Open | Command::Save | Command::SaveText | Command::Example => {
                "file"
            }
            Command::Convert | Command::CopyHtml => "convert",
            Command::ShowOptions | Command::FocusMode | Command::ResetWindow => "window",
        }
    }

    /// Whether `query` selects this command. An empty query matches
    /// everything, so the palette opens showing the whole list.
    pub fn matches(&self, query: &str) -> bool {
        fuzzy_match(query, &self.label())
    }
}

/// A subsequence match, case-insensitive: every query character must appear in
/// the target, in order, so `nwd` finds "New document" and `vh` finds
/// "View: HTML only". Chosen over a substring match because the palette is
/// meant to be typed at, not browsed; over a scored match because a stable
/// ranking is easier to reason about than a fuzzy heuristic.
///
/// An empty query matches everything (short-circuit before walking the
/// target).
pub fn fuzzy_match(query: &str, target: &str) -> bool {
    let mut query_chars = query.chars().flat_map(char::to_lowercase).peekable();
    // Empty, or folds away to nothing: matches everything.
    if query_chars.peek().is_none() {
        return true;
    }
    for target_char in target.chars().flat_map(char::to_lowercase) {
        if Some(target_char) == query_chars.peek().copied() {
            query_chars.next();
            if query_chars.peek().is_none() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_query_matches_every_command() {
        for command in Command::all() {
            assert!(command.matches(""), "empty query should show {:?}", command);
        }
    }

    #[test]
    fn every_command_is_found_by_its_own_label() {
        for command in Command::all() {
            assert!(
                command.matches(&command.label()),
                "{:?} should match its own label",
                command
            );
        }
    }

    #[test]
    fn a_subsequence_query_finds_a_command() {
        assert!(fuzzy_match("nwd", "New document"));
        assert!(fuzzy_match("NEW", "New document"));
        assert!(fuzzy_match("vh", "View: HTML only"));
        assert!(fuzzy_match("so", "Show options"));
    }

    #[test]
    fn a_query_that_skips_or_reorders_letters_does_not_match() {
        assert!(!fuzzy_match("dwn", "New document"));
        assert!(!fuzzy_match("zzz", "New document"));
        assert!(!fuzzy_match("documents new", "New document"));
    }

    #[test]
    fn commands_carry_a_stable_category() {
        assert_eq!(Command::Example.category(), "file");
        assert_eq!(Command::Convert.category(), "convert");
        assert_eq!(Command::ViewHtml.category(), "view");
        assert_eq!(Command::Preset(0).category(), "preset");
        assert_eq!(Command::ResetWindow.category(), "window");
    }

    #[test]
    fn presets_are_spelled_out_by_name() {
        let preset = Command::Preset(0);
        assert_eq!(preset.label(), "Preset: Plain");
        assert!(preset.matches("Plain"));
    }

    #[test]
    fn the_list_has_one_entry_per_preset_plus_the_actions() {
        assert_eq!(
            Command::all().len(),
            13 + presets::PRESETS.len(),
            "the base actions and every preset should be reachable"
        );
        let presets_in_list = Command::all()
            .iter()
            .filter(|command| matches!(command, Command::Preset(_)))
            .count();
        assert_eq!(presets_in_list, presets::PRESETS.len());
    }
}
