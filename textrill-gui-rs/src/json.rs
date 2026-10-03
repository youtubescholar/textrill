// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The small JSON subset the settings file needs.
//!
//! `mainwindow.py` stored the option set with `json.dumps`, so the settings
//! file already holds a flat JSON object. Rather than add a JSON dependency for
//! one string, this module reads and writes exactly that shape: an object whose
//! values are booleans, integers, strings, arrays of strings, or the
//! `{"ALIGN": true, ...}` object `table_type` uses. Anything else makes the
//! whole object unreadable, which mirrors Python's `except ValueError` on a
//! corrupt blob.

/// A value in the settings JSON subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Bool(bool),
    Number(i64),
    Str(String),
    Array(Vec<String>),
    /// `table_type`: named switches.
    Object(Vec<(String, bool)>),
}

/// Encode an object, in the same spacing `json.dumps` uses (`", "` and `": "`).
pub fn write_object(entries: &[(&str, Json)]) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        quote_into(&mut out, key);
        out.push_str(": ");
        write_value(&mut out, value);
    }
    out.push('}');
    out
}

fn write_value(out: &mut String, value: &Json) {
    match value {
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Number(n) => out.push_str(&n.to_string()),
        Json::Str(s) => quote_into(out, s),
        Json::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                quote_into(out, item);
            }
            out.push(']');
        }
        Json::Object(pairs) => {
            out.push('{');
            for (i, (key, on)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                quote_into(out, key);
                out.push_str(": ");
                out.push_str(if *on { "true" } else { "false" });
            }
            out.push('}');
        }
    }
}

/// Quote and escape a string. Non-ASCII is emitted as UTF-8, which both Python's
/// `json.loads` and this parser accept.
fn quote_into(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse a JSON object of the supported shapes. Returns `None` for anything
/// malformed, exactly as Python ignored a bad blob.
pub fn parse_object(text: &str) -> Option<Vec<(String, Json)>> {
    let chars: Vec<char> = text.trim().chars().collect();
    let mut parser = Parser { chars, pos: 0 };
    parser.skip_ws();
    parser.expect('{')?;
    let mut entries = Vec::new();
    parser.skip_ws();
    if parser.peek() == Some('}') {
        parser.pos += 1;
        return Some(entries);
    }
    loop {
        parser.skip_ws();
        let key = parser.string()?;
        parser.skip_ws();
        parser.expect(':')?;
        parser.skip_ws();
        let value = parser.value()?;
        entries.push((key, value));
        parser.skip_ws();
        match parser.next()? {
            ',' => continue,
            '}' => break,
            _ => return None,
        }
    }
    Some(entries)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, want: char) -> Option<()> {
        if self.next()? == want {
            Some(())
        } else {
            None
        }
    }

    fn value(&mut self) -> Option<Json> {
        match self.peek()? {
            '{' => self.object(),
            '[' => self.array(),
            '"' => Some(Json::Str(self.string()?)),
            't' => self.literal("true").map(|()| Json::Bool(true)),
            'f' => self.literal("false").map(|()| Json::Bool(false)),
            '-' | '0'..='9' => self.number().map(Json::Number),
            _ => None,
        }
    }

    fn literal(&mut self, word: &str) -> Option<()> {
        for want in word.chars() {
            if self.next()? != want {
                return None;
            }
        }
        Some(())
    }

    fn number(&mut self) -> Option<i64> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some('0'..='9')) {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse().ok()
    }

    fn array(&mut self) -> Option<Json> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Some(Json::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.string()?);
            self.skip_ws();
            match self.next()? {
                ',' => continue,
                ']' => break,
                _ => return None,
            }
        }
        Some(Json::Array(items))
    }

    fn object(&mut self) -> Option<Json> {
        self.expect('{')?;
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Some(Json::Object(pairs));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(':')?;
            self.skip_ws();
            let on = match self.peek()? {
                't' => {
                    self.literal("true")?;
                    true
                }
                'f' => {
                    self.literal("false")?;
                    false
                }
                _ => return None,
            };
            pairs.push((key, on));
            self.skip_ws();
            match self.next()? {
                ',' => continue,
                '}' => break,
                _ => return None,
            }
        }
        Some(Json::Object(pairs))
    }

    fn string(&mut self) -> Option<String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.next()? {
                '"' => return Some(out),
                '\\' => match self.next()? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{08}'),
                    'f' => out.push('\u{0c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => out.push(self.unicode_escape()?),
                    _ => return None,
                },
                c => out.push(c),
            }
        }
    }

    /// `\uXXXX`, joining a UTF-16 surrogate pair when one follows.
    fn unicode_escape(&mut self) -> Option<char> {
        let first = self.hex4()?;
        match first {
            0xD800..=0xDBFF => {
                let save = self.pos;
                if self.next() == Some('\\') && self.next() == Some('u') {
                    let second = self.hex4()?;
                    if (0xDC00..=0xDFFF).contains(&second) {
                        let code = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
                        return char::from_u32(code);
                    }
                }
                self.pos = save;
                None
            }
            0xDC00..=0xDFFF => None,
            _ => char::from_u32(first),
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..4 {
            value = value * 16 + self.next()?.to_digit(16)?;
        }
        Some(value)
    }
}
