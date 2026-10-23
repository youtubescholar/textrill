use super::decode::bom;
use super::*;
use crate::options::Encoding;
use fancy_regex::Regex;
use std::io::{self, BufRead, Write};

impl Converter {
    /// Convert `input` to `out` one paragraph at a time.
    ///
    /// All cross-paragraph state lives in `self`, so records in order produce
    /// exactly the bytes [`Converter::convert_text`] would while holding one
    /// paragraph at a time. The record boundary is the reference's `$/ = ""`
    /// paragraph mode (see [`ParagraphReader`]).
    ///
    /// Input must be valid UTF-8; a UTF-16/32 BOM or NUL structure is an
    /// [`io::ErrorKind::InvalidData`] error rather than a silent replacement
    /// `char`, since the buffered `Auto` path would have decoded it as CP1252.
    pub fn convert_stream<R: BufRead, W: Write>(
        &mut self,
        mut input: R,
        out: &mut W,
    ) -> io::Result<()> {
        // Refuse a wide encoding up front, before a byte is written: the NUL
        // structure that marks UTF-16 is decodable as UTF-8 (a NUL is U+0000),
        // so per-line validation alone would let a UTF-16 file through.
        let head = input.fill_buf()?;
        if let Some((encoding, _)) = bom(head) {
            if encoding != Encoding::Utf8 {
                return Err(not_utf8_error());
            }
        }
        if sniff_utf16_or_32(head).is_some() {
            return Err(not_utf8_error());
        }
        self.resolved = Resolved::Utf8;
        let mut reader = ParagraphReader::new(input);
        let mut rec = String::new();
        let mut first = true;
        while reader.next_record(&mut rec)? {
            let para = rec.strip_suffix('\n').unwrap_or(&rec);
            if first {
                let start = self.do_file_start(para);
                out.write_all(start.as_bytes())?;
                first = false;
            }
            self.links.sect_once_done = vec![false; self.links.rules.len()];
            let body = self.process_chunk(para, false, false);
            out.write_all(body.as_bytes())?;
            out.write_all(b"\n")?;
            self.print_count += 1;
        }
        let mut body = String::new();
        self.finish_body(&mut body);
        out.write_all(body.as_bytes())?;
        let tail = self.finish_tail();
        out.write_all(tail.as_bytes())?;
        Ok(())
    }
}

/// The error [`Converter::convert_stream`] returns for input that is not UTF-8.
pub(super) fn not_utf8_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "--stream needs UTF-8 input; use the buffered path for other encodings",
    )
}

/// Yields paragraph records from a reader the way [`paragraph_records`] does from
/// a string: a record ends at the first line that is exactly `"\n"`, and a
/// whitespace-only line is not blank. Each line is validated as UTF-8 on its own,
/// which is safe because a newline byte never occurs inside a UTF-8 sequence.
pub(super) struct ParagraphReader<R> {
    inner: R,
    line: Vec<u8>,
}

impl<R: BufRead> ParagraphReader<R> {
    pub(super) fn new(inner: R) -> Self {
        ParagraphReader {
            inner,
            line: Vec::new(),
        }
    }

    /// Read the next record into `out`, returning `false` at end of input.
    pub(super) fn next_record(&mut self, out: &mut String) -> io::Result<bool> {
        out.clear();
        loop {
            self.line.clear();
            if self.inner.read_until(b'\n', &mut self.line)? == 0 {
                return Ok(!out.is_empty());
            }
            let line = std::str::from_utf8(&self.line)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if out.is_empty() {
                if line == "\n" {
                    continue;
                }
                out.push_str(line);
                continue;
            }
            out.push_str(line);
            if line == "\n" {
                return Ok(true);
            }
        }
    }
}

pub(super) fn paragraph_records(s: &str) -> Vec<String> {
    // Perl `$/ = ""` paragraph slurp mode: a record ends at the first blank line,
    // that line's newline included; blank lines before a record are skipped and a
    // whitespace-only line is not blank.
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
pub(super) fn blank_sep_at(chars: &[char], i: usize) -> bool {
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
pub(super) fn split_blank_lines(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out: Vec<String> = Vec::new();
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
    // Perl's `split` drops *every* trailing empty field, not just a missing one,
    // so a string that is nothing but the separator yields no fields; without
    // this the field pushed by a separator at the very end would survive.
    while out.last().is_some_and(|f| f.is_empty()) {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------- helper extension

/// `fancy_regex::Regex::replace_all` and `replace` call `try_replacen(..).unwrap()`
/// internally, so they panic when the engine returns a runtime error -- which is
/// how the backtracking VM reports an exhausted step budget. Any pattern that
/// reaches the backtracker (one with a look-around, which this port also
/// generates from `\b` and `$`, or a back-reference) can therefore panic a
/// `replace_all`. This helper iterates `captures_iter` and drops the `Err`
/// results with `flatten`, so a meeting of a backtracker pattern and the budget
/// is treated as "no further match" rather than a crash. Use it, not
/// `replace_all`, for any pattern that is not provably linear.
pub(super) trait ReplAll {
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
