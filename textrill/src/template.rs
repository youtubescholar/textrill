// textrill — convert plain text to HTML.
//
// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Opt-in page templates (P5.5).
//!
//! The engine produces named HTML blocks and a template decides where they go.
//! A slot is written `{{textrill:name}}`, with the `textrill` namespace chosen
//! precisely so a template can also carry another engine's `{{ }}` tokens
//! (Mustache, Handlebars, Jinja2, Vue, …) without this pass touching them:
//! **only** known `textrill` slots are substituted, an unknown `textrill` slot
//! is a hard error, and every other `{{...}}` is passed through byte for byte.
//!
//! This is not a template language. There are no loops, conditionals,
//! expressions or includes; each slot is one pre-rendered block, so there is
//! nothing to iterate. The engine already knows the things a user must not
//! hand-write — anchor ids, TOC targets, pager links — and renders them here.

/// The slots a template may use, in a stable order for diagnostics.
pub const SLOTS: &[&str] = &[
    "content",
    "toc",
    "title",
    "head",
    "pager",
    "citations",
    "glossary",
];

/// Substitute every known `{{textrill:name}}` in `template` with its value.
///
/// Total by construction: [`apply`] leaves unknown `textrill` slots and all
/// non-`textrill` `{{...}}` verbatim rather than failing. The command line
/// rejects an unknown slot up front with [`validate`], so on that path a
/// malformed template never reaches conversion; a library caller that skips
/// validation degrades to passing the token through, which is visible and
/// harmless, rather than dropping content.
pub fn apply(template: &str, slots: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(pos) = rest.find("{{") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        if let Some(body) = after.strip_prefix("textrill:") {
            if let Some(end) = body.find("}}") {
                let name = &body[..end];
                if let Some((_, value)) = slots.iter().find(|(n, _)| *n == name) {
                    out.push_str(value);
                    rest = &body[end + 2..];
                    continue;
                }
            }
        }
        // Not a known slot: emit the `{{` literally and rescan from just after
        // it, so the rest of the token (and any later `{{`) flows through.
        out.push_str("{{");
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Check a template before conversion: every `textrill` slot must be known, a
/// `{{textrill:var:name}}` must name a parameter declared with `--var`, and a
/// `{{textrill:content}}` slot must be present.
///
/// Reports the first problem as a message a user can act on. Non-`textrill`
/// tokens are ignored entirely, since they belong to another engine.
pub fn validate(template: &str, declared_vars: &[&str]) -> Result<(), String> {
    let mut content = false;
    let mut rest = template;
    while let Some(pos) = rest.find("{{") {
        let after = &rest[pos + 2..];
        if let Some(body) = after.strip_prefix("textrill:") {
            if let Some(end) = body.find("}}") {
                let name = &body[..end];
                let known = if let Some(var) = name.strip_prefix("var:") {
                    if declared_vars.contains(&var) {
                        true
                    } else if declared_vars.is_empty() {
                        return Err(format!(
                            "template uses `{{{{textrill:{name}}}}}` but no --var \
                             parameter is declared (use --var name=value)"
                        ));
                    } else {
                        return Err(format!(
                            "template uses `{{{{textrill:{name}}}}}` but that variable \
                             is not declared; declared: {}",
                            declared_vars.join(", ")
                        ));
                    }
                } else {
                    SLOTS.contains(&name)
                };
                if !known {
                    return Err(format!(
                        "unknown template slot `{{{{textrill:{name}}}}}`; expected one \
                         of: {}",
                        SLOTS.join(", ")
                    ));
                }
                if name == "content" {
                    content = true;
                }
                rest = &body[end + 2..];
                continue;
            }
        }
        rest = after;
    }
    if !content {
        return Err(
            "template must contain a {{textrill:content}} slot for the converted body".to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots() -> Vec<(&'static str, &'static str)> {
        vec![
            ("content", "<p>hi</p>\n"),
            ("toc", "<nav></nav>\n"),
            ("title", "A &amp; B"),
            ("head", "<title>A</title>\n"),
            ("pager", ""),
        ]
    }

    #[test]
    fn replaces_known_slots_in_order() {
        let t = "{{textrill:head}}<body>{{textrill:content}}</body>{{textrill:pager}}";
        assert_eq!(
            apply(t, &slots()),
            "<title>A</title>\n<body><p>hi</p>\n</body>"
        );
    }

    #[test]
    fn passes_other_engines_tokens_through() {
        // A Mustache/Jinja-looking token must survive untouched, and the
        // namespace exists precisely so this pass does not eat it.
        let t = "{{#each x}}{{name}}{{/each}}{{textrill:content}}{{ user.name }}";
        assert_eq!(
            apply(t, &slots()),
            "{{#each x}}{{name}}{{/each}}<p>hi</p>\n{{ user.name }}"
        );
    }

    #[test]
    fn unknown_textrill_slot_is_left_verbatim() {
        let t = "{{textrill:nope}}{{textrill:content}}";
        assert_eq!(apply(t, &slots()), "{{textrill:nope}}<p>hi</p>\n");
    }

    #[test]
    fn unclosed_token_is_literal() {
        // No `}}`, so this is not a slot at all and must survive untouched.
        assert_eq!(
            apply("{{textrill:contents", &slots()),
            "{{textrill:contents"
        );
    }

    #[test]
    fn validate_accepts_a_minimal_template() {
        assert!(validate("{{textrill:content}}", &[]).is_ok());
        assert!(validate("<div>{{textrill:content}}{{textrill:toc}}</div>", &[]).is_ok());
    }

    #[test]
    fn validate_rejects_unknown_slots_and_missing_content() {
        let err = validate("{{textrill:nope}}{{textrill:content}}", &[]).unwrap_err();
        assert!(err.contains("nope"), "{err}");
        let err = validate("<div>{{textrill:toc}}</div>", &[]).unwrap_err();
        assert!(err.contains("content"), "{err}");
    }

    #[test]
    fn validate_ignores_other_engines_tokens() {
        assert!(validate("{{#if x}}{{content}}{{/if}}{{textrill:content}}", &[]).is_ok());
    }

    #[test]
    fn validate_accepts_declared_var_slots() {
        assert!(validate(
            "<p>{{textrill:var:author}}</p>{{textrill:content}}",
            &["author", "theme"]
        )
        .is_ok());
    }

    #[test]
    fn validate_rejects_undeclared_var_slots() {
        // A var that was never declared is the same class of defect as an
        // unknown slot: it would render as an empty hole with no warning.
        let err = validate(
            "<p>{{textrill:var:typo}}</p>{{textrill:content}}",
            &["author"],
        )
        .unwrap_err();
        assert!(err.contains("typo"), "{err}");
        assert!(
            err.contains("author"),
            "the message names the declared vars: {err}"
        );
        let err = validate("{{textrill:var:none}}{{textrill:content}}", &[]).unwrap_err();
        assert!(err.contains("no --var"), "{err}");
    }

    #[test]
    fn validate_still_rejects_unknown_textrill_slots() {
        // The 4.4 guard is extended, not loosened: `var`, `varx`, `vary` are
        // not in the fixed set and none of them is a `var:` family member.
        for name in ["var", "varx", "vary"] {
            let t = format!("{{{{textrill:{name}}}}}{{{{textrill:content}}}}");
            let err = validate(&t, &["x"]).unwrap_err();
            assert!(
                err.contains("unknown template slot"),
                "{name}: expected unknown-slot error, got: {err}"
            );
        }
    }

    #[test]
    fn apply_inserts_var_values_verbatim_once() {
        let t = "{{textrill:var:title}}. {{textrill:var:title}}.{{textrill:content}}";
        let slots = [("var:title", "A & B <b>C</b>"), ("content", "<p>hi</p>\n")];
        assert_eq!(
            apply(t, &slots),
            "A & B <b>C</b>. A & B <b>C</b>.<p>hi</p>\n"
        );
    }

    #[test]
    fn apply_never_rescans_an_inserted_value() {
        // A value that itself looks like a template token must not be
        // interpreted as one: the substitution is a single pass, so textrill's
        // own slots cannot be smuggled in through a var (no cascade). This is
        // what makes a user-supplied value safe inside a template.
        let t = "{{textrill:var:x}}{{textrill:content}}";
        let slots = [("var:x", "{{textrill:toc}}{{other}}"), ("content", "C")];
        assert_eq!(apply(t, &slots), "{{textrill:toc}}{{other}}C");
    }

    #[test]
    fn apply_allows_an_empty_var_value() {
        // An empty value is legal: it is how a template makes a field
        // optional. The surrounding markup the author wrote still stands.
        let t = "<h1>{{textrill:var:subtitle}}</h1>{{textrill:content}}";
        let slots = [("var:subtitle", ""), ("content", "C")];
        assert_eq!(apply(t, &slots), "<h1></h1>C");
    }
}
