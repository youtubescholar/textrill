// textrill — convert plain text to HTML.
//
// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Which URL schemes may reach an `href`.
//!
//! The converted text is untrusted, so this is the one place that decides which
//! schemes are safe. It scans finished markup, covering every `href` producer
//! (including `-h->` raw HTML). [`scrub_hrefs`] returns `None` when it changed
//! nothing, keeping output byte-identical to the reference, and unwraps a
//! refused anchor rather than deleting it so text survives. Default refuses
//! [`DANGEROUS_SCHEMES`]; a strict `--allowed_url_schemes` list is the allowlist.

/// Schemes refused by default: the script-bearing ones, plus `file` (points at
/// the reader's filesystem) and `data` (attacker-authored HTML).
pub const DANGEROUS_SCHEMES: &[&str] = &["javascript", "data", "vbscript", "file"];

/// Schemes the built-in dictionary links, kept for `--help` and for a strict list.
pub const WELL_KNOWN_SCHEMES: &[&str] = &[
    "ftp", "ftps", "gopher", "http", "https", "mailto", "news", "nntp", "telnet", "wais",
];

/// Which schemes a generated `href` may use. Compared ASCII-case-insensitively,
/// because a browser does: `JaVaScRiPt:` is the same scheme as `javascript:`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UrlPolicy {
    /// Refuse [`DANGEROUS_SCHEMES`] and allow everything else, including
    /// relative references and unknown custom schemes.
    #[default]
    Default,
    /// Allow exactly these schemes, plus relative references.
    Strict(Vec<String>),
}

impl UrlPolicy {
    /// A policy allowing exactly `schemes`. An empty or all-whitespace list is
    /// [`UrlPolicy::Default`], not "allow nothing": `--allowed_url_schemes ''`
    /// means "no opinion".
    pub fn strict<I, S>(schemes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut allowed: Vec<String> = Vec::new();
        for s in schemes {
            let s = s.as_ref().trim();
            if s.is_empty() {
                continue;
            }
            let lower = s.to_ascii_lowercase();
            if !allowed.contains(&lower) {
                allowed.push(lower);
            }
        }
        if allowed.is_empty() {
            return UrlPolicy::Default;
        }
        allowed.sort();
        UrlPolicy::Strict(allowed)
    }

    /// May `url` be used as an `href`? A URL with no scheme is a relative
    /// reference (`page.html`, `/x`, `#chunk-2`) and is always allowed,
    /// including the generated TOC and pager links.
    pub fn allows(&self, url: &str) -> bool {
        let Some(scheme) = scheme_of(url) else {
            return true;
        };
        match self {
            UrlPolicy::Default => !DANGEROUS_SCHEMES.contains(&scheme.as_str()),
            UrlPolicy::Strict(list) => list.contains(&scheme),
        }
    }

    /// Is this the default tier? [`scrub_hrefs`] otherwise has nothing to enforce.
    pub fn is_default(&self) -> bool {
        matches!(self, UrlPolicy::Default)
    }

    /// A description for a diagnostic: refused schemes, or allowed under strict.
    pub fn describe(&self) -> String {
        match self {
            UrlPolicy::Default => format!("refusing {}", DANGEROUS_SCHEMES.join(", ")),
            UrlPolicy::Strict(list) => format!("allowing only {}", list.join(", ")),
        }
    }
}

/// The lower-cased scheme of `url`, or `None` if it is relative.
///
/// Mirrors two browser normalisations, since skipping them could be walked past:
/// TAB, LF and CR are removed from anywhere in the scheme, and leading C0
/// controls and space are skipped. No entity decoding is needed: `&` is escaped
/// to `&amp;` before the link pass, so `&#58;` never decodes to a colon.
pub fn scheme_of(url: &str) -> Option<String> {
    let b = url.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] <= 0x20 || b[i] == 0x7f) {
        i += 1;
    }
    if i == b.len() || !b[i].is_ascii_alphabetic() {
        return None;
    }
    let mut scheme = String::new();
    scheme.push(b[i] as char);
    i += 1;
    while i < b.len() {
        let c = b[i];
        if is_stripped_everywhere(c) {
            i += 1;
            continue;
        }
        if !(c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.') {
            break;
        }
        scheme.push(c as char);
        i += 1;
    }
    if i < b.len() && b[i] == b':' {
        Some(scheme.to_ascii_lowercase())
    } else {
        // No colon, so the `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` run is
        // part of a relative path, not a scheme: `docs/readme:2` is a file.
        None
    }
}

/// Bytes a browser removes from a URL wherever they appear.
fn is_stripped_everywhere(c: u8) -> bool {
    matches!(c, b'\t' | b'\n' | b'\r')
}

/// Remove every anchor whose `href` uses a scheme the policy refuses.
///
/// Returns `None` when nothing was removed, so safe documents stay byte-identical
/// to the reference. A refused anchor is *unwrapped*: the tags go, the words
/// stay; an unterminated anchor loses only its open tag. Refused schemes are
/// pushed to `dropped` in first-seen order.
pub fn scrub_hrefs(html: &str, policy: &UrlPolicy, dropped: &mut Vec<String>) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut out: Option<String> = None;
    // `scan` finds the next `<a` and may skip allowed anchors; `copied` tracks
    // how much of `html` is in `buf` and advances only when something is
    // removed. They must not move together, or an allowed anchor skipped earlier
    // loses its open tag when a later refusal copies the gap.
    let mut scan = 0;
    let mut copied = 0;
    while let Some(rel) = lower[scan..].find("<a") {
        let open = scan + rel;
        // Require a tag boundary after `<a`, or `<abbr`/`<area`/`</a` match too.
        let after = lower.as_bytes().get(open + 2).copied();
        let is_anchor = matches!(after, Some(b' ') | Some(b'>') | Some(b'\t') | Some(b'\n'));
        if !is_anchor {
            scan = open + 2;
            continue;
        }
        let tag_end = match html[open..].find('>') {
            Some(p) => open + p,
            None => break,
        };
        let href = match find_href(&html[open..tag_end]) {
            Some((value, _)) => value,
            None => {
                scan = tag_end + 1;
                continue;
            }
        };
        let refused = match scheme_of(href) {
            None => None,
            Some(_) if policy.allows(href) => None,
            Some(s) => Some(s),
        };
        let Some(scheme) = refused else {
            scan = tag_end + 1;
            continue;
        };
        if !dropped.contains(&scheme) {
            dropped.push(scheme.clone());
        }
        let buf = out.get_or_insert_with(|| String::with_capacity(html.len()));
        buf.push_str(&html[copied..open]);
        // Unwrap: drop the tags, keep the words; deleting loses text.
        let inner_start = tag_end + 1;
        match find_close_a(&lower[inner_start..]) {
            // `start` is the offset of `</a`, `end` one past its `>`.
            Some((start, end)) => {
                buf.push_str(&html[inner_start..inner_start + start]);
                copied = inner_start + end;
            }
            // Unterminated: emit the rest as ordinary text and end the scan.
            None => {
                buf.push_str(&html[inner_start..]);
                copied = html.len();
            }
        }
        scan = copied;
    }
    let mut buf = out?;
    buf.push_str(&html[copied..]);
    Some(buf)
}

/// The `href` value inside one open tag, as `(value, value_end)`.
fn find_href(tag: &str) -> Option<(&str, usize)> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("href") {
        let at = from + rel;
        // `xhref=` is not `href`, and neither is a value that merely ends in it.
        let before_ok = at == 0 || !is_attr_char(lower.as_bytes()[at - 1]);
        from = at + 4;
        if !before_ok {
            continue;
        }
        let b = lower.as_bytes();
        let mut i = at + 4;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            continue;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = match b.get(i) {
            Some(&c @ (b'"' | b'\'')) => c,
            _ => continue,
        };
        i += 1;
        let value_start = i;
        let value_end = tag[i..].find(quote as char)? + value_start;
        return Some((&tag[value_start..value_end], value_end));
    }
    None
}

/// Byte that cannot end an attribute name.
fn is_attr_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':' | b'.')
}

/// The `</a>` that closes an anchor, as `(offset, end)`, allowing `</a >`.
fn find_close_a(lower_tail: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(rel) = lower_tail[from..].find("</a") {
        let at = from + rel;
        let b = lower_tail.as_bytes();
        let mut i = at + 3;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'>' {
            return Some((at, i + 1));
        }
        from = at + 3;
    }
    None
}

/// Outbound web schemes that warrant `rel="nofollow noreferrer"`.
///
/// A referrer is only sent on a network navigation and `nofollow` is a
/// hyperlink hint, so a fragment, a relative path and `mailto:`/`news:` are
/// left alone.
fn is_external(href: &str) -> bool {
    matches!(
        scheme_of(href).as_deref(),
        Some("http" | "https" | "ftp" | "ftps")
    )
}

/// Whether an already lower-cased open tag carries the attribute `name`.
fn has_attr(tag_lower: &str, name: &str) -> bool {
    let b = tag_lower.as_bytes();
    let mut from = 0;
    while let Some(rel) = tag_lower[from..].find(name) {
        let at = from + rel;
        let before_ok = at == 0 || !is_attr_char(b[at - 1]);
        let after_ok = !matches!(b.get(at + name.len()), Some(&c) if is_attr_char(c));
        if before_ok && after_ok {
            return true;
        }
        from = at + name.len();
    }
    false
}

/// Add `rel="nofollow noreferrer"` to every anchor whose `href` is an external
/// web link (see [`is_external`]).
///
/// This is the privacy counterpart to [`scrub_hrefs`]: it runs over the same
/// finished markup, so it also covers `-h->` raw anchors from a link dictionary.
/// Returns `None` when nothing changed, so the default output stays
/// byte-identical to the reference. An anchor that already carries a `rel` is
/// left as the operator wrote it, and the tag case follows `lower_case_tags`.
pub fn add_link_rel(html: &str, lower_case_tags: bool) -> Option<String> {
    let attr = if lower_case_tags {
        " rel=\"nofollow noreferrer\""
    } else {
        " REL=\"nofollow noreferrer\""
    };
    let lower = html.to_ascii_lowercase();
    let mut out: Option<String> = None;
    let mut scan = 0;
    let mut copied = 0;
    while let Some(rel) = lower[scan..].find("<a") {
        let open = scan + rel;
        let after = lower.as_bytes().get(open + 2).copied();
        if !matches!(after, Some(b' ') | Some(b'>') | Some(b'\t') | Some(b'\n')) {
            scan = open + 2;
            continue;
        }
        let tag_end = match html[open..].find('>') {
            Some(p) => open + p,
            None => break,
        };
        let Some((value, value_end)) = find_href(&html[open..tag_end]) else {
            scan = tag_end + 1;
            continue;
        };
        if !is_external(value) || has_attr(&lower[open..tag_end], "rel") {
            scan = tag_end + 1;
            continue;
        }
        // Insert right after the closing quote of the href value, so the new
        // attribute cannot land inside the value.
        let insert = open + value_end + 1;
        let buf = out.get_or_insert_with(|| String::with_capacity(html.len() + 24));
        buf.push_str(&html[copied..insert]);
        buf.push_str(attr);
        copied = insert;
        scan = tag_end + 1;
    }
    let mut buf = out?;
    buf.push_str(&html[copied..]);
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> UrlPolicy {
        UrlPolicy::default()
    }

    fn scrub(html: &str) -> (Option<String>, Vec<String>) {
        let mut dropped = Vec::new();
        let out = scrub_hrefs(html, &policy(), &mut dropped);
        (out, dropped)
    }

    #[test]
    fn finds_a_scheme() {
        assert_eq!(scheme_of("http://x/").as_deref(), Some("http"));
        assert_eq!(scheme_of("HTTPS://x/").as_deref(), Some("https"));
        assert_eq!(scheme_of("mailto:a@b").as_deref(), Some("mailto"));
        assert_eq!(scheme_of("news:comp.lang.perl").as_deref(), Some("news"));
    }

    #[test]
    fn a_relative_url_has_no_scheme() {
        for u in [
            "#chunk-2",
            "/abs/path",
            "other.html",
            "docs/readme:2",
            "",
            "//host/path",
            "1abc:no",
        ] {
            assert_eq!(scheme_of(u), None, "{u:?} should be relative");
        }
    }

    #[test]
    fn tab_and_newline_cannot_hide_a_scheme() {
        // A browser resolves these to `javascript:`, so the check must too.
        assert_eq!(
            scheme_of("java\tscript:alert(1)").as_deref(),
            Some("javascript")
        );
        assert_eq!(
            scheme_of("java\nscript:alert(1)").as_deref(),
            Some("javascript")
        );
        assert_eq!(
            scheme_of("java\rscript:alert(1)").as_deref(),
            Some("javascript")
        );
        assert_eq!(scheme_of("  javascript:x").as_deref(), Some("javascript"));
        assert_eq!(
            scheme_of("\u{0}javascript:x").as_deref(),
            Some("javascript")
        );
    }

    #[test]
    fn an_entity_encoded_colon_is_not_a_scheme() {
        // `&` is escaped before the link pass, so `&amp;#58;` stays literal text.
        assert_eq!(scheme_of("javascript&amp;#58;alert(1)"), None);
    }

    #[test]
    fn default_policy_refuses_script_schemes() {
        let p = policy();
        for u in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "file:///etc/passwd",
        ] {
            assert!(!p.allows(u), "{u} should be refused");
        }
        for u in [
            "http://example.com/",
            "https://example.com/",
            "ftp://h/f",
            "mailto:a@b",
            "news:comp.lang.perl",
            // An unknown scheme does not execute; refusing it would break
            // legitimate internal links.
            "xyz://example.com",
            "#chunk-1",
            "relative.html",
        ] {
            assert!(p.allows(u), "{u} should be allowed");
        }
    }

    #[test]
    fn an_operator_can_widen_the_policy() {
        let p = UrlPolicy::strict(["http", " file "]);
        assert!(p.allows("file:///etc/passwd"));
        assert!(p.allows("http://x/"));
        assert!(!p.allows("javascript:x"));
        assert_eq!(p, UrlPolicy::Strict(vec!["file".into(), "http".into()]));
    }

    #[test]
    fn a_strict_policy_is_the_allowlist() {
        // The strong form: nothing reaches the output that is not on the list.
        let p = UrlPolicy::strict(["http", "https"]);
        assert!(p.allows("http://x/"));
        assert!(p.allows("https://x/"));
        for u in ["ftp://h/f", "mailto:a@b", "xyz://x", "file:///etc/passwd"] {
            assert!(!p.allows(u), "{u} should be refused by a strict policy");
        }
    }

    #[test]
    fn an_empty_list_means_the_default_tier() {
        // `''` means "no opinion" (the default), not "refuse all" or "allow all".
        let p = UrlPolicy::strict([""]);
        assert!(p.is_default());
        assert!(!p.allows("javascript:x"));
        assert!(p.allows("xyz://x"));
    }

    #[test]
    fn scrubs_a_javascript_anchor_and_keeps_the_text() {
        let (out, dropped) = scrub("<p>Click <A HREF=\"javascript:alert(1)\">here</A> now</p>");
        assert_eq!(out.unwrap(), "<p>Click here now</p>");
        assert_eq!(dropped, vec!["javascript".to_string()]);
    }

    #[test]
    fn scrubs_lower_case_and_single_quoted_and_spaced_spellings() {
        for html in [
            "<a href='javascript:x'>t</a>",
            "<a  href = \"javascript:x\" >t</a>",
            "<a class=\"c\" href=\"data:text/html,x\">t</a>",
            "<a href=\"vbscript:x\">t</a>",
        ] {
            let (out, _) = scrub(html);
            assert_eq!(out.unwrap(), "t", "{html}");
        }
    }

    #[test]
    fn an_unterminated_anchor_loses_only_its_open_tag() {
        let (out, _) = scrub("<a href=\"javascript:x\">visible words");
        assert_eq!(out.unwrap(), "visible words");
    }

    #[test]
    fn each_scheme_is_reported_once() {
        let html = "<a href=\"javascript:a\">1</a><a href=\"javascript:b\">2</a>\
                    <a href=\"data:x\">3</a>";
        let (out, dropped) = scrub(html);
        assert_eq!(out.unwrap(), "123");
        assert_eq!(dropped, vec!["javascript".to_string(), "data".to_string()]);
    }

    #[test]
    fn allowed_anchors_are_left_byte_identical() {
        for html in [
            "<a href=\"#chunk-1\">One</a>",
            "<a href=\"http://e.com/\">e</a>",
            "<a href=\"mailto:a@b\">m</a>",
            "<a name=\"x\">no href</a>",
            "<abbr title=\"t\">not an anchor</abbr>",
            "<area href=\"javascript:x\">",
            "plain text, no markup at all",
            "",
        ] {
            assert_eq!(scrub(html).0, None, "{html} should be untouched");
        }
    }

    #[test]
    fn a_relative_href_survives_alongside_a_refused_one() {
        let (out, _) = scrub("<a href=\"#a\">keep</a><a href=\"javascript:x\">drop</a>");
        assert_eq!(out.unwrap(), "<a href=\"#a\">keep</a>drop");
    }

    #[test]
    fn an_xhref_attribute_is_not_an_href() {
        // `xhref` is a different attribute: it must not be read as the href...
        assert_eq!(scrub("<a xhref=\"javascript:x\">t</a>").0, None);
        // ...nor, when a real href is present, take precedence over it.
        assert_eq!(
            scrub("<a xhref=\"javascript:x\" href=\"#ok\">t</a>").0,
            None
        );
        // The real attribute is the one that decides, in either position.
        let (out, dropped) = scrub("<a href=\"javascript:x\" xhref=\"#ok\">t</a>");
        assert_eq!(out.unwrap(), "t");
        assert_eq!(dropped, vec!["javascript".to_string()]);
    }

    #[test]
    fn several_refused_anchors_in_one_pass() {
        let html = "<p><a href=\"javascript:1\">a</a> mid <a href=\"data:2\">b</a> end</p>";
        let (out, dropped) = scrub(html);
        assert_eq!(out.unwrap(), "<p>a mid b end</p>");
        assert_eq!(dropped.len(), 2);
    }

    fn rel(html: &str) -> Option<String> {
        add_link_rel(html, true)
    }

    #[test]
    fn external_links_get_rel_and_internal_ones_do_not() {
        let html = "<a href=\"http://e.com/x\">a</a> \
                    <a href=\"https://e.com/\">b</a> \
                    <a href=\"#frag\">c</a> \
                    <a href=\"rel.html\">d</a> \
                    <a href=\"mailto:a@b\">e</a>";
        let out = rel(html).unwrap();
        assert!(out.contains("<a href=\"http://e.com/x\" rel=\"nofollow noreferrer\">a</a>"));
        assert!(out.contains("<a href=\"https://e.com/\" rel=\"nofollow noreferrer\">b</a>"));
        assert!(out.contains("<a href=\"#frag\">c</a>"));
        assert!(out.contains("<a href=\"rel.html\">d</a>"));
        assert!(out.contains("<a href=\"mailto:a@b\">e</a>"));
        assert_eq!(out.matches("rel=").count(), 2);
    }

    #[test]
    fn nothing_external_is_left_byte_identical() {
        for html in [
            "plain text",
            "",
            "<a href=\"#chunk-1\">One</a>",
            "<a href=\"mailto:a@b\">m</a>",
            "<a href=\"news:comp.lang.perl\">n</a>",
            "<a name=\"x\">anchor, no href</a>",
            "<abbr title=\"t\">not an anchor</abbr>",
        ] {
            assert_eq!(rel(html), None, "{html}");
        }
    }

    #[test]
    fn the_rel_is_added_once_and_the_case_follows_the_tag() {
        let once = rel("<a href=\"http://e.com/\">e</a>").unwrap();
        assert_eq!(
            once,
            "<a href=\"http://e.com/\" rel=\"nofollow noreferrer\">e</a>"
        );
        assert_eq!(rel(&once), None, "already tagged");
        assert_eq!(once.matches("rel=").count(), 1);

        assert_eq!(
            add_link_rel("<A HREF=\"http://e.com/\">e</A>", false).unwrap(),
            "<A HREF=\"http://e.com/\" REL=\"nofollow noreferrer\">e</A>"
        );
    }

    #[test]
    fn an_anchors_own_rel_is_preserved() {
        assert_eq!(rel("<a href=\"http://e.com/\" rel=\"me\">e</a>"), None);
        assert_eq!(
            rel("<a class=\"c\" href=\"http://e.com/\">e</a>").unwrap(),
            "<a class=\"c\" href=\"http://e.com/\" rel=\"nofollow noreferrer\">e</a>"
        );
    }

    #[test]
    fn ftp_is_external_but_a_lookalike_scheme_is_not() {
        assert!(rel("<a href=\"ftp://h/f\">f</a>").is_some());
        assert_eq!(rel("<a href=\"httpx://h/f\">t</a>"), None);
    }
}
