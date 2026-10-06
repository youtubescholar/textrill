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
//! The engine's output is an HTML *document*, usually published somewhere, and
//! one of its inputs is not under the converter's control: the text being
//! converted. A document can name a URL of its own choosing in the output, and
//! Perl's `HTML::TextToHTML` never looked at what that URL was, so
//! `<URL:javascript:alert(1)>` in an otherwise ordinary file came out of the
//! reference as a live `javascript:` link. This module is the one place that
//! decides, so the property is "no executing scheme reaches the output" rather
//! than a rule each producer has to remember.
//!
//! Three properties matter, and they are why this is a scan over finished markup
//! rather than a check at each construction site:
//!
//! * **Total.** Every producer of an `href` is covered, including one added
//!   later, and including raw HTML a dictionary injects with `-h->`.
//! * **Byte-identical when nothing is refused.** [`scrub_hrefs`] returns `None`
//!   if it changed nothing, so the reference goldens do not move for any input
//!   that does not actually contain a dangerous scheme.
//! * **Loses no text.** A refused anchor is unwrapped, not deleted: the visible
//!   words survive. Refusing to *convert* untrusted input would let a hostile
//!   document deny service to the converter, which is the wrong direction for
//!   the fail-safe to point. Hard failure is still right for `--style_url`,
//!   which is operator input -- see
//!   [`Options::validate`](crate::options::Options::validate).
//!
//! # Two tiers, and why not just an allowlist
//!
//! An allowlist is the stronger guarantee and the obvious design, and
//! `--allowed_url_schemes` gives exactly one. It is not the *default*, because
//! an allowlist has to enumerate every scheme a legitimate document might
//! mention, and getting that list wrong breaks real input in a way that is hard
//! to diagnose. Upstream's own CI fixture is the worked example: its dictionary
//! contains one rule, `|xyz:[\w/\.:+\-]+| -> $&`, which links `xyz://…`. No
//! allowlist short of "include `xyz`" survives that, and `xyz` is not a scheme
//! anyone could have predicted.
//!
//! So the default is the smaller question, which is also the one that actually
//! matters: which schemes *do something*. A browser asked to navigate to
//! `xyz://example.com` does nothing, shows nothing, and executes nothing. A
//! browser asked to navigate to `javascript:…` runs it in the page's origin.
//! [`DANGEROUS_SCHEMES`] is that set, and it is short.
//!
//! The honest cost: a scheme nobody thought of would get through. That is the
//! price of not breaking `xyz://`, and the mitigation is the strict mode rather
//! than a longer list -- `--allowed_url_schemes https` refuses everything the
//! default allows, including anything added to a browser after this was
//! written.
//!
//! Operator-authored dictionaries get a louder treatment than document text: a
//! rule with a statically-known bad URL is refused at load with a diagnostic,
//! because the operator can fix it. See [`Links`](crate::links).

/// Schemes refused by default, whatever else is allowed.
///
/// The script-bearing schemes, which execute in the page's origin, plus `file`,
/// which lets a converted document point at the reader's filesystem. `data:`
/// is here for the same reason as `javascript:`: a `data:text/html` href
/// navigates to attacker-authored HTML.
///
/// This is a denylist, and [`UrlPolicy::strict`] is the allowlist; see the
/// module docs for why that is the default split.
pub const DANGEROUS_SCHEMES: &[&str] = &["javascript", "data", "vbscript", "file"];

/// Schemes the built-in dictionary links, kept for `--help` and for the
/// operator who wants to write a strict list and start from this one.
pub const WELL_KNOWN_SCHEMES: &[&str] = &[
    "ftp", "ftps", "gopher", "http", "https", "mailto", "news", "nntp", "telnet", "wais",
];

/// Which schemes a generated `href` may use.
///
/// Schemes are compared ASCII-case-insensitively, because a browser does:
/// `JaVaScRiPt:` is the same scheme as `javascript:`. Only ASCII case folding is
/// wanted here -- a full Unicode fold would need the whole scheme table to be
/// case-closed, which no browser assumes either.
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
    /// A policy allowing exactly `schemes`.
    ///
    /// An empty or all-whitespace list is [`UrlPolicy::Default`] rather than
    /// "allow nothing": a caller cannot express "refuse every scheme" by
    /// accident, and `--allowed_url_schemes ''` means "no opinion", which is
    /// what an unset option must mean for the round trip through a front end's
    /// settings file to be harmless.
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

    /// May `url` be used as an `href`?
    ///
    /// A URL with no scheme is a *relative* reference -- `page.html`, `/x`,
    /// `#chunk-2` -- and is always allowed: it names something inside the
    /// document set the converter was asked to produce, which is what the
    /// generated TOC and pager links are, and it is what
    /// `<URL:foo:label>` produces when the label has no colon.
    pub fn allows(&self, url: &str) -> bool {
        let Some(scheme) = scheme_of(url) else {
            return true;
        };
        match self {
            UrlPolicy::Default => !DANGEROUS_SCHEMES.contains(&scheme.as_str()),
            UrlPolicy::Strict(list) => list.contains(&scheme),
        }
    }

    /// Is this the default tier? [`scrub_hrefs`] returns early when not, because
    /// with no list to compare against there is nothing to enforce.
    pub fn is_default(&self) -> bool {
        matches!(self, UrlPolicy::Default)
    }

    /// A description for a diagnostic: the refused schemes under the default
    /// tier, or the allowed ones under a strict one.
    pub fn describe(&self) -> String {
        match self {
            UrlPolicy::Default => format!("refusing {}", DANGEROUS_SCHEMES.join(", ")),
            UrlPolicy::Strict(list) => format!("allowing only {}", list.join(", ")),
        }
    }
}

/// The lower-cased scheme of `url`, or `None` if it is relative.
///
/// Two normalisations, both because a browser performs them before it looks at
/// the scheme, and a check that skipped them could be walked past:
///
/// * **TAB, LF and CR are removed from anywhere in the scheme.** A browser
///   strips them from the whole URL, so `java&#9;script:` and a literal tab
///   both arrive as `javascript:`.
/// * **Leading C0 control characters and space are skipped.** `java\nscript:`
///   is the same attack as a tab.
///
/// No entity decoding happens, and none is needed. Body text has `&` escaped to
/// `&amp;` before the link pass sees it, so a document writing `&#58;` puts the
/// six literal characters `&amp;#58;` in the attribute; the browser's single
/// decode pass yields the text `&#58;`, not a colon. Confirming that cost one
/// release, and the escaping is in `chars::escape`.
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
/// Returns `None` when there was nothing to remove, which is the case for every
/// document that does not actually contain a dangerous scheme -- so the output
/// is byte-identical to the reference and no allocation happens.
///
/// A refused anchor is *unwrapped*: the `<a …>` and its `</a>` go, the words
/// between them stay. An unterminated anchor (an open tag with no `</a>`, which a
/// dictionary's `-h->` HTML can produce) loses only its open tag.
///
/// Schemes are collected into `dropped`, in first-seen order, so the caller can
/// report each one once rather than once per match.
///
/// There is no early exit for the default tier: the default tier has a denylist
/// to enforce, so it has to walk the markup like any other. The "no work done"
/// property is the `None` return below, not a shortcut.
pub fn scrub_hrefs(html: &str, policy: &UrlPolicy, dropped: &mut Vec<String>) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut out: Option<String> = None;
    // Two cursors, because they move differently. `scan` is where the next
    // `<a` is looked for and advances past every anchor it declines to touch;
    // `copied` is how much of `html` is already in `buf` and only ever moves
    // when something is actually removed. Advancing `scan` must not drag
    // `copied` forward, or an allowed anchor skipped earlier would lose the
    // bytes of its own open tag when a later refusal copies the gap.
    let mut scan = 0;
    let mut copied = 0;
    while let Some(rel) = lower[scan..].find("<a") {
        let open = scan + rel;
        // `<abbr`, `<area` and `<article` all start `<a`; require the tag to end
        // there so they are not mistaken for an anchor. A `</a` is not a start
        // tag either.
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
        // Unwrap rather than delete: the open tag goes, the words between the
        // tags stay, and the close tag goes. Dropping the whole element would
        // lose text, which is the one outcome not allowed here.
        let inner_start = tag_end + 1;
        match find_close_a(&lower[inner_start..]) {
            // `start` is the offset of `</a`, `end` one past its `>`.
            Some((start, end)) => {
                buf.push_str(&html[inner_start..inner_start + start]);
                copied = inner_start + end;
            }
            // Unterminated: the open tag goes and the rest of the string is
            // ordinary text, so it is emitted once here and the scan is over.
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

/// The `href` value inside one open tag, as `(value, value_end)`, with the
/// attribute name matched case-insensitively and `=` and the quote allowed
/// whitespace around them -- the spellings a dictionary's raw HTML can produce.
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

/// The `</a>` that closes an anchor, as `(offset, end)`, allowing whitespace
/// and attributes-free spelling variants (`</a >`).
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
        // What a browser resolves these to is `javascript:`, so this is what the
        // check has to resolve them to.
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
        // `&` is escaped before the link pass, so the attribute holds the six
        // characters `&amp;#58;` and the browser's one decode pass leaves text.
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
            // An unknown scheme is not an executing one, and refusing it would
            // break legitimate links to internal applications -- see the
            // module docs and upstream's `xyz://` CI fixture.
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
        // `--allowed_url_schemes ''` must mean "no opinion", not "refuse
        // everything" (which would break every document) and not "allow
        // everything" (which would be a silently-open default). It has to mean
        // the default, so that an unset option round-tripping through a front
        // end's settings file lands somewhere safe.
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
}
