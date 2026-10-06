//! Opt-in citations and glossary (Phase 5).
//!
//! Both features are off by default and, when off, this module does nothing at
//! all — the output is byte-identical to a build without it.
//!
//! # The syntax
//!
//! A **reference** and a **definition**, per kind:
//!
//! ```text
//! {{textrill:cite:knuth1984}}          a citation reference
//! {{textrill:def:cite:knuth1984}} … {{/textrill:def:cite:knuth1984}}
//!
//! {{textrill:gloss:monad}}            a glossary reference; the key is also
//!                                     the term, and it is displayed as-is
//! {{textrill:def:gloss:monad}} … {{/textrill:def:gloss:monad}}
//! ```
//!
//! Definitions are **balanced blocks** rather than one-token-with-a-payload
//! because the content is prose that wants to wrap, and a `::`-separated
//! single-line form would force a citation onto one line.
//!
//! Nothing outside the `textrill:` namespace is ever interpreted. `[^1]`,
//! `^1`, `(1)`, `@handle`, `[1]` — none of them mean anything here, in any
//! mode. That is the whole of the collision argument: a marker can only be one
//! if the author typed this exact namespace, and the only cost is that
//! `{{textrill:` in prose is reserved. Two modes' tokens cannot collide either,
//! because the kind is in the token.
//!
//! # Why a definition list and not a CSS reveal
//!
//! The design note asked for both — an end-of-document list *and* a CSS-only
//! reveal at the reference — and those cannot both hold one copy of the text.
//! CSS cannot move content from the end of a document to the place it was
//! referenced, so a reveal has to *duplicate* every definition at every
//! reference site. A definition list keeps one copy, and a reference is an
//! anchor to it. That is also the only shape that survives `--extract`, where
//! there is no list to jump to, and the only one that is legible when CSS fails
//! to load.
//!
//! # Which is a hard error
//!
//! Everything ambiguous, when the relevant mode is on:
//!
//! * a reference with no definition, and a definition with no reference
//!   (both directions — an orphan definition is usually a typo'd key, and a
//!   dangling reference is a lost citation)
//! * a definition given twice for one key
//! * an empty definition
//! * an unbalanced block, or a stray close tag
//! * any other `{{textrill:…}}` token. Passing it through would make a typo
//!   like `{{textrill:cites:x}}` silently render as literal text in the output,
//!   which is the one outcome a namespaced marker is supposed to make
//!   impossible. Template slots are unknown on the same terms, in
//!   [`template::validate`](crate::template::validate).
//!
//! # Why this runs on the rendered body, not the source
//!
//! It has to run on the body, because a reference must become markup that
//! carries a number assigned in document order — which is only known once the
//! whole document has been seen. Scanning the source instead would mean either
//! buffering the whole conversion (defeating `--stream`, which this mode refuses
//! anyway) or assigning numbers in a second pass over markup that had already
//! been written.
//!
//! The cost of running late is that a marker inside preformatted text would be
//! collected as if it were prose. Definitions are therefore lifted out *with*
//! their surrounding lines, and the caller is expected to have marked code as
//! code; a `<pre>` block is plain text to this pass. See the note in
//! [`collect`].

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
    /// A `{{textrill:def:…}}` block supplied its content.
    ///
    /// Tracked separately from `referenced`, because the two directions are
    /// different mistakes with different fixes: a reference with no definition
    /// is a lost citation, and a definition nothing references is usually a
    /// mistyped key. Collapsing them into one flag loses whichever half.
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
    /// Diagnostics gathered during the scan, consumed by [`validate`].
    ///
    /// The scan never stops, so that a library caller still gets a body back
    /// and can decide for itself; the command line refuses on any of these
    /// before writing output.
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

    /// The two section blocks, concatenated, for injection at the end of the
    /// body.
    ///
    /// Order is citations then glossary, not the order the kinds were enabled,
    /// so the output does not depend on flag order.
    /// The finished `<section>` for one kind, or empty if it has no definitions.
    ///
    /// Separate from [`Notes::render`] so a template can place the two lists
    /// independently.
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
                // `<ol>` because the visible label is a number, and a number is
                // what `list-style-type` renders by default.
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
                // `<dl>` because the term is authored text, not a number, and a
                // generated marker in a `<dt>` would be a lie about the source.
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

    /// The finished lists for every kind, citations first.
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

/// Is this `key` usable? Non-empty, and free of the characters that would let
/// it break out of the `id="…"` it lands in.
///
/// A key is written by the author and ends up in an `id`, so this is an
/// injection boundary, not a style question: `"` and `<` and whitespace are
/// refused rather than escaped, because a key containing them is a mistake and
/// escaping would turn the mistake into a working id that links nowhere.
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
/// Deliberately strict about `}}`: a token is only a marker if it closes here.
/// `{{textrill:cite:x}` with no close is literal text, and reporting it as a
/// dangling reference would fail a document over a typo that renders fine.
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

/// Line-oriented state, so a definition's content can be lifted out with the
/// whitespace that surrounded it rather than left as a ragged edge.
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

/// Find, validate and rewrite every note marker in `body`.
///
/// Returns the rewritten body and the collected notes. `enabled` is
/// `(citations, glossary)`; a kind that is off has its tokens left as literal
/// text rather than refused, so turning a mode off makes the document render
/// rather than fail.
///
/// # Preformatted text
///
/// A `<pre>` block is plain text here, so a marker inside one is collected as
/// if it were prose. That is a real limitation and it is deliberate: this pass
/// runs on rendered markup, where it cannot tell code from prose, and adding a
/// `<pre>`/`<code>` skipper would be an approximation of the same question —
/// `make_preformatted` runs *before* this, so an indented block is already
/// `<pre>` by the time the markers are seen, and skipping those regions would
/// cover the common case. It is left undone rather than half-done because the
/// alternative that is actually correct is a source-level scan, and the reason
/// this is a body pass at all is in the module docs. An author showing the
/// syntax in a code sample should keep `--citations` off for that file.
pub fn collect(body: &str, enabled: (bool, bool)) -> (String, Notes) {
    let mut notes = Notes::default();
    let mut out = String::with_capacity(body.len());
    let mut cur = Cursor { body, at: 0 };
    let mut pending = 0usize; // bytes of `body` already copied to `out`
    let mut open: Option<(Kind, String, usize)> = None; // kind, key, content start
                                                        // Whether the block opened a paragraph of its own, so closing it can take
                                                        // the empty `<p></p>` with it.
    let mut open_was_paragraph = false;

    let on = |k: Kind| match k {
        Kind::Citation => enabled.0,
        Kind::Glossary => enabled.1,
    };

    while cur.at < body.len() {
        let here = cur.at;

        // Inside a definition block, everything up to its closing tag is
        // content and must be stepped over rather than copied: the block's text
        // is read from `ostart` in one piece when the close tag arrives, so
        // copying it here would leave the definition in the body *and* in the
        // note list. Content may itself contain `{{`; it cannot end a block.
        if open.is_some() && !body[here..].starts_with("{{/textrill:") {
            cur.at = next_marker(body, here + 1);
            skip(&mut pending, body, cur.at);
            continue;
        }

        // A closing tag whose kind is off is literal text, exactly like its
        // opening tag: reporting it as stray would refuse a document for
        // markers the user never asked textrill to interpret.
        if kind_of_close(body, here).is_some_and(|k| !on(k)) {
            cur.at = next_marker(body, here + 1);
            flush(&mut out, &mut pending, body, cur.at);
            continue;
        }

        // Every branch below must call `flush` for the text it steps over.
        // Advancing `cur.at` without flushing looks harmless and is not: a
        // later flush starts from `pending`, so the skipped text would be
        // emitted a second time, duplicating a whole definition block.
        if body[here..].starts_with("{{/textrill:") {
            // Match on a copy: the borrow of `open` would still be live when the
            // block is closed and `open` reassigned.
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
                    // The closing tag must name the definition the opening tag
                    // named. Accepting any close would let one typo end a block
                    // and silently nest everything up to the next close inside it.
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
                        // Leave `open` alone: a correct close tag may still
                        // follow, and consuming this one would nest it.
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
                    // Drop the whole block: its opening line, its content and
                    // its closing tag. Anything else on the opening or closing
                    // line is kept, so a stray marker in prose is not deleted
                    // along with the block.
                    // A block that is a whole paragraph takes its `<p></p>`
                    // with it, or the output gains a stray `<p></p>`. The
                    // `</p>` may sit on the closing line or the one after it, so
                    // look from the tag itself rather than from end of line.
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
            // `here + 1`, not `here`: the text at `here` may itself be the `{{`
            // of something that is not a marker, and searching from `here`
            // would return `here` and spin forever.
            cur.at = next_marker(body, here + 1);
            flush(&mut out, &mut pending, body, cur.at);
            continue;
        }

        // A `{{textrill:` with no closing `}}` is not a token at all. It is
        // ordinary text and stays that way: failing here would refuse a document
        // over a typo that renders exactly as written.
        if token_len(body, here).is_none() {
            cur.at = next_marker(body, here + 1);
            flush(&mut out, &mut pending, body, cur.at);
            continue;
        }

        let (kind, is_open, key) = match key_of(body, here) {
            Ok(v) => v,
            Err(e) => {
                // A well-formed token in our namespace that we cannot read is a
                // mistyped marker, not prose. Refuse rather than render it.
                report(&mut notes, e);
                cur.at = next_marker(body, here + 1);
                flush(&mut out, &mut pending, body, cur.at);
                continue;
            }
        };
        let end = here + token_len(body, here).expect("checked by key_of");

        if !on(kind) {
            // The kind is off: the token stays literal text. Flushing it is what
            // keeps it from being emitted twice.
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
            // Content starts on the line after the opening tag, so a definition
            // written on one line still works.
            let content_start = cur.line_end(end);
            open = Some((kind, key, content_start));
            // Drop the opening line, keeping anything before it.
            cur.at = cur.line_end(end);
            flush(&mut out, &mut pending, body, here);
            skip(&mut pending, body, cur.at);
            // The engine has already wrapped these lines in `<p>`, so a block
            // that is a paragraph by itself leaves a stray `<p></p>` behind if
            // only the block is removed. Recorded after the flush, because
            // whether the block opens a paragraph is a question about what has
            // been emitted *including* the text before it -- asking earlier
            // looks at the paragraph that ended before the block.
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

/// Copy `body[pending..upto]` into `out` and mark it consumed.
///
/// `upto` is clamped, because a converter must not panic on a hand-edited
/// document: losing trailing text is recoverable, a crash is not.
fn flush(out: &mut String, pending: &mut usize, body: &str, upto: usize) {
    let upto = upto.min(body.len()).max(*pending);
    out.push_str(&body[*pending..upto]);
    *pending = upto;
}

/// Step `pending` over `body[..upto]` without copying, for text being replaced
/// rather than kept. Copying here would emit the marker alongside its markup.
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
/// The placeholder is overwritten by the real definition when the block is
/// reached; a reference whose definition never arrives is caught in
/// [`validate`], not here.
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
                // Two blocks for one key. Which one wins would be a guess, and
                // a guess here means silently losing a citation.
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

/// The markup for one reference.
///
/// Only the first reference to a key carries an `id`. Giving every mention the
/// same id would repeat it in the output, which is invalid HTML and leaves the
/// note's "back" link ambiguous about where it goes; giving each mention a
/// suffixed id would instead make every mention but the first unaddressable. So
/// the first is the anchor, and the rest are plain links to the same note.
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

/// The next position worth examining: the next `{{`, or the end.
///
/// Stepping by a line instead of by a token would skip every marker that is not
/// at the start of a line, and a marker in running prose is the normal case.
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
/// definition has a reference, no key is defined twice, nothing is empty.
///
/// `body_errors` are carried over from parsing. `Ok` is returned only when
/// there is nothing to report.
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

/// One diagnostic per line. No program-name prefix: the caller already prints
/// one, and `textrill: textrill: …` reads like a nested tool.
fn format_one(errors: Vec<String>) -> String {
    errors.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Join lines with newlines and end with one.
    ///
    /// Written out rather than using `\`-continuations because a continuation
    /// strips its own leading whitespace, and these tests care about exactly
    /// which whitespace surrounds a definition block.
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
        // The common layout: definitions after the prose, one block each.
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
        // Two mentions of one citation used to emit `id="note-ref-a"` twice:
        // invalid HTML, and a "back" link with two possible targets.
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

        // Every generated id in the document is unique, and every fragment a
        // generated link points at exists.
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
        // A close naming something else must not end the block, or the next
        // block would be silently nested inside this one.
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
        // The other direction, and a different mistake: nothing lost, but a key
        // nothing points at is usually a typo that will become a lost citation.
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
        // Each of these is unmistakably an attempt to use our namespace, so
        // each is refused rather than passed through as text.
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
        // `{{textrill:cite:x}` with no close renders as written; failing here
        // would refuse a document over a typo that is harmless in the output.
        let (out, notes, v) = run("<p>{{textrill:cite:x}</p>\n");
        assert!(v.is_ok(), "{v:?}");
        assert!(out.contains("{{textrill:cite:x}"), "{out}");
        assert!(notes.is_empty());
    }

    #[test]
    fn one_mode_can_be_on_while_the_other_is_off() {
        // Enabling one mode must not draw the other into it. A glossary marker
        // is not a stray citation, so it is neither collected nor refused; it
        // stays literal text, which is what keeps the default output identical.
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
        // The content is already-rendered body HTML, so it keeps whatever the
        // engine did to it -- including entities -- and nothing is re-escaped.
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
