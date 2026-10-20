//! Pack manifests (`template.toml`): this schema over the strict TOML subset
//! in [`super::tomlsub`], written by hand because the engine stays
//! dependency-free.
//!
//! The syntax rules — and every loud refusal of a file written as full TOML —
//! live in the shared scanner; this module owns only the manifest's keys and
//! sections: top-level `key = value` plus exactly two array-of-table forms,
//! `[[templates]]` and `[[templates.vars]]`. Unknown keys and sections are
//! hard errors naming the line, so a manifest written as full TOML fails with
//! a precise message instead of being silently misread.
//!
//! This module answers only "was the file understood?"; the semantics
//! (known `schema`, reverse-DNS `id`, semver `version`, required fields) are
//! checked by pack validation once the syntax is known good. An empty or
//! partial document therefore parses and is rejected later with a message
//! about the missing field. Covered by the tests below.

use super::tomlsub::{check_key, err, section_name, Value};

/// A parsed `template.toml`. Every field the subset can carry is optional at
/// parse time; validation decides which the schema requires.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub schema: Option<i64>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub authors: Option<Vec<String>>,
    pub license: Option<String>,
    pub tags: Option<Vec<String>>,
    pub templates: Vec<TemplateEntry>,
}

/// One `[[templates]]` entry: a single template file shipped by the pack.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TemplateEntry {
    pub file: Option<String>,
    /// `"document"` or `"body"`; validated later.
    pub model: Option<String>,
    pub slots: Option<Vec<String>>,
    pub preview: Option<String>,
    pub vars: Vec<VarEntry>,
}

/// One `[[templates.vars]]` entry: a parameter the template takes, backed by
/// a `{{textrill:var:name}}` slot in its file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VarEntry {
    pub name: Option<String>,
    pub required: Option<bool>,
}

/// The top-level keys the subset knows, for the "unknown key" message.
const TOP_KEYS: &str = "schema, id, name, version, description, authors, license, tags";

/// The keys one `[[templates]]` entry knows.
const TEMPLATE_KEYS: &str = "file, model, slots, preview";

/// The keys one `[[templates.vars]]` entry knows.
const VAR_KEYS: &str = "name, required";

/// Which array-of-table section the current `key = value` lines belong to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Top,
    Template,
    Vars,
}

/// Parse a `template.toml` document.
///
/// Errors read `line N: message`, naming the 1-based line the problem was
/// seen on.
pub fn parse(input: &str) -> Result<Manifest, String> {
    let mut manifest = Manifest::default();
    let mut section = Section::Top;
    for (idx, line) in input.lines().enumerate() {
        let n = idx + 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(rest) = t.strip_prefix("[[") {
            section = start_section(&mut manifest, rest, n)?;
            continue;
        }
        if t.starts_with('[') {
            return err(
                n,
                format!(
                    "`{t}` is not in the manifest subset: only the [[templates]] \
                     and [[templates.vars]] array-of-table forms are allowed"
                ),
            );
        }
        let Some(eq) = t.find('=') else {
            return err(n, format!("expected `key = value`, got `{t}`"));
        };
        let key = t[..eq].trim();
        check_key(key, n)?;
        let value = Value::parse(&t[eq + 1..], n)?;
        match section {
            Section::Top => set_top(&mut manifest, key, value, n)?,
            Section::Template => {
                let last = manifest
                    .templates
                    .last_mut()
                    .expect("the Template section implies a pushed entry");
                set_template(last, key, value, n)?;
            }
            Section::Vars => {
                let last = manifest
                    .templates
                    .last_mut()
                    .expect("the Vars section implies a pushed entry")
                    .vars
                    .last_mut()
                    .expect("the Vars section implies a pushed var");
                set_var(last, key, value, n)?;
            }
        }
    }
    Ok(manifest)
}

/// Recognise a `[[…]]` header (its leading `[[` already stripped) and push
/// the entry the section introduces, so later keys land in the right place.
fn start_section(manifest: &mut Manifest, rest: &str, n: usize) -> Result<Section, String> {
    let name = section_name(rest, n)?;
    match name {
        "templates" => {
            manifest.templates.push(TemplateEntry::default());
            Ok(Section::Template)
        }
        "templates.vars" => {
            let Some(last) = manifest.templates.last_mut() else {
                return err(n, "`[[templates.vars]]` appears before any `[[templates]]`");
            };
            last.vars.push(VarEntry::default());
            Ok(Section::Vars)
        }
        "" => err(n, "empty section name"),
        other => err(
            n,
            format!(
                "unsupported section `[[{other}]]`; the subset has [[templates]] \
                 and [[templates.vars]]"
            ),
        ),
    }
}

/// Place one `key = value` line in the manifest header.
fn set_top(manifest: &mut Manifest, key: &str, v: Value, n: usize) -> Result<(), String> {
    match key {
        "schema" => {
            once(&manifest.schema, key, n)?;
            manifest.schema = Some(want_int(key, v, n)?);
        }
        "id" => {
            once(&manifest.id, key, n)?;
            manifest.id = Some(want_string(key, v, n)?);
        }
        "name" => {
            once(&manifest.name, key, n)?;
            manifest.name = Some(want_string(key, v, n)?);
        }
        "version" => {
            once(&manifest.version, key, n)?;
            manifest.version = Some(want_string(key, v, n)?);
        }
        "description" => {
            once(&manifest.description, key, n)?;
            manifest.description = Some(want_string(key, v, n)?);
        }
        "authors" => {
            once(&manifest.authors, key, n)?;
            manifest.authors = Some(want_strs(key, v, n)?);
        }
        "license" => {
            once(&manifest.license, key, n)?;
            manifest.license = Some(want_string(key, v, n)?);
        }
        "tags" => {
            once(&manifest.tags, key, n)?;
            manifest.tags = Some(want_strs(key, v, n)?);
        }
        "templates" => {
            return err(
                n,
                "template entries belong in a `[[templates]]` section, not a \
                 `templates = …` value",
            );
        }
        other => return err(n, unknown(other, TOP_KEYS)),
    }
    Ok(())
}

/// Place one `key = value` line in the current `[[templates]]` entry.
fn set_template(entry: &mut TemplateEntry, key: &str, v: Value, n: usize) -> Result<(), String> {
    match key {
        "file" => {
            once(&entry.file, key, n)?;
            entry.file = Some(want_string(key, v, n)?);
        }
        "model" => {
            once(&entry.model, key, n)?;
            entry.model = Some(want_string(key, v, n)?);
        }
        "slots" => {
            once(&entry.slots, key, n)?;
            entry.slots = Some(want_strs(key, v, n)?);
        }
        "preview" => {
            once(&entry.preview, key, n)?;
            entry.preview = Some(want_string(key, v, n)?);
        }
        "vars" => {
            return err(
                n,
                "`vars` belongs in a `[[templates.vars]]` section, not a \
                 `vars = …` value",
            );
        }
        "name" | "required" => {
            return err(
                n,
                format!("`{key}` belongs in a `[[templates.vars]]` entry, not in `[[templates]]`"),
            );
        }
        other => return err(n, unknown(other, TEMPLATE_KEYS)),
    }
    Ok(())
}

/// Place one `key = value` line in the current `[[templates.vars]]` entry.
fn set_var(var: &mut VarEntry, key: &str, v: Value, n: usize) -> Result<(), String> {
    match key {
        "name" => {
            once(&var.name, key, n)?;
            var.name = Some(want_string(key, v, n)?);
        }
        "required" => {
            once(&var.required, key, n)?;
            var.required = Some(want_bool(key, v, n)?);
        }
        other => return err(n, unknown(other, VAR_KEYS)),
    }
    Ok(())
}

/// The one "this key was already set" message, so the duplicate rule reads
/// the same wherever a key lands. TOML forbids repeating a key, and silently
/// keeping the last value would be exactly the misread this reader exists to
/// prevent.
fn once<T>(slot: &Option<T>, key: &str, n: usize) -> Result<(), String> {
    if slot.is_some() {
        return err(n, format!("duplicate key `{key}`"));
    }
    Ok(())
}

fn unknown(key: &str, known: &str) -> String {
    format!("unknown key `{key}`; this part of the subset has: {known}")
}

/// Coerce a parsed value to the type its key promises.
fn want_string(key: &str, v: Value, n: usize) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s),
        _ => err(n, format!("`{key}` expects a string value")),
    }
}

fn want_int(key: &str, v: Value, n: usize) -> Result<i64, String> {
    match v {
        Value::Int(i) => Ok(i),
        _ => err(n, format!("`{key}` expects an integer value")),
    }
}

fn want_bool(key: &str, v: Value, n: usize) -> Result<bool, String> {
    match v {
        Value::Bool(b) => Ok(b),
        _ => err(n, format!("`{key}` expects true or false")),
    }
}

fn want_strs(key: &str, v: Value, n: usize) -> Result<Vec<String>, String> {
    match v {
        Value::Strs(v) => Ok(v),
        _ => err(
            n,
            format!("`{key}` expects an array of strings, like [\"a\", \"b\"]"),
        ),
    }
}

/// The manifest from the roadmap (§5.2), which every reader must keep reading.
#[cfg(test)]
const ROADMAP_EXAMPLE: &str = r#"
schema = 1
id = "org.example.academic-book"     # reverse-DNS, globally unique
name = "Academic Book"
version = "1.2.0"                    # semver
description = "A print-ready academic book skeleton."
authors = ["A. Scholar <a@example.org>"]
license = "CC0-1.0"
tags = ["book", "print", "academic"]

[[templates]]
file = "book.html"
model = "document"                   # "document" | "body"
slots = ["content", "toc", "head", "header", "citations", "glossary"]
preview = "preview.png"

[[templates.vars]]
name = "author"
required = false
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// The documented example parses to exactly the documented shape.
    #[test]
    fn reads_the_roadmap_example() {
        let m = parse(ROADMAP_EXAMPLE).expect("the example is in the subset");
        assert_eq!(m.schema, Some(1));
        assert_eq!(m.id.as_deref(), Some("org.example.academic-book"));
        assert_eq!(m.name.as_deref(), Some("Academic Book"));
        assert_eq!(m.version.as_deref(), Some("1.2.0"));
        assert_eq!(
            m.description.as_deref(),
            Some("A print-ready academic book skeleton.")
        );
        assert_eq!(
            m.authors.as_deref(),
            Some(&["A. Scholar <a@example.org>".to_string()][..])
        );
        assert_eq!(m.license.as_deref(), Some("CC0-1.0"));
        assert_eq!(
            m.tags.as_deref(),
            Some(
                &[
                    "book".to_string(),
                    "print".to_string(),
                    "academic".to_string()
                ][..]
            )
        );
        assert_eq!(m.templates.len(), 1);
        let t = &m.templates[0];
        assert_eq!(t.file.as_deref(), Some("book.html"));
        assert_eq!(t.model.as_deref(), Some("document"));
        assert_eq!(
            t.slots.as_deref(),
            Some(
                &["content", "toc", "head", "header", "citations", "glossary"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()[..]
            )
        );
        assert_eq!(t.preview.as_deref(), Some("preview.png"));
        assert_eq!(
            t.vars,
            vec![VarEntry {
                name: Some("author".to_string()),
                required: Some(false),
            }]
        );
    }

    /// Whitespace, blank lines and whole-line comments change nothing.
    #[test]
    fn ignores_comments_and_blank_lines() {
        let doc = "# a comment\n\nschema = 1\nid = \"a.b\"\n\n# another\n";
        let m = parse(doc).unwrap();
        assert_eq!(m.schema, Some(1));
        assert_eq!(m.id.as_deref(), Some("a.b"));
        assert!(m.templates.is_empty());
    }

    /// An empty manifest is a parse-level success; required fields belong to
    /// validation, which runs after the syntax is known good.
    #[test]
    fn an_empty_document_parses() {
        assert_eq!(parse("").unwrap(), Manifest::default());
        assert_eq!(parse("# only a comment\n").unwrap(), Manifest::default());
    }

    /// The `#` that starts a comment is only a comment outside a string.
    #[test]
    fn a_hash_inside_a_string_is_content() {
        let m = parse("description = \"a # b\"").unwrap();
        assert_eq!(m.description.as_deref(), Some("a # b"));
    }

    /// A trailing comment after a value is fine, on every value shape.
    #[test]
    fn a_trailing_comment_after_a_value_is_allowed() {
        let m = parse("schema = 1 # the schema\nlicense = \"MIT\" # SPDX\ntags = [\"x\"] # end")
            .unwrap();
        assert_eq!(m.schema, Some(1));
        assert_eq!(m.license.as_deref(), Some("MIT"));
        assert_eq!(m.tags.as_deref(), Some(&["x".to_string()][..]));
    }

    /// The value splits at the first `=`, so one inside a string survives.
    #[test]
    fn the_value_splits_at_the_first_equals() {
        let m = parse("description = \"a = b\"").unwrap();
        assert_eq!(m.description.as_deref(), Some("a = b"));
    }

    /// The escapes a manifest may need all decode, and a `\\u` escape yields
    /// the character rather than its source text.
    #[test]
    fn strings_decode_the_documented_escapes() {
        let m =
            parse("name = \"a\\nb\\t\\\"q\\\"\\\\z\\u00e9\\U0001F600\"\nauthors = [\"x\\u00e9\"]")
                .unwrap();
        assert_eq!(m.name.as_deref(), Some("a\nb\t\"q\"\\z\u{e9}\u{1F600}"));
        assert_eq!(m.authors.as_deref(), Some(&["x\u{e9}".to_string()][..]));
    }

    /// A negative and a `+`-prefixed integer are both plain integers.
    #[test]
    fn integers_may_be_signed() {
        assert_eq!(parse("schema = -3").unwrap().schema, Some(-3));
        assert_eq!(parse("schema = +7").unwrap().schema, Some(7));
    }

    /// An empty array and a trailing comma are both in the subset.
    #[test]
    fn arrays_may_be_empty_or_trailing_comma() {
        assert_eq!(parse("tags = []").unwrap().tags, Some(Vec::new()));
        assert_eq!(
            parse("tags = [\"a\", \"b\",]").unwrap().tags,
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    /// Two `[[templates]]` sections build two entries, and a new one resets
    /// the section so its keys do not leak into the previous entry.
    #[test]
    fn two_template_sections_build_two_entries() {
        let m = parse(
            "[[templates]]\nfile = \"a.html\"\n\n[[templates]]\nfile = \"b.html\"\nmodel = \"body\"\n",
        )
        .unwrap();
        assert_eq!(m.templates.len(), 2);
        assert_eq!(m.templates[0].file.as_deref(), Some("a.html"));
        assert_eq!(m.templates[0].model, None);
        assert_eq!(m.templates[1].file.as_deref(), Some("b.html"));
        assert_eq!(m.templates[1].model.as_deref(), Some("body"));
    }

    /// `[[templates.vars]]` attaches to the template it follows, and several
    /// vars stack up under that one template.
    #[test]
    fn vars_attach_to_the_following_template() {
        let m = parse(
            "[[templates]]\nfile = \"a.html\"\n[[templates.vars]]\n\
             name = \"one\"\n[[templates.vars]]\nname = \"two\"\nrequired = true\n\
             [[templates]]\nfile = \"b.html\"\n",
        )
        .unwrap();
        assert_eq!(m.templates[0].vars.len(), 2);
        assert_eq!(m.templates[0].vars[0].name.as_deref(), Some("one"));
        assert_eq!(m.templates[0].vars[0].required, None);
        assert_eq!(m.templates[0].vars[1].name.as_deref(), Some("two"));
        assert_eq!(m.templates[0].vars[1].required, Some(true));
        assert!(m.templates[1].vars.is_empty());
    }

    /// CRLF documents are plain TOML and must parse the same as LF ones.
    #[test]
    fn crf_line_endings_are_accepted() {
        let m = parse("schema = 1\r\nid = \"a.b\"\r\n").unwrap();
        assert_eq!(m.schema, Some(1));
        assert_eq!(m.id.as_deref(), Some("a.b"));
    }

    // ---- The loud failures. Each is a shape a real TOML document can carry
    // ---- that the subset does not, and each must name its line.

    /// Assert a document fails, and that the message carries `line N` and
    /// every fragment given.
    fn rejects(doc: &str, line: usize, fragments: &[&str]) {
        let got = parse(doc).expect_err("expected a hard error");
        assert!(
            got.contains(&format!("line {line}:")),
            "expected `line {line}:`, got: {got}"
        );
        for f in fragments {
            assert!(got.contains(f), "expected `{f}` in: {got}");
        }
    }

    #[test]
    fn rejects_a_real_table_header() {
        rejects(
            "[templates]\nfile = \"a.html\"\n",
            1,
            &["[templates]", "subset"],
        );
    }

    #[test]
    fn rejects_an_inline_table() {
        rejects(
            "description = { a = 1 }\n",
            1,
            &["`{`", "not in the TOML subset"],
        );
    }

    #[test]
    fn rejects_a_multi_line_string() {
        rejects("description = \"\"\"\nnope\n", 1, &["unexpected text"]);
    }

    #[test]
    fn rejects_a_float() {
        rejects("schema = 1.0\n", 1, &["1.0", "floats"]);
    }

    #[test]
    fn rejects_a_date_like_bare_word() {
        rejects("version = 2026-10-19\n", 1, &["2026-10-19"]);
    }

    #[test]
    fn rejects_a_dotted_key() {
        rejects("a.b = 1\n", 1, &["a.b", "bare key"]);
    }

    #[test]
    fn rejects_a_quoted_key() {
        rejects("\"schema\" = 1\n", 1, &["bare key"]);
    }

    #[test]
    fn rejects_an_unknown_top_level_key() {
        rejects(
            "colour = \"red\"\n",
            1,
            &["colour", "unknown key", TOP_KEYS],
        );
    }

    #[test]
    fn rejects_an_unknown_template_key() {
        rejects(
            "[[templates]]\nfile = \"a.html\"\ncolour = \"red\"\n",
            3,
            &["colour", "unknown key", TEMPLATE_KEYS],
        );
    }

    #[test]
    fn rejects_an_unknown_var_key() {
        rejects(
            "[[templates]]\nfile = \"a.html\"\n[[templates.vars]]\ncolour = \"red\"\n",
            4,
            &["colour", "unknown key", VAR_KEYS],
        );
    }

    #[test]
    fn rejects_a_repeated_key() {
        rejects("id = \"a.b\"\nid = \"c.d\"\n", 2, &["duplicate key `id`"]);
        rejects(
            "[[templates]]\nfile = \"a.html\"\nfile = \"b.html\"\n",
            3,
            &["duplicate key `file`"],
        );
        rejects(
            "[[templates]]\nfile = \"a.html\"\n[[templates.vars]]\n\
             name = \"x\"\nname = \"y\"\n",
            5,
            &["duplicate key `name`"],
        );
    }

    #[test]
    fn rejects_an_unsupported_section() {
        rejects("[[notes]]\n", 1, &["[[notes]]", "unsupported section"]);
    }

    #[test]
    fn rejects_vars_before_any_template() {
        rejects("[[templates.vars]]\nname = \"x\"\n", 1, &["before any"]);
    }

    #[test]
    fn rejects_an_unterminated_section_header() {
        rejects("[[templates\n", 1, &["unterminated"]);
    }

    #[test]
    fn rejects_text_after_a_section_header() {
        rejects("[[templates]] junk\n", 1, &["unexpected text"]);
    }

    #[test]
    fn rejects_a_key_without_a_value() {
        rejects("id =\n", 1, &["missing value"]);
    }

    #[test]
    fn rejects_a_line_that_is_not_key_value() {
        rejects("just some words\n", 1, &["expected `key = value`"]);
    }

    #[test]
    fn rejects_an_unterminated_string() {
        rejects("id = \"abc\n", 1, &["unterminated string"]);
    }

    #[test]
    fn rejects_a_dangling_escape() {
        rejects("id = \"abc\\", 1, &["dangling"]);
    }

    #[test]
    fn rejects_an_unknown_escape() {
        rejects("id = \"a\\qb\"\n", 1, &["unknown escape `\\q`"]);
    }

    #[test]
    fn rejects_a_truncated_unicode_escape() {
        rejects("id = \"a\\u12\"", 1, &["truncated"]);
        rejects("id = \"a\\uZZZZ\"\n", 1, &["hexadecimal"]);
    }

    #[test]
    fn rejects_a_surrogate_unicode_escape() {
        rejects("id = \"\\uD800\"\n", 1, &["scalar value"]);
    }

    #[test]
    fn rejects_trailing_text_after_a_value() {
        rejects("id = \"a\" b\n", 1, &["unexpected text"]);
        rejects("schema = 1 2\n", 1, &["unexpected text"]);
    }

    #[test]
    fn rejects_a_non_string_array_element() {
        rejects("tags = [a]\n", 1, &["only \"strings\""]);
        rejects("tags = [1]\n", 1, &["only \"strings\""]);
    }

    #[test]
    fn rejects_a_missing_comma_between_elements() {
        rejects("tags = [\"a\" \"b\"]\n", 1, &["expected `,` or `]`"]);
    }

    #[test]
    fn rejects_an_unterminated_array() {
        rejects("tags = [\"a\"\n", 1, &["unterminated array"]);
    }

    #[test]
    fn rejects_templates_and_vars_as_plain_values() {
        rejects("templates = [\"a.html\"]\n", 1, &["[[templates]]"]);
        rejects("[[templates]]\nvars = []\n", 2, &["[[templates.vars]]"]);
        rejects("vars = []\n", 1, &["unknown key `vars`"]);
    }

    #[test]
    fn rejects_a_var_key_in_a_template_entry() {
        rejects(
            "[[templates]]\nrequired = true\n",
            2,
            &["[[templates.vars]]"],
        );
    }

    /// The failing line is the one the message names, even deep in a file.
    #[test]
    fn names_the_line_of_a_late_problem() {
        let doc = "schema = 1\nid = \"a.b\"\nname = \"A\"\n\nversion = not-a-number\n";
        rejects(doc, 5, &["line 5:", "not-a-number"]);
    }

    /// A wrong-typed value blames the key that promised the type.
    #[test]
    fn blames_the_key_for_a_wrong_type() {
        rejects("schema = \"one\"\n", 1, &["`schema` expects an integer"]);
        rejects("id = 1\n", 1, &["`id` expects a string"]);
        rejects(
            "[[templates]]\nfile = \"a.html\"\n[[templates.vars]]\nrequired = \"yes\"\n",
            4,
            &["`required` expects true or false"],
        );
        rejects("tags = \"x\"\n", 1, &["`tags` expects an array of strings"]);
    }
}
