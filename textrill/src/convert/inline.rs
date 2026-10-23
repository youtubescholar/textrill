use super::stream::ReplAll;
use super::*;

impl Converter {
    pub(super) fn iscaps(&mut self, line: &str) -> bool {
        let min = self.opts.min_caps_length;
        let pat = format!(r"^[^a-z<]*[A-Z]{{{min},}}[^a-z<]*$");
        self.re(&pat).is_match(line).unwrap_or(false)
    }

    pub(super) fn caps(&mut self, line_ref: &mut String, action: &mut u32) {
        if !self.opts.caps_tag.is_empty() && self.iscaps(line_ref) {
            let tag = self.opts.caps_tag.clone();
            self.tagline(&tag, line_ref);
            *action |= CAPS;
        }
    }

    // ------------------------------------------------------- inline markup

    pub(super) fn do_delim(
        &mut self,
        line_ref: &mut String,
        _action: &mut u32,
        delim: &str,
        tag: &str,
    ) {
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        if delim == "#" {
            // `#x#` where `x` is a single letter and the surrounding positions
            // are not word boundaries (the `\B` assertions). Pattern kept free
            // of `\B`/lookaround so fancy-regex stays on its linear path;
            // the boundary condition is applied in code instead.
            let re1 = self.re(r"#([A-Za-z])#").clone();
            self.delim_replace(line_ref, &re1, non_boundary('#'), ltag);
            // special treatment of # for the #num case and the #link case
            if line_ref.contains('#') {
                let re2 = self.re(r"#([^\d#][^#]*[^# \t\n])#").clone();
                if !line_ref.contains("<a") && !line_ref.contains("<A") {
                    self.delim_replace(line_ref, &re2, no_list_or_para_tag, ltag);
                } else {
                    *line_ref = self.delim_loop(line_ref, &re2, no_list_or_para_tag, tag);
                }
            }
        } else if delim == "^" {
            let re1 = self.re(r#"\^((?![^^]*(?:<li>|<LI>|<p>|<P>))(\w|["'<>])[^^]*)\^"#);
            if line_ref.contains('^') {
                *line_ref =
                    re1.replace_all_captures(line_ref, |c| ltag(c.get(1).unwrap().as_str()));
            }
            let re2 = self.re(r"\^([A-Za-z])\^").clone();
            self.delim_replace(line_ref, &re2, non_boundary('^'), ltag);
        } else if delim == "_" {
            let re1 = self.re(r"_([A-Za-z])_").clone();
            let r1m = self.delim_replace(line_ref, &re1, non_boundary('_'), ltag);
            if r1m {
                let re2 = self.re(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#).clone();
                self.delim_replace(line_ref, &re2, not_preceded_by_word_or_underscore, ltag);
            } else if line_ref.contains('_') {
                let re2 = self.re(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#).clone();
                *line_ref =
                    self.delim_loop(line_ref, &re2, not_preceded_by_word_or_underscore, tag);
            }
        } else if delim.chars().count() == 1 {
            let db = class_body(delim);
            let dch = delim.chars().next().unwrap();
            let re1 = self.re(&format!(r"[{db}]([A-Za-z])[{db}]")).clone();
            self.delim_replace(line_ref, &re1, non_boundary(dch), ltag);
            // `delim ... delim` where the content is one or more non-delimiter
            // characters ending in a "word/punctuation" character. The Perl
            // pattern's leading `(?<!delim)` is applied in code (`accept`)
            // so the regex itself stays free of lookaround (linear).
            let re2 = self
                .re(&format!(
                    r"[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
                ))
                .clone();
            self.delim_replace(line_ref, &re2, not_preceded_by(dch), ltag);
        } else {
            // Perl interpolates `${delim}` straight into these patterns, so a
            // metacharacter delimiter (e.g. `**`) makes Perl raise "Quantifier
            // follows nothing" and drop the substitution; escaping makes the
            // pattern mean what it obviously meant. The `(?<!...)` on the first
            // one is a fixed-width literal, so it moves into `accept` too: left in
            // the regex it is a lookbehind, which a multi-character delimiter run
            // put through the backtracking VM for tens of seconds.
            let d = fancy_regex::escape(delim);
            let re1 = self
                .re(&format!(
                    r#"{d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
                ))
                .clone();
            if line_ref.contains(delim) {
                self.delim_replace(line_ref, &re1, not_preceded_by_str(d.to_string()), ltag);
            }
            let re2 = self.re(&format!(r"{d}\]([A-Za-z]){d}"));
            *line_ref = re2.replace_all_captures(line_ref, |c| ltag(c.get(1).unwrap().as_str()));
        }
    }

    /// Perl-style `s///g` for a *linear* regex (no lookaround, `\B` or backrefs,
    /// so the regex crate stays on its non-backtracking path).
    ///
    /// `captures_from_pos` locates each candidate; `accept` applies whatever
    /// condition the original expressed with a lookbehind/`\B`. Rejected
    /// candidates are kept verbatim and scanning continues just past them.
    ///
    /// Returns whether anything was replaced.
    pub(super) fn delim_replace(
        &self,
        line_ref: &mut String,
        re: &Regex,
        accept: impl Fn(&str, usize, usize) -> bool,
        replace: impl Fn(&str) -> String,
    ) -> bool {
        let text = std::mem::take(line_ref);
        let mut out = String::with_capacity(text.len());
        // `last` is how much of `text` was appended to `out`; `pos` is where the
        // next candidate is searched. Rejected candidates stay in place so their
        // bytes are flushed by a later accept or the final tail.
        let mut last = 0;
        let mut pos = 0;
        let mut changed = false;
        loop {
            match re.captures_from_pos(&text, pos).ok().flatten() {
                None => break,
                Some(caps) => {
                    let m = match caps.get(0) {
                        Some(m) => m,
                        None => break,
                    };
                    if accept(&text, m.start(), m.end()) {
                        out.push_str(&text[last..m.start()]);
                        out.push_str(&replace(caps.get(1).unwrap().as_str()));
                        last = m.end();
                        pos = m.end();
                        changed = true;
                    } else {
                        pos = m.start() + char_len(&text, m.start());
                    }
                }
            }
        }
        out.push_str(&text[last..]);
        *line_ref = out;
        changed
    }

    /// As `delim_replace`, but for a line that contains generated markup:
    /// the replacement is skipped inside link contexts, which is what the
    /// `contains("<a")` split in `do_delim` selects. Left to right, always
    /// consuming at least one character so the loop terminates.
    pub(super) fn delim_loop(
        &mut self,
        line_ref: &mut String,
        re: &Regex,
        accept: impl Fn(&str, usize, usize) -> bool,
        tag: &str,
    ) -> String {
        let mut line_with_links = String::new();
        loop {
            let cur = line_ref.clone();
            match re.captures(&cur).ok().flatten() {
                None => break,
                Some(caps) => {
                    let m = caps.get(0).unwrap();
                    if !accept(&cur, m.start(), m.end()) {
                        // A rejected candidate is not consumed: the original
                        // regex retries one character along, so a candidate
                        // starting *inside* this one can still match. The skipped
                        // character is emitted verbatim.
                        let keep = m.start() + char_len(&cur, m.start());
                        line_with_links.push_str(&cur[..keep]);
                        *line_ref = cur[keep..].to_string();
                        continue;
                    }
                    let pre = cur[..m.start()].to_string();
                    let mut linkme = cur[m.start()..m.end()].to_string();
                    let post = cur[m.end()..].to_string();
                    line_with_links.push_str(&pre);
                    if !self.links.in_link_context(&linkme, &line_with_links) {
                        let rebuilt = if let Some(c) = re.captures(&linkme).ok().flatten() {
                            if let Some(g) = c.get(1) {
                                format!("<{tag}>{}</{tag}>", g.as_str())
                            } else {
                                linkme.clone()
                            }
                        } else {
                            linkme.clone()
                        };
                        linkme = rebuilt;
                    }
                    line_with_links.push_str(&linkme);
                    *line_ref = post;
                }
            }
        }
        format!("{line_with_links}{line_ref}")
    }

    // ------------------------------------------------------- links + markup

    pub(super) fn apply_links(&mut self, para_ref: &mut String, para_action: &mut u32) {
        if self.opts.make_links && !self.links.rules.is_empty() {
            self.links.check_dictionary_links(para_ref);
            // The single point where document text and dictionary URLs become an
            // `href`, so it decides whether the scheme is one a browser will act
            // on. Placed here rather than at each construction site so `links.rs`
            // stays a faithful port and a future producer is covered. Runs over
            // the finished paragraph, so one scan per paragraph, not per match.
            let mut dropped = Vec::new();
            if let Some(scrubbed) =
                crate::urlscheme::scrub_hrefs(para_ref, &self.url_policy, &mut dropped)
            {
                *para_ref = scrubbed;
                for scheme in dropped {
                    if !self.dropped_schemes.contains(&scheme) {
                        eprintln!(
                            "textrill: dropped a link with the {scheme:?} URL scheme; \
                             the text is kept. Pass --allowed_url_schemes {scheme} to allow it."
                        );
                        self.dropped_schemes.push(scheme);
                    }
                }
            }
            // Private reading: after a link is known to be allowed, tag the
            // outbound web ones. The same finished-paragraph pass as the scrub,
            // so it also covers `-h->` raw anchors from the dictionary.
            if self.opts.no_referrer {
                if let Some(rel) =
                    crate::urlscheme::add_link_rel(para_ref, self.opts.lower_case_tags)
                {
                    *para_ref = rel;
                }
            }
        }
        let ls = self.opts.lower_case_tags;
        if !self.opts.bold_delimiter.is_empty() {
            let tag = if ls { "strong" } else { "STRONG" };
            let d = self.opts.bold_delimiter.clone();
            self.do_delim(para_ref, para_action, &d, tag);
        }
        if !self.opts.italic_delimiter.is_empty() {
            let tag = if ls { "em" } else { "EM" };
            let d = self.opts.italic_delimiter.clone();
            self.do_delim(para_ref, para_action, &d, tag);
        }
        if !self.opts.underline_delimiter.is_empty() {
            let tag = if ls { "u" } else { "U" };
            let d = self.opts.underline_delimiter.clone();
            self.do_delim(para_ref, para_action, &d, tag);
        }
        *para_action |= LINK;
    }
}

/// The condition the `\B` in `\B delim ([A-Za-z]) delim \B` places on the
/// flanking characters: both sides must agree on word-ness. One side is the
/// delimiter, so the neighbour must be a word character exactly when the
/// delimiter is (`_a_` stays literal, ` #a# ` is marked up). Neighbours are
/// checked byte-wise; the delimiter's word-ness comes from the character.
pub(super) fn non_boundary(delim: char) -> impl Fn(&str, usize, usize) -> bool {
    let d_word = delim_is_word(delim);
    move |t: &str, s: usize, e: usize| {
        let left_word = s > 0 && is_word_byte(t.as_bytes()[s - 1]);
        let right_word = e < t.len() && is_word_byte(t.as_bytes()[e]);
        left_word == d_word && right_word == d_word
    }
}

/// `\w` (word char) on a **byte**, as Perl's `\b` sees it: ASCII alphanumeric
/// or underscore. Every byte of a multi-byte character is `>= 0x80`, so not a
/// word byte.
pub(super) fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The same question about a delimiter, asked of the character: `d as u8`
/// truncates `é` to `0xE9`, which is not a byte of `é`'s own UTF-8 encoding.
pub(super) fn delim_is_word(delim: char) -> bool {
    delim.is_ascii() && is_word_byte(delim as u8)
}

/// `(?<![delim])`: the delimiter must not be the character immediately before
/// the match. Checked on the preceding *character*, since a byte-wise test with
/// `delim as u8` never rejects anything (`ééwordé` would be marked up).
pub(super) fn not_preceded_by(delim: char) -> impl Fn(&str, usize, usize) -> bool {
    move |t: &str, s: usize, _e: usize| !t[..s].ends_with(delim)
}

/// `(?<![_A-Za-z0-9])` from the underscore pattern.
pub(super) fn not_preceded_by_word_or_underscore(t: &str, s: usize, _e: usize) -> bool {
    s == 0 || !(t.as_bytes()[s - 1] == b'_' || is_word_byte(t.as_bytes()[s - 1]))
}

/// `(?<!delim)` from the multi-character pattern, where the delimiter is a
/// literal string and so may be more than one byte long.
pub(super) fn not_preceded_by_str(delim: String) -> impl Fn(&str, usize, usize) -> bool {
    let d = delim.into_bytes();
    move |t: &str, s: usize, _e: usize| -> bool {
        s < d.len() || t.as_bytes()[s - d.len()..s] != *d
    }
}

/// The bold pattern's `(?![^#]*(?:<li>|<LI>|<P>|<p>))`, applied in code. It only
/// inspects the match's own interior, since every group character is `[^#]`.
/// Byte-wise on purpose: slicing a `str` could panic when `s + 2` is not a
/// character boundary.
pub(super) fn no_list_or_para_tag(t: &str, s: usize, e: usize) -> bool {
    none_of(
        t.as_bytes(),
        s + 2,
        e - 1,
        &[b"<li>", b"<LI>", b"<P>", b"<p>"],
    )
}

/// UTF-8 length in bytes of the character starting at `byte_pos` in `text`.
pub(super) fn char_len(text: &str, byte_pos: usize) -> usize {
    match text[byte_pos..].chars().next() {
        Some(c) => c.len_utf8(),
        None => 1,
    }
}

/// Whether none of `needles` occurs in `hay[from..to]`. Byte-wise, so indices may
/// fall inside a multi-byte character without panicking; callers pass ASCII
/// needles.
pub(super) fn none_of(hay: &[u8], from: usize, to: usize, needles: &[&[u8]]) -> bool {
    let window = &hay[from..to];
    !needles
        .iter()
        .any(|n| window.windows(n.len()).any(|w| w == *n))
}
