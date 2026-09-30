//! The txt2html conversion engine.
//!
//! A faithful port of HTML::TextToHTML v3.0's `process_para`,
//! `process_chunk`, `txt2html`, `do_file_start` and the associated helper
//! subroutines.

use std::collections::HashMap;

use fancy_regex::Regex;

use crate::chars;
use crate::links::{self, LinkParser};
use crate::options::Options;

/// Read a text file for conversion. Perl reads raw bytes and keeps 8-bit
/// characters intact; mimic that by decoding UTF-8 when possible and
/// falling back to Latin-1 (byte == code point).
pub fn read_any_file(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => e.into_bytes().into_iter().map(|b| b as char).collect(),
    })
}

/// Chop trailing whitespace and a DOS CR, i.e. what the Perl
/// `s/[ \t]*\x0D$//` did.
///
/// This started life as a regular expression, and it is still written as
/// `[ \t]*\x0D$` in the Perl source. It cannot stay one here: `links.rs`
/// rewrites every `$` into the lookahead `(?=\n?$)`, which takes the pattern
/// off fancy-regex's fast automaton and onto the backtracker, where a large
/// paragraph exhausts the step budget and panics. A 1 MB paragraph with no
/// blank lines is enough.
///
/// It is anchored at the end of the paragraph, so it is just string surgery,
/// and string surgery has no budget to exhaust.
///
/// The subtlety is that Perl's `$` means "end of text, *or* before a single
/// trailing newline", so this cannot be `trim_end()` followed by a check for
/// `\r`: a trailing `\r\n` has to be recognised as well.
fn chop_trailing_cr(s: &str) -> String {
    let (body, had_nl) = match s.strip_suffix('\n') {
        Some(b) => (b, true),
        None => (s, false),
    };
    if !body.ends_with('\r') {
        return s.to_string(); // no match, so the paragraph is left alone
    }
    // The `[ \t]*` sits *before* the CR, so the CR comes off first and the
    // spaces and tabs are trimmed afterwards. Trimming first leaves a stray CR.
    let trimmed = body[..body.len() - 1].trim_end_matches([' ', '\t']);
    let mut out = String::with_capacity(trimmed.len() + 1);
    out.push_str(trimmed);
    if had_nl {
        out.push('\n');
    }
    out
}

/// Chop leading whitespace and a DOS CR, i.e. what the Perl
/// `s/^[ \t]*\x0D//` did. See [`chop_trailing_cr`] for why this is not a
/// regular expression.
///
/// The leading pattern only removes anything when a CR actually follows the run
/// of whitespace. Trimming the leading whitespace unconditionally would
/// corrupt every indented paragraph in the document.
fn chop_leading_cr(s: &str) -> String {
    match s.trim_start_matches([' ', '\t']).strip_prefix('\r') {
        Some(rest) => rest.to_string(),
        None => s.to_string(),
    }
}

// mode bits
pub const NONE: u32 = 0;
pub const LIST: u32 = 1;
pub const HRULE: u32 = 2;
pub const PAR: u32 = 4;
pub const PRE: u32 = 8;
pub const END: u32 = 16;
pub const BREAK: u32 = 32;
pub const HEADER: u32 = 64;
pub const MAILHEADER: u32 = 128;
pub const MAILQUOTE: u32 = 256;
pub const CAPS: u32 = 512;
pub const LINK: u32 = 1024;
pub const PRE_EXPLICIT: u32 = 2048;
pub const TABLE: u32 = 4096;
pub const IND_BREAK: u32 = 8192;
pub const LIST_START: u32 = 16384;
pub const LIST_ITEM: u32 = 32768;

const OL: u8 = 1;
const UL: u8 = 2;
const DL: u8 = 3;

const TAB_ALIGN: u32 = 1;
const TAB_PGSQL: u32 = 2;
const TAB_BORDER: u32 = 3;
const TAB_DELIM: u32 = 4;

const TAG_START: u8 = 1;
const TAG_END: u8 = 2;
const TAG_EMPTY: u8 = 3;

const PROG: &str = "HTML::TextToHTML";
const VERSION: &str = "3.0";

fn subtract_modes(v: u32, mask: u32) -> u32 {
    (v | mask) - mask
}

fn is_perl_space(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\x0c' || c == '\x0b'
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Split a string into lines the way `split(/^/, $para)` does
/// (each line keeps its trailing newline, except a final un-terminated one).
fn split_lines(s: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        cur.push(c);
        if c == '\n' {
            lines.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

pub struct Converter {
    pub opts: Options,
    links: LinkParser,
    tags: Vec<String>,
    mode: u32,
    listnum: usize,
    list_nice_indent: String,
    list_indent: Vec<usize>,
    list: Vec<u8>,
    list_prefix: Vec<String>,
    number_match: String,
    term_match: String,
    heading_styles: HashMap<String, usize>,
    num_heading_styles: usize,
    heading_count: Vec<usize>,
    non_header_anchor: usize,
    prev_para_action: u32,
    preformat_enabled: bool,
    re_cache: HashMap<String, Regex>,
    print_count: u32,
}

impl Converter {
    pub fn new(mut opts: Options) -> Self {
        opts.deal_with_options();
        let preformat_enabled =
            opts.endpreformat_trigger_lines != 0 || opts.use_preformat_marker;

        let mut heading_styles = HashMap::new();
        let mut num_heading_styles = 0;
        if opts.use_mosaic_header {
            for s in ["*", "=", "+", "-", "~", "."] {
                num_heading_styles += 1;
                heading_styles.insert(s.to_string(), num_heading_styles);
            }
        }

        let links = links::load_links(&opts);
        let number_match_default = if opts.bullets_ordered.is_empty() {
            r"(\d+|[A-Za-z_])".to_string()
        } else {
            format!(
                r"(\d+|[A-Za-z]|[{0}])",
                class_body(&opts.bullets_ordered)
            )
        };

        Converter {
            number_match: number_match_default,
            term_match: r"(\w\w+)".to_string(),
            tags: Vec::new(),
            mode: 0,
            listnum: 0,
            list_nice_indent: String::new(),
            list_indent: Vec::new(),
            list: Vec::new(),
            list_prefix: Vec::new(),
            heading_styles,
            num_heading_styles,
            heading_count: Vec::new(),
            non_header_anchor: 0,
            prev_para_action: 0,
            opts,
            links,
            preformat_enabled,
            re_cache: HashMap::new(),
            print_count: 0,
        }
    }

    fn re(&mut self, pat: &str) -> &Regex {
        let key = format!("(?s){pat}");
        if !self.re_cache.contains_key(&key) {
            let re = Regex::new(&links::translate_pattern(&key))
                .unwrap_or_else(|e| panic!("bad regex {pat:?}: {e}"));
            self.re_cache.insert(key.clone(), re);
        }
        self.re_cache.get(&key).unwrap()
    }

    fn re_i(&mut self, pat: &str) -> &Regex {
        let key = format!("(?s)(?i){pat}");
        if !self.re_cache.contains_key(&key) {
            let re = Regex::new(&links::translate_pattern(&key))
                .unwrap_or_else(|e| panic!("bad regex {pat:?}: {e}"));
            self.re_cache.insert(key.clone(), re);
        }
        self.re_cache.get(&key).unwrap()
    }

    // ------------------------------------------------------- tags

    fn get_tag(&mut self, in_tag: &str, tag_type: u8, inside_tag: &str) -> String {
        let open_tag = self.tags.last().cloned().unwrap_or_default();
        let mut tag_prefix = String::new();

        if self.opts.xhtml {
            if open_tag == "p"
                && in_tag == "p"
                && tag_type != TAG_END
            {
                tag_prefix = self.close_tag("p");
            } else if open_tag == "p"
                && (in_tag.starts_with("hr")
                    || in_tag == "ul"
                    || in_tag == "ol"
                    || in_tag == "dl"
                    || in_tag == "pre"
                    || in_tag == "table"
                    || in_tag.starts_with('h'))
            {
                tag_prefix = self.close_tag("p");
            } else if open_tag == "li" && in_tag == "li" && tag_type != TAG_END {
                tag_prefix = self.close_tag("li");
            } else if open_tag == "li"
                && (in_tag == "ul" || in_tag == "ol")
                && tag_type == TAG_END
            {
                tag_prefix = self.close_tag("li");
            } else if open_tag == "dt" && in_tag == "dd" && tag_type != TAG_END {
                tag_prefix = self.close_tag("dt");
            } else if open_tag == "dd" && in_tag == "dt" && tag_type != TAG_END {
                tag_prefix = self.close_tag("dd");
            } else if open_tag == "dd" && in_tag == "dl" && tag_type == TAG_END {
                tag_prefix = self.close_tag("dd");
            }
        }

        if tag_type == TAG_END {
            let out = self.close_tag(in_tag);
            return if tag_prefix.is_empty() {
                out
            } else {
                format!("{tag_prefix}{out}")
            };
        }

        let mut out_tag = in_tag.to_string();
        if self.opts.lower_case_tags {
            out_tag = out_tag.to_ascii_lowercase();
        } else {
            out_tag = out_tag.to_ascii_uppercase();
        }
        let out;
        if tag_type == TAG_EMPTY {
            if self.opts.xhtml {
                out = format!("<{out_tag}{inside_tag}/>");
            } else {
                out = format!("<{out_tag}{inside_tag}>");
            }
        } else {
            self.tags.push(in_tag.to_string());
            out = format!("<{out_tag}{inside_tag}>");
        }
        if tag_prefix.is_empty() {
            out
        } else {
            format!("{tag_prefix}{out}")
        }
    }

    fn close_tag(&mut self, in_tag: &str) -> String {
        let open_tag = self.tags.pop().unwrap_or_default();
        let in_tag = if in_tag.is_empty() { &open_tag } else { in_tag };
        if !open_tag.is_empty() && open_tag != in_tag {
            self.tags.push(open_tag.clone());
        }
        let mut out_tag = in_tag.to_string();
        if self.opts.lower_case_tags {
            out_tag = out_tag.to_ascii_lowercase();
        } else {
            out_tag = out_tag.to_ascii_uppercase();
        }
        format!("</{out_tag}>")
    }

    // ------------------------------------------------------- simple blocks

    fn hrule(&mut self, lines: &mut Vec<String>, actions: &mut Vec<u32>, ind: usize) {
        let hrmin = self.opts.hrule_min;
        let pat = format!(r"^\s*([\-_~=*]\s*){{{hrmin},}}$");
        if self.re(&pat).is_match(&lines[ind]).unwrap_or(false) {
            let tag = self.get_tag("hr", TAG_EMPTY, "");
            lines[ind] = format!("{tag}\n");
            actions[ind] |= HRULE;
        } else if lines[ind].contains('\x0c') {
            actions[ind] |= HRULE;
            let tag = self.get_tag("hr", TAG_EMPTY, "");
            lines[ind] = lines[ind].replace('\x0c', &format!("\n{tag}\n"));
        }
    }

    fn shortline(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        i: usize,
        prev: &mut String,
        prev_action: &mut u32,
        prev_line_len: usize,
    ) {
        let tag = self.get_tag("br", TAG_EMPTY, "");
        if !lines[i].trim().is_empty()
            && prev.trim().is_empty() == false
            && prev_line_len < self.opts.short_line_length
            && actions[i] & (END | HEADER | HRULE | LIST | IND_BREAK | PAR) == 0
            && *prev_action & (HEADER | HRULE | BREAK | IND_BREAK) == 0
        {
            let c = prev.pop().unwrap_or('\0');
            prev.push_str(&tag);
            prev.push(c);
            *prev_action |= BREAK;
        }
    }

    fn is_mailheader(&self, rows: &[String]) -> bool {
        let re = links::ascii_re_cached(r"^(?:From:?)|Newsgroups: ");
        if rows.is_empty() {
            return false;
        }
        re.is_match(&rows[0]).unwrap_or(false)
    }

    fn mailheader(&mut self, rows_ref: &mut Vec<String>) {
        let mut rows = rows_ref.clone();
        if self.is_mailheader(rows_ref) {
            self.mode |= MAILHEADER;
            if self.opts.escape_html_chars {
                rows[0] = chars::escape(&rows[0]);
            }
            self.anchor_mail(&mut rows[0]);
            if rows[0].ends_with('\n') {
                rows[0].pop();
            }
            let tag = self.get_tag("p", TAG_START, " class='mail_header'");
            let tag2 = self.get_tag("br", TAG_EMPTY, "");
            rows[0] = format!("<!-- New Message -->\n{tag}{}{tag2}\n", rows[0]);
            let rlen = rows.len();
            for rn in 1..rlen {
                if self.opts.escape_html_chars {
                    rows[rn] = chars::escape(&rows[rn]);
                }
                if rn != rlen - 1 {
                    let tag3 = self.get_tag("br", TAG_EMPTY, "");
                    if rows[rn].ends_with('\n') {
                        rows[rn].pop();
                    }
                    rows[rn] = format!("{}{}\n", rows[rn], tag3);
                }
            }
        }
        *rows_ref = rows;
    }

    fn mailquote(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        next: Option<&str>,
    ) {
        let starts_quote = {
            let l = &lines[ind];
            let re1 = links::ascii_re_cached(r"^\w*&gt");
            let re2 = links::ascii_re_cached(r"^[\|:]");
            (re1.is_match(l).unwrap_or(false) || re2.is_match(l).unwrap_or(false))
                && next.is_some()
                && !next.unwrap().trim().is_empty()
        };
        if starts_quote {
            let tag = self.get_tag("br", TAG_EMPTY, "");
            // Perl: s/$/<tag>/ with `$` matching before the trailing newline
            if let Some(stripped) = lines[ind].strip_suffix('\n') {
                lines[ind] = format!("{stripped}{tag}\n");
            } else {
                lines[ind].push_str(&tag);
            }
            actions[ind] |= BREAK | MAILQUOTE;
            if *prev_action & (BREAK | MAILQUOTE) == 0 {
                let tag = self.get_tag("p", TAG_START, " class='quote_mail'");
                prev.push_str(&tag);
                actions[ind] |= PAR;
            }
        }
    }

    fn paragraph(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        _indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        line_indent: usize,
        prev_indent: usize,
        is_fragment: bool,
        line_no: usize,
    ) {
        let par_indent = self.opts.par_indent;
        let cond = !lines[ind].trim().is_empty()
            && subtract_modes(actions[ind], END | MAILQUOTE | CAPS | BREAK) == 0
            && (prev.trim().is_empty()
                || actions[ind] & END != 0
                || line_indent > prev_indent + par_indent)
            && !(is_fragment && line_no == 0);
        if cond {
            if self.opts.indent_par_break
                && !prev.trim().is_empty()
                && actions[ind] & END == 0
                && line_indent > prev_indent + par_indent
            {
                let tag = self.get_tag("br", TAG_EMPTY, "");
                prev.push_str(&tag);
                prev.push_str(&"&nbsp;".repeat(line_indent));
                lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
                *prev_action |= BREAK;
                actions[ind] |= IND_BREAK;
            } else if self.opts.preserve_indent {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                prev.push_str(&"&nbsp;".repeat(line_indent));
                lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
                actions[ind] |= PAR;
            } else {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                actions[ind] |= PAR;
            }
        } else if self.opts.indent_par_break
            && self.mode & (PRE | TABLE | LIST) == 0
            && !prev.trim().is_empty()
            && actions[ind] & END == 0
            && *prev_action & (IND_BREAK | PAR) != 0
            && subtract_modes(actions[ind], END | MAILQUOTE | CAPS) == 0
            && line_indent > par_indent
            && line_indent == prev_indent
        {
            let tag = self.get_tag("br", TAG_EMPTY, "");
            prev.push_str(&tag);
            prev.push_str(&"&nbsp;".repeat(line_indent));
            lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
            *prev_action |= BREAK;
            actions[ind] |= IND_BREAK;
        }
    }

    // ------------------------------------------------------- lists

    fn listprefix(&mut self, line: &str) -> (String, String, String, String) {
        let bullets = class_body(&self.opts.bullets);
        let bullets_full = format!("[{}]", bullets);
        let number_match = self.number_match.clone();
        let term_match = self.term_match.clone();

        let pat_bullet = format!(r"^\s*{}\s+\S", bullets_full);
        let pat_ordered = format!(r"^\s*{number_match}[\.\)\]:]\s+\S");
        let pat_term = format!(r"^\s*{term_match}:$");

        let is_bullet = self.re(&pat_bullet).is_match(line).unwrap_or(false);
        let is_ordered = self.re(&pat_ordered).is_match(line).unwrap_or(false);
        let is_term = self.re(&pat_term).is_match(line).unwrap_or(false);
        if !is_bullet && !is_ordered && !is_term {
            return (String::new(), String::new(), String::new(), String::new());
        }

        let mut term = String::new();
        if let Some(caps) = self.re(&pat_term).captures(line).ok().flatten() {
            if let Some(g) = caps.get(1) {
                term = g.as_str().to_string();
            }
        }
        let mut number = String::new();
        let pat_num = format!(r"^\s*{number_match}\S\s+\S");
        if let Some(caps) = self.re(&pat_num).captures(line).ok().flatten() {
            if let Some(g) = caps.get(1) {
                number = g.as_str().to_string();
            }
        }
        if !self.opts.bullets_ordered.is_empty() {
            let bop = class_body(&self.opts.bullets_ordered);
            let bop_full = format!("[{}]", bop);
            if self.re(&bop_full).is_match(&number).unwrap_or(false) {
                number = "1".to_string();
            }
        }
        // slippery "o" bullet exception (Perl sets $number = 0, which is falsy)
        if self.opts.bullets.contains('o') {
            let re = links::ascii_re_cached(r"^\s*o\s");
            if re.is_match(line).unwrap_or(false) {
                number.clear();
            }
        }

        let prefix;
        let rawprefix;
        if !term.is_empty() {
            let pat = format!(r"^(\s*{term_match}.)$");
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            prefix = rawprefix.replace(&term, "");
        } else if !number.is_empty() {
            // the captured number string is truthy in Perl (even "0")
            let pat = format!(r"^(\s*{number_match}.)");
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            prefix = rawprefix.replace(&number, "");
        } else {
            let pat = format!(r"^(\s*{}.)", bullets_full);
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            prefix = rawprefix.clone();
        }
        (prefix, number, rawprefix, term)
    }

    fn startlist(
        &mut self,
        prefix: &str,
        number: &str,
        _rawprefix: &str,
        term: &str,
        _lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        _indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        total_prefix: &str,
    ) -> bool {
        // The reference indexes these stacks by __listnum (assignment, not
        // push), so entries below listnum stay but are overwritten.
        while self.list_prefix.len() <= self.listnum {
            self.list_prefix.push(String::new());
        }
        self.list_prefix[self.listnum] = prefix.to_string();
        let num_truthy = !number.is_empty();
        let tag;
        if num_truthy {
            if number != "1" && number != "a" && number != "A" {
                return false;
            }
            tag = self.get_tag("ol", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = OL;
        } else if !term.is_empty() {
            tag = self.get_tag("dl", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = DL;
        } else {
            tag = self.get_tag("ul", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = UL;
        }
        let _ = tag;
        while self.list_indent.len() <= self.listnum {
            self.list_indent.push(0);
        }
        self.list_indent[self.listnum] = total_prefix.chars().count();
        self.listnum += 1;
        self.list_nice_indent = " ".repeat(self.listnum * self.opts.indent_width);
        actions[ind] |= LIST;
        actions[ind] |= LIST_START;
        self.mode |= LIST;
        true
    }

    fn endlist(&mut self, num_lists: usize, prev: &mut String, line_action: &mut u32) {
        let mut n = num_lists;
        while n > 0 {
            self.list_nice_indent = " ".repeat((self.listnum - 1) * self.opts.indent_width);
            let lt = self.list.get(self.listnum - 1).copied().unwrap_or(0);
            let tag;
            if lt == UL {
                tag = self.get_tag("ul", TAG_END, "");
            } else if lt == OL {
                tag = self.get_tag("ol", TAG_END, "");
            } else if lt == DL {
                tag = self.get_tag("dl", TAG_END, "");
            } else {
                tag = String::new();
            }
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            self.list_indent.pop();
            self.listnum = self.listnum.saturating_sub(1);
            n -= 1;
        }
        *line_action |= END;
        if self.listnum == 0 {
            self.mode ^= LIST & self.mode;
        }
    }

    fn continuelist(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        ind: usize,
        term: &str,
    ) {
        let list_indent = self.list_nice_indent.clone();
        let lt = self.list.get(self.listnum - 1).copied().unwrap_or(0);
        if lt == UL {
            let bullets_full = format!("[{}]", class_body(&self.opts.bullets));
            let pat = format!(r"^\s*{bullets_full} \s*");
            // compile once and reuse: this runs for every list item, and
            // building the pattern afresh each time dominated large documents
            let re = self.re(&pat);
            let matched = re.is_match(&lines[ind]).unwrap_or(false);
            if matched {
                let tag = self.get_tag("li", TAG_START, "");
                let re = self.re(&pat);
                let replaced = re
                    .replace(&lines[ind], format!("{list_indent}{tag}"))
                    .to_string();
                lines[ind] = replaced;
                actions[ind] |= LIST_ITEM;
            }
        }
        if lt == OL {
            let num_match = self.number_match.clone();
            let pat = format!(r"^\s*{num_match}.\s*");
            let tag = self.get_tag("li", TAG_START, "");
            let re = self.re(&pat);
            let replaced = re
                .replace(&lines[ind], format!("{list_indent}{tag}"))
                .to_string();
            lines[ind] = replaced;
            actions[ind] |= LIST_ITEM;
        }
        if lt == DL && !term.is_empty() {
            let term_match = self.term_match.clone();
            let tag = self.get_tag("dt", TAG_START, "");
            let tag2 = self.get_tag("dt", TAG_END, "");
            let term_clean = term.replace('_', " ");
            let pat = format!(r"^\s*{term_match}.$");
            let re = self.re(&pat);
            let replaced = re
                .replace(
                    &lines[ind],
                    format!("{list_indent}{tag}{term_clean}{tag2}"),
                )
                .to_string();
            lines[ind] = replaced;
            let tag = self.get_tag("dd", TAG_START, "");
            lines[ind].push_str(&tag);
            actions[ind] |= LIST_ITEM;
        }
        actions[ind] |= LIST;
    }

    fn total_prefix(&self, line: &str, term: bool) -> String {
        if term {
            // ^(\s*)term.$  -> leading whitespace only
            let ws: String = line
                .chars()
                .take_while(|c| is_perl_space(*c))
                .collect();
            return format!("{ws}{}", " ".repeat(self.opts.indent_width));
        }
        let ch: Vec<char> = line.chars().collect();
        let n = ch.len();
        let mut i = 0;
        while i < n && is_perl_space(ch[i]) {
            i += 1;
        }
        while i < n
            && (is_word_char(ch[i])
                || self.opts.bullets.contains(ch[i])
                || self.opts.bullets_ordered.contains(ch[i]))
        {
            i += 1;
        }
        if i < n {
            i += 1; // the '.' any char
        }
        while i < n && is_perl_space(ch[i]) {
            i += 1;
        }
        ch[..i].iter().collect()
    }

    fn liststuff(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
    ) {
        let (prefix, number, rawprefix, term) = self.listprefix(&lines[ind]);
        let _ = rawprefix;

        if prefix.is_empty() {
            if ind > 0 && !prev.trim().is_empty() {
                return;
            }
            if ind == 0
                && self.listnum > 0
                && indents[ind] == *self.list_indent.last().unwrap()
            {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                actions[ind] |= PAR;
                return;
            }
            if self.listnum > 0 {
                let nl = self.listnum;
                self.endlist(nl, prev, &mut actions[ind]);
            }
            return;
        }

        let mut prefix_alternate: Option<String> = None;
        if number.chars().count() > 1 {
            prefix_alternate = Some(
                format!(
                    "{}{}",
                    " ".repeat(number.chars().count() - 1),
                    prefix
                ),
            );
        }

        // walk back to a matching prefix
        let mut i = self.listnum as i64 - 1;
        while i >= 0 && prefix != self.list_prefix[i as usize] {
            if number.chars().count() > 1 {
                if let Some(pa) = &prefix_alternate {
                    if pa == &self.list_prefix[i as usize] {
                        break;
                    }
                }
            }
            i -= 1;
        }

        let total = self.total_prefix(&lines[ind], !term.is_empty());

        let mut islist = true;
        i += 1;
        if i > 0 && i as usize != self.listnum {
            let nl = self.listnum - i as usize;
            self.endlist(nl, prev, &mut actions[ind]);
            islist = false;
        } else if self.listnum == 0 || i as usize != self.listnum {
            if indents[ind] > 0
                || ind == 0
                || (ind > 0 && prev.trim().is_empty())
                || (ind > 0
                    && *prev_action & (BREAK | HEADER | CAPS) != 0)
            {
                islist = self.startlist(
                    &prefix,
                    &number,
                    "",
                    &term,
                    lines,
                    actions,
                    indents,
                    ind,
                    prev,
                    &total,
                );
            } else {
                return;
            }
        }

        if self.mode & LIST != 0 {
            self.continuelist(lines, actions, ind, &term);
        }
        if islist {
            indents[ind] = total.chars().count();
        }
        let _ = prev_action;
    }

    // ------------------------------------------------------- tables

    fn table_spaces(rows: &[String], para_len: usize) -> String {
        let mut spaces: Vec<u8> = Vec::new();
        let mut min = para_len;
        for row in rows {
            if row.len() < min {
                min = row.len();
            }
            let bytes = row.as_bytes();
            if spaces.is_empty() {
                spaces = bytes.to_vec();
            } else {
                for (i, b) in bytes.iter().enumerate() {
                    if i >= spaces.len() {
                        break;
                    }
                    spaces[i] |= b;
                }
            }
        }
        for b in spaces.iter_mut() {
            if *b != b' ' {
                // 'X' keeps offsets byte==char (the reference uses \xff,
                // which would be multi-byte in a Rust &str).
                *b = b'X';
            }
        }
        spaces.truncate(min);
        spaces.iter().map(|&b| b as char).collect()
    }

    fn is_aligned_table(rows: &[String], para_len: usize) -> bool {
        if rows.len() < 2 {
            return false;
        }
        let spaces = Self::table_spaces(rows, para_len);
        let mut starts: Vec<usize> = Vec::new();
        if !spaces.starts_with(' ') {
            starts.push(0);
        }
        let re = links::ascii_re_cached(r"(?:^| ) +(?=[^ ])");
        for m in re.find_iter(&spaces).flatten() {
            starts.push(m.end());
        }
        rows.len() >= 2 && starts.len() >= 2
    }

    fn table_columns(rows: &[String], para_len: usize) -> (Vec<usize>, Vec<usize>) {
        let spaces = Self::table_spaces(rows, para_len);
        let max = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut starts: Vec<usize> = Vec::new();
        let mut ends: Vec<usize> = Vec::new();
        if !spaces.starts_with(' ') {
            starts.push(0);
        }
        let re = links::ascii_re_cached(r"((?:^| ) +)(?=[^ ])");
        for caps in re.captures_iter(&spaces).flatten() {
            let g = caps.get(1).unwrap();
            ends.push(g.start());
            starts.push(g.end());
        }
        if spaces.starts_with(' ') {
            if !ends.is_empty() {
                ends.remove(0);
            }
        }
        ends.push(max);
        (starts, ends)
    }

    fn make_aligned_table(
        &mut self,
        rows: &mut Vec<String>,
        para_len: usize,
    ) -> bool {
        let (starts, ends) = Self::table_columns(rows, para_len);
        if rows.len() < 2 || starts.len() < 2 {
            return false;
        }
        self.mode |= TABLE;

        let align_idx: Vec<usize> = (0..starts.len())
            .map(|col| {
                let width = ends[col] - starts[col];
                let mut count = [0usize; 4];
                for row in rows.iter() {
                    let cell = byte_slice(row, starts[col], width);
                    let a = if cell.starts_with(' ') { 2 } else { 0 };
                    let b = if cell.ends_with(' ')
                        || byte_len(cell) < width
                    {
                        1
                    } else {
                        0
                    };
                    count[a + b] += 1;
                }
                let mut align = 0;
                let population = count[1] + count[2] + count[3];
                for x in 1..=3 {
                    if count[x] * 2 > population {
                        align = x;
                        break;
                    }
                }
                align
            })
            .collect();

        let mut new_rows: Vec<String> = Vec::new();
        for row in rows.iter() {
            let mut out = String::new();
            out.push_str(&self.get_tag("tr", TAG_START, ""));
            for (col, &sc) in starts.iter().enumerate() {
                let width = ends[col] - sc;
                let mut cell = byte_slice(row, sc, width).to_string();
                cell = cell.trim_matches(' ').to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                let inside = if self.opts.xhtml {
                    match align_idx[col] {
                        2 => " style=\"text-align: right;\"".to_string(),
                        3 => " style=\"text-align: center;\"".to_string(),
                        _ => String::new(),
                    }
                } else if self.opts.lower_case_tags {
                    match align_idx[col] {
                        2 => " align=\"right\"".to_string(),
                        3 => " align=\"center\"".to_string(),
                        _ => String::new(),
                    }
                } else {
                    match align_idx[col] {
                        2 => " ALIGN=\"RIGHT\"".to_string(),
                        3 => " ALIGN=\"CENTER\"".to_string(),
                        _ => String::new(),
                    }
                };
                let tag = self.get_tag("td", TAG_START, &inside);
                let tag2 = self.close_tag("td");
                out.push_str(&format!("{tag}{cell}{tag2}"));
            }
            out.push_str(&self.close_tag("tr"));
            new_rows.push(out);
        }

        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, "")
        };
        new_rows[0] = format!("{tag}\n{}", new_rows[0]);
        let tag = self.close_tag("table");
        let last = new_rows.last_mut().unwrap();
        last.push_str(&format!("\n{tag}"));
        *rows = new_rows;
        true
    }

    fn make_pgsql_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        // split the heading row on \s+|\s+
        let head_row = rows.remove(0);
        let hre = links::ascii_re_cached(r"\s+\|\s+");
        let headings: Vec<String> = hre
            .split(&head_row)
            .filter_map(|f| {
                let f = f.ok()?.trim().to_string();
                if f.is_empty() {
                    None
                } else {
                    Some(f)
                }
            })
            .collect();
        // skip the ----+--- line
        if !rows.is_empty() {
            rows.remove(0);
        }
        // grab the N rows line
        let n_rows = rows.pop().unwrap_or_default();
        // build the table
        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        let mut thead = String::new();
        thead.push_str(&self.get_tag("thead", TAG_START, ""));
        thead.push_str(&self.get_tag("tr", TAG_START, ""));
        for col in headings {
            let mut col = col.trim().to_string();
            let _ = &mut col;
            let tag1 = self.get_tag("th", TAG_START, "");
            let tag2 = self.close_tag("th");
            thead.push_str(&format!("{tag1}{col}{tag2}"));
        }
        thead.push_str(&self.close_tag("tr"));
        thead.push_str(&self.close_tag("thead"));
        tab_lines.push(format!("{thead}\n"));
        tab_lines.push(format!("{}\n", self.get_tag("tbody", TAG_START, "")));

        for row in &rows {
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split('|') {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.close_tag("tbody")));
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));

        // and add the N rows line
        let ptag = self.get_tag("p", TAG_START, "");
        tab_lines.push(format!("{ptag}{n_rows}\n"));
        if self.opts.xhtml {
            let ptag2 = self.get_tag("p", TAG_END, "");
            let last = tab_lines.last_mut().unwrap();
            if let Some(pos) = last.find('\n') {
                last.insert_str(pos, &ptag2);
            }
        }
        *out = tab_lines;
        true
    }

    fn make_border_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        // skip the +----+---+ line
        rows.remove(0);
        // get the head row and cut off the start and end |
        let mut head_row = rows.remove(0);
        head_row = head_row.trim_start_matches(|c| c == ' ' || c == '\t').to_string();
        if head_row.starts_with('|') {
            head_row.remove(0);
        }
        if head_row.ends_with('|') {
            head_row.pop();
        }
        let hre = links::ascii_re_cached(r"\s+\|\s+");
        let headings: Vec<String> = hre
            .split(&head_row)
            .filter_map(|f| {
                let f = f.ok()?.trim().to_string();
                if f.is_empty() {
                    None
                } else {
                    Some(f)
                }
            })
            .collect();
        // skip the +----+---+ line
        rows.remove(0);
        // skip the last +----+---+ line
        rows.pop();

        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        let mut thead = String::new();
        thead.push_str(&self.get_tag("thead", TAG_START, ""));
        thead.push_str(&self.get_tag("tr", TAG_START, ""));
        for col in headings {
            let col = col.trim().to_string();
            let tag1 = self.get_tag("th", TAG_START, "");
            let tag2 = self.close_tag("th");
            thead.push_str(&format!("{tag1}{col}{tag2}"));
        }
        thead.push_str(&self.close_tag("tr"));
        thead.push_str(&self.close_tag("thead"));
        tab_lines.push(format!("{thead}\n"));
        tab_lines.push(format!("{}\n", self.get_tag("tbody", TAG_START, "")));
        for row in &rows {
            let mut row = row.trim_end_matches('\n').to_string();
            // cut off the start and end |
            row = row.trim_start_matches(|c| c == ' ' || c == '\t').to_string();
            if row.starts_with('|') {
                row.remove(0);
            }
            if row.ends_with('|') {
                row.pop();
            }
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split('|') {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.close_tag("tbody")));
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));
        *out = tab_lines;
        true
    }

    fn make_delim_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        let delim = {
            let re = links::ascii_re_cached(r"^\s*([^A-Za-z0-9])");
            re.captures(&rows[0])
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().chars().next().unwrap())
        };
        let Some(delim) = delim else { return false };

        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        for row in &rows {
            let row = row.trim_end_matches('\n');
            let mut row: Vec<char> = row.chars().collect();
            // cut off leading whitespace and one leading delimiter
            while let Some(&c) = row.first() {
                if c == ' ' || c == '\t' {
                    row.remove(0);
                } else {
                    break;
                }
            }
            if row.first() == Some(&delim) {
                row.remove(0);
            }
            if row.last() == Some(&delim) {
                row.pop();
            }
            let row: String = row.iter().collect();
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split(delim) {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));
        *out = tab_lines;
        true
    }

    fn tablestuff(&mut self, table_type: u32, rows: &mut Vec<String>, para_len: usize) -> bool {
        match table_type {
            TAB_ALIGN => self.make_aligned_table(rows, para_len),
            TAB_PGSQL => self.make_pgsql_table(rows),
            TAB_BORDER => self.make_border_table(rows),
            TAB_DELIM => self.make_delim_table(rows),
            _ => false,
        }
    }

    fn get_table_type(&self, rows: &[String], para_len: usize) -> u32 {
        if self.opts.table_type.delim && is_delim_table(rows) {
            TAB_DELIM
        } else if self.opts.table_type.align && Self::is_aligned_table(rows, para_len)
        {
            TAB_ALIGN
        } else if self.opts.table_type.pgsql && is_pgsql_table(rows) {
            TAB_PGSQL
        } else if self.opts.table_type.border && is_border_table(rows) {
            TAB_BORDER
        } else {
            0
        }
    }

    // ------------------------------------------------------- preformat

    fn is_preformatted(&mut self, line: &str) -> bool {
        let n = self.opts.preformat_whitespace_min;
        let re1 = format!(r"\s{{{n},}}\S+");
        let re2 = format!(r"\.{{{n},}}\S+");
        self.re(&re1).is_match(line).unwrap_or(false)
            || self.re(&re2).is_match(line).unwrap_or(false)
    }

    fn split_end_explicit_preformat(&mut self, para: &mut String) -> String {
        let mut pre_str = String::new();
        if self.mode & PRE_EXPLICIT != 0 {
            let pe_mark = self.opts.preformat_end_marker.clone();
            let re = self.re_i(&pe_mark);
            if re.is_match(para).unwrap_or(false) {
                // split on first match
                let m = re.find(para).ok().flatten().unwrap();
                let pre = para[..m.start()].to_string();
                *para = para[m.end()..].to_string();
                pre_str = if self.opts.escape_html_chars {
                    chars::escape(&pre)
                } else {
                    pre
                };
                let tag = self.close_tag("pre");
                pre_str.push_str(&format!("{tag}\n"));
                self.mode ^= (PRE | PRE_EXPLICIT) & self.mode;
            } else {
                pre_str = if self.opts.escape_html_chars {
                    chars::escape(para)
                } else {
                    para.clone()
                };
                *para = String::new();
            }
        }
        pre_str
    }

    fn endpreformat(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        ind: usize,
        prev: &mut String,
    ) {
        if self.mode & PRE_EXPLICIT != 0 {
            let pe_mark = self.opts.preformat_end_marker.clone();
            let re = self.re_i(&pe_mark);
            if re.is_match(&lines[ind]).unwrap_or(false) {
                if ind == 0 {
                    let tag = self.close_tag("pre");
                    lines[ind] = format!("{tag}\n");
                } else {
                    let tag = self.close_tag("pre");
                    prev.push_str(&format!("{tag}\n"));
                    lines[ind] = String::new();
                }
                self.mode ^= (PRE | PRE_EXPLICIT) & self.mode;
                actions[ind] |= END;
            }
            return;
        }

        let cond = !self.is_preformatted(&lines[ind])
            && (self.opts.endpreformat_trigger_lines == 1
                || (ind + 1 < lines.len()
                    && !self.is_preformatted(&lines[ind + 1]))
                || ind + 1 >= lines.len());
        if cond {
            if ind == 0 {
                let tag = self.close_tag("pre");
                *prev = format!("{tag}\n");
            } else {
                let tag = self.close_tag("pre");
                prev.push_str(&format!("{tag}\n"));
            }
            self.mode ^= PRE & self.mode;
            actions[ind] |= END;
        }
    }

    fn preformat(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        next: Option<&str>,
    ) {
        if self.opts.use_preformat_marker {
            let pstart = self.opts.preformat_start_marker.clone();
            let re = self.re_i(&pstart);
            if re.is_match(&lines[ind]).unwrap_or(false) {
                if prev.ends_with("<p>") {
                    prev.truncate(prev.len() - 3);
                    self.tags.pop();
                }
                let tag = self.get_tag("pre", TAG_START, " class='quote_explicit'");
                lines[ind] = format!("{tag}\n");
                self.mode |= PRE | PRE_EXPLICIT;
                actions[ind] |= PRE;
                return;
            }
        }

        if *prev_action & MAILQUOTE == 0
            && actions[ind] & MAILQUOTE == 0
            && (self.opts.preformat_trigger_lines == 0
                || (self.is_preformatted(&lines[ind])
                    && (self.opts.preformat_trigger_lines == 1
                        || (next.is_some()
                            && self.is_preformatted(next.unwrap())))))
        {
            if prev.ends_with("<p>") {
                prev.truncate(prev.len() - 3);
                self.tags.pop();
            }
            let tag = self.get_tag("pre", TAG_START, "");
            lines[ind] = format!("{tag}\n{}", lines[ind]);
            self.mode |= PRE;
            actions[ind] |= PRE;
        }
    }

    // ------------------------------------------------------- headings/anchors

    fn make_new_anchor(&mut self, heading_level: usize) -> String {
        if heading_level == 0 {
            let a = format!("{}", self.non_header_anchor);
            self.non_header_anchor += 1;
            return a;
        }
        let mut anchor = String::from("section");
        if self.heading_count.len() < heading_level {
            self.heading_count.resize(heading_level, 0);
        }
        self.heading_count[heading_level - 1] += 1;
        for i in (heading_level..self.heading_count.len()).rev() {
            self.heading_count[i] = 0;
        }
        for i in 0..heading_level {
            if self.heading_count[i] == 0 {
                self.heading_count[i] = 1;
            }
            anchor.push_str(&format!("_{}", self.heading_count[i]));
        }
        anchor
    }

    fn anchor_mail(&mut self, line_ref: &mut String) {
        if self.opts.make_anchors {
            let anchor = self.make_new_anchor(0);
            // s/([^ ]*)/<a name="$anchor">$1<\/a>/   (first non-space run)
            let re = links::ascii_re_cached(r"[^ ]*");
            if let Some(m) = re.find(&*line_ref).ok().flatten() {
                let inner = m.as_str().to_string();
                let rep = if self.opts.lower_case_tags {
                    format!("<a name=\"{anchor}\">{inner}</a>")
                } else {
                    format!("<A NAME=\"{anchor}\">{inner}</A>")
                };
                line_ref.replace_range(m.start()..m.end(), &rep);
            }
        }
    }

    fn anchor_heading(&mut self, level: usize, line_ref: &mut String) {
        if self.opts.make_anchors {
            let anchor = self.make_new_anchor(level);
            if self.opts.lower_case_tags {
                let re = links::ascii_re_cached(r"(<h\d>)(.*)(</h\d>)");
                *line_ref = re
                    .replace(
                        &*line_ref,
                        format!("$1<a name=\"{anchor}\">$2</a>$3"),
                    )
                    .to_string();
            } else {
                let re = links::ascii_re_cached(r"(<H\d>)(.*)(</H\d>)");
                *line_ref = re
                    .replace(
                        &*line_ref,
                        format!("$1<A NAME=\"{anchor}\">$2</A>$3"),
                    )
                    .to_string();
            }
        }
    }

    fn is_ul_list_line(&mut self, line: &str) -> bool {
        let (prefix, number, _, _) = self.listprefix(line);
        !prefix.is_empty() && number.is_empty()
    }

    fn heading_level(&mut self, style: &str) -> usize {
        if !self.heading_styles.contains_key(style) {
            self.num_heading_styles += 1;
            self.heading_styles.insert(style.to_string(), self.num_heading_styles);
        }
        self.heading_styles[style]
    }

    fn is_heading(&mut self, line: &str, next: Option<&str>) -> bool {
        if line.trim().is_empty() {
            return false;
        }
        if self.is_ul_list_line(line) {
            return false;
        }
        let Some(next) = next else { return false };
        let re = links::ascii_re_cached(r"^\s*[-=*.~+]+\s*$");
        if !re.is_match(next).unwrap_or(false) {
            return false;
        }
        let (hoffset, heading) = match links::ascii_re_cached(r"^(\s*)(.+)$")
            .captures(line)
            .ok()
            .flatten()
        {
            Some(c) => (
                c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            ),
            None => return false,
        };
        let heading_c = links::ascii_re_cached(r"&[^;]+;")
            .replace_all(&heading, "X")
            .to_string();
        let (uoffset, underline) = match links::ascii_re_cached(r"^(\s*)(\S+)\s*$")
            .captures(next)
            .ok()
            .flatten()
        {
            Some(c) => (
                c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            ),
            None => return false,
        };
        let lendiff = (heading_c.chars().count() as i64 - underline.chars().count() as i64).abs();
        let offsetdiff = (hoffset.chars().count() as i64 - uoffset.chars().count() as i64).abs();
        let _ = (heading, offsetdiff);
        if (lendiff as usize <= self.opts.underline_length_tolerance)
            || (offsetdiff as usize <= self.opts.underline_offset_tolerance)
        {
            return true;
        }
        false
    }

    fn heading(&mut self, line_ref: &mut String, next_ref: &mut String) {
        // ($uoffset, $underline) = ${$next_ref} =~ /^(\s*)(\S+)\s*$/
        let underline = {
            let re = links::ascii_re_cached(r"^(\s*)(\S+)\s*$");
            let mut u = String::new();
            if let Some(c) = re.captures(next_ref).ok().flatten() {
                if let Some(g) = c.get(2) {
                    u = g.as_str().to_string();
                }
            }
            u
        };
        let mut style = underline.chars().next().unwrap_or(' ').to_string();
        if self.iscaps(line_ref) {
            style.push('C');
        }
        *next_ref = " ".to_string();
        let level = self.heading_level(&style);
        if self.opts.escape_html_chars {
            *line_ref = chars::escape(line_ref);
        }
        self.tagline(&format!("H{level}"), line_ref);
        self.anchor_heading(level, line_ref);
    }

    fn is_custom_heading(&mut self, line: &str) -> bool {
        for reg in self.opts.custom_heading_regexp.clone() {
            let re = self.re(&reg);
            if re.is_match(line).unwrap_or(false) {
                return true;
            }
        }
        false
    }

    fn custom_heading(&mut self, line_ref: &mut String) {
        let mut i = 0;
        for reg in self.opts.custom_heading_regexp.clone() {
            let re = self.re(&reg);
            if re.is_match(&*line_ref).unwrap_or(false) {
                let level = if self.opts.explicit_headings {
                    i + 1
                } else {
                    self.heading_level(&format!("Cust{i}"))
                };
                if self.opts.escape_html_chars {
                    *line_ref = chars::escape(line_ref);
                }
                let tag_level = format!("H{level}");
                self.tagline(&tag_level, line_ref);
                self.anchor_heading(level, line_ref);
                return;
            }
            i += 1;
        }
    }

    fn unhyphenate_para(&mut self, para_ref: &mut String) {
        // s/(\s*)([^\W\d_]*)\-\n(\s*)([^\W\d_]+[\)\}\]\.,:;\'\"\>]*\s*)/$1$2$4\n$3/gs
        let re = self.re(r#"(\s*)([A-Za-z]*)-\n(\s*)([A-Za-z]+['").,:;}>]*\s*)"#);
        *para_ref = re
            .replace_all_captures(para_ref, |c| {
                let g = |i: usize| -> String {
                    c.get(i).map(|m| m.as_str().to_string()).unwrap_or_default()
                };
                format!("{}{}{}\n{}", g(1), g(2), g(4), g(3))
            });
    }

    fn tagline(&mut self, tag: &str, line_ref: &mut String) {
        // Perl chomp: drop a trailing newline (and CR), not any char
        if line_ref.ends_with('\n') {
            line_ref.pop();
        }
        if line_ref.ends_with('\r') {
            line_ref.pop();
        }
        let re = links::ascii_re_cached(r"^\s*(.*)$");
        let caps = re.captures(&*line_ref).ok().flatten().unwrap();
        let body = caps.get(1).unwrap().as_str().to_string();
        let tag1 = self.get_tag(tag, TAG_START, "");
        let tag2 = self.close_tag(tag);
        *line_ref = format!("{tag1}{body}{tag2}\n");
    }

    fn iscaps(&mut self, line: &str) -> bool {
        let min = self.opts.min_caps_length;
        let pat = format!(r"^[^a-z<]*[A-Z]{{{min},}}[^a-z<]*$");
        self.re(&pat).is_match(line).unwrap_or(false)
    }

    fn caps(&mut self, line_ref: &mut String, action: &mut u32) {
        if !self.opts.caps_tag.is_empty() && self.iscaps(line_ref) {
            let tag = self.opts.caps_tag.clone();
            self.tagline(&tag, line_ref);
            *action |= CAPS;
        }
    }

    // ------------------------------------------------------- inline markup

    fn do_delim(&mut self, line_ref: &mut String, _action: &mut u32, delim: &str, tag: &str) {
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        if delim == "#" {
            // `#x#` where `x` is a single letter and the surrounding positions
            // are not word boundaries (the `\B` assertions). Pattern kept free
            // of `\B`/lookaround so fancy-regex stays on its linear path;
            // the boundary condition is applied in code instead.
            let re1 = self.re(r"#([A-Za-z])#").clone();
            self.delim_replace(line_ref, &re1, non_boundary('#'), &ltag);
            // special treatment of # for the #num case and the #link case
            if line_ref.contains('#') {
                let re2 = self.re(r"#([^\d#][^#]*[^# \t\n])#").clone();
                if !line_ref.contains("<a") && !line_ref.contains("<A") {
                    self.delim_replace(line_ref, &re2, no_list_or_para_tag, &ltag);
                } else {
                    *line_ref = self.delim_loop(line_ref, &re2, no_list_or_para_tag, tag);
                }
            }
        } else if delim == "^" {
            let re1 = self.re(
                r#"\^((?![^^]*(?:<li>|<LI>|<p>|<P>))(\w|["'<>])[^^]*)\^"#,
            );
            if line_ref.contains('^') {
                *line_ref = re1
                    .replace_all_captures(line_ref, |c| {
                        ltag(c.get(1).unwrap().as_str())
                    });
            }
            let re2 = self.re(r"\^([A-Za-z])\^").clone();
            self.delim_replace(line_ref, &re2, non_boundary('^'), &ltag);
        } else if delim == "_" {
            let re1 = self.re(r"_([A-Za-z])_").clone();
            let r1m = self.delim_replace(line_ref, &re1, non_boundary('_'), &ltag);
            if r1m {
                let re2 = self.re(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#).clone();
                self.delim_replace(line_ref, &re2, not_preceded_by_word_or_underscore, &ltag);
            } else if line_ref.contains('_') {
                let re2 = self.re(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#).clone();
                *line_ref = self.delim_loop(line_ref, &re2, not_preceded_by_word_or_underscore, tag);
            }
        } else if delim.chars().count() == 1 {
            let db = class_body(delim);
            let dch = delim.chars().next().unwrap();
            let re1 = self.re(&format!(r"[{db}]([A-Za-z])[{db}]")).clone();
            self.delim_replace(line_ref, &re1, non_boundary(dch), &ltag);
            // `delim ... delim` where the content is one or more non-delimiter
            // characters ending in a "word/punctuation" character. The Perl
            // pattern's leading `(?<!delim)` is applied in code (`accept`)
            // so the regex itself stays free of lookaround (linear).
            let re2 = self.re(&format!(
                r"[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
            )).clone();
            self.delim_replace(line_ref, &re2, not_preceded_by(dch), &ltag);
        } else {
            // Perl interpolates `${delim}` straight into these patterns, so a
            // delimiter holding a regex metacharacter (`**` being the
            // obvious one) makes Perl itself raise "Quantifier follows
            // nothing in regex" and drop the substitution. The port used to
            // panic on the same input; escaping makes the pattern mean what
            // it obviously meant. Identical output for any delimiter Perl
            // could actually compile.
            //
            // The `(?<!...)` on the first one is a fixed-width literal
            // assertion, so it moves into `accept` like the others: left in
            // the regex it is a lookbehind, and a paragraph carrying a
            // multi-character delimiter run put it through the backtracking
            // VM for tens of seconds.
            let d = fancy_regex::escape(delim);
            let re1 = self
                .re(&format!(
                    r#"{d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
                ))
                .clone();
            if line_ref.contains(delim) {
                self.delim_replace(line_ref, &re1, not_preceded_by_str(d.to_string()), &ltag);
            }
            let re2 = self.re(&format!(r"{d}\]([A-Za-z]){d}"));
            *line_ref = re2.replace_all_captures(line_ref, |c| {
                ltag(c.get(1).unwrap().as_str())
            });
        }
    }

    /// Perl-style `s///g` for a *linear* regex (one with no lookaround, `\B`
    /// or backrefs, so the regex crate stays on its non-backtracking path).
    ///
    /// `captures_from_pos` locates each candidate match; `accept` then applies
    /// whatever condition the original regex expressed with a lookbehind/`\B`
    /// (which fancy-regex would otherwise run through its exploding
    /// backtracking VM). Rejected candidates are kept verbatim and scanning
    /// continues just past them, mirroring how Perl's engine advances.
    ///
    /// Returns whether anything was replaced.
    fn delim_replace(
        &self,
        line_ref: &mut String,
        re: &Regex,
        accept: impl Fn(&str, usize, usize) -> bool,
        replace: impl Fn(&str) -> String,
    ) -> bool {
        let text = std::mem::take(line_ref);
        let mut out = String::with_capacity(text.len());
        // `last` is how much of `text` has already been appended to `out`;
        // `pos` is where the next candidate is searched for.  Rejected
        // candidates are left in place (advancing `pos` just past their first
        // character) so their bytes are flushed unmodified by a later accept
        // or by the final tail.
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
    fn delim_loop(
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
                        // starting *inside* this one can still match. Skipping
                        // the whole span loses it -- which is how a `#` run
                        // spanning a `</p><p>` boundary used to swallow the
                        // pair that followed. The skipped character is
                        // ordinary text and is emitted verbatim.
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

    fn apply_links(&mut self, para_ref: &mut String, para_action: &mut u32) {
        if self.opts.make_links && !self.links.rules.is_empty() {
            self.links.check_dictionary_links(para_ref);
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

    // ------------------------------------------------------- main pipeline

    pub fn process_chunk(&mut self, chunk: &str, close_tags: bool, is_fragment: bool) -> String {
        // Perl `split(/\r?\n\r?\n/, $chunk)` semantics
        let para_strs = split_blank_lines(chunk);
        let mut ret = String::new();
        if para_strs.is_empty() {
            // splitting an empty string yields no elements in Perl
        } else if para_strs.len() == 1 {
            ret += &self.process_para(chunk, close_tags, is_fragment);
        } else {
            let last = para_strs.len() - 1;
            for (i, p) in para_strs.iter().enumerate() {
                let mut p = p.clone();
                if !p.ends_with('\n') {
                    p.push('\n');
                }
                let ct = i == last && close_tags;
                ret += &self.process_para(&p, ct, false);
            }
        }
        ret
    }

    pub fn process_para(&mut self, para: &str, close_tags: bool, is_fragment: bool) -> String {
        let mut para = para.to_string();
        let mut para_action = NONE;

        // tables and mailheaders don't carry over from one para to the next
        if self.mode & TABLE != 0 {
            self.mode ^= TABLE;
        }
        if self.mode & MAILHEADER != 0 {
            self.mode ^= MAILHEADER;
        }

        if self.opts.demoronize {
            chars::demoronize_char(&mut para);
        }

        if !self.opts.link_only {
            // Chop trailing whitespace and DOS CRs
            para = chop_trailing_cr(&para);
            // Chop leading whitespace and DOS CRs
            para = chop_leading_cr(&para);
            para = para.replace('\r', ""); // remove any stray carriage returns
            let para_len = para.chars().count();
            let mut done_lines: Vec<String> = Vec::new();

            // PRE_EXPLICIT may carry over
            if self.mode & PRE_EXPLICIT != 0 {
                let pre_str = self.split_end_explicit_preformat(&mut para);
                if !pre_str.is_empty() {
                    done_lines.push(pre_str);
                }
            }

            // The explicit-preformat continuation above swallows the whole
            // paragraph when the end marker is absent: everything left is
            // verbatim preformatted text, and there is nothing for the
            // paragraph machinery below to do.  `para = done_lines.join("")`
            // lives *inside* that block, so without this the buffered lines
            // were dropped on the floor and only the tags survived:
            //
            //   printf '<pre>\n\nX' | txt2html --use_preformat_marker
            //   reference  <pre class='quote_explicit'>\nX\n</pre>
            //   port       <pre class='quote_explicit'>\n\n</pre>     (before)
            //
            // Everything after the first blank line of an explicit quote was
            // lost, since a blank line is what ends a paragraph.
            //
            // This is not a `return`: the continuation text still has to reach
            // the tail of this function, because the reference runs every
            // paragraph through apply_links (TextToHTML.pm:1375) and then the
            // demoronize/entities passes.  Returning early left the
            // bold/italic/underline delimiters unprocessed inside an explicit
            // quote -- "*d*" stayed literal where the reference emitted
            // "<em>d</em>".
            let pre_continuation = para.is_empty();
            if pre_continuation {
                para = done_lines.join("");
                // The same trailing-newline chop the main path applies to a
                // continuing PRE, so the two agree on where paragraphs end
                // inside an explicit quote.
                if self.mode & (LIST | PRE) != 0 {
                    while para.ends_with('\n') {
                        para.pop();
                    }
                }
            }

            if !pre_continuation && !para.is_empty() {
                // split into lines and compute indent/len
                let mut para_lines = split_lines(&para);
                if para_lines.is_empty() {
                    para_lines.push(String::new());
                }
                let mut para_line_len: Vec<usize> = Vec::new();
                let mut para_line_indent: Vec<usize> = Vec::new();
                let mut para_line_action: Vec<u32> = Vec::new();
                let mut i = 0;
                for line in &mut para_lines {
                    // tabs -> spaces
                    while let Some(tab) = line.find('\t') {
                        let tw = self.opts.tab_width;
                        let spaces = " ".repeat(
                            tw - ((tab) % tw),
                        );
                        line.replace_range(tab..tab + 1, &spaces);
                    }
                    para_line_len.push(line.chars().count());
                    if line.trim().is_empty() {
                        para_line_indent.push(if i == 0 {
                            0
                        } else {
                            para_line_indent[i - 1]
                        });
                    } else {
                        let ws = line.chars().take_while(|c| *c == ' ').count();
                        para_line_indent.push(ws);
                    }
                    para_line_action.push(NONE);
                    i += 1;
                }

                // structural detection
                let mut is_table = false;
                let mut table_type = 0;
                let mut is_mailheader_para = false;
                let mut is_header = false;
                let mut is_custom_header = false;
                if !self.opts.custom_heading_regexp.is_empty() {
                    is_custom_header = self.is_custom_heading(&para_lines[0]);
                }
                if self.opts.make_tables && para_lines.len() > 1 {
                    table_type = self.get_table_type(&para_lines, para_len);
                    is_table = table_type != 0;
                }
                if !self.opts.explicit_headings && para_lines.len() > 1 && !is_table {
                    is_header = self.is_heading(
                        &para_lines[0],
                        Some(&para_lines[1]),
                    );
                }
                if self.opts.mailmode && !is_table && !is_custom_header {
                    is_mailheader_para = self.is_mailheader(&para_lines);
                }

                // end the list if we can end it
                if self.mode & LIST != 0
                    && (is_table || is_mailheader_para || is_header || is_custom_header)
                {
                    let mut list_end = String::new();
                    let mut action = 0;
                    let nl = self.listnum;
                    self.endlist(nl, &mut list_end, &mut action);
                    done_lines.push(list_end);
                    self.prev_para_action |= END;
                }

                // end the PRE if we can end it
                if self.mode & PRE != 0
                    && self.mode & PRE_EXPLICIT == 0
                    && (is_table || is_mailheader_para || !self.is_preformatted(&para_lines[0]))
                    && self.opts.preformat_trigger_lines != 0
                {
                    let tag = self.close_tag("pre");
                    let pre_end = format!("{tag}\n");
                    self.mode ^= PRE & self.mode;
                    done_lines.push(pre_end);
                    self.prev_para_action |= END;
                }

                // keep blank lines inside preformatted text
                if self.mode & PRE != 0 {
                    done_lines.push("\n".to_string());
                }

                // start-of-para structures that eat lines
                if is_custom_header {
                    let header = para_lines.remove(0);
                    para_line_len.remove(0);
                    para_line_indent.remove(0);
                    para_line_action.remove(0);
                    let mut header = header;
                    self.custom_heading(&mut header);
                    done_lines.push(header);
                    self.prev_para_action |= HEADER;
                } else if is_header {
                    let header = para_lines.remove(0);
                    para_line_len.remove(0);
                    para_line_indent.remove(0);
                    para_line_action.remove(0);
                    let underline = para_lines.remove(0);
                    para_line_len.remove(0);
                    para_line_indent.remove(0);
                    para_line_action.remove(0);
                    let mut header = header;
                    let mut underline = underline;
                    self.heading(&mut header, &mut underline);
                    done_lines.push(header);
                    self.prev_para_action |= HEADER;
                }

                // tables
                if self.opts.make_tables && is_table {
                    if self.tablestuff(table_type, &mut para_lines, para_len) {
                        done_lines.append(&mut para_lines);
                    }
                }

                // mailheader
                if is_mailheader_para && self.mode & TABLE == 0 && !para_lines.is_empty() {
                    let mut rows = para_lines.clone();
                    self.mailheader(&mut rows);
                    done_lines.extend(rows);
                    para_lines.clear();
                }

                // process line-by-line
                let mut prev = String::new();
                let mut prev_action = self.prev_para_action;
                let mut idx = 0;
                while idx < para_lines.len() {
                    if idx > 0 {
                        prev = para_lines[idx - 1].clone();
                        prev_action = para_line_action[idx - 1];
                    }
                    self.process_line(
                        &mut para_lines,
                        &mut para_line_action,
                        &mut para_line_indent,
                        &para_line_len,
                        idx,
                        &mut prev,
                        &mut prev_action,
                        is_fragment,
                    );
                    if idx == 0 {
                        if !prev.trim().is_empty() {
                            para_lines[0] = format!("{prev}{}", para_lines[0]);
                        }
                    } else {
                        para_lines[idx - 1] = prev.clone();
                        para_line_action[idx - 1] = prev_action;
                    }
                    idx += 1;
                }
                let last_action = para_line_action.last().copied().unwrap_or(NONE);
                para_action = last_action;
                para_line_action.clear();

                done_lines.extend(para_lines);

                // now put the para back together
                para = done_lines.join("");

                // XHTML: close an open paragraph
                if self.opts.xhtml {
                    if self.tags.last().map(|t| t == "p").unwrap_or(false) {
                        para.push_str(&self.close_tag("p"));
                    }
                }

                if self.opts.unhyphenation
                    && self
                        .re(r"[A-Za-z]\-\n\s*[A-Za-z]")
                        .is_match(&para)
                        .unwrap_or(false)
                    && (self.mode & (PRE | HEADER | MAILHEADER | TABLE | BREAK)) == 0
                {
                    self.unhyphenate_para(&mut para);
                }
                // chop trailing newlines for continuing lists and PRE
                if self.mode & LIST != 0 || self.mode & PRE != 0 {
                    while para.ends_with('\n') {
                        para.pop();
                    }
                }
            }
        }

        // apply links and bold/italic/underline formatting
        if !para.trim().is_empty() {
            self.apply_links(&mut para, &mut para_action);
        }

        if close_tags && self.mode & LIST != 0 {
            let nl = self.listnum;
            self.endlist(nl, &mut para, &mut para_action);
        }
        if close_tags && self.opts.xhtml {
            while !self.tags.is_empty() {
                para.push_str(&self.close_tag(""));
            }
        }

        // remaining Microsoft character codes -> HTML
        if self.opts.demoronize && !self.opts.eight_bit_clean {
            para = chars::demoronize_code(&para);
        }
        if !self.opts.eight_bit_clean {
            para = chars::entities(&para);
        }

        self.prev_para_action = para_action;
        para
    }

    fn process_line(
        &mut self,
        lines: &mut Vec<String>,
        actions: &mut Vec<u32>,
        indents: &mut Vec<usize>,
        line_lens: &[usize],
        i: usize,
        prev: &mut String,
        prev_action: &mut u32,
        is_fragment: bool,
    ) {
        if self.opts.escape_html_chars {
            lines[i] = chars::escape(&lines[i]);
        }

        let next = if i + 1 < lines.len() {
            Some(lines[i + 1].clone())
        } else {
            None
        };

        if self.opts.mailmode && self.mode & PRE_EXPLICIT == 0 {
            self.mailquote(lines, actions, i, prev, prev_action, next.as_deref());
        }

        if self.mode & PRE != 0 && self.opts.preformat_trigger_lines != 0 {
            self.endpreformat(lines, actions, i, prev);
        }

        if self.mode & PRE == 0 {
            self.hrule(lines, actions, i);
        }
        if self.mode & PRE == 0 && !lines[i].trim().is_empty() {
            self.liststuff(lines, actions, indents, i, prev, prev_action);
        }
        if actions[i] & (HEADER | LIST) == 0
            && self.mode & (LIST | PRE) == 0
            && self.preformat_enabled
        {
            self.preformat(
                lines,
                actions,
                i,
                prev,
                prev_action,
                next.as_deref(),
            );
        }
        if self.mode & PRE == 0 {
            let line_indent = indents[i];
            let prev_indent = if i == 0 { 0 } else { indents[i - 1] };
            self.paragraph(
                lines,
                actions,
                indents,
                i,
                prev,
                prev_action,
                line_indent,
                prev_indent,
                is_fragment,
                i,
            );
        }
        if self.mode & (PRE | LIST) == 0 {
            let prev_line_len = if i == 0 {
                0
            } else {
                line_lens[i - 1]
            };
            self.shortline(lines, actions, i, prev, prev_action, prev_line_len);
        }
        if self.mode & PRE == 0 {
            self.caps(&mut lines[i], &mut actions[i]);
        }
    }

    // ------------------------------------------------------- file start / top-level

    pub fn do_file_start(&mut self, para: &str) -> String {
        let mut out = String::new();
        if !self.opts.extract {
            // split(/\n/, $para), not str::lines(): lines() also strips the \r
            // of a CRLF pair, so a CRLF file would lose it from --titlefirst
            // where the reference keeps it (TextToHTML.pm:5143).
            let first_line = para.split('\n').next().unwrap_or("").to_string();

            if !self.opts.doctype.is_empty() {
                if self.opts.xhtml {
                    out.push_str("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\"\n");
                    out.push_str("\"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\">\n");
                    out.push_str(&self.get_tag(
                        "html",
                        TAG_START,
                        " xmlns=\"http://www.w3.org/1999/xhtml\"",
                    ));
                    out.push('\n');
                } else {
                    out.push_str("<!DOCTYPE HTML PUBLIC \"");
                    out.push_str(&self.opts.doctype);
                    out.push_str("\">\n");
                    out.push_str(&self.get_tag("html", TAG_START, ""));
                    out.push('\n');
                }
            }
            out.push_str(&self.get_tag("head", TAG_START, ""));
            out.push('\n');

            if self.opts.titlefirst && self.opts.title.is_empty() {
                // ($tit) = $first_line =~ /^ *(.*)/
                let tit = first_line.trim_start_matches(' ').to_string();
                let tit = tit.trim_end_matches(' ').to_string();
                if self.opts.escape_html_chars {
                    self.opts.title = chars::escape(&tit);
                } else {
                    self.opts.title = tit;
                }
            }
            if self.opts.title.is_empty() {
                self.opts.title = String::new();
            }
            out.push_str(&self.get_tag("title", TAG_START, ""));
            out.push_str(&self.opts.title);
            out.push_str(&self.close_tag("title"));
            out.push('\n');

            if self.opts.append_head_available() {
                if let Some(contents) = read_any_file(&self.opts.append_head) {
                    out.push_str(&contents);
                }
            }

            if self.opts.lower_case_tags {
                out.push_str(&self.get_tag(
                    "meta",
                    TAG_EMPTY,
                    &format!(" name=\"generator\" content=\"{PROG} v{VERSION}\""),
                ));
            } else {
                out.push_str(&self.get_tag(
                    "meta",
                    TAG_EMPTY,
                    &format!(" NAME=\"generator\" CONTENT=\"{PROG} v{VERSION}\""),
                ));
            }
            out.push('\n');
            if !self.opts.style_url.is_empty() {
                let style_url = self.opts.style_url.clone();
                if self.opts.lower_case_tags {
                    out.push_str(&self.get_tag(
                        "link",
                        TAG_EMPTY,
                        &format!(
                            " rel=\"stylesheet\" type=\"text/css\" href=\"{style_url}\""
                        ),
                    ));
                } else {
                    out.push_str(&self.get_tag(
                        "link",
                        TAG_EMPTY,
                        &format!(
                            " REL=\"stylesheet\" TYPE=\"text/css\" HREF=\"{style_url}\""
                        ),
                    ));
                }
                out.push('\n');
            }
            out.push_str(&self.close_tag("head"));
            out.push('\n');
            let body_deco = self.opts.body_deco.clone();
            if !body_deco.is_empty() {
                out.push_str(&self.get_tag("body", TAG_START, &body_deco));
            } else {
                out.push_str(&self.get_tag("body", TAG_START, ""));
            }
            out.push('\n');
        }

        if !self.opts.prepend_file.is_empty() {
            if let Some(contents) = read_any_file(&self.opts.prepend_file) {
                out.push_str(&contents);
            }
        }
        out
    }

    /// Convert the whole input (could already be pre-split paragraphs)
    /// through the full `txt2html` pipeline.
    pub fn txt2html(&mut self) -> String {
        let mut sources: Vec<String> = Vec::new();
        let source_type;
        if !self.opts.infile.is_empty() {
            source_type = "file".to_string();
            for f in &self.opts.infile {
                if f == "-" {
                    // stdin provided by caller
                    let mut buf = String::new();
                    use std::io::Read;
                    let _ = std::io::stdin().read_to_string(&mut buf);
                    sources.push(buf);
                } else {
                    match read_any_file(f) {
                        Some(c) => sources.push(c),
                        None => {
                            eprintln!("Could not open {f}\n");
                            continue;
                        }
                    }
                }
            }
        } else if !self.opts.instring.is_empty() {
            source_type = "string".to_string();
            sources = self.opts.instring.clone();
        } else {
            return String::new();
        }

        self.convert_sources(sources, source_type == "string")
    }

    /// Convert text held in memory exactly as if it had been read from a
    /// file: paragraph records, the file-start document header, the
    /// append/prepend files and so on. This is what the Python bindings use.
    pub fn convert_text(&mut self, text: &str) -> String {
        self.convert_sources(vec![text.to_string()], false)
    }

    fn convert_sources(&mut self, sources: Vec<String>, string_mode: bool) -> String {
        let mut out = String::new();
        let mut count = 0;
        for source in &sources {
            if string_mode {
                // each instring element is treated as one paragraph
                let mut para = source.clone();
                if let Some(stripped) = para.strip_suffix('\n') {
                    para = stripped.to_string();
                }
                if count == 0 {
                    out.push_str(&self.do_file_start(&para));
                }
                self.links.sect_once_done = vec![false; self.links.rules.len()];
                let p = self.process_chunk(&para, false, false);
                out.push_str(&p);
                out.push('\n');
                self.print_count += 1;
                count += 1;
                continue;
            }

            // paragraph mode ($/ = "") over LF text; then process_chunk
            for rec in paragraph_records(source) {
                let mut para = rec;
                if let Some(stripped) = para.strip_suffix('\n') {
                    para = stripped.to_string();
                }
                if count == 0 {
                    out.push_str(&self.do_file_start(&para));
                }
                self.links.sect_once_done = vec![false; self.links.rules.len()];
                let p = self.process_chunk(&para, false, false);
                out.push_str(&p);
                out.push('\n');
                self.print_count += 1;
                count += 1;
            }
        }

        if self.mode & LIST != 0 {
            let nl = self.listnum;
            self.endlist(nl, &mut out, &mut 0);
        }
        if self.mode & PRE != 0 {
            let tag = self.close_tag("pre");
            out.push_str(&tag);
        }
        if self.opts.xhtml && !self.opts.extract && !self.tags.is_empty() {
            let mut open_tag = self
                .tags
                .last()
                .cloned()
                .unwrap_or_default();
            while !self.tags.is_empty()
                && open_tag != "body"
                && open_tag != "html"
            {
                out.push_str(&self.close_tag(""));
                open_tag = self.tags.last().cloned().unwrap_or_default();
            }
            out.push('\n');
        }
        if !self.opts.append_file.is_empty() {
            if let Some(contents) = read_any_file(&self.opts.append_file) {
                out.push_str(&contents);
            }
        }
        if self.print_count > 0 && !self.opts.extract {
            out.push_str(&self.close_tag("body"));
            out.push('\n');
            out.push_str(&self.close_tag("html"));
            out.push('\n');
        }
        out
    }
}

impl Options {
    fn append_head_available(&self) -> bool {
        !self.append_head.is_empty()
    }
}

/// The condition the `\B` assertions in `\B delim ([A-Za-z]) delim \B`
/// place on the characters flanking a match, expressed in code.
///
/// A `\B` position is not a word boundary, i.e. the two sides agree on
/// word-ness. One side is always the delimiter itself, so the neighbouring
/// character must be a word character exactly when the delimiter is; `_` is
/// the delimiter in common use that is a word character, which is why the
/// test asserts `_a_` is *not* turned into markup while ` #a# ` is.
///
/// The neighbours are examined a byte at a time, which is what Perl's `\b`
/// does on the bytes it was given: every byte of a multi-byte character is
/// `>= 0x80` and so is not a word byte. The delimiter's own word-ness has to
/// come from the character, though -- `delim as u8` truncates `é` to `0xE9`,
/// which is not a byte that appears in its UTF-8 encoding.
fn non_boundary(delim: char) -> impl Fn(&str, usize, usize) -> bool {
    let d_word = delim_is_word(delim);
    move |t: &str, s: usize, e: usize| {
        let left_word = s > 0 && is_word_byte(t.as_bytes()[s - 1]);
        let right_word = e < t.len() && is_word_byte(t.as_bytes()[e]);
        left_word == d_word && right_word == d_word
    }
}

/// `\w` (word char) on a **byte**, which is what Perl's `\b` sees when it is
/// handed a byte string: ASCII alphanumeric or underscore. Every byte of a
/// multi-byte character is `>= 0x80` and so is not a word byte, which is why
/// this is the right test for a *neighbour* even when the text is UTF-8.
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The same question about a delimiter, which has to be asked of the
/// character rather than a byte: `d as u8` truncates `é` to `0xE9`, and
/// `0xE9` is not a byte that appears in `é`'s own UTF-8 encoding. No
/// non-ASCII character is ever a word character.
fn delim_is_word(delim: char) -> bool {
    delim.is_ascii() && is_word_byte(delim as u8)
}

/// `(?<![delim])` from the single-character general pattern: the delimiter
/// must not be the character immediately before the match.
///
/// The check is on the preceding *character*. `delim as u8` truncates the code
/// point -- `é` becomes `0xE9`, which is not a byte of its own UTF-8 encoding
/// -- so a byte-wise test never rejects anything, and `ééwordé` came out
/// marked up where Perl leaves it alone.
fn not_preceded_by(delim: char) -> impl Fn(&str, usize, usize) -> bool {
    move |t: &str, s: usize, _e: usize| t[..s].chars().next_back() != Some(delim)
}

/// `(?<![_A-Za-z0-9])` from the underscore pattern.
fn not_preceded_by_word_or_underscore(t: &str, s: usize, _e: usize) -> bool {
    s == 0 || !(t.as_bytes()[s - 1] == b'_' || is_word_byte(t.as_bytes()[s - 1]))
}

/// `(?<!delim)` from the multi-character pattern, where the delimiter is a
/// literal string and so may be more than one byte long.
fn not_preceded_by_str(delim: String) -> impl Fn(&str, usize, usize) -> bool {
    let d = delim.into_bytes();
    move |t: &str, s: usize, _e: usize| -> bool { s < d.len() || t.as_bytes()[s - d.len()..s] != *d }
}

/// The bold pattern's `(?![^#]*(?:<li>|<LI>|<P>|<p>))`, applied in code.
///
/// It only ever inspects the match's own interior: every character of the
/// group is `[^#]`, so the `[^#]*` inside the assertion cannot reach past the
/// closing `#` -- it starts one character in, after the group's leading
/// `[^\d#]`, and stops at that `#`.
///
/// Byte-wise on purpose. The tags are ASCII and no UTF-8 continuation byte can
/// be part of one, so this is exactly equivalent -- and unlike slicing a `str`
/// it cannot panic when `s + 2` is not a character boundary, which it need not
/// be in a paragraph of 8-bit characters.
fn no_list_or_para_tag(t: &str, s: usize, e: usize) -> bool {
    none_of(
        t.as_bytes(),
        s + 2,
        e - 1,
        &[b"<li>", b"<LI>", b"<P>", b"<p>"],
    )
}

/// UTF-8 length in bytes of the character starting at `byte_pos` in `text`.
fn char_len(text: &str, byte_pos: usize) -> usize {
    match text[byte_pos..].chars().next() {
        Some(c) => c.len_utf8(),
        None => 1,
    }
}

/// Whether none of `needles` occurs in `hay[from..to]`.
///
/// Byte-wise, so `from`/`to` may fall inside a multi-byte character (which
/// is what a regex match position can do inside a run of 8-bit text) without
/// panicking the way slicing a `str` would. Every caller passes ASCII
/// needles, which no UTF-8 continuation byte can be part of, so the answer
/// is the same as a `str` search.
fn none_of(hay: &[u8], from: usize, to: usize, needles: &[&[u8]]) -> bool {
    let window = &hay[from..to];
    !needles.iter().any(|n| window.windows(n.len()).any(|w| w == *n))
}

fn class_body(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            ']' => out.push_str("\\]"),
            '\\' => out.push_str("\\\\"),
            '^' => out.push_str("\\^"),
            '-' => out.push_str("\\-"),
            '[' => out.push_str("\\["),
            _ => out.push(c),
        }
    }
    out
}

/// Compile a (fragment) table regexp with ASCII semantics and dotall.
fn table_re(pat: &str) -> Regex {
    let translated = links::translate_pattern(pat);
    Regex::new(&format!("(?s){translated}")).unwrap()
}

fn is_pgsql_table(rows: &[String]) -> bool {
    // A PGSQL table must have at least 4 rows
    if rows.len() < 4 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    // possible caption
    if !r[0].contains('|') {
        if table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false) {
            r.remove(0);
        }
    }
    if r.len() < 4 {
        return false;
    }
    if !table_re(r"^\s*\w+\s+\|\s+").is_match(r[0]).unwrap_or(false) {
        return false; // Colname |
    }
    if !table_re(r"^\s*[-]+[+][-]+").is_match(r[1]).unwrap_or(false) {
        return false; // ----+----
    }
    if !table_re(r"^\s*[^|]*\s+\|\s+").is_match(r[2]).unwrap_or(false) {
        return false; // value |
    }
    if !table_re(r"\(\d+\s+rows\)").is_match(r[r.len() - 1]).unwrap_or(false) {
        return false; // (N rows)
    }
    true
}

fn is_border_table(rows: &[String]) -> bool {
    // A BORDER table must have at least 5 rows
    if rows.len() < 5 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    if !r[0].contains('|') {
        if table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false) {
            r.remove(0);
        }
    }
    if r.len() < 5 {
        return false;
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$").is_match(r[0]).unwrap_or(false) {
        return false; // +----+----+
    }
    if !table_re(r"^\s*\|\s*\w+\s+\|\s+.*\|$").is_match(r[1]).unwrap_or(false) {
        return false; // | Colname |
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$").is_match(r[2]).unwrap_or(false) {
        return false; // +----+----+
    }
    if !table_re(r"^\s*\|\s*[^|]*\s+\|\s+.*\|$").is_match(r[3]).unwrap_or(false) {
        return false; // | value |
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$")
        .is_match(r[r.len() - 1])
        .unwrap_or(false)
    {
        return false; // +----+----+
    }
    true
}

fn is_delim_table(rows: &[String]) -> bool {
    if rows.len() < 2 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    // possible caption
    if !table_re(r"[^\w\s]").is_match(r[0]).unwrap_or(false)
        && table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false)
    {
        r.remove(0);
    }
    if r.len() < 2 {
        return false;
    }
    // find a possible delimiter
    let delim = if let Some(c) = table_re(r"^\s*([^A-Za-z0-9])")
        .captures(r[0])
        .ok()
        .flatten()
        .and_then(|c| c.get(1))
    {
        // get rid of ^ and [] and \
        let d = c.as_str().replace(['^', '[', ']', '\\'], "");
        if d.is_empty() {
            return false; // no delimiter after all
        }
        d
    } else {
        return false;
    };
    // There needs to be at least three delimiters in the row
    let dc = links::ascii_re_cached(&format!("[{}]", &delim));
    let total_num_delims = dc.find_iter(r[0]).flatten().count();
    if total_num_delims < 3 {
        return false;
    }
    // All rows must start and end with the delimiter
    // and have $total_num_delims number of them
    let re_start = table_re(&format!(r"^\s*[{delim}]"));
    let re_end = table_re(&format!(r"[{delim}]\s*$"));
    for row in &r {
        if !re_start.is_match(row).unwrap_or(false) {
            return false;
        }
        if !re_end.is_match(row).unwrap_or(false) {
            return false;
        }
        if dc.find_iter(row).flatten().count() != total_num_delims {
            return false;
        }
    }
    true
}

fn strip_leading_spaces(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut removed = 0;
    let mut idx = 0;
    while idx < chars.len() && removed < n {
        if chars[idx] == ' ' {
            removed += 1;
            idx += 1;
        } else {
            break;
        }
    }
    chars[idx..].iter().collect()
}

fn byte_slice(s: &str, start: usize, len: usize) -> &str {
    let bytes = s.as_bytes();
    let end = (start + len).min(bytes.len());
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
}

fn byte_len(s: &str) -> usize {
    s.len()
}

/// Split a string into paragraphs the way Perl's paragraph mode (`$/ = ""`)
/// does for LF text: on runs of blank (empty) lines.
fn paragraph_records(s: &str) -> Vec<String> {
    // Perl `$/ = ""` paragraph slurp mode: a record ends at the first
    // blank (empty) line, that line's newline included.  Blank lines
    // before a record are skipped, repeated blank lines are discarded,
    // and a trailing blank run yields no record.  Whitespace-only lines
    // are not blank.
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in s.split_inclusive('\n') {
        if cur.is_empty() {
            if line == "\n" {
                continue;
            }
            cur.push_str(line);
            continue;
        }
        cur.push_str(line);
        if line == "\n" {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Does `\r?\n\r?\n` match starting at index `i`?
fn blank_sep_at(chars: &[char], i: usize) -> bool {
    let n = chars.len();
    let mut j = i;
    if j < n && chars[j] == '\r' {
        j += 1;
    }
    if j >= n || chars[j] != '\n' {
        return false;
    }
    j += 1;
    if j < n && chars[j] == '\r' {
        j += 1;
    }
    if j >= n || chars[j] != '\n' {
        return false;
    }
    true
}

/// Perl `split(/\r?\n\r?\n/, $s)`: keep leading and middle empty fields,
/// drop trailing empty fields.
fn split_blank_lines(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < n {
        if blank_sep_at(&chars, i) {
            out.push(chars[start..i].iter().collect());
            let mut j = i;
            if chars[j] == '\r' {
                j += 1;
            }
            if chars[j] == '\n' {
                j += 1;
            }
            if j < n && chars[j] == '\r' {
                j += 1;
            }
            if j < n && chars[j] == '\n' {
                j += 1;
            }
            i = j;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < n {
        out.push(chars[start..].iter().collect());
    }
    out
}

// ---------------------------------------------------------------- helper extension

trait ReplAll {
    fn replace_all_captures(
        &self,
        text: &str,
        f: impl Fn(&fancy_regex::Captures) -> String,
    ) -> String;
}

impl ReplAll for Regex {
    fn replace_all_captures(
        &self,
        text: &str,
        f: impl Fn(&fancy_regex::Captures) -> String,
    ) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in self.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&f(&m));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }
}
#[cfg(test)]
mod cr_chop_tests {
    use super::{chop_leading_cr, chop_trailing_cr};

    /// The patterns the two helpers replaced, compiled exactly as the engine
    /// compiled them.
    ///
    /// They have to go through `ascii_re_cached`, not `Regex::new`, because
    /// that is where Perl's `$` is restored: `$` here is rewritten to the
    /// lookahead `(?=\n?$)` so it matches before a trailing newline as Perl's
    /// does. A bare fancy-regex `$` is end-of-text only, so compiling the
    /// pattern directly would have made this test assert the wrong thing --
    /// it did, until the exhaustive pass caught it.
    fn re_trailing() -> &'static fancy_regex::Regex {
        crate::links::ascii_re_cached(r"[ \t]*\x0D$")
    }
    fn re_leading() -> &'static fancy_regex::Regex {
        crate::links::ascii_re_cached(r"^[ \t]*\x0D")
    }

    /// Exhaustive over the alphabet that matters here. The helpers are string
    /// surgery standing in for a regex, and the failure mode is a silent
    /// difference on one awkward input, so this checks all of them rather than
    /// a handful of examples someone thought of.
    #[test]
    fn matches_the_regex_it_replaced() {
        let alphabet = ['a', ' ', '\t', '\r', '\n'];
        let mut inputs: Vec<String> = vec![String::new()];
        // every string of length 0..=4 over the alphabet
        let mut level = inputs.clone();
        for _ in 0..4 {
            let mut next = Vec::new();
            for s in &level {
                for &c in &alphabet {
                    let mut t = s.clone();
                    t.push(c);
                    next.push(t);
                }
            }
            inputs.extend(next.iter().cloned());
            level = next;
        }
        assert_eq!(inputs.len(), 1 + 5 + 25 + 125 + 625);
        for s in &inputs {
            let want_t = re_trailing().replace(s, "").to_string();
            let want_l = re_leading().replace(s, "").to_string();
            assert_eq!(chop_trailing_cr(s), want_t, "trailing, input {s:?}");
            assert_eq!(chop_leading_cr(s), want_l, "leading, input {s:?}");
        }
    }

    #[test]
    fn trailing_chop_handles_crlf() {
        // `$` also matches before one trailing newline, so a DOS line ending
        // has to be recognised. A plain `ends_with('\r')` would miss these.
        assert_eq!(chop_trailing_cr("abc \r"), "abc");
        assert_eq!(chop_trailing_cr("abc \r\n"), "abc\n");
        assert_eq!(chop_trailing_cr("abc\r\n"), "abc\n");
        assert_eq!(chop_trailing_cr("\r"), "");
        assert_eq!(chop_trailing_cr("\r\n"), "\n");
        // no match: left exactly as it was
        assert_eq!(chop_trailing_cr("abc"), "abc");
        assert_eq!(chop_trailing_cr("abc\n"), "abc\n");
        assert_eq!(chop_trailing_cr(" abc "), " abc ");
    }

    #[test]
    fn leading_chop_needs_a_cr() {
        // Trimming leading whitespace unconditionally would destroy the
        // indentation of every paragraph in the document.
        assert_eq!(chop_leading_cr("  \tabc"), "  \tabc");
        assert_eq!(chop_leading_cr("  abc"), "  abc");
        assert_eq!(chop_leading_cr("  \rabc"), "abc");
        assert_eq!(chop_leading_cr("  \t\rabc"), "abc");
        assert_eq!(chop_leading_cr("\rabc"), "abc");
        assert_eq!(chop_leading_cr("  \r"), "");
    }
}

/// Differential test for the linear (`delim_replace`) delimiter substitution.
///
/// The `\B` and `(?<!...)` variants these replace drove fancy-regex's
/// backtracking VM, which explodes (or errors) on paragraphs of ~500 KB --
/// see the `do_delim` comment. The linear form must agree with the original
/// regexes on every input, so this compares them directly. The original
/// patterns run fine on the short strings used here; the explosion only
/// shows up at scale.
#[cfg(test)]
mod delim_linear_tests {
    use super::*;
    use crate::links::ascii_re_cached;
    use crate::options::Options;

    fn conv() -> Converter {
        let mut opts = Options::default();
        opts.deal_with_options();
        Converter::new(opts)
    }

    /// The original `replace_all_captures`: iterate matches with
    /// `captures_iter`, i.e. what the engine used to do everywhere.
    fn orig_replace(re: &fancy_regex::Regex, text: &str, tag: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in re.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&format!("<{tag}>{}</{tag}>", m.get(1).unwrap().as_str()));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }

    pub(crate) fn check_pair(c: &mut Converter, text: &str, delim: char, tag: &str) {
        let db = class_body(&delim.to_string());
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");

        // single alpha char between delims, wrapped in \B assertions
        let re1_orig = ascii_re_cached(&format!(r"\B[{db}]([A-Za-z])[{db}]\B"));
        let re1_lin = ascii_re_cached(&format!(r"[{db}]([A-Za-z])[{db}]"));
        let expect = orig_replace(re1_orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, re1_lin, non_boundary(delim), &ltag);
        assert_eq!(got, expect, "re1 mismatch for {text:?} delim={delim}");

        // multi-char content ending in a "word/punct" char, leading
        // (?<!delim)
        let re2_orig = ascii_re_cached(&format!(
            r"(?<![{db}])[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
        ));
        let re2_lin = ascii_re_cached(&format!(
            r"[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
        ));
        let expect = orig_replace(re2_orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, re2_lin, not_preceded_by(delim), &ltag);
        assert_eq!(got, expect, "re2 mismatch for {text:?} delim={delim}");
    }

    #[test]
    fn linear_delim_matches_original_regexes() {
        let cases = [
            "", "a", "*", "**", "***", "*a*", "x*a*", "*a*y*", " *a* ", "**a*",
            "*a**", "*a*b*", "*a b*", "*a!*", "a*b*c*", "*ab*", "*  *", "*\t*",
            "*a*b*c*", "*a**b*", "*a*b**c*", "#a#", "x#a#", "#ab#", "**#a#",
            "#a**", "#a#b#", "_a_", "x_a_", "_a_b_", "__a_", "_ab_", "_a!_",
            "^a^", "x^a^", "^a^b^", "^^a^", "^a^^", "a*b_#c^d", "*a**", "3#4#",
            "_", "#", "^", "*", "a", " a ", "1*2*3", "*a*#b#_c_^d^",
            // A non-ASCII delimiter immediately before another one. The
            // assertion is `(?<!é)`, and the byte the code point truncates to
            // -- 0xE9 for `é` -- is not a byte that `é` actually contains, so
            // a byte-wise check rejects nothing and the assertion is
            // vacuous. `ééwordé` is the shape that came back from a customer
            // file marked up as emphasis where Perl leaves it alone.
            "ééwordé", "üüxü", "ééé", "üxüyü", "xééyéé", "ééwördé", "éé X üéü",
        ];
        let mut c = conv();
        for text in cases {
            for &(delim, tag) in DELIMS {
                check_pair(&mut c, text, delim, tag);
            }
        }
    }

    /// The delimiters exercised everywhere below. The last two are not ASCII,
    /// and they are here because the byte-oriented shortcut `delim as u8` is
    /// silently wrong for them: `é` truncates to `0xE9`, which is not a byte of
    /// its own UTF-8 encoding, so a preceding-delimiter test built on it never
    /// rejects anything and `ééwordé` got marked up where Perl leaves it.
    pub(crate) const DELIMS: &[(char, &str)] = &[
        ('*', "em"),
        ('#', "strong"),
        ('_', "u"),
        ('^', "em"),
        ('é', "em"),
        ('ü', "strong"),
    ];

    /// Randomized differential: the alphabet is the union of delimiter chars
    /// and word/punct characters, so the generator keeps producing the shapes
    /// the boundary and lookbehind conditions care about (adjacent delimiters,
    /// single-char content, delimiter right after a word char, ...).
    #[test]
    fn linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', '9', ' ', '\t', '\n', '!', '.', '-', '_', '#',
            '*', '^', '<', '>', '&', '"', '/', '=', '@', '`', '{', '}', '~',
        ];
        let mut c = conv();
        let mut state = 0x243F_6A88_85A3_08D3u64;
        let mut next = || {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 24) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            for &(delim, tag) in DELIMS {
                check_pair(&mut c, &text, delim, tag);
            }
        }
    }
}

/// Same differential check for the two remaining `do_delim` rewrites: the
/// bold `#...#` pattern and the underscore pattern, both of which lost a
/// lookaround to `delim_replace`'s `accept` callback.
#[cfg(test)]
mod delim_wide_linear_tests {
    use super::delim_linear_tests::check_pair;
    use super::*;
    use crate::links::ascii_re_cached;
    use crate::options::Options;

    fn conv() -> Converter {
        let mut opts = Options::default();
        opts.deal_with_options();
        Converter::new(opts)
    }

    fn orig_replace(re: &fancy_regex::Regex, text: &str, tag: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in re.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&format!("<{tag}>{}</{tag}>", m.get(1).unwrap().as_str()));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }

    fn check_bold(c: &mut Converter, text: &str) {
        let tag = "strong";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let orig = ascii_re_cached(r"#([^\d#](?![^#]*(?:<li>|<LI>|<P>|<p>))[^#]*[^# \t\n])#");
        let lin = ascii_re_cached(r"#([^\d#][^#]*[^# \t\n])#");
        let no_tag = no_list_or_para_tag;
        let expect = orig_replace(orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, lin, no_tag, &ltag);
        assert_eq!(got, expect, "bold re2 mismatch for {text:?}");
    }

    fn check_under(c: &mut Converter, text: &str) {
        let tag = "u";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let orig = ascii_re_cached(r#"(?<![_A-Za-z0-9])_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#);
        let lin = ascii_re_cached(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#);
        let not_word = not_preceded_by_word_or_underscore;
        let expect = orig_replace(orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, lin, not_word, &ltag);
        assert_eq!(got, expect, "under re2 mismatch for {text:?}");
    }

    #[test]
    fn wide_linear_delim_matches_original_regexes() {
        let cases = [
            "", "#", "##", "###", "#a#", "x#a#", "#a#b#", "#ab cd#", "#1a#", "#a1#",
            "#<p>x#", "#x<p>y#", "#<li>#", "#<LI>x#", "#<P>#", "# a#", "#a #", "#a\t#",
            "#a\nb#", "##a##", "#a# #b#", "#<a href=\"#\">#", "_", "__", "_a_", "x_a_",
            "_a_b_", "__a_", "_ab cd_", "_a_ _b_", "_a b _", "_a!_", "_a b_", "9_a_",
            "a_b_", "_a_b_", "#x#_y_", "_#x#_", "#_a_#", "***", "#a#b#_c_", "# #",
            "#<p><p>#", "#<li> <P> <p> <LI>#", "#\t#", "#\n#", "#a# #", "#5#", "# a b #",
        ];
        let mut c = conv();
        for text in cases {
            check_bold(&mut c, text);
            check_under(&mut c, text);
        }
    }

    /// The multi-character delimiter branch, whose leading `(?<!delim)` also
    /// moved into `accept`. `d` is a metachar-free multi-character delimiter
    /// so the original pattern compiles the way it does for every delimiter
    /// Perl can actually handle.
    fn check_multi(c: &mut Converter, text: &str, delim: &str) {
        let tag = "em";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let d = fancy_regex::escape(delim);
        let orig = ascii_re_cached(&format!(
            r#"(?<!{d}){d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
        ));
        let lin = ascii_re_cached(&format!(
            r#"{d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
        ));
        let expect = orig_replace(orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, lin, not_preceded_by_str(d.to_string()), &ltag);
        assert_eq!(got, expect, "multi mismatch for {text:?} delim={delim}");
    }

    /// `delim_loop` has to behave like the regex it replaces when a candidate
    /// is rejected: the engine retries one character along, so a match that
    /// starts *inside* the rejected span must still be found. Regression test
    /// for the `#`/`</p><p>` case the P3 fuzzer found -- the first `#` pair
    /// spans a paragraph boundary and is rejected, and the pair nested in it
    /// has to be picked up on the retry.
    #[test]
    fn wide_delim_retry_finds_match_inside_rejected_span() {
        let cases = [
            "#a</p><p>b#c#",
            "#a<p>b#c#",
            "#<li>x#y#",
            "#<P>x#y#",
            "#<p>#y#",
            "pre #a</p><p>b# mid #c#",
            "#a# #<p>b#c#",
            "_a</p><p>b_c_d_",
            "#<p>only#",
            "#<p>#",
            "##<p>x#y#",
            "#no close at all",
            "_a</p><p>b_c_d_e_",
        ];
        let mut c = conv();
        for text in cases {
            // bold path, including the in-link-context variant that selects
            // `delim_loop` in the first place
            let re = c.re(r"#([^\d#][^#]*[^# \t\n])#").clone();
            let no_tag = no_list_or_para_tag;
            let expect = orig_replace(
                ascii_re_cached(r"#([^\d#](?![^#]*(?:<li>|<LI>|<P>|<p>))[^#]*[^# \t\n])#"),
                text,
                "strong",
            );
            let got = c.delim_loop(&mut text.to_string(), &re, no_tag, "strong");
            assert_eq!(got, expect, "delim_loop bold mismatch for {text:?}");
        }
    }

    /// 8-bit characters: a regex match position can land in the middle of a
    /// multi-byte character, so any code that indexes a `str` by a match
    /// offset has to cope. Regression test for the panic
    /// "start byte index N is not a char boundary" that the bold rewrite hit
    /// on the reference's own `umlauttest` fixture.
    #[test]
    fn linear_delim_handles_multibyte_text() {
        let cases = [
            "ver\u{e4}ndern *ver\u{e4}ndern* zu",
            "#ver\u{e4}ndern# #\u{c4}NDERN# #a\u{fc}b#",
            "_geht_ _ver\u{e4}ndern_ _\u{c4}NDERN_",
            "#a\u{e4}b#<p>c#", "#a\u{e4}<p>b#", "#a\u{e4}<li>b#",
            "#a\u{20ac}b# #a\u{2192}b# #\u{1f600}#",
            "*\u{e4}* *\u{1f600}x*", "_\u{e4}_ _a\u{e4}b_",
            "\u{e4}#b#\u{e4}", "#\u{e4}b\u{e4}#", "#\u{e4}#",
            "@@\u{e4}b@@", "XY\u{e4}XY", "@@a\u{e4}@@", "XYa\u{e4}XY",
            "#\u{4e2d}\u{6587}#", "_\u{4e2d}\u{6587}_", "#a#\u{4e2d}#b#",
        ];
        let mut c = conv();
        for text in cases {
            check_bold(&mut c, text);
            check_under(&mut c, text);
            for delim in ["XY", "@@"] {
                check_multi(&mut c, text, delim);
            }
        }
    }

    /// Same, at random: multi-byte characters mixed with delimiters, so
    /// match offsets land off character boundaries often.
    #[test]
    fn linear_delim_handles_multibyte_random() {
        let alphabet: &[char] = &[
            'a', 'b', '0', ' ', '_', '#', '*', '^', 'X', 'Y', '@', '.', '\u{e4}',
            '\u{fc}', '\u{c4}', '\u{20ac}', '\u{4e2d}', '\u{2192}', '\n', '\t',
        ];
        // a delimiter is 0xC3 0xA9, which shares its trailing byte 0xA9 with
        // several other characters, so a byte-wise "is the previous character
        // the delimiter" test gets the wrong answer on exactly these inputs
        const NONASCII: &[(char, &str)] = &[('\u{e9}', "em"), ('\u{fc}', "strong")];
        let mut c = conv();
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 24) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            check_bold(&mut c, &text);
            check_under(&mut c, &text);
            for delim in ["XY", "@@"] {
                check_multi(&mut c, &text, delim);
            }
            for (delim, tag) in NONASCII {
                check_pair(&mut c, &text, *delim, tag);
            }
        }
    }

    #[test]
    fn multi_linear_delim_matches_original_regex() {
        let cases = [
            "", "XY", "XYXY", "XYwordXY", "aXYwordXYb", "XYword XY", "XYword ",
            "XYword", "XY w XY", "XYwordXYXYwordXY", "xXYwordXY", "XYwordXYx",
            "@@w@@", "w@@w@@", "a@@b@@", "@@ab@@", "@@a b@@", "@@@@", "@@a@@b@@",
            "XY''XY", "XY'a'bXY", "XY\"x\"XY", "XY\"XY", "**bold**", "XYaXYbXY",
            " XYwordXY ", "\nXYwordXY\n", "XY\tXY", "@@1@@", "@@a1@@", "@@a!@@",
        ];
        let mut c = conv();
        for text in cases {
            for delim in ["XY", "@@"] {
                check_multi(&mut c, text, delim);
            }
        }
    }

    #[test]
    fn multi_linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', ' ', '\t', '\n', '!', '.', '-', '@', 'X', 'Y',
            '*', '"', '\'', '_', '#', '^', '<', '>', '&', '/', '=', '`', '{',
        ];
        let mut c = conv();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 28) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            for delim in ["XY", "@@"] {
                check_multi(&mut c, &text, delim);
            }
        }
    }

    #[test]
    fn wide_linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', '9', ' ', '\t', '\n', '!', '.', '-', '_', '#',
            '*', '^', '<', '>', '&', '"', '/', '=', '@', '`', '{', '}', '~', '?',
            ';', ':', '\'',
        ];
        let mut c = conv();
        let mut state = 0xB502_6F5A_A966_19E9u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 28) as usize;
            let text: String = (0..len)
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect();
            check_bold(&mut c, &text);
            check_under(&mut c, &text);
        }
    }
}
