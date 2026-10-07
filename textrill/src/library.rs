//! The shipped template library (Phase 4.2).
//!
//! Five templates you can use without writing one -- let `--template_library`
//! pick a template by name and the engine treats it exactly as if you had
//! passed the corresponding `--body_template` or `--document_template` file.
//!
//! The templates are content, not machinery: each is a plain HTML file under
//! `templates/`, embedded into the binary with [`include_str!`] so a static
//! build carries the whole library and works from any directory. The library
//! deliberately uses **only the seven fixed slots** and no
//! `{{textrill:var:...}}` slots, so every shipped template converts with zero
//! required arguments and produces no silent-empty or invisible frames. A
//! template that wants a byline, date or the like is expected to be copied and
//! given `{{textrill:var:name}}` slots of its own, which is exactly what `--var`
//! is for (Phase 4.1).
//!
//! Each name carries its model: `article`, `book`, `manpage` and `slide` own
//! the whole document, `bare` is the minimal body wrapper and converts
//! byte-for-byte like no template at all.

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

    /// Every shipped template is a valid template on its own: it contains the
    /// `content` slot, uses only the seven fixed slots, and never a
    /// `{{textrill:var:...}}` slot that would then demand an undeclared `--var`.
    #[test]
    fn every_shipped_template_validates_with_no_parameters() {
        for s in LIBRARY {
            crate::template::validate(s.html, &[]).unwrap_or_else(|e| panic!("{}: {e}", s.name));
        }
    }

    /// The fixed-slots-only rule is a literal one, so a template that sneaks a
    /// var slot in fails this gate instead of shipping a silent variance.
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

    /// The library is exactly the documented five, in a stable order, with the
    /// documented models.
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
