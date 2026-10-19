//! The `textrill templates` command surface (roadmap §5.4).
//!
//! The subcommands share one shape: parse into [`Options`] (so rc files and
//! `--template_dir` reach discovery), act, and report through [`Outcome`],
//! whose two streams and exit code the binary writes for it — a test can run
//! one with a controlled environment instead of touching the real one. The
//! catalog listing itself is shared by `--list_templates` and
//! `textrill templates list`; the verbs are added alongside the dispatch in
//! `main.rs`.

use std::fmt::Write as _;

use crate::options::Options;
use crate::pack::{self, Entry};

/// What a `templates` command wants written: the two streams and the process
/// exit code. Discovery never fails, so a listing always exits 0; a verb
/// reports its own refusal on stderr with code 1.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: u8,
}

/// `--list_templates` (roadmap §5.4): print the discovered catalog as text or
/// JSON per [`Options::list_templates`], discovery problems on stderr. Always
/// exits 0 — a broken pack is a warning, not a reason to hide the rest.
pub fn list_templates(opts: &Options) -> Outcome {
    let catalog = pack::discover(&opts.search_path());
    listing(&catalog, opts.list_templates == "json", None)
}

/// The listing both `--list_templates` and `textrill templates list` print:
/// one line per template on stdout, the problems seen on the way on stderr.
/// `tag` keeps only templates carrying that tag; the shipped set carries none,
/// so a tag filter shows pack templates only. Catalog order is already
/// deterministic (roots, then sorted packs, then manifest order).
fn listing(catalog: &pack::Catalog, json: bool, tag: Option<&str>) -> Outcome {
    let entries: Vec<&Entry> = catalog
        .entries
        .iter()
        .filter(|e| tag.is_none_or(|t| e.tags.iter().any(|have| have == t)))
        .collect();
    let text = if json {
        json_listing(&entries)
    } else {
        text_listing(&entries)
    };
    let mut problems = String::new();
    for problem in &catalog.problems {
        problems.push_str(problem);
        problems.push('\n');
    }
    Outcome {
        stdout: text.into_bytes(),
        stderr: problems.into_bytes(),
        code: 0,
    }
}

/// Text form: `pack-id · name · model · source-label`, one per line.
fn text_listing(entries: &[&Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        let _ = writeln!(
            out,
            "{} \u{b7} {} \u{b7} {} \u{b7} {}",
            e.pack_id,
            e.name,
            e.model.name(),
            e.source.label()
        );
    }
    out
}

/// JSON form: one object per template inside a pretty array, one array element
/// per line so a pipe into `jq` (or a line-wise reader) sees whole records.
/// The whole catalog's fields, not just the text columns: a caller that wants
/// to inspect a pack must not need a second command to get at it.
fn json_listing(entries: &[&Entry]) -> String {
    if entries.is_empty() {
        return "[]\n".to_string();
    }
    let mut out = String::from("[\n");
    for (i, e) in entries.iter().enumerate() {
        let root = match &e.root {
            Some(p) => jstr(&p.display().to_string()),
            None => "null".to_string(),
        };
        let tags = jstr_list(&e.tags);
        let slots = jstr_list(&e.slots);
        let vars: Vec<String> = e
            .vars
            .iter()
            .map(|v| {
                format!(
                    "{{\"name\": {}, \"required\": {}}}",
                    jstr(&v.name),
                    v.required
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "  {{\"address\": {}, \"pack_id\": {}, \"pack_name\": {}, \"name\": {}, \
             \"model\": {}, \"source\": {}, \"version\": {}, \"description\": {}, \
             \"tags\": {}, \"slots\": {}, \"vars\": [{}], \"root\": {}}}{}",
            jstr(&e.address()),
            jstr(&e.pack_id),
            jstr(&e.pack_name),
            jstr(&e.name),
            jstr(e.model.name()),
            jstr(e.source.label()),
            jstr(&e.version),
            jstr(&e.description),
            tags,
            slots,
            vars.join(", "),
            root,
            if i + 1 == entries.len() { "" } else { "," }
        );
    }
    out.push_str("]\n");
    out
}

/// A JSON array of strings on one line.
fn jstr_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| jstr(s)).collect();
    format!("[{}]", quoted.join(", "))
}

/// A JSON string literal: quote, backslash and the control characters escaped,
/// everything else (including the UTF-8 the templates carry) verbatim.
fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
