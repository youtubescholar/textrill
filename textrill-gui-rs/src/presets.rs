// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Named starting points, in the spirit of a transcoder's presets.
//!
//! Each preset is a small, documented bundle of options applied together, so a
//! first-time user picks "Book" rather than hunting for the shipped template
//! and the sectioning flags in the panel. The bundle is defined once, here, and
//! [`Preset::options`] is the whole of it: the window compares the live options
//! against the chosen preset's bundle to decide whether to show a "Modified"
//! marker, so a preset cannot mean one thing on its button and another in the
//! panel.
//!
//! A preset is not a mode. It only writes options, everything it sets stays
//! visible and editable in the options panel, and the panel's own "Template
//! source" radio reflects the same `template_library` field a preset sets.

use textrill::cli::{self, Opt};
use textrill::options::Options;

/// One named starting point.
pub struct Preset {
    /// The stable spelling persisted in settings and the key [`get`] looks up.
    pub key: &'static str,
    /// The name shown on the button.
    pub name: &'static str,
    /// The shipped template this preset selects (`""` for none).
    library: &'static str,
    /// Also turn on sectioning and a table of contents. Only set for templates
    /// that carry the `{{textrill:toc}}` slot the engine requires.
    outline: bool,
}

impl Preset {
    /// The options this preset stands for: the engine defaults with the
    /// preset's own changes applied. Comparing live options against this is
    /// what drives the "Modified" marker.
    pub fn options(&self) -> Options {
        // `toc` implies sectioning in the engine; setting `section` alongside
        // it keeps the panel's two checkboxes agreeing with what is produced,
        // so a preset never shows one checked and the other clear.
        Options {
            template_library: self.library.to_string(),
            section: self.outline,
            toc: self.outline,
            ..Options::default()
        }
    }

    /// The one-line explanation shown when the button is hovered.
    pub fn description(&self) -> String {
        match (self.library.is_empty(), self.outline) {
            (true, _) => "The engine's own defaults, with no template.".to_string(),
            (false, true) => format!(
                "The shipped {:?} template, sectioned with a table of contents.",
                self.library
            ),
            (false, false) => format!("The shipped {:?} template.", self.library),
        }
    }
}

/// Every preset, in the order the bar shows them. `Plain` (the engine defaults)
/// is first and is the fallback for an unreadable stored key.
pub const PRESETS: &[Preset] = &[
    Preset {
        key: "plain",
        name: "Plain",
        library: "",
        outline: false,
    },
    Preset {
        key: "article",
        name: "Article",
        library: "article",
        outline: true,
    },
    Preset {
        key: "book",
        name: "Book",
        library: "book",
        outline: true,
    },
    Preset {
        key: "manpage",
        name: "Man page",
        library: "manpage",
        outline: false,
    },
    Preset {
        key: "slide",
        name: "Slides",
        library: "slide",
        outline: false,
    },
    Preset {
        key: "bare",
        name: "Bare",
        library: "bare",
        outline: false,
    },
    Preset {
        key: "accessible",
        name: "Accessible",
        library: "accessible",
        outline: true,
    },
];

/// The preset stored under `key`, or `None` for an unknown key.
pub fn get(key: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.key == key)
}

/// The fallback preset (Plain), which is also what an unknown stored key means.
pub fn default_preset() -> &'static Preset {
    &PRESETS[0]
}

/// Whether two option sets are equal, option by option, in the engine's own
/// canonical spellings.
///
/// Comparison goes through `cli::get_opt` rather than `PartialEq` on
/// [`Options`] (which is not derived) so it covers every option the CLI exposes
/// without a field-by-field implementation that a new option could silently
/// outgrow.
pub fn same_options(a: &Options, b: &Options) -> bool {
    Opt::ALL
        .iter()
        .all(|&opt| cli::get_opt(a, opt) == cli::get_opt(b, opt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_is_first_and_is_the_fallback() {
        assert_eq!(PRESETS[0].key, "plain");
        assert_eq!(default_preset().key, "plain");
        assert!(get("plain").is_some());
        assert!(get("nonsense").is_none());
    }

    #[test]
    fn keys_and_names_are_unique_and_nonempty() {
        for (i, a) in PRESETS.iter().enumerate() {
            assert!(!a.key.is_empty() && !a.name.is_empty());
            for b in &PRESETS[i + 1..] {
                assert_ne!(a.key, b.key, "duplicate preset key {}", a.key);
                assert_ne!(a.name, b.name, "duplicate preset name {}", a.name);
            }
        }
    }

    /// Every template a preset names is a real shipped template, and every
    /// preset that turns on the table of contents picks a template that carries
    /// the required slot — otherwise the preset would apply an option the
    /// engine then rejects.
    #[test]
    fn every_preset_is_a_valid_bundle() {
        for preset in PRESETS {
            if preset.library.is_empty() {
                assert!(!preset.outline, "{}: no template, no toc slot", preset.key);
                continue;
            }
            let shipped = textrill::library::get(preset.library)
                .unwrap_or_else(|| panic!("{}: unknown template {}", preset.key, preset.library));
            if preset.outline {
                assert!(
                    shipped.html.contains("{{textrill:toc}}"),
                    "{}: {} has no toc slot",
                    preset.key,
                    preset.library
                );
            }
        }
    }

    #[test]
    fn the_bundle_is_what_the_button_means() {
        let plain = get("plain").unwrap().options();
        assert!(plain.template_library.is_empty());
        assert!(!plain.toc && !plain.section);

        let book = get("book").unwrap().options();
        assert_eq!(book.template_library, "book");
        assert!(book.toc && book.section);
    }

    #[test]
    fn equality_follows_the_options_not_the_key() {
        let plain = get("plain").unwrap().options();
        assert!(same_options(&plain, &get("plain").unwrap().options()));

        let book = get("book").unwrap().options();
        let mut tweaked = book.clone();
        tweaked.toc = false;
        assert!(!same_options(&book, &tweaked));
        assert!(!same_options(&plain, &book));
    }
}
