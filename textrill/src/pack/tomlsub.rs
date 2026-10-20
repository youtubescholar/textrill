//! The shared scanner for textrill's strict TOML subset (roadmap §5.2, §6).
//!
//! Two schemas are read with this subset — a pack's `template.toml`
//! (manifest.rs) and a catalog's `catalog.toml` (catalog.rs) — and both must
//! fail the same loud way: anything outside the subset is a hard error naming
//! the line, never a silent misread. The syntax machinery lives here so the
//! two readers cannot drift; each schema keeps its own keys and sections.
//!
//! The subset: bare `key = value` lines, `[[array.of.tables]]` headers, and
//! four value shapes — basic strings with escapes, integers, booleans, and
//! arrays of strings. A table `[x]`, an inline table `{…}`, a multi-line
//! string, a date, a dotted or quoted key, a float, an unknown key and a
//! duplicate key are all refused with `line N:` in front of the message.
//! The semantics (required fields, reverse-DNS ids, semver) belong to the
//! schema, not to this scanner. Covered by the tests in manifest.rs, which
//! pins every refusal shape this module exists to serve.

/// One parsed value, by the only four shapes the subset allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    Strs(Vec<String>),
}

/// Fail with a `line N:` prefix, the one diagnostic shape this reader emits.
pub(super) fn err<T>(n: usize, message: impl std::fmt::Display) -> Result<T, String> {
    Err(format!("line {n}: {message}"))
}

/// Reject anything that is not a bare key: the subset has no dotted keys
/// (`a.b = …`) and no quoted keys, so both are a hard error.
pub(super) fn check_key(key: &str, n: usize) -> Result<(), String> {
    if key.is_empty() {
        return err(n, "missing key before `=`");
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return err(
            n,
            format!(
                "`{key}` is not a bare key (the subset allows letters, digits, \
                 `-` and `_` only; dotted and quoted keys are not in it)"
            ),
        );
    }
    Ok(())
}

/// Recognise a `[[…]]` header (its leading `[[` already stripped) and return
/// the section name, refusing trailing text so `[[templates]] junk` cannot
/// slip through.
pub(super) fn section_name(rest: &str, n: usize) -> Result<&str, String> {
    let Some(end) = rest.find("]]") else {
        return err(n, "unterminated `[[section]]` header (missing `]]`)");
    };
    let name = rest[..end].trim();
    let after = rest[end + 2..].trim();
    if !(after.is_empty() || after.starts_with('#')) {
        return err(n, format!("unexpected text after `]]`: `{after}`"));
    }
    Ok(name)
}

/// After a complete value only whitespace or a comment may follow.
pub(super) fn end_of_value(text: &str, i: usize, n: usize) -> Result<(), String> {
    let rest = text[i..].trim_start();
    if rest.is_empty() || rest.starts_with('#') {
        Ok(())
    } else {
        err(n, format!("unexpected text after the value: `{rest}`"))
    }
}

impl Value {
    /// Parse the text after a `=` into one of the four allowed shapes,
    /// refusing trailing text so `x = "a" b` cannot slip through.
    pub(super) fn parse(text: &str, n: usize) -> Result<Value, String> {
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        match b.get(i) {
            None => err(n, "missing value after `=`"),
            Some(b'"') => {
                let (s, next) = scan_string(text, i, n)?;
                end_of_value(text, next, n)?;
                Ok(Value::Str(s))
            }
            Some(b'[') => {
                let (v, next) = scan_str_array(text, i, n)?;
                end_of_value(text, next, n)?;
                Ok(Value::Strs(v))
            }
            Some(_) => {
                let start = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'#' {
                    i += 1;
                }
                let tok = &text[start..i];
                let value = match tok {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => match tok.parse::<i64>() {
                        Ok(v) => Value::Int(v),
                        // Naming the offending token before any trailing-text
                        // check is what makes an inline table `{ a = 1 }`
                        // read as "`{` is not a value shape", not as stray
                        // text after a value the reader never saw.
                        Err(_) => {
                            return err(
                                n,
                                format!(
                                    "`{tok}` is not in the TOML subset (expected a \
                                     string, an integer, true or false, or a [\"…\"] \
                                     array of strings; floats are not allowed)"
                                ),
                            );
                        }
                    },
                };
                end_of_value(text, i, n)?;
                Ok(value)
            }
        }
    }
}

/// Scan one basic string starting at `text[i] == '"'`, honouring the escapes
/// a manifest may need, and return the index just past its closing quote.
fn scan_string(text: &str, i: usize, n: usize) -> Result<(String, usize), String> {
    let b = text.as_bytes();
    let mut out = String::new();
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'"' => return Ok((out, j + 1)),
            b'\\' => {
                j += 1;
                let Some(&e) = b.get(j) else {
                    return err(n, "string ends with a dangling `\\`");
                };
                match e {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'u' | b'U' => {
                        let digits = if e == b'u' { 4 } else { 8 };
                        if j + digits >= b.len() {
                            return err(n, format!("truncated `\\{}…` escape", e as char));
                        }
                        let hex = &b[j + 1..j + 1 + digits];
                        if !hex.iter().all(u8::is_ascii_hexdigit) {
                            return err(
                                n,
                                format!("`\\{}…` needs {digits} hexadecimal digits", e as char),
                            );
                        }
                        let hex = std::str::from_utf8(hex).expect("checked ASCII above");
                        let cp = u32::from_str_radix(hex, 16)
                            .map_err(|_| format!("line {n}: `\\{hex}` is not a code point"))?;
                        let ch = char::from_u32(cp).ok_or_else(|| {
                            format!("line {n}: `\\{hex}` is not a Unicode scalar value")
                        })?;
                        out.push(ch);
                        j += digits;
                    }
                    other => {
                        return err(n, format!("unknown escape `\\{}` in string", other as char));
                    }
                }
                j += 1;
            }
            _ => {
                let ch = text[j..].chars().next().expect("j is in bounds");
                out.push(ch);
                j += ch.len_utf8();
            }
        }
    }
    err(n, "unterminated string (missing the closing `\"`)")
}

/// Scan one `["a", "b"]` array of strings starting at `text[i] == '['`,
/// honouring TOML's trailing comma, and return the index past its `]`.
fn scan_str_array(text: &str, i: usize, n: usize) -> Result<(Vec<String>, usize), String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut j = i + 1;
    loop {
        j = skip_ws(b, j);
        match b.get(j) {
            None => return err(n, "unterminated array (missing `]`)"),
            Some(b']') => return Ok((out, j + 1)),
            Some(b'"') => {
                let (s, next) = scan_string(text, j, n)?;
                out.push(s);
                j = next;
            }
            Some(_) => {
                return err(
                    n,
                    "only \"strings\" are allowed inside an array (the subset has \
                     string arrays only)",
                );
            }
        }
        j = skip_ws(b, j);
        match b.get(j) {
            Some(b',') => j += 1,
            Some(b']') => return Ok((out, j + 1)),
            None => return err(n, "unterminated array (missing `]`)"),
            Some(_) => err(n, "expected `,` or `]` between array elements")?,
        }
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}
