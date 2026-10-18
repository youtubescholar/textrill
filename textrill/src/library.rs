//! The shipped template library: five templates selectable by
//! `--template_library`, each treated like the matching `--body_template` or
//! `--document_template` file.
//!
//! Plain HTML under `templates/`, embedded with [`include_str!`] and using only
//! the fixed slots, so each converts with no arguments; verified by the tests.

/// Which part of the page a shipped template owns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Model {
    /// Wraps the body inside the engine's own prolog and epilog.
    Body,
    /// Owns the whole document: doctype, head and body.
    Document,
}

/// One shipped template: its name, model, and embedded markup.
pub struct Shipped {
    pub name: &'static str,
    pub model: Model,
    pub html: &'static str,
}

pub const LIBRARY: &[Shipped] = &[
    Shipped {
        name: "article",
        model: Model::Document,
        html: include_str!("../templates/article.html"),
    },
    Shipped {
        name: "book",
        model: Model::Document,
        html: include_str!("../templates/book.html"),
    },
    Shipped {
        name: "manpage",
        model: Model::Document,
        html: include_str!("../templates/manpage.html"),
    },
    Shipped {
        name: "slide",
        model: Model::Document,
        html: include_str!("../templates/slide.html"),
    },
    Shipped {
        name: "bare",
        model: Model::Body,
        html: include_str!("../templates/bare.html"),
    },
];

/// Look up a shipped template by exact name.
pub fn get(name: &str) -> Option<&'static Shipped> {
    LIBRARY.iter().find(|s| s.name == name)
}

/// The available names, in library order, for messages and `--help`.
pub fn names() -> impl Iterator<Item = &'static str> {
    LIBRARY.iter().map(|s| s.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shipped template validates on its own with only the fixed slots.
    #[test]
    fn every_shipped_template_validates_with_no_parameters() {
        for s in LIBRARY {
            crate::template::validate(s.html, &[]).unwrap_or_else(|e| panic!("{}: {e}", s.name));
        }
    }

    /// Guards the fixed-slots-only rule in [`LIBRARY`].
    #[test]
    fn no_shipped_template_uses_a_var_slot() {
        for s in LIBRARY {
            assert!(
                !s.html.contains("{{textrill:var:"),
                "{} must not use a var slot",
                s.name
            );
        }
    }

    /// Pins the library to the documented five, in a stable order and models.
    #[test]
    fn the_library_is_the_documented_five() {
        let got: Vec<(&str, Model)> = LIBRARY.iter().map(|s| (s.name, s.model)).collect();
        assert_eq!(
            got,
            vec![
                ("article", Model::Document),
                ("book", Model::Document),
                ("manpage", Model::Document),
                ("slide", Model::Document),
                ("bare", Model::Body),
            ]
        );
    }

    #[test]
    fn an_unknown_template_is_reported_named_but_not_silent() {
        assert!(get("nope").is_none());
        assert!(get("article").is_some());
        let listed: Vec<&str> = names().collect();
        assert_eq!(listed.len(), LIBRARY.len());
        assert!(listed.contains(&"article"));
    }
}
