//! Link dictionary engine: parses dictionaries and applies link rules.
//!
//! Port of the link half of HTML::TextToHTML v3.0. Covered by tests/linktest.rs.

use std::cell::RefCell;
use std::collections::HashMap;

use fancy_regex::Regex;

use crate::options::Options;

pub const LINK_NOCASE: u8 = 1;
pub const LINK_EVAL: u8 = 2;
pub const LINK_HTML: u8 = 4;
pub const LINK_ONCE: u8 = 8;
pub const LINK_SECT_ONCE: u8 = 16;

/// One compiled dictionary rule.
#[derive(Debug)]
pub struct LinkRule {
    pub label: String,
    pub switches: u8,
    pub pattern: String,
    pub regex: Regex,
    pub replacement: String,
    /// A literal every match of `regex` must contain, when one could be proven;
    /// `None` means no filter. Sound one direction only (see
    /// [`crate::prefilter`]): a wrong literal silently drops links.
    pub prefilter: Option<crate::prefilter::Alternatives>,
}

impl LinkRule {
    /// Could this rule match the already-lowercased `haystack_folded`?
    /// `false` guarantees `regex.captures` would return `None`, so the caller
    /// may skip it.
    pub fn may_match(&self, haystack_folded: &[u8]) -> bool {
        match &self.prefilter {
            None => true,
            Some(alts) => crate::prefilter::may_match_prefolded(haystack_folded, alts),
        }
    }
}

/// Expand `\s \S \w \W \d \D \b \B` to explicit ASCII classes so matches are
/// Perl-like without `/u`, and rewrite `$` to Perl's end-of-text-or-before-a-
/// trailing-newline meaning (regex-crate `$` is end-of-text only).
fn expand_ascii_escapes(pat: &str) -> String {
    let mut out = String::new();
    let mut chars = pat.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        if c == '[' {
            in_class = true;
            out.push('[');
            continue;
        }
        if c == ']' {
            in_class = false;
            out.push(']');
            continue;
        }
        if c == '$' {
            if in_class {
                out.push('$');
            } else {
                out.push_str(r"(?=\n?$)");
            }
            continue;
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        let next = match chars.peek() {
            Some(&n) => n,
            None => {
                out.push('\\');
                break;
            }
        };
        match next {
            's' => {
                chars.next();
                out.push_str("[ \\t\\r\\n\\x0c\\x0b]");
            }
            'S' => {
                chars.next();
                out.push_str("[^ \\t\\r\\n\\x0c\\x0b]");
            }
            'w' => {
                chars.next();
                out.push_str("[A-Za-z0-9_]");
            }
            'W' => {
                chars.next();
                out.push_str("[^A-Za-z0-9_]");
            }
            'd' => {
                chars.next();
                out.push_str("[0-9]");
            }
            'D' => {
                chars.next();
                out.push_str("[^0-9]");
            }
            'b' => {
                chars.next();
                out.push_str(
                    "(?:(?<=[A-Za-z0-9_])(?![A-Za-z0-9_])|(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]))",
                );
            }
            'B' => {
                chars.next();
                out.push_str(
                    "(?:(?<=[A-Za-z0-9_])(?=[A-Za-z0-9_])|(?<![A-Za-z0-9_])(?![A-Za-z0-9_]))",
                );
            }
            '>' => {
                // Perl: `\>` is a literal '>'; regex-crate treats it as an anchor.
                chars.next();
                out.push('>');
            }
            '<' => {
                // Same for `\<` (start-of-word anchor in regex-crate).
                chars.next();
                out.push('<');
            }
            _ => {
                chars.next();
                out.push('\\');
                out.push(next);
            }
        }
    }
    out
}

/// Compile a Perl-style pattern with the /s (dotall) flag, ASCII semantics and
/// optional /i, reporting a pattern that does not compile.
///
/// The single place a pattern becomes a `Regex`. Flags go on first, translation
/// second -- the order the engine's behaviour is defined by.
// The error type is upstream's; boxing it would change the signature for no gain.
#[allow(clippy::result_large_err)]
pub fn try_compile_pattern(pat: &str, nocase: bool) -> Result<Regex, fancy_regex::Error> {
    let full = if nocase {
        format!("(?s)(?i){pat}")
    } else {
        format!("(?s){pat}")
    };
    Regex::new(&translate_pattern(&full))
}

/// True if `pat` can match the empty string, which would make the substitution
/// loop in [`LinkParser::check_dictionary_links`] spin forever.
///
/// Tested by matching `""`, since [`translate_pattern`] rewrites `\b` to a
/// zero-width lookaround and the source text is not a reliable guide. Callers
/// apply this only to switch combinations that reach the loop; `-o` and `-s`
/// substitute once and terminate.
pub fn can_match_empty(pat: &str, nocase: bool) -> bool {
    match try_compile_pattern(pat, nocase) {
        Ok(re) => re.is_match("").unwrap_or(false),
        // Already reported by `add_regexp`; do not claim a second reason.
        Err(_) => false,
    }
}

/// [`try_compile_pattern`] for already-validated patterns. Panics on failure,
/// which is an internal bug.
pub fn compile_pattern(pat: &str, nocase: bool) -> Regex {
    try_compile_pattern(pat, nocase).expect("valid pattern")
}

/// Translate Perl POSIX classes and `\W\d`-style shortcuts to ASCII.
pub fn translate_pattern(pat: &str) -> String {
    let ascii = expand_ascii_escapes(pat);
    ascii
        .replace("[:alpha:]", "A-Za-z")
        .replace("[:alnum:]", "A-Za-z0-9")
        .replace("[:lower:]", "a-z")
        .replace("[:upper:]", "A-Z")
        .replace("[:punct:]", "!-/:-@\\[-`{-~")
        .replace("[:space:]", " \\t\\r\\n\\x0c\\x0b")
}

/// Compile a Perl-style pattern with ASCII semantics and Perl-style `$`.
pub fn ascii_re(pat: &str) -> Regex {
    Regex::new(&translate_pattern(pat)).expect("valid pattern")
}

/// Memoized `ascii_re`, for fixed patterns that sniff every paragraph (list
/// prefixes, table shapes, mail headers).
///
/// The `&'static str` parameter is load-bearing: this returns a leaked
/// `&'static Regex`, so a document-derived pattern is a compile error rather
/// than unbounded cache growth. Use [`ascii_re`] for dynamic patterns.
pub fn ascii_re_cached(pat: &'static str) -> &'static Regex {
    thread_local! {
        static CACHE: RefCell<HashMap<String, &'static Regex>> = RefCell::new(HashMap::new());
    }
    CACHE.with(|cache| {
        // Bounded so a caller using dynamic patterns cannot grow it forever.
        const MAX_CACHED: usize = 128;
        let mut cache = cache.borrow_mut();
        if let Some(re) = cache.get(pat) {
            return *re;
        }
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        let re: &'static Regex = Box::leak(Box::new(
            Regex::new(&translate_pattern(pat)).expect("valid pattern"),
        ));
        cache.insert(pat.to_string(), re);
        re
    })
}

/// Expand a replacement template containing `$0`, `$1`, ... and `$&`.
fn expand_template(template: &str, caps: &fancy_regex::Captures, whole: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() && chars[i + 1] == '$' {
            out.push('$');
            i += 2;
            continue;
        }
        if c == '$' && i + 1 < chars.len() {
            let nxt = chars[i + 1];
            if nxt == '&' {
                out.push_str(whole);
                i += 2;
                continue;
            }
            if let Some(d) = nxt.to_digit(10) {
                if let Some(g) = caps.get(d as usize) {
                    out.push_str(g.as_str());
                }
                i += 2;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Lower-case the HTML tags in an h-switch replacement template.
pub fn lower_html_tags(template: &str) -> String {
    let mut s = template.to_string();
    let g = |c: &fancy_regex::Captures, i: usize| -> String {
        c.get(i).map(|m| m.as_str().to_string()).unwrap_or_default()
    };
    // closing tags: </X...>
    let re = Regex::new(r"(?i)(</)([A-Z]+)(>)").unwrap();
    s = re
        .replace_all(&s, |c: &fancy_regex::Captures<'_>| {
            format!("{}{}{}", g(c, 1), g(c, 2).to_ascii_lowercase(), g(c, 3))
        })
        .to_string();
    let re = Regex::new(r"(?i)(<)([A-Z]+)(>)").unwrap();
    s = re
        .replace_all(&s, |c: &fancy_regex::Captures<'_>| {
            format!("{}{}{}", g(c, 1), g(c, 2).to_ascii_lowercase(), g(c, 3))
        })
        .to_string();
    let re = Regex::new(r"(?i)(<)(A\s*HREF)([^>]*>)").unwrap();
    s = re
        .replace_all(&s, |c: &fancy_regex::Captures<'_>| {
            format!("{}{}{}", g(c, 1), g(c, 2).to_ascii_lowercase(), g(c, 3))
        })
        .to_string();
    s
}

/// Port of `glob2regexp`.
pub fn glob2regexp(glob: &str) -> String {
    let mut re = String::new();
    let mut prev_backslash = false;
    let mut regexp = String::new();
    let mut i = 0;
    let len = glob.chars().count();
    let v: Vec<char> = glob.chars().collect();
    // Escape funky chars
    let mut escaped_glob = String::new();
    for c in v.iter() {
        if !(c.is_ascii_alphanumeric()
            || *c == '_'
            || *c == '['
            || *c == ']'
            || *c == '*'
            || *c == '?'
            || *c == '|'
            || *c == '\\')
        {
            escaped_glob.push('\\');
        }
        escaped_glob.push(*c);
    }
    while i < len {
        let c = escaped_glob.chars().nth(i).unwrap();
        if prev_backslash {
            prev_backslash = false;
            regexp.push(c);
            i += 1;
            continue;
        }
        if c == '\\' {
            prev_backslash = true;
            i += 1;
            continue;
        }
        if c == '?' {
            regexp.push('.');
            i += 1;
            continue;
        }
        if c == '*' {
            regexp.push_str(".*");
            i += 1;
            continue;
        }
        regexp.push(c);
        i += 1;
    }
    re.push_str("\\b");
    re.push_str(&regexp);
    re.push_str("\\b");
    re
}

pub struct LinkParser {
    pub rules: Vec<LinkRule>,
    pub label_seen: std::collections::HashSet<String>,
    pub lower_case_tags: bool,
    pub once_done: Vec<bool>,
    pub sect_once_done: Vec<bool>,
    /// Dictionary patterns that did not compile and were skipped, so a front
    /// end can report them. See `add_regexp`.
    pub rejected_patterns: Vec<String>,
    /// Which schemes may reach an `href`. See [`crate::urlscheme`].
    pub policy: crate::urlscheme::UrlPolicy,
}

impl LinkParser {
    pub fn new(lower_case_tags: bool, policy: crate::urlscheme::UrlPolicy) -> Self {
        LinkParser {
            rules: Vec::new(),
            label_seen: std::collections::HashSet::new(),
            lower_case_tags,
            once_done: Vec::new(),
            sect_once_done: Vec::new(),
            rejected_patterns: Vec::new(),
            policy,
        }
    }

    fn add_rule(&mut self, label: &str, pattern: &str, url: &str, switches: u8) {
        if self.label_seen.contains(label) {
            return;
        }
        self.label_seen.insert(label.to_string());

        // A rule whose URL is written out in full is checked here, where the
        // operator can still fix it. URLs with capture refs (`$1`, `$&`) and
        // `-h->` raw HTML are only known at substitution time; the scrub over
        // the finished paragraph catches both.
        if switches & LINK_HTML == 0 && !url.contains('$') && !self.policy.allows(url) {
            let scheme = crate::urlscheme::scheme_of(url).unwrap_or_default();
            let msg = format!(
                "textrill: ignoring link-dictionary rule {label:?}: its URL uses the \
                 {scheme:?} scheme, which this conversion refuses ({}). \
                 Name it in --allowed_url_schemes to keep the rule.",
                self.policy.describe()
            );
            eprintln!("{msg}");
            self.rejected_patterns.push(msg);
            return;
        }

        let mut repl;
        if switches & LINK_HTML == 0 {
            if self.lower_case_tags {
                repl = format!("<a href=\"{url}\">$&</a>");
            } else {
                repl = format!("<A HREF=\"{url}\">$&</A>");
            }
        } else {
            repl = url.to_string();
            if self.lower_case_tags {
                repl = lower_html_tags(&repl);
            }
        }
        let regex = compile_pattern(pattern, switches & LINK_NOCASE != 0);
        // Prefer the translated pattern (what actually runs), falling back to
        // the original when translation made it unparsable: translation turns
        // `\b`/`\B`/`$` into look-around, which `regex-syntax` rejects.
        let prefilter = crate::prefilter::required_literal(regex.as_str())
            .or_else(|| crate::prefilter::required_literal(pattern));
        self.rules.push(LinkRule {
            label: label.to_string(),
            switches,
            pattern: pattern.to_string(),
            regex,
            replacement: repl,
            prefilter,
        });
        self.once_done.push(false);
        self.sect_once_done.push(false);
    }

    fn add_regexp(&mut self, label: &str, pattern: &str, url: &str, switches: u8) {
        // A `/pattern/` entry reaches the regex engine verbatim, so it is the
        // one dictionary form that can fail to compile; report and skip. The
        // option-level patterns are validated up front in `Options::validate`.
        if let Err(e) = try_compile_pattern(pattern, switches & LINK_NOCASE != 0) {
            let msg = format!("textrill: ignoring link-dictionary pattern {pattern:?}: {e}");
            eprintln!("{msg}");
            self.rejected_patterns.push(msg);
            return;
        }
        // A pattern that matches the empty string spins the substitution loop
        // in `check_dictionary_links` forever (see `can_match_empty`); only
        // guarded for switch combinations that reach it, since `-o`/`-s`
        // substitute once and Perl accepts them.
        if switches & (LINK_ONCE | LINK_SECT_ONCE) == 0
            && can_match_empty(pattern, switches & LINK_NOCASE != 0)
        {
            let msg = format!(
                "textrill: ignoring link-dictionary pattern {pattern:?}: it matches the empty \
                 string, which would never terminate. In a /.../ entry | is a regex alternation \
                 operator, so /|x|/ matches nothing at every position; write |x| (no slashes) to use \
                 the pipe-delimited dictionary form."
            );
            eprintln!("{msg}");
            self.rejected_patterns.push(msg);
            return;
        }
        self.add_rule(label, pattern, url, switches);
    }

    fn add_literal(&mut self, label: &str, pattern: &str, url: &str, switches: u8) {
        let mut p = String::new();
        for ch in pattern.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '_' {
                p.push('\\');
            }
            p.push(ch);
        }
        let p = format!("\\b{p}\\b");
        self.add_rule(label, &p, url, switches);
    }

    fn add_glob(&mut self, label: &str, pattern: &str, url: &str, switches: u8) {
        let p = glob2regexp(pattern);
        self.add_rule(label, &p, url, switches);
    }

    /// Filter comment and colon-terminated lines before parsing.
    pub fn filter_dict(&self, dict: &str) -> String {
        let mut out = String::new();
        for line in dict.split('\n') {
            // Anchored at the very start of the line (no /m): leading
            // whitespace before '#' stops the skip.
            if line.starts_with('#') {
                continue;
            }
            // skip lines that end with unescaped ':'  (/^.*[^\\]:\s*$/)
            let t = line.trim_end_matches([' ', '\t', '\r']);
            if t.ends_with(':') {
                let b = t.as_bytes();
                let preceded = b.len() >= 2 && b[b.len() - 2] != b'\\' && b[b.len() - 2] != b'\r';
                if preceded {
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// Port of `parse_dict`.
    pub fn parse_dict(&mut self, _dictfile: &str, dict: &str) {
        let pattern = r"\s*(.+)\s+\-+([iehos]+\-+)?>\s*(.*\S+)\s*\n";
        let re = Regex::new(pattern).unwrap();
        for caps in re.captures_iter(dict) {
            let caps = caps.unwrap();
            if caps.len() < 4 {
                continue;
            }
            let mut key = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let options = caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let url = caps
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let mut switches: u8 = 0;
            if options.contains('i') || options.contains('I') {
                switches |= LINK_NOCASE;
            }
            if options.contains('e') || options.contains('E') {
                switches |= LINK_EVAL;
            }
            if options.contains('h') || options.contains('H') {
                switches |= LINK_HTML;
            }
            if options.contains('o') || options.contains('O') {
                switches |= LINK_ONCE;
            }
            if options.contains('s') || options.contains('S') {
                switches |= LINK_SECT_ONCE;
            }
            key = key.trim_end().to_string();

            if let Some(k) = key.strip_prefix('/') {
                let k = k.strip_suffix('/').unwrap_or(k);
                self.add_regexp(k, k, &url, switches);
            } else if let Some(k) = key.strip_prefix('|') {
                let k = k.strip_suffix('|').unwrap_or(k);
                // escape all slashes (only matters for /-delimited regexes)
                self.add_regexp(k, k, &url, switches);
            } else if key.contains('"') || key.starts_with('"') {
                let mut k = key.clone();
                if k.starts_with('"') {
                    k.remove(0);
                }
                if k.ends_with('"') {
                    k.pop();
                }
                self.add_literal(&k, &k, &url, switches);
            } else {
                self.add_glob(&key, &key, &url, switches);
            }
        }
    }

    /// Check if we are inside a link or a tag (port of `in_link_context`).
    pub fn in_link_context(&self, mat: &str, before: &str) -> bool {
        let lower = mat.to_ascii_lowercase();
        // m@</?A>@i   (also matches a bare <a> with no attributes)
        if lower.contains("</a>") || lower.contains("<a>") {
            return true;
        }
        let (oa, ca) = if self.lower_case_tags {
            ("<a ", "</a>")
        } else {
            ("<A ", "</A>")
        };
        let final_open = before.rfind(oa);
        let final_close = before.rfind(ca);
        if let Some(fo) = final_open {
            let fc = final_close.map(|x| x as i64).unwrap_or(-1);
            if fc < 0 || fo as i64 > fc {
                return true;
            }
        }
        let final_open = before.rfind('<');
        let final_close = before.rfind('>');
        if let Some(fo) = final_open {
            let fc = final_close.map(|x| x as i64).unwrap_or(-1);
            if fc < 0 || fo as i64 > fc {
                return true;
            }
        }
        false
    }

    /// Port of `check_dictionary_links`.
    pub fn check_dictionary_links(&mut self, para_ref: &mut String) {
        let i_len = self.rules.len();
        // One ASCII-lowercased paragraph shared by every rule; folding once
        // makes the filter cheaper than the regex calls it removes.
        let mut folded = para_ref.as_bytes().to_vec();
        folded.make_ascii_lowercase();
        // Substitutions rewrite `para_ref`, so the fold must follow. A stale
        // fold is silent link loss, not a crash, so track whether text changed.
        let mut fold_is_stale = false;
        for i in 0..i_len {
            if fold_is_stale {
                folded.clear();
                folded.extend_from_slice(para_ref.as_bytes());
                folded.make_ascii_lowercase();
                fold_is_stale = false;
            }
            // A `false` here proves `captures` would return None, so skipping
            // cannot lose a link.
            if !self.rules[i].may_match(&folded) {
                continue;
            }
            let rule_switches = self.rules[i].switches;
            let mut line_with_links = String::new();
            if rule_switches & LINK_ONCE != 0 {
                if !self.once_done[i] {
                    if let Some(caps) = self.rules[i].regex.captures(&*para_ref).ok().flatten() {
                        let m = caps.get(0).unwrap();
                        let (pre, matched, post) = split_front(para_ref, m.start(), m.end());
                        self.once_done[i] = true;
                        // The guard must see the text before the match, or it
                        // cannot see a surrounding <a> and nests anchors.
                        line_with_links.push_str(&pre);
                        let mut linkme = matched;
                        if !self.in_link_context(&linkme, &line_with_links) {
                            linkme = self.repl(i, &linkme);
                        }
                        line_with_links.push_str(&linkme);
                        *para_ref = post;
                        fold_is_stale = true;
                    }
                }
                if !line_with_links.is_empty() {
                    *para_ref = format!("{line_with_links}{para_ref}");
                    fold_is_stale = true;
                }
            } else if rule_switches & LINK_SECT_ONCE != 0 {
                if !self.sect_once_done[i] {
                    if let Some(caps) = self.rules[i].regex.captures(&*para_ref).ok().flatten() {
                        let m = caps.get(0).unwrap();
                        let (pre, matched, post) = split_front(para_ref, m.start(), m.end());
                        self.sect_once_done[i] = true;
                        // same ordering as the LINK_ONCE branch
                        line_with_links.push_str(&pre);
                        let mut linkme = matched;
                        if !self.in_link_context(&linkme, &line_with_links) {
                            linkme = self.repl(i, &linkme);
                        }
                        line_with_links.push_str(&linkme);
                        *para_ref = post;
                        fold_is_stale = true;
                    }
                }
                if !line_with_links.is_empty() {
                    *para_ref = format!("{line_with_links}{para_ref}");
                    fold_is_stale = true;
                }
            } else {
                loop {
                    // Re-check each iteration: a previous substitution may have
                    // consumed the literal.
                    if !self.rules[i].may_match(&folded) {
                        break;
                    }
                    let caps_opt = self.rules[i].regex.captures(para_ref).ok().flatten();
                    match caps_opt {
                        None => break,
                        Some(caps) => {
                            let m = caps.get(0).unwrap();
                            let (pre, matched, post) = split_front(para_ref, m.start(), m.end());
                            line_with_links.push_str(&pre);
                            let mut linkme = matched;
                            if !self.in_link_context(&linkme, &line_with_links) {
                                linkme = self.repl(i, &linkme);
                            }
                            line_with_links.push_str(&linkme);
                            *para_ref = post;
                            // Refreshed at the next rule instead, since several
                            // may run before the text is read again.
                            fold_is_stale = true;
                        }
                    }
                }
                if !line_with_links.is_empty() {
                    *para_ref = format!("{line_with_links}{para_ref}");
                    fold_is_stale = true;
                }
            }
        }
    }

    fn repl(&self, i: usize, matched: &str) -> String {
        let rule = &self.rules[i];
        let re = &rule.regex;
        if let Some(caps) = re.captures(matched).ok().flatten() {
            expand_template(&rule.replacement, &caps, matched)
        } else {
            matched.to_string()
        }
    }
}

fn split_front(s: &str, start: usize, end: usize) -> (String, String, String) {
    (
        s[..start].to_string(),
        s[start..end].to_string(),
        s[end..].to_string(),
    )
}

/// The built-in system dictionary (from HTML::TextToHTML v3.0).
pub const SYSTEM_DICT: &str = "\
#
# Global links dictionary file for HTML::TextToHTML
# http://www.katspace.com/tools/text_to_html
# http://txt2html.sourceforge.net/
# based on links dictionary for Seth Golub's txt2html
# http://www.aigeek.com/txt2html/
#
# This dictionary contains some patterns for converting obvious URLs,
# ftp sites, hostnames, email addresses and the like to hrefs.
#
# Original adapted from the html.pl package by Oscar Nierstrasz in
# the Software Archive of the Software Composition Group
# http://iamwww.unibe.ch/~scg/Src/
#

# Some people even like to mark the URL label explicitly <URL:foo:label>
/&lt;URL:([-\\w\\.\\/:~_\\@]+):([a-zA-Z0-9'() ]+)&gt;/ -h-> <A HREF=\"$1\">$2</A>

# Some people like to mark URLs explicitly <URL:foo>
/&lt;URL:\\s*([^\\s\"]+?)\\s*&gt;/ -h-> <A HREF=\"$1\">$1</A>

#  <http://site>
/&lt;(http:[^\\s\"]+?)\\s*&gt;/ -h-> &lt;<A HREF=\"$1\">$1</A>&gt;

# Urls: <service>:<rest-of-url>

|snews:[\\w\\.]+|        -> $&
|news:[\\w\\.]+|         -> $&
|nntp:[\\w/\\.:+\\-]+|    -> $&
|http:[\\w/\\.:\\@+\\-~\\%#?=&;,]+[\\w/]|  -> $&
|shttp:[\\w/\\.:+\\-~\\%#?=&;,]+| -> $&
|https:[\\w/\\.:+\\-~\\%#?=&;,]+| -> $&
|file:[\\w/\\.:+\\-]+|     -> $&
|ftp:[\\w/\\.:+\\-]+|      -> $&
|wais:[\\w/\\.:+\\-]+|     -> $&
|gopher:[\\w/\\.:+\\-]+|   -> $&
|telnet:[\\w/\\@\\.:+\\-]+|   -> $&

# catch some newsgroups to avoid confusion with sites:
|([^\\w\\-/\\.:\\@>])(alt\\.[\\w\\.+\\-]+[\\w+\\-]+)|    -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(bionet\\.[\\w\\.+\\-]+[\\w+\\-]+)| -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(bit\\.[\\w\\.+\\-]+[\\w+\\-]+)|    -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(biz\\.[\\w\\.+\\-]+[\\w+\\-]+)|    -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(clari\\.[\\w\\.+\\-]+[\\w+\\-]+)|  -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(comp\\.[\\w\\.+\\-]+[\\w+\\-]+)|   -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(gnu\\.[\\w\\.+\\-]+[\\w+\\-]+)|    -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(humanities\\.[\\w\\.+\\-]+[\\w+\\-]+)|
          -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(k12\\.[\\w\\.+\\-]+[\\w+\\-]+)|    -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(misc\\.[\\w\\.+\\-]+[\\w+\\-]+)|   -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(news\\.[\\w\\.+\\-]+[\\w+\\-]+)|   -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(rec\\.[\\w\\.+\\-]+[\\w+\\-]+)|    -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(soc\\.[\\w\\.+\\-]+[\\w+\\-]+)|    -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(talk\\.[\\w\\.+\\-]+[\\w+\\-]+)|   -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(us\\.[\\w\\.+\\-]+[\\w+\\-]+)|     -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(ch\\.[\\w\\.+\\-]+[\\w+\\-]+)|     -h-> $1<A HREF=\"news:$2\">$2</A>
|([^\\w\\-/\\.:\\@>])(de\\.[\\w\\.+\\-]+[\\w+\\-]+)|     -h-> $1<A HREF=\"news:$2\">$2</A>

# FTP locations (with directory):
# anonymous@<site>:<path>
|(anonymous\\@)([[:alpha:]][\\w\\.+\\-]+\\.[[:alpha:]]{2,}):(\\s*)([\\w\\d+\\-/\\.]+)|
  -h-> $1<A HREF=\"ftp://$2/$4\">$2:$4</A>$3

# ftp@<site>:<path>
|(ftp\\@)([[:alpha:]][\\w\\.+\\-]+\\.[[:alpha:]]{2,}):(\\s*)([\\w\\d+\\-/\\.]+)|
  -h-> $1<A HREF=\"ftp://$2/$4\">$2:$4</A>$3

# Email address
|[[:alnum:]_\\+\\-\\.]+\\@([[:alnum:]][\\w\\.+\\-]+\\.[[:alpha:]]{2,})|
  -> mailto:$&

# <site>:<path>
|([^\\w\\-/\\.:\\@>])([[:alpha:]][\\w\\.+\\-]+\\.[[:alpha:]]{2,}):(\\s*)([\\w\\d+\\-/\\.]+)|
  -h-> $1<A HREF=\"ftp://$2/$4\">$2:$4</A>$3

# NB: don't confuse an http server with a port number for
# an FTP location!
# internet number version: <internet-num>:<path>
|([^\\w\\-/\\.:\\@])(\\d{2,}\\.\\d{2,}\\.\\d+\\.\\d+):([\\w\\d+\\-/\\.]+)|
  -h-> $1<A HREF=\"ftp://$2/$3\">$2:$3</A>

# telnet <site> <port>
|telnet ([[:alpha:]][\\w+\\-]+(\\.[\\w\\.+\\-]+)+\\.[[:alpha:]]{2,})\\s+(\\d{2,4})|
  -h-> telnet <A HREF=\"telnet://$1:$3/\">$1 $3</A>

# ftp <site>
|ftp ([[:alpha:]][\\w+\\-]+(\\.[\\w\\.+\\-]+)+\\.[[:alpha:]]{2,})|
  -h-> ftp <A HREF=\"ftp://$1/\">$1</A>

# host with \"ftp\" in the machine name
|\\b([[:alpha:]][\\w])*ftp[\\w]*(\\.[\\w+\\-]+){2,}| -h-> ftp <A HREF=\"ftp://$&/\">$&</A>

# ftp.foo.net/blah/
|ftp(\\.[\\w\\@:-]+)+/[^\\s\"]+| -> ftp://$&

# www.thehouse.org/txt2html/
|www(\\.[\\w\\@:-]+)+/[^\\s\"]+| -> http://$&

# host with \"www\" in the machine name
|\\b([[:alpha:]][\\w])*www[\\w]*(\\.[\\w+\\-]+){2,}| -> http://$&/

# <site> <port>
|([[:alpha:]][\\w+\\-]+\\.[\\w+\\-]+\\.[[:alpha:]]{2,})\\s+(\\d{2,4})|
  -h-> <A HREF=\"telnet://$1:$2/\">$1 $2</A>

# just internet numbers with port:
|([^\\w\\-/\\.:\\@])(\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3})\\s+(\\d{1,4})|
  -h-> $1<A HREF=\"telnet://$2:$3\">$2 $3</A>

# just internet numbers:
|([^\\w\\-/\\.:\\@])(\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3})|
  -h-> $1<A HREF=\"telnet://$2\">$2</A>

# RFCs
/RFC ?(\\d+)/ -i-> http://www.cis.ohio-state.edu/rfc/rfc$1.txt

# Seth and his amazing conversion program    :-)

\"Seth Golub\"  -o-> http://www.aigeek.com/
\"txt2html\"    -o-> https://github.com/resurrecting-open-source-projects/txt2html

# Kathryn and her amazing modules 8-)
\"Kathryn Andersen\"  -o-> http://www.katspace.com/
\"HTML::TextToHTML\"  -o-> http://www.katspace.com/tools/text_to_html/
\"hypertoc\"          -o-> http://www.katspace.com/tools/hypertoc/
\"HTML::GenToc\"      -o-> http://www.katspace.com/tools/hypertoc/

# End of global dictionary
";

/// Build the link rule set from the given options plus the system dictionary.
pub fn load_links(opts: &Options) -> LinkParser {
    let mut parser = LinkParser::new(opts.lower_case_tags, opts.url_policy());
    // The system dictionary (and everything else) loads only when make_links
    // is set; the default dictionary is appended to the user dictionaries when
    // it exists.
    if !opts.make_links {
        return parser;
    }
    let mut dict_files: Vec<&str> = Vec::new();
    for dict_file in &opts.links_dictionaries {
        dict_files.push(dict_file);
    }
    if !opts.default_link_dict.is_empty() && std::path::Path::new(&opts.default_link_dict).is_file()
    {
        dict_files.push(&opts.default_link_dict);
    }
    for dict_file in dict_files {
        if let Ok(contents) = std::fs::read_to_string(dict_file) {
            let filtered = parser.filter_dict(&contents);
            parser.parse_dict(dict_file, &filtered);
        }
    }
    let filtered = parser.filter_dict(SYSTEM_DICT);
    parser.parse_dict("DATA", &filtered);
    parser
}

/// Parse a link-dictionary string (used by the GUI) into rules.
#[allow(dead_code)]
pub fn load_links_from_text(opts: &Options, dict_text: &str) -> LinkParser {
    let mut parser = LinkParser::new(opts.lower_case_tags, opts.url_policy());
    let filtered = parser.filter_dict(dict_text);
    parser.parse_dict("user", &filtered);
    parser
}
