//! Opt-in citations and glossary, off by default; when off, output is
//! byte-identical to a build without the feature.
//!
//! A reference is `{{textrill:cite:key}}`; a definition is a balanced
//! `{{textrill:def:cite:key}}…{{/textrill:def:cite:key}}` block (likewise
//! `gloss`). Only exact `textrill:` tokens are interpreted, so a typo like
//! `{{textrill:cites:x}}` is refused rather than rendered as text. With a mode
//! on, a dangling reference, an unreferenced/duplicate/empty definition, an
//! unbalanced block, a stray close, or an unknown token is a hard error.
//!
//! The pass runs on the rendered body, not the source, because reference
//! numbers are assigned in document order; a `<pre>` block is plain text to it,
//! so an author showing the syntax in a sample must keep the mode off.
//! Covered by tests/notest.rs.

use std::collections::HashMap;

/// The kinds of note, and the token prefix that introduces each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Citation,
    Glossary,
}

impl Kind {
    /// The token fragment, as in `{{textrill:cite:…}}`.
    pub fn token(self) -> &'static str {
        match self {
            Kind::Citation => "cite",
            Kind::Glossary => "gloss",
        }
    }

    /// `notes` for citations, `glossary` for glossary terms.
    pub fn list_class(self) -> &'static str {
        match self {
            Kind::Citation => "notes",
            Kind::Glossary => "glossary",
        }
    }

    fn all() -> [Kind; 2] {
        [Kind::Citation, Kind::Glossary]
    }
}

/// One definition, with the key it was declared under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub key: String,
    /// The already-escaped body HTML, with the surrounding newlines trimmed.
    pub content: String,
    /// The 1-based number assigned by first reference, for citations.
    pub number: usize,
    /// Some `{{textrill:…}}` referenced this key.
    pub referenced: bool,
    /// A `{{textrill:def:…}}` block supplied its content. Kept separate from
    /// `referenced`: an undefined reference and an unreferenced definition are
    /// different mistakes needing different fixes.
    pub defined: bool,
}

/// Collected notes for one conversion.
#[derive(Debug, Default)]
pub struct Notes {
    /// Definitions per kind, in the order their keys were first referenced.
    pub definitions: HashMap<Kind, Vec<Definition>>,
    /// Whether each kind had any markers at all, so an empty section is not
    /// rendered.
    pub used: HashMap<Kind, bool>,
    /// Diagnostics gathered during the scan, consumed by [`validate`]. The scan
    /// never stops, so a library caller still gets a body back; the command line
    /// refuses on any of these before writing output.
    pub errors: Vec<String>,
}

impl Notes {
    /// The definitions of one kind, in reference order.
    pub fn of(&self, kind: Kind) -> &[Definition] {
        self.definitions
            .get(&kind)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Is anything to render at all?
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// The finished `<section>` for one kind, or empty if it has no
    /// definitions. Separate from [`Notes::render`] so a template can place the
    /// two lists independently.
    pub fn render_one(&self, kind: Kind) -> String {
        let defs = self.of(kind);
        if defs.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        {
            let title = match kind {
                Kind::Citation => "Notes",
                Kind::Glossary => "Glossary",
            };
            out.push_str("<section class=\"");
            out.push_str(kind.list_class());
            out.push_str("\" id=\"");
            out.push_str(kind.list_class());
            out.push_str("\">\n<h2 class=\"");
            out.push_str(kind.list_class());
            out.push_str("-heading\">");
            out.push_str(title);
            out.push_str("</h2>\n");
            if kind == Kind::Citation {
                // `<ol>`: the visible label is a number, which `<ol>` renders.
                out.push_str("<ol class=\"notes-list\">\n");
                for d in defs {
                    out.push_str("<li class=\"note\" id=\"note-");
                    out.push_str(&d.key);
                    out.push_str("\"><a class=\"note-back\" href=\"#note-ref-");
                    out.push_str(&d.key);
                    out.push_str("\">");
                    out.push_str(&d.number.to_string());
                    out.push_str("</a> ");
                    out.push_str(&d.content);
                    out.push_str("</li>\n");
                }
                out.push_str("</ol>\n");
            } else {
                // `<dl>`: the term is authored text, not a generated number.
                out.push_str("<dl class=\"glossary-list\">\n");
                for d in defs {
                    out.push_str("<dt id=\"gloss-");
                    out.push_str(&d.key);
                    out.push_str("\">");
                    out.push_str(&d.key);
                    out.push_str("</dt>\n<dd class=\"gloss-def\">");
                    out.push_str(&d.content);
                    out.push_str("</dd>\n");
                }
                out.push_str("</dl>\n");
            }
            out.push_str("</section>\n");
        }
        out
    }

    /// The two section blocks concatenated for injection at the end of the body:
    /// citations then glossary, independent of the order the kinds were enabled.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for kind in Kind::all() {
            out.push_str(&self.render_one(kind));
        }
        out
    }
}

/// Split a note kind out of an already-validated key, for diagnostics.
fn key_in(text: &str, kind: Kind) -> Option<&str> {
    text.strip_prefix(kind.token())
        .and_then(|rest| rest.strip_prefix(':'))
}

/// Is this `key` usable? Non-empty and free of characters that would break out
/// of the `id="…"` it lands in. This is an injection boundary, not a style
/// question: `"`, `<` and whitespace are refused rather than escaped, since
/// escaping would turn a mistake into a working id that links nowhere.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The marker namespace, braces included: `{{textrill:`.
const PREFIX: &str = "{{textrill:";

/// The length of the whole token starting at `pos`, or `None` if it is not a
/// well-formed one.
///
/// Strict about `}}`: without a close the text is literal, not a dangling
/// reference, so a typo that renders fine does not fail the document.
fn token_len(body: &str, pos: usize) -> Option<usize> {
    let rest = body.get(pos..)?.strip_prefix(PREFIX)?;
    let end = rest.find("}}")?;
    Some(PREFIX.len() + end + 2)
}

/// The `key` inside a complete marker token, validated.
fn key_of(body: &str, pos: usize) -> Result<(Kind, bool, String), String> {
    let len = token_len(body, pos).expect("caller checked");
    let token = &body[pos..pos + len];
    let inner = &token[PREFIX.len()..len - 2];
    if let Some(key) = inner.strip_prefix("def:") {
        for kind in Kind::all() {
            if let Some(k) = key_in(key, kind) {
                return finish(kind, true, k, token);
            }
        }
    }
    for kind in Kind::all() {
        if let Some(k) = key_in(inner, kind) {
            return finish(kind, false, k, token);
        }
    }
    Err(format!(
        "unknown marker {token}: expected {{{{textrill:cite:key}}}} or \
         {{{{textrill:gloss:key}}}} for a reference, or an opening \
         {{{{textrill:def:cite:key}}}} / {{{{textrill:def:gloss:key}}}} for a definition"
    ))
}

fn finish(
    kind: Kind,
    is_open: bool,
    key: &str,
    token: &str,
) -> Result<(Kind, bool, String), String> {
    if !valid_key(key) {
        return Err(format!(
            "note key {key:?} in {{{{{token}}}}} must be non-empty and use only \
             letters, digits, `-`, `_` and `.`"
        ));
    }
    Ok((kind, is_open, key.to_string()))
}

/// Line-oriented state, so definition content keeps its surrounding whitespace.
struct Cursor<'a> {
    body: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    /// Start of the next line at or after `from`.
    fn line_start(&self, from: usize) -> usize {
        let head = &self.body[..from];
        match head.rfind('\n') {
            Some(nl) => nl + 1,
            None => 0,
        }
    }

    /// One past the newline that ends the line containing `from`.
    fn line_end(&self, from: usize) -> usize {
        match self.body[from..].find('\n') {
            Some(nl) => from + nl + 1,
            None => self.body.len(),
        }
    }
}

/// Find, validate and rewrite every note marker in `body`, returning the
/// rewritten body and the collected notes.
///
/// `enabled` is `(citations, glossary)`. A kind that is off has its tokens left
/// as literal text rather than refused, so turning a mode off makes the document
/// render rather than fail. See the module docs for the `<pre>` limitation.
pub fn collect(body: &str, enabled: (bool, bool)) -> (String, Notes) {
    let mut notes = Notes::default();
    let mut out = String::with_capacity(body.len());
    let mut cur = Cursor { body, at: 0 };
    let mut pending = 0usize; // bytes of `body` already copied to `out`
    let mut open: Option<(Kind, String, usize)> = None; // kind, key, content start
                                                        // Whether the block opened its own paragraph, so closing can take the empty <p></p>.
    let mut open_was_paragraph = false;

    let on = |k: Kind| match k {
        Kind::Citation => enabled.0,
        Kind::Glossary => enabled.1,
    };

    while cur.at < body.len() {
        let here = cur.at;

        // Inside a block everything is content and is stepped over, not copied:
        // the text is read in one piece when the close arrives. Content may
        // contain `{{`; it cannot end a block.
        if open.is_some() && !body[here..].starts_with("{{/textrill:") {
            cur.at = next_marker(body, here + 1);
            skip(&mut pending, body, cur.at);
            continue;
        }

        // A close whose kind is off is literal text, like its opening tag:
        // reporting it as stray would refuse markers the user never enabled.
        if kind_of_close(body, here).is_some_and(|k| !on(k)) {
            cur.at = next_marker(body, here + 1);
            flush(&mut out, &mut pending, body, cur.at);
            continue;
        }

        // Every branch must flush the text it steps over: advancing `cur.at`
        // without flushing re-emits that text on the next flush.
        if body[here..].starts_with("{{/textrill:") {
            // Match on a copy: the borrow of `open` is live when `open` is
            // reassigned below.
            match open.clone() {
                None => {
                    let tag = closing_tag_at(body, here);
                    report(
                        &mut notes,
                        format!("stray closing tag {tag}: no definition block is open"),
                    );
                    cur.at = next_marker(body, here + 1);
                    flush(&mut out, &mut pending, body, cur.at);
                    continue;
                }
                Some((okind, okey, ostart)) => {
                    // The close must name the open definition; accepting any
                    // close would let one typo silently nest the next block.
                    let expected = format!("{{{{/textrill:def:{}:{}}}}}", okind.token(), okey);
                    if !body[here..].starts_with(&expected) {
                        report(
                            &mut notes,
                            format!(
                                "closing tag {} does not match the open block for {okey:?} \
                                 ({okind:?}); expected {expected:?}",
                                closing_tag_at(body, here)
                            ),
                        );
                        // Leave `open` set: a correct close may still follow.
                        cur.at = next_marker(body, here + 1);
                        flush(&mut out, &mut pending, body, cur.at);
                        continue;
                    }
                    let mut end = here + expected.len();
                    let content = body[ostart..here].trim_matches(['\n', '\r', ' ', '\t']);
                    if content.is_empty() {
                        report(
                            &mut notes,
                            format!("definition {okey:?} for {okind:?} is empty"),
                        );
                    } else {
                        push_def(&mut notes, okind, &okey, Some(content.to_string()));
                    }
                    // Drop the block's lines but keep anything else on them, so
                    // a stray marker in prose survives. A block that is a whole
                    // paragraph also takes its `<p></p>`; the `</p>` may be on
                    // the next line, so search from the tag, not end of line.
                    if open_was_paragraph && out.trim_end().ends_with("<p>") {
                        let after = &body[end..];
                        let lead = after.len() - after.trim_start().len();
                        if after[lead..].starts_with("</p>") {
                            out.truncate(out.trim_end().len() - "<p>".len());
                            end = end + lead + "</p>".len();
                        }
                    }
                    cur.at = cur.line_end(end);
                    flush(&mut out, &mut pending, body, cur.line_start(ostart));
                    skip(&mut pending, body, cur.at);
                    open_was_paragraph = false;
                    open = None;
                    continue;
                }
            }
        }

        if !body[here..].starts_with("{{textrill:") {
            // `here + 1`: searching from `here` could return `here` and spin.
            cur.at = next_marker(body, here + 1);
            flush(&mut out, &mut pending, body, cur.at);
            continue;
        }

        // A `{{textrill:` with no `}}` is ordinary text: failing would refuse a
        // document over a typo that renders as written.
        if token_len(body, here).is_none() {
            cur.at = next_marker(body, here + 1);
            flush(&mut out, &mut pending, body, cur.at);
            continue;
        }

        let (kind, is_open, key) = match key_of(body, here) {
            Ok(v) => v,
            Err(e) => {
                // A well-formed token we cannot read is a mistyped marker; refuse.
                report(&mut notes, e);
                cur.at = next_marker(body, here + 1);
                flush(&mut out, &mut pending, body, cur.at);
                continue;
            }
        };
        let end = here + token_len(body, here).expect("checked by key_of");

        if !on(kind) {
            // Kind off: the token stays literal. Flushing avoids emitting it twice.
            cur.at = end;
            flush(&mut out, &mut pending, body, cur.at);
            continue;
        }

        if is_open {
            if let Some((prev_kind, prev_key, _)) = &open {
                report(
                    &mut notes,
                    format!("definition {prev_key:?} for {prev_kind:?} is never closed"),
                );
            }
            // Content starts on the next line, so a one-line definition works.
            let content_start = cur.line_end(end);
            open = Some((kind, key, content_start));
            // Drop the opening line, keeping anything before it.
            cur.at = cur.line_end(end);
            flush(&mut out, &mut pending, body, here);
            skip(&mut pending, body, cur.at);
            // The engine wrapped these lines in `<p>`, so a paragraph-sized
            // block would leave a stray `<p></p>`. Recorded after the flush:
            // whether the block opens a paragraph depends on the text before it.
            open_was_paragraph = out.trim_end().ends_with("<p>");
        } else {
            let (number, first) = push_def(&mut notes, kind, &key, None);
            let mark = ref_markup(kind, &key, number, first);
            cur.at = end;
            flush(&mut out, &mut pending, body, here);
            out.push_str(&mark);
            skip(&mut pending, body, cur.at);
        }
    }
    if let Some((kind, key, _)) = &open {
        report(
            &mut notes,
            format!("definition {key:?} for {kind:?} is never closed"),
        );
    }
    flush(&mut out, &mut pending, body, body.len());
    (out, notes)
}

/// Copy `body[pending..upto]` into `out` and mark it consumed. `upto` is
/// clamped so a hand-edited document cannot panic the converter.
fn flush(out: &mut String, pending: &mut usize, body: &str, upto: usize) {
    let upto = upto.min(body.len()).max(*pending);
    out.push_str(&body[*pending..upto]);
    *pending = upto;
}

/// Step `pending` over `body[..upto]` without copying, for replaced text;
/// copying would emit the marker alongside its markup.
fn skip(pending: &mut usize, body: &str, upto: usize) {
    *pending = upto.min(body.len()).max(*pending);
}

/// The kind a closing tag at `pos` names, if it names one.
fn kind_of_close(body: &str, pos: usize) -> Option<Kind> {
    Kind::all()
        .into_iter()
        .find(|k| body[pos..].starts_with(&format!("{{{{/textrill:def:{}:", k.token())))
}

/// The closing tag at `pos`, for a diagnostic, or the rest if it has no `}}`.
fn closing_tag_at(body: &str, pos: usize) -> &str {
    let rest = &body[pos..];
    match rest.find("}}") {
        Some(e) => &rest[..e + 2],
        None => rest,
    }
}

/// Record a reference, and the placeholder definition if this is its first
/// time. Returns the reference's number.
///
/// The placeholder is overwritten when the block is reached; a reference whose
/// definition never arrives is caught in [`validate`], not here.
fn push_def(notes: &mut Notes, kind: Kind, key: &str, content: Option<String>) -> (usize, bool) {
    notes.used.insert(kind, true);
    let found = notes
        .definitions
        .get(&kind)
        .and_then(|l| l.iter().position(|d| d.key == key));
    let i = match found {
        Some(i) => i,
        None => {
            let list = notes.definitions.entry(kind).or_default();
            let number = list.len() + 1;
            list.push(Definition {
                key: key.to_string(),
                content: String::new(),
                number,
                referenced: false,
                defined: false,
            });
            list.len() - 1
        }
    };
    let list = notes.definitions.get_mut(&kind).expect("just inserted");
    let d = &mut list[i];
    let number = d.number;
    let first = !d.referenced;
    match content {
        None => d.referenced = true,
        Some(text) => {
            if d.defined {
                // Two blocks for one key; guessing which wins would lose a citation.
                notes.errors.push(format!(
                    "definition for {key:?} ({kind:?}) is given more than once"
                ));
            } else {
                d.defined = true;
                d.content = text;
            }
        }
    }
    (number, first)
}

/// The markup for one reference. Only the first reference to a key carries an
/// `id`: repeating the id would be invalid HTML and leave the note's "back"
/// link ambiguous, so the first is the anchor and the rest are plain links.
fn ref_markup(kind: Kind, key: &str, number: usize, first: bool) -> String {
    match kind {
        Kind::Citation => {
            let id = if first {
                format!(" id=\"note-ref-{key}\"")
            } else {
                String::new()
            };
            format!("<a class=\"note-ref\"{id} href=\"#note-{key}\">[{number}]</a>")
        }
        Kind::Glossary => {
            let id = if first {
                format!(" id=\"gloss-ref-{key}\"")
            } else {
                String::new()
            };
            format!("<a class=\"gloss-ref\"{id} href=\"#gloss-{key}\">{key}</a>")
        }
    }
}

/// The next position worth examining: the next `{{`, or the end. Stepping by
/// line would skip every marker not at the start of a line.
fn next_marker(body: &str, from: usize) -> usize {
    body[from..]
        .find("{{")
        .map(|i| from + i)
        .unwrap_or(body.len())
}

/// Record a diagnostic for [`validate`] to refuse on.
fn report(notes: &mut Notes, msg: String) {
    notes.errors.push(msg);
}

/// Check the collected notes: every reference has a definition, every
/// definition a reference, no key is defined twice, nothing is empty. `Ok` only
/// when nothing is reported.
pub fn validate(notes: &Notes) -> Result<(), String> {
    let mut errors = notes.errors.clone();
    for kind in Kind::all() {
        for d in notes.of(kind) {
            if !d.defined || d.content.is_empty() {
                errors.push(format!(
                    "reference to {key:?} has no definition ({kind:?})",
                    key = d.key
                ));
            } else if !d.referenced {
                errors.push(format!(
                    "definition for {} ({kind:?}) is never referenced",
                    d.key
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format_one(errors))
    }
}

/// One diagnostic per line. No program-name prefix, since the caller prints one.
fn format_one(errors: Vec<String>) -> String {
    errors.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Join lines with newlines and end with one. Written out rather than using
    /// `\`-continuations, since those strip leading whitespace these tests check.
    fn doc(lines: &[&str]) -> String {
        let mut s = lines.join("\n");
        s.push('\n');
        s
    }

    /// Every `id="…"` value in `markup`, in order.
    fn ids(markup: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = markup;
        while let Some(i) = rest.find("id=\"") {
            rest = &rest[i + 4..];
            if let Some(e) = rest.find('"') {
                out.push(rest[..e].to_string());
                rest = &rest[e + 1..];
            }
        }
        out
    }

    fn run(body: &str) -> (String, Notes, Result<(), String>) {
        let (out, notes) = collect(body, (true, true));
        let v = validate(&notes);
        (out, notes, v)
    }

    #[test]
    fn nothing_happens_when_nothing_is_present() {
        let body = "<p>plain &amp; simple</p>\n";
        let (out, notes, v) = run(body);
        assert_eq!(out, body);
        assert!(notes.is_empty());
        assert!(v.is_ok());
    }

    #[test]
    fn a_disabled_kind_passes_its_token_through() {
        // Turning a mode off must make a document render, not fail.
        let (out, notes) = collect("<p>{{textrill:cite:x}}</p>\n", (false, false));
        assert_eq!(out, "<p>{{textrill:cite:x}}</p>\n");
        assert!(notes.is_empty());
    }

    #[test]
    fn a_citation_is_numbered_in_first_reference_order() {
        let (out, notes, v) = run(&doc(&[
            "<p>a {{textrill:cite:knuth}} b {{textrill:cite:lamport}} c {{textrill:cite:knuth}}</p>",
            "{{textrill:def:cite:knuth}}",
            "Knuth.",
            "{{/textrill:def:cite:knuth}}",
            "{{textrill:def:cite:lamport}}",
            "Lamport.",
            "{{/textrill:def:cite:lamport}}",
        ]));
        assert!(v.is_ok(), "{v:?}");
        assert!(out.contains(">[1]</a>"), "{out}");
        assert!(out.contains(">[2]</a>"), "{out}");
        assert_eq!(
            out.matches(">[1]</a>").count(),
            2,
            "a repeat reference reuses its number"
        );
        let list = notes.render();
        assert!(
            list.contains("<li class=\"note\" id=\"note-knuth\">"),
            "{list}"
        );
        assert!(list.contains("Knuth."), "{list}");
        assert!(
            !out.contains("Knuth."),
            "the definition must not stay in the body"
        );
    }

    #[test]
    fn a_definition_block_is_removed_from_the_body() {
        let (out, _, v) = run(&doc(&[
            "<p>x {{textrill:cite:a}}</p>",
            "{{textrill:def:cite:a}}",
            "Definition text.",
            "{{/textrill:def:cite:a}}",
        ]));
        assert!(v.is_ok(), "{v:?}");
        assert_eq!(
            out,
            "<p>x <a class=\"note-ref\" id=\"note-ref-a\" href=\"#note-a\">[1]</a></p>\n"
        );
    }

    #[test]
    fn a_definition_on_its_own_line_loses_only_its_own_lines() {
        let (out, _, v) = run(&doc(&[
            "<p>prose</p>",
            "",
            "{{textrill:def:cite:a}}",
            "First.",
            "{{/textrill:def:cite:a}}",
            "<p>prose {{textrill:cite:a}}</p>",
        ]));
        assert!(v.is_ok(), "{v:?}");
        assert!(!out.contains("First."), "{out}");
        assert!(out.contains("<p>prose</p>\n\n<p>prose "), "{out:?}");
    }

    #[test]
    fn a_repeated_reference_does_not_repeat_an_id() {
        let (out, notes, v) = run(&doc(&[
            "<p>{{textrill:cite:a}} then {{textrill:cite:a}}</p>",
            "{{textrill:def:cite:a}}",
            "Note.",
            "{{/textrill:def:cite:a}}",
        ]));
        assert!(v.is_ok(), "{v:?}");
        assert_eq!(out.matches("id=\"note-ref-a\"").count(), 1, "{out}");
        assert_eq!(
            out.matches("href=\"#note-a\"").count(),
            2,
            "both must still link: {out}"
        );

        // Every generated id is unique and every link target exists.
        let whole = format!("{out}{}", notes.render());
        let all = ids(&whole);
        let mut sorted = all.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(all.len(), sorted.len(), "duplicate id in {whole}");
        for target in ["#note-a", "#note-ref-a"] {
            let want = &target[1..];
            assert!(
                all.iter().any(|i| i == want),
                "{target} has no id in {all:?}"
            );
        }
    }

    #[test]
    fn a_glossary_term_displays_its_own_key() {
        let (out, notes, v) = run(&doc(&[
            "<p>the {{textrill:gloss:monad}} idea</p>",
            "{{textrill:def:gloss:monad}}",
            "A monad.",
            "{{/textrill:def:gloss:monad}}",
        ]));
        assert!(v.is_ok(), "{v:?}");
        assert!(out.contains(">monad</a>"), "{out}");
        let list = notes.render();
        assert!(list.contains("<dt id=\"gloss-monad\">monad</dt>"), "{list}");
    }

    #[test]
    fn a_dangling_reference_is_refused() {
        let (_, _, v) = run("<p>{{textrill:cite:missing}}</p>\n");
        let e = v.unwrap_err();
        assert!(e.contains("missing"), "{e}");
        assert!(e.contains("no definition"), "{e}");
    }

    #[test]
    fn a_closing_tag_must_match_the_block_it_closes() {
        // A mismatched close must not end the block, or the next nests inside.
        let (_, _, v) = run(&doc(&[
            "<p>{{textrill:cite:lonely}}</p>",
            "{{textrill:def:cite:lonely}}",
            "Text.",
            "{{/textrill:def:lonelyx}}",
        ]));
        let e = v.unwrap_err();
        assert!(e.contains("does not match"), "{e}");
        assert!(e.contains("lonelyx"), "{e}");
    }

    #[test]
    fn a_stray_closing_tag_is_refused() {
        let (_, _, v) = run(&doc(&[
            "<p>x</p>",
            "{{/textrill:def:cite:a}}",
            "{{textrill:cite:a}}",
        ]));
        assert!(v.unwrap_err().contains("stray"));
    }

    #[test]
    fn an_unreferenced_definition_is_refused() {
        // The other direction: an unreferenced key is usually a typo.
        let (_, _, v) = run(&doc(&[
            "<p>nothing</p>",
            "{{textrill:def:cite:lonely}}",
            "Text.",
            "{{/textrill:def:cite:lonely}}",
        ]));
        assert!(v.unwrap_err().contains("never referenced"));
    }

    #[test]
    fn a_definition_given_twice_is_refused() {
        let (_, _, v) = run(&doc(&[
            "<p>{{textrill:cite:a}}</p>",
            "{{textrill:def:cite:a}}",
            "First.",
            "{{/textrill:def:cite:a}}",
            "{{textrill:def:cite:a}}",
            "Second.",
            "{{/textrill:def:cite:a}}",
        ]));
        assert!(v.unwrap_err().contains("more than once"));
    }

    #[test]
    fn an_empty_definition_is_refused() {
        let (_, _, v) = run(&doc(&[
            "<p>{{textrill:cite:a}}</p>",
            "{{textrill:def:cite:a}}",
            "{{/textrill:def:cite:a}}",
        ]));
        assert!(v.is_err());
    }

    #[test]
    fn an_unclosed_block_is_refused() {
        let (_, _, v) = run(&doc(&[
            "<p>{{textrill:cite:a}}</p>",
            "{{textrill:def:cite:a}}",
            "text",
        ]));
        let e = v.unwrap_err();
        assert!(e.contains("never closed"), "{e}");
    }

    #[test]
    fn a_key_that_would_break_out_of_an_id_is_refused() {
        // An injection boundary, not a style rule: the key lands in id="…".
        for bad in ["a\"b", "a<b", "a b", "a:b", ""] {
            let body = format!("<p>{{{{textrill:cite:{bad}}}}}</p>\n");
            let (_, _, v) = run(&body);
            assert!(v.is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn a_typo_in_the_namespace_is_not_silently_literal() {
        // Each is an attempt to use our namespace, so each is refused, not passed.
        for body in [
            "<p>{{textrill:cites:x}}</p>\n",
            "<p>{{textrill:cite}}</p>\n",
            "<p>{{textrill:def:x}}</p>\n",
            "<p>{{textrill:}}</p>\n",
        ] {
            let (_, _, v) = run(body);
            assert!(v.is_err(), "{body:?} must be refused, not rendered as text");
        }
    }

    #[test]
    fn prose_that_merely_looks_like_a_note_is_left_alone() {
        // The no-inference rule, which is the whole reason for namespacing.
        let body = doc(&[
            "<p>a footnote[^1] and a caret^2 and (3) and [4] and @five</p>",
            "<p>also {{ and }} and {{textrill}} and {{ textrill:cite:x }}</p>",
        ]);
        let (out, notes, v) = run(&body);
        assert!(v.is_ok(), "{v:?}");
        assert_eq!(out, body, "nothing may be interpreted");
        assert!(notes.is_empty());
    }

    #[test]
    fn a_glossary_reference_does_not_satisfy_a_citation_definition() {
        let (_, _, v) = run(&doc(&[
            "<p>{{textrill:gloss:x}}</p>",
            "{{textrill:def:cite:x}}",
            "T.",
            "{{/textrill:def:cite:x}}",
        ]));
        assert!(
            v.unwrap_err().contains("Glossary"),
            "the kind is part of the key"
        );
    }

    #[test]
    fn an_unterminated_token_is_literal_text_not_an_error() {
        // An unterminated token renders as written; failing would refuse a
        // harmless typo.
        let (out, notes, v) = run("<p>{{textrill:cite:x}</p>\n");
        assert!(v.is_ok(), "{v:?}");
        assert!(out.contains("{{textrill:cite:x}"), "{out}");
        assert!(notes.is_empty());
    }

    #[test]
    fn one_mode_can_be_on_while_the_other_is_off() {
        // Enabling one mode must not draw the other in: an off-kind marker stays
        // literal text, not a stray, keeping default output identical.
        let body = doc(&[
            "<p>{{textrill:cite:a}} and {{textrill:gloss:b}}</p>",
            "{{textrill:def:cite:a}}",
            "Cite.",
            "{{/textrill:def:cite:a}}",
            "{{textrill:def:gloss:b}}",
            "Gloss.",
            "{{/textrill:def:gloss:b}}",
        ]);
        let (out, notes) = collect(&body, (true, false));
        assert!(
            validate(&notes).is_ok(),
            "nothing was collected, so nothing dangles"
        );
        assert!(
            out.contains("id=\"note-ref-a\""),
            "citations must still work: {out}"
        );
        assert!(
            out.contains("{{textrill:gloss:b}}"),
            "the reference stays literal: {out}"
        );
        assert!(
            out.contains("{{textrill:def:gloss:b}}"),
            "and so does its block: {out}"
        );
        let list = notes.render();
        assert!(list.contains("Cite."), "{list}");
        assert!(
            !list.contains("Gloss."),
            "the disabled mode must not reach the notes: {list}"
        );
    }

    #[test]
    fn definition_content_is_escaped_document_markup_not_injected() {
        // Content is already-rendered body HTML: it keeps entities, not re-escaped.
        let (_, notes, _) = run(&doc(&[
            "<p>{{textrill:cite:a}}</p>",
            "{{textrill:def:cite:a}}",
            "<em>Smith &amp; Jones</em>",
            "{{/textrill:def:cite:a}}",
        ]));
        let list = notes.render();
        assert!(list.contains("<em>Smith &amp; Jones</em>"), "{list}");
    }
}
