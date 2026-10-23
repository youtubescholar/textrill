//! The textrill conversion engine.
//!
//! A faithful port of HTML::TextToHTML v3.0's `process_para`,
//! `process_chunk`, `txt2html`, `do_file_start` and the associated helper
//! subroutines.

use fancy_regex::Regex;
use std::collections::HashMap;

use crate::chars;
use crate::links::{self, LinkParser};
use crate::options::Options;

mod decode;
mod headings;
mod inline;
mod lists;
mod pages;
mod paragraph;
mod preformat;
mod stream;
mod tables;
mod tags;

#[cfg(test)]
mod tests;

pub use decode::*;

use self::decode::{chop_leading_cr, chop_trailing_cr};
use self::stream::split_blank_lines;

/// One or more inputs that could not be opened, and the output built from
/// whatever *was* readable. The caller writes that output and exits non-zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableInput {
    /// The input paths that could not be opened, in the order given.
    pub unreadable: Vec<String>,
    /// The document produced from the inputs that were readable; empty when none
    /// were.
    pub out: String,
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

// The generator meta names what produced the file. Deliberately not the Perl
// module's name -- credit for HTML::TextToHTML belongs in LICENSE -- and the
// version comes from the crate so bumping Cargo.toml cannot leave it stale.
const PROG: &str = "textrill";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn subtract_modes(v: u32, mask: u32) -> u32 {
    (v | mask) - mask
}

fn is_perl_space(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\x0c' || c == '\x0b'
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Number of leading whitespace characters, counting tabs as one each.
fn leading_spaces(s: &str) -> usize {
    s.chars().take_while(|c| is_perl_space(*c)).count()
}

/// Split into lines the way `split(/^/, $para)` does; each line keeps its
/// trailing newline except a final un-terminated one.
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
    /// Set by `try_convert`; read back with `resolved_encoding()`.
    resolved: Resolved,
    /// The text between `<head>` and `</head>`, for `{{textrill:head}}`.
    head_inner: String,
    /// The final, escaped document title, for `{{textrill:title}}`.
    title_text: String,
    /// The template file's contents, read once in [`Converter::new`]; empty if
    /// there is no template.
    template_text: String,
    /// Set when a note or glossary set is unusable, so the caller can report it
    /// and write nothing: a dangling `[1]` must not reach disk.
    pub notes_error: Option<String>,
    /// True when `template_text` is a whole-document template, not a wrapper.
    document_template: bool,
    /// The scheme policy, built once in [`Converter::new`] so the scrub does not
    /// rebuild it per paragraph; `links::load_links` is given this same value.
    url_policy: crate::urlscheme::UrlPolicy,
    /// Schemes already reported, so two hundred `javascript:` links warn once.
    dropped_schemes: Vec<String>,
}

impl Converter {
    pub fn new(mut opts: Options) -> Self {
        opts.deal_with_options();

        // An unreadable append/prepend/head file is reported once, at
        // construction, and dropped -- the reference does the same (`!-r`
        // check and clear), so a missing style or header cannot abort a
        // conversion, but it is also not silently absent from the output.
        for flag in ["append_file", "prepend_file", "append_head"] {
            let path = match flag {
                "append_file" => &opts.append_file,
                "prepend_file" => &opts.prepend_file,
                _ => &opts.append_head,
            };
            if !path.is_empty() && std::fs::File::open(path).is_err() {
                eprintln!("Can't find or read {path}");
                match flag {
                    "append_file" => opts.append_file.clear(),
                    "prepend_file" => opts.prepend_file.clear(),
                    _ => opts.append_head.clear(),
                }
            }
        }

        let preformat_enabled = opts.endpreformat_trigger_lines != 0 || opts.use_preformat_marker;

        let mut heading_styles = HashMap::new();
        let mut num_heading_styles = 0;
        if opts.use_mosaic_header {
            for s in ["*", "=", "+", "-", "~", "."] {
                num_heading_styles += 1;
                heading_styles.insert(s.to_string(), num_heading_styles);
            }
        }

        // Resolve the template once at construction so conversion cannot fail on
        // IO; a caller that skips validation gets no templating, not an error.
        let (template_text, document_template) = match opts.template_source() {
            Ok((2, _, _)) => (String::new(), false),
            Ok((whole, body, _)) => (body, whole == 1),
            Err(_) => (String::new(), false),
        };

        // One policy, shared with the dictionary loader, so a rule kept at
        // load and a paragraph scrubbed later can never disagree.
        let url_policy = opts.url_policy();
        let links = links::load_links(&opts);
        let number_match_default = if opts.bullets_ordered.is_empty() {
            r"(\d+|[A-Za-z_])".to_string()
        } else {
            format!(r"(\d+|[A-Za-z]|[{0}])", class_body(&opts.bullets_ordered))
        };

        Converter {
            notes_error: None,
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
            resolved: Resolved::Utf8,
            head_inner: String::new(),
            title_text: String::new(),
            template_text,
            url_policy,
            dropped_schemes: Vec::new(),
            document_template,
        }
    }

    /// The most compiled patterns [`Converter::re_cache`] will hold; mirrors
    /// `links::ascii_re_cached`'s 128.
    ///
    /// A document exercising every pattern-building construct compiles 19
    /// patterns, so 128 is ~6x the worst realistic case. Patterns are memoised
    /// per *pattern*, so the same input does not grow it.
    const RE_CACHE_MAX: usize = 128;

    /// Insert into the pattern cache, clearing it wholesale when full. The cache
    /// is a pure memo, so clearing cannot change the output; a compiled `Regex`
    /// is expensive to build and the working set is small, so this beats an LRU.
    fn cache_pattern(&mut self, key: &str, re: Regex) -> &Regex {
        if self.re_cache.len() >= Self::RE_CACHE_MAX {
            self.re_cache.clear();
        }
        self.re_cache.insert(key.to_string(), re);
        // Looked up rather than reached for: after a `clear()` this is the only
        // entry, but without one `values().next()` could return a different
        // pattern's Regex.
        self.re_cache.get(key).expect("just inserted")
    }

    fn re(&mut self, pat: &str) -> &Regex {
        let key = format!("(?s){pat}");
        if !self.re_cache.contains_key(&key) {
            // User patterns are compiled and reported by `Options::validate`
            // before conversion, so reaching here with a bad one means validation
            // was bypassed -- an internal bug, and a panic is the right response.
            let re = links::try_compile_pattern(pat, false)
                .unwrap_or_else(|e| panic!("bad regex {pat:?}: {e}"));
            return self.cache_pattern(&key, re);
        }
        self.re_cache.get(&key).unwrap()
    }

    fn re_i(&mut self, pat: &str) -> &Regex {
        let key = format!("(?s)(?i){pat}");
        if !self.re_cache.contains_key(&key) {
            let re = links::try_compile_pattern(pat, true)
                .unwrap_or_else(|e| panic!("bad regex {pat:?}: {e}"));
            return self.cache_pattern(&key, re);
        }
        self.re_cache.get(&key).unwrap()
    }

    // ------------------------------------------------------- tags

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
            // paragraph when the end marker is absent, so the buffered lines must
            // be joined back or they are dropped. Not a `return`: the text still
            // has to reach apply_links and the demoronize/entities passes at the
            // tail, or delimiters inside an explicit quote stay unprocessed.
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
                for (i, line) in para_lines.iter_mut().enumerate() {
                    // tabs -> spaces
                    while let Some(tab) = line.find('\t') {
                        let tw = self.opts.tab_width;
                        let spaces = " ".repeat(tw - ((tab) % tw));
                        line.replace_range(tab..tab + 1, &spaces);
                    }
                    para_line_len.push(line.chars().count());
                    if line.trim().is_empty() {
                        para_line_indent.push(if i == 0 { 0 } else { para_line_indent[i - 1] });
                    } else {
                        let ws = line.chars().take_while(|c| *c == ' ').count();
                        para_line_indent.push(ws);
                    }
                    para_line_action.push(NONE);
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
                    is_header = self.is_heading(&para_lines[0], Some(&para_lines[1]));
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
                if self.opts.make_tables
                    && is_table
                    && self.tablestuff(table_type, &mut para_lines, para_len)
                {
                    done_lines.append(&mut para_lines);
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

                // Close an open paragraph (XHTML, or HTML5 with the default
                // close_optional_tags).
                if self.closes_optional_tags()
                    && self.tags.last().map(|t| t == "p").unwrap_or(false)
                {
                    para.push_str(&self.close_tag("p"));
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
        if close_tags && self.closes_optional_tags() {
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

    #[allow(clippy::too_many_arguments)]
    fn process_line(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
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
            self.preformat(lines, actions, i, prev, prev_action, next.as_deref());
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
            let prev_line_len = if i == 0 { 0 } else { line_lens[i - 1] };
            self.shortline(lines, actions, i, prev, prev_action, prev_line_len);
        }
        if self.mode & PRE == 0 {
            self.caps(&mut lines[i], &mut actions[i]);
        }
    }

    /// Convert the whole input through the full pipeline. An unreadable input is
    /// returned to the caller rather than skipped: the reference prints to stderr
    /// and exits 0 with a 0-byte output, which a Makefile reads as success. Only
    /// the exit code differs; readable inputs are still converted.
    pub fn try_convert(&mut self) -> Result<String, UnreadableInput> {
        let mut sources: Vec<String> = Vec::new();
        let source_type;
        let mut unreadable: Vec<String> = Vec::new();
        // The most notable encoding seen across the inputs. UTF-8 is the floor:
        // an ASCII file is valid UTF-8 and must not be reported as a legacy
        // encoding just because another input in the run was.
        let mut worst = Resolved::Utf8;
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
                    match read_with(f, self.opts.encoding) {
                        Some((c, resolved)) => {
                            if resolved.notice_rank() > worst.notice_rank() {
                                worst = resolved;
                            }
                            sources.push(c);
                        }
                        None => {
                            eprintln!("Could not open {f}\n");
                            // Keep going rather than bailing out, so a
                            // multi-file invocation still converts the readable
                            // files; only the exit code changes.
                            unreadable.push(f.clone());
                        }
                    }
                }
            }
        } else if !self.opts.instring.is_empty() {
            source_type = "string".to_string();
            sources = self.opts.instring.clone();
        } else {
            return Ok(String::new());
        }

        self.resolved = worst;
        let out = self.convert_sources(sources, source_type == "string");
        if unreadable.is_empty() {
            Ok(out)
        } else {
            Err(UnreadableInput { unreadable, out })
        }
    }

    /// The encoding [`Converter::try_convert`] resolved the input with, for a
    /// status bar or GUI writing the text back out. Only file inputs carry an
    /// encoding; `instring` and `process_chunk` are already-decoded `str`s.
    pub fn resolved_encoding(&self) -> Resolved {
        self.resolved
    }

    /// As [`Converter::try_convert`], but discarding the unreadable-file error
    /// so output is always produced. This is the reference's behaviour, used by
    /// the Python bindings and the in-process tests.
    pub fn convert(&mut self) -> String {
        match self.try_convert() {
            Ok(out) => out,
            Err(e) => e.out,
        }
    }

    /// Convert text held in memory exactly as if it had been read from a file:
    /// paragraph records, the file-start document header, append/prepend files.
    pub fn convert_text(&mut self, text: &str) -> String {
        self.convert_sources(vec![text.to_string()], false)
    }

    fn convert_sources(&mut self, sources: Vec<String>, string_mode: bool) -> String {
        self.notes_error = None;
        let (start, body, tail) = self.convert_to_parts(sources, string_mode);
        // Number before sectioning so the TOC labels carry the numbers.
        let body = if self.opts.number_headings {
            crate::section::number_headings(&body)
        } else {
            body
        };
        // Sectioning is a pure post-pass over the body, so the reference path is
        // untouched when the flags are off. The TOC is kept separate so a
        // template can place it.
        let (toc, body) = if self.opts.section || self.opts.toc {
            crate::section::sectionize_parts(&body, self.opts.toc)
        } else {
            (String::new(), body)
        };
        // Collect the note sets over the finished body, so definition content
        // is rendered markup. On failure the body is left as it was and the
        // error recorded: a refused document produces no output.
        let (stripped, citations, glossary) = match self.collect_notes(&body) {
            Ok(n) => n,
            Err(e) => {
                self.notes_error = Some(e);
                (String::new(), String::new(), String::new())
            }
        };
        let body = if stripped.is_empty() { body } else { stripped };
        // A template replaces the default arrangement. `--body_template` wraps
        // the body inside the engine's own prolog and epilog; a
        // `--document_template` owns the whole page, so neither is emitted.
        if !self.template_text.is_empty() {
            return self.apply_template(&start, &body, &toc, &tail, &citations, &glossary);
        }
        let mut body = body;
        // Without a template there is nowhere to put the lists, so they go at
        // the end of the body -- after the prose, and inside `--extract`'s
        // output too.
        body.push_str(&citations);
        body.push_str(&glossary);
        // `--lang_runs` marks passages in another script. The declare-and-document
        // policy lives in `langdetect`; the pass is refused with `--stream`,
        // which cannot afford the document-wide scan. The templated path applies
        // the same wrap to the body slot.
        if self.opts.lang_runs {
            body = crate::langdetect::wrap_runs(&body, &self.opts.lang);
        }
        let header = self.default_document_header();
        let mut out =
            String::with_capacity(start.len() + header.len() + toc.len() + body.len() + tail.len());
        out.push_str(&start);
        out.push_str(&header);
        out.push_str(&toc);
        out.push_str(&body);
        out.push_str(&tail);
        out
    }
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
