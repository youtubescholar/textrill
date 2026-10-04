// textrill — convert plain text to HTML.
//
// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Opt-in sectioning and table-of-contents generation (Phase 5).
//!
//! This module is a **pure post-pass** over the body HTML the engine has
//! already produced. It never runs unless `--section`, `--toc` or `--chunk` is
//! set, so byte-identical default output is untouched.
//!
//! Sections are the heading-delimited runs of the converted body. Each gets a
//! sequential `chunk-N` id assigned here, not the `make_anchors`
//! `section_x_y` names. Positional ids cannot collide and cannot produce a
//! dead TOC link, which is the failure the plan warned about when a TOC reuses
//! heading-derived anchors.
//!
//! The markup emitted here is lower-case and HTML5-flavoured (`<article>`,
//! `<nav>`); it is intended to be used together with `--html5`. It does not
//! change any tag the engine itself emits.

/// One heading-delimited section of the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Heading level, 1–6.
    pub level: usize,
    /// Sequential id, `chunk-1`, `chunk-2`, ….
    pub id: String,
    /// Heading text with tags stripped, entities left as-is.
    pub label: String,
    /// The raw generated HTML of the section, heading included.
    pub html: String,
}

/// Recognise a heading line emitted by the engine.
///
/// The engine writes a heading alone on one line as `<h1>…</h1>` (or upper
/// case), optionally with an `<a name="…">` anchor inside. Only column-zero
/// lines are considered, so a heading-looking string indented inside `<pre>`
/// (reachable only with `--no-escape_HTML_chars`) is not mistaken for one.
fn parse_heading(line: &str) -> Option<(usize, String)> {
    let t = line.trim_end_matches(['\r', ' ', '\t']);
    let b = t.as_bytes();
    if b.len() < 8 || b[0] != b'<' {
        return None;
    }
    if !(b[1] == b'h' || b[1] == b'H') || b[3] != b'>' {
        return None;
    }
    let level = match b[2] {
        b'1'..=b'6' => (b[2] - b'0') as usize,
        _ => return None,
    };
    let lower = t.to_ascii_lowercase();
    let close = format!("</h{level}>");
    if !lower.ends_with(&close) {
        return None;
    }
    let inner = t[4..t.len() - close.len()].to_string();
    Some((level, inner))
}

/// Remove HTML tags, keeping character entities and text.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Split a converted body into its preamble and its heading-delimited sections.
///
/// The preamble is everything before the first heading; it is returned
/// unchanged and is never wrapped in an `<article>`.
pub fn split_sections(body: &str) -> (String, Vec<Section>) {
    let mut preamble = String::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<Section> = None;
    let mut next_id = 0usize;

    for line in body.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        match parse_heading(content) {
            Some((level, inner)) => {
                if let Some(sec) = current.take() {
                    sections.push(sec);
                }
                next_id += 1;
                current = Some(Section {
                    level,
                    id: format!("chunk-{next_id}"),
                    label: strip_tags(&inner),
                    html: line.to_string(),
                });
            }
            None => match current.as_mut() {
                Some(sec) => sec.html.push_str(line),
                None => preamble.push_str(line),
            },
        }
    }
    if let Some(sec) = current {
        sections.push(sec);
    }
    (preamble, sections)
}

/// Render a flat TOC; `toc-hN` on each item lets CSS indent by level.
fn render_toc(sections: &[Section]) -> String {
    let mut out = String::with_capacity(64 + sections.len() * 56);
    out.push_str("<nav class=\"toc\" id=\"toc\">\n<ol class=\"toc-list\">\n");
    for s in sections {
        out.push_str("<li class=\"toc-h");
        out.push_str(&s.level.to_string());
        out.push_str("\"><a href=\"#");
        out.push_str(&s.id);
        out.push_str("\">");
        out.push_str(&s.label);
        out.push_str("</a></li>\n");
    }
    out.push_str("</ol>\n</nav>\n");
    out
}

/// Wrap each section in an `<article class="section" id="chunk-N">` and,
/// when `toc`, prepend a generated table of contents.
///
/// A body with no headings is returned unchanged, so `--toc` on a heading-less
/// document is a no-op rather than an empty navigation.
pub fn sectionize(body: &str, toc: bool) -> String {
    let (preamble, sections) = split_sections(body);
    if sections.is_empty() {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len() + sections.len() * 64);
    if toc {
        out.push_str(&render_toc(&sections));
    }
    out.push_str(&preamble);
    for s in &sections {
        out.push_str("<article class=\"section\" id=\"");
        out.push_str(&s.id);
        out.push_str("\">\n");
        out.push_str(&s.html);
        if !s.html.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("</article>\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> String {
        "<h1><a name=\"section_1\">One</a></h1>\n\n<p>a\n</p>\n\
         <h2><a name=\"section_1_1\">Two</a></h2>\n\n<p>b\n</p>\n\
         <h1><a name=\"section_2\">Three</a></h1>\n\n<p>c\n</p>\n"
            .to_string()
    }

    #[test]
    fn parse_heading_accepts_both_cases_and_anchors() {
        assert_eq!(parse_heading("<h1>Hi</h1>").unwrap(), (1, "Hi".into()));
        assert_eq!(
            parse_heading("<H2><A NAME=\"section_1\">Hi</A></H2>").unwrap(),
            (2, "<A NAME=\"section_1\">Hi</A>".into())
        );
    }

    #[test]
    fn parse_heading_rejects_non_headings() {
        assert!(parse_heading("<p>x</p>").is_none());
        assert!(parse_heading("<hr/>").is_none());
        assert!(parse_heading(" <h1>x</h1>").is_none());
        assert!(parse_heading("<h7>x</h7>").is_none());
        assert!(parse_heading("<h1>x</h2>").is_none());
    }

    #[test]
    fn strips_nested_tags_from_labels() {
        assert_eq!(strip_tags("<a name=\"x\">A &amp; B</a>"), "A &amp; B");
        assert_eq!(strip_tags("plain"), "plain");
    }

    #[test]
    fn splits_into_sequential_ids() {
        let (pre, secs) = split_sections(&body());
        assert!(pre.is_empty());
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].id, "chunk-1");
        assert_eq!(secs[1].id, "chunk-2");
        assert_eq!(secs[2].id, "chunk-3");
        assert_eq!(secs[0].label, "One");
        assert_eq!(secs[1].level, 2);
    }

    #[test]
    fn wraps_sections_and_keeps_preamble_out() {
        let input = "intro\n<h1>T</h1>\n<p>b</p>\n";
        let out = sectionize(input, false);
        assert!(out.starts_with("intro\n"));
        assert!(out.contains(
            "<article class=\"section\" id=\"chunk-1\">\n<h1>T</h1>\n<p>b</p>\n</article>\n"
        ));
        assert!(!out.contains("<nav"));
    }

    #[test]
    fn toc_links_each_section() {
        let out = sectionize(&body(), true);
        assert!(out.starts_with("<nav class=\"toc\" id=\"toc\">\n"));
        assert!(out.contains("<li class=\"toc-h1\"><a href=\"#chunk-1\">One</a></li>"));
        assert!(out.contains("<li class=\"toc-h2\"><a href=\"#chunk-2\">Two</a></li>"));
    }

    #[test]
    fn no_headings_is_a_noop() {
        let input = "<p>just text\n</p>\n";
        assert_eq!(sectionize(input, true), input);
    }
}
