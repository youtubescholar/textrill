// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! A plain-language reading of the current settings.
//!
//! The options panel is exact but terse: an option set says
//! `template_library=book`, `section=true`, `toc=true`. The summary turns the
//! same option set into the sentences a person would use, so the landing view
//! can answer "what is about to happen?" without reading the panel. It is
//! derived from the live options every time it is drawn, so it cannot fall out
//! of step with them.
//!
//! It is deliberately one string rather than a row of labelled widgets: the
//! words "Title", "Template" and "Preset" are already widget labels elsewhere
//! in the window, and a summary that repeated them as labels would make a
//! by-label query (and a screen reader's list of controls) ambiguous. As prose
//! it reads as one thing, which is what a summary is.

use textrill::options::Options;

use crate::presets::Preset;

/// Describe the current settings as a short, multi-line paragraph.
///
/// `preset` and `edited` name the chosen starting point and whether the options
/// have been changed away from its bundle, so the summary agrees with the
/// preset strip above it.
pub fn describe(opts: &Options, preset: &Preset, edited: bool) -> String {
    let preset_line = if edited {
        format!("Preset: {} (edited).", preset.name)
    } else {
        format!("Preset: {}.", preset.name)
    };
    [
        preset_line,
        format!("Uses {}, {}.", template_phrase(opts), outline_phrase(opts)),
        encoding_phrase(opts),
        title_phrase(opts),
    ]
    .join("\n")
}

/// Which template (if any) the options select.
fn template_phrase(opts: &Options) -> String {
    if !opts.template_library.is_empty() {
        format!("the shipped \"{}\" template", opts.template_library)
    } else if !opts.template.is_empty() {
        format!("your template file {}", opts.template)
    } else if !opts.document_template.is_empty() {
        format!("your whole-page template {}", opts.document_template)
    } else {
        "no template (a bare HTML fragment)".to_string()
    }
}

/// How the content is broken up.
fn outline_phrase(opts: &Options) -> &'static str {
    match (opts.section, opts.toc) {
        (true, true) => "split into sections with a generated table of contents",
        (true, false) => "split into sections",
        (false, true) => "with a generated table of contents",
        (false, false) => "as a single page",
    }
}

/// What is read and what is written. The GUI always writes UTF-8 and declares
/// it, whatever the input turns out to be.
fn encoding_phrase(opts: &Options) -> String {
    format!("Reads {} text and writes UTF-8 HTML.", opts.encoding.name())
}

/// The document title, if one is set.
fn title_phrase(opts: &Options) -> String {
    if opts.title.trim().is_empty() {
        "Title: none.".to_string()
    } else {
        format!("Title: \u{201c}{}\u{201d}.", opts.title.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;

    fn book() -> &'static Preset {
        presets::get("book").expect("book preset")
    }

    #[test]
    fn a_plain_document_says_so() {
        let text = describe(&Options::default(), presets::default_preset(), false);
        assert!(text.contains("Preset: Plain."), "{text}");
        assert!(text.contains("no template"), "{text}");
        assert!(text.contains("as a single page"), "{text}");
        assert!(text.contains("Title: none."), "{text}");
    }

    #[test]
    fn a_preset_is_named_and_its_bundle_described() {
        let text = describe(&book().options(), book(), false);
        assert!(text.contains("Preset: Book."), "{text}");
        assert!(text.contains("shipped \"book\" template"), "{text}");
        assert!(text.contains("table of contents"), "{text}");
    }

    #[test]
    fn editing_away_from_the_preset_is_visible_in_the_summary() {
        let text = describe(&book().options(), book(), true);
        assert!(text.contains("Preset: Book (edited)."), "{text}");
    }

    #[test]
    fn a_title_and_a_file_template_are_named() {
        let opts = Options {
            title: "My Document".to_string(),
            template: "wrapper.html".to_string(),
            ..Options::default()
        };
        let text = describe(&opts, presets::default_preset(), false);
        assert!(text.contains("your template file wrapper.html"), "{text}");
        assert!(
            text.contains("Title: \u{201c}My Document\u{201d}."),
            "{text}"
        );
    }
}
