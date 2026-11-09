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
    /// Wrap each heading-delimited section in an `<article id="chunk-N">`.
    /// Only set for templates that carry the `{{textrill:toc}}` slot when
    /// [`Self::toc`] is also set.
    section: bool,
    /// Generate a table of contents. Implies [`Self::section`] unless
    /// [`Self::section_chars`] supplies the wrappers instead.
    toc: bool,
    /// Split one document into parts of at most this many raw characters.
    /// Non-zero replaces heading sectioning with chunk wrappers.
    section_chars: usize,
}

impl Preset {
    /// The options this preset stands for: the engine defaults with the
    /// preset's own changes applied. Comparing live options against this is
    /// what drives the "Modified" marker.
    pub fn options(&self) -> Options {
        // `toc` implies heading sectioning unless character chunks provide the
        // wrappers. Keeping the flags explicit makes the panel agree with what
        // the engine produces, and avoids the `--section --section_chars`
        // refusal.
        Options {
            template_library: self.library.to_string(),
            section: self.section || (self.toc && self.section_chars == 0),
            section_chars: self.section_chars,
            toc: self.toc,
            ..Options::default()
        }
    }

    /// The one-line explanation shown when the button is hovered.
    pub fn description(&self) -> String {
        if self.library.is_empty() {
            return "The engine's own defaults, with no template.".to_string();
        }
        if self.section_chars > 0 {
            return format!(
                "The shipped {:?} template in parts of about {} characters with a table of contents.",
                self.library, self.section_chars
            );
        }
        match (self.section || self.toc, self.toc) {
            (true, true) => format!(
                "The shipped {:?} template, split into sections with a table of contents.",
                self.library
            ),
            (true, false) => format!(
                "The shipped {:?} template, split into sections without a table of contents.",
                self.library
            ),
            (false, _) => {
                format!(
                    "The shipped {:?} template as one continuous document.",
                    self.library
                )
            }
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
        section: false,
        toc: false,
        section_chars: 0,
    },
    Preset {
        key: "article",
        name: "Article",
        library: "article",
        section: false,
        toc: false,
        section_chars: 0,
    },
    Preset {
        key: "sections",
        name: "Sections",
        library: "article",
        section: true,
        toc: false,
        section_chars: 0,
    },
    Preset {
        key: "contents",
        name: "Contents",
        library: "article",
        section: true,
        toc: true,
        section_chars: 0,
    },
    Preset {
        key: "book",
        name: "Book",
        library: "book",
        section: true,
        toc: true,
        section_chars: 0,
    },
    Preset {
        key: "manpage",
        name: "Man page",
        library: "manpage",
        section: false,
        toc: false,
        section_chars: 0,
    },
    Preset {
        key: "slide",
        name: "Slides",
        library: "slide",
        section: true,
        toc: true,
        section_chars: 0,
    },
    Preset {
        key: "readable",
        name: "Readable 20k",
        library: "readable",
        section: false,
        toc: true,
        section_chars: 20_000,
    },
    Preset {
        key: "bare",
        name: "Bare",
        library: "bare",
        section: false,
        toc: false,
        section_chars: 0,
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
                assert!(
                    !preset.section && !preset.toc,
                    "{}: no template, no toc slot",
                    preset.key
                );
                continue;
            }
            let shipped = textrill::library::get(preset.library)
                .unwrap_or_else(|| panic!("{}: unknown template {}", preset.key, preset.library));
            if preset.toc {
                assert!(
                    shipped.html.contains("{{textrill:toc}}"),
                    "{}: {} has no toc slot",
                    preset.key,
                    preset.library
                );
            }
        }
    }

    /// A contents list without sectioning would link to ids the engine never
    /// emitted, so no preset may ask for one without either heading sectioning
    /// or character chunks.
    #[test]
    fn a_contents_list_always_implies_sections() {
        for preset in PRESETS {
            if preset.toc && preset.section_chars == 0 {
                assert!(preset.section, "{}: toc without section", preset.key);
            }
            if preset.section_chars > 0 {
                assert!(
                    !preset.section,
                    "{}: char chunks plus heading section",
                    preset.key
                );
            }
        }
    }

    #[test]
    fn the_bundle_is_what_the_button_means() {
        let plain = get("plain").unwrap().options();
        assert!(plain.template_library.is_empty());
        assert!(!plain.toc && !plain.section);

        let article = get("article").unwrap().options();
        assert_eq!(article.template_library, "article");
        assert!(!article.toc && !article.section);

        let sections = get("sections").unwrap().options();
        assert_eq!(sections.template_library, "article");
        assert!(sections.section && !sections.toc);

        let contents = get("contents").unwrap().options();
        assert_eq!(contents.template_library, "article");
        assert!(contents.toc && contents.section);

        let book = get("book").unwrap().options();
        assert_eq!(book.template_library, "book");
        assert!(book.toc && book.section);

        let slide = get("slide").unwrap().options();
        assert_eq!(slide.template_library, "slide");
        assert!(slide.toc && slide.section);

        let readable = get("readable").unwrap().options();
        assert_eq!(readable.template_library, "readable");
        assert_eq!(readable.section_chars, 20_000);
        assert!(readable.toc && !readable.section);
    }

    /// The three article presets are a progression, not duplicates: each one
    /// must produce a different bundle from the others.
    #[test]
    fn the_article_presets_are_distinct() {
        let article = get("article").unwrap().options();
        let sections = get("sections").unwrap().options();
        let contents = get("contents").unwrap().options();
        assert!(!same_options(&article, &sections));
        assert!(!same_options(&article, &contents));
        assert!(!same_options(&sections, &contents));
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
