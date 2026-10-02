//! Option files: `@file`, `~/.txt2htmlrc` and `./.txt2htmlrc`.
//!
//! The reference script reads option files before parsing its command line:
//! `scripts/txt2html:838-845` calls
//! `Getopt::ArgvFile::argvFile(startupFilename=>".txt2htmlrc", home=>1, current=>1)`,
//! and the POD at `scripts/txt2html:509,759-771` documents all three forms as
//! active behaviour. The port had none of it, so `textrill @opts.txt` treated
//! `@opts.txt` as an input filename and failed with `Could not open @opts.txt`
//! -- a confusing failure rather than an honest "unknown option".
//!
//! # Precedence
//!
//! `@file` first, then `~/.txt2htmlrc`, then `./.txt2htmlrc`, then the command
//! line, so the command line always wins. That matches upstream, where
//! `argvFile` prepends its expansion to `@ARGV` and the command line is
//! therefore parsed last.
//!
//! # Syntax
//!
//! One option per line, shell-like but not shell: whitespace around a line is
//! trimmed, `#` starts a comment, and a value may be quoted. Options use the
//! same spelling as the command line, including `--name=value`, `--name value`
//! and the `no` prefix for booleans. A `--` line ends option processing, so
//! everything after it is an input filename -- which is what `Getopt::ArgvFile`
//! does and is the only way to name a file that begins with a dash.
//!
//! # Errors
//!
//! A bad option is reported as `file:line: message`, which is the whole
//! ergonomic win over upstream: Perl's own error names neither the file nor the
//! line, so a typo in a dotfile produces an unhelpful message.

use std::path::{Path, PathBuf};

/// One option-file origin, for diagnostics and for testing precedence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    /// Where the options came from, as the user would name it.
    pub label: String,
}

/// A token split out of an option file, with the line it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub line: usize,
}

/// Split option-file text into tokens, or return `Err` with the offending line.
///
/// Exposed separately from applying them so the tokenizer can be tested without
/// a filesystem, and so a diagnostic can be produced before anything is
/// mutated.
pub fn tokenize(text: &str) -> Result<Vec<Token>, (usize, String)> {
    let mut out = Vec::new();
    let mut options_done = false;
    for (idx, raw) in text.lines().enumerate() {
        let lineno = idx + 1;
        if options_done {
            // Everything after a `--` line is a filename, kept verbatim even if
            // it is empty or contains a comment marker.
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                out.push(Token {
                    value: trimmed.to_string(),
                    line: lineno,
                });
            }
            continue;
        }

        let line = strip_comment(raw.trim());
        if line.is_empty() {
            continue;
        }
        if line == "--" {
            options_done = true;
            continue;
        }
        for token in split_tokens(line) {
            out.push(Token {
                value: token,
                line: lineno,
            });
        }
    }
    Ok(out)
}

/// Remove a `#` comment, honouring quotes so a value may contain one.
///
/// An unquoted `#` starts a comment wherever it appears, which is what
/// `Getopt::ArgvFile`'s own reader does. A quoted `#` does not.
pub fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let (mut quote, mut escaped) = (None, false);
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match b {
            b'\\' => escaped = true,
            b'"' | b'\'' => match quote {
                Some(q) if q == b => quote = None,
                Some(_) => {}
                None => quote = Some(b),
            },
            b'#' if quote.is_none() => return line[..i].trim_end(),
            _ => {}
        }
    }
    line.trim_end()
}

/// Split a line into tokens on unquoted whitespace, honouring `"`, `'` and `\`.
pub fn split_tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut have = false;
    let (mut quote, mut escaped) = (None, false);
    for c in line.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
            have = true;
            continue;
        }
        match c {
            '\\' => {
                escaped = true;
                have = true;
            }
            '"' | '\'' => match quote {
                Some(q) if q == c => quote = None,
                Some(_) => cur.push(c),
                None => quote = Some(c),
            },
            c if c.is_whitespace() && quote.is_none() => {
                if have {
                    out.push(std::mem::take(&mut cur));
                    have = false;
                }
            }
            c => {
                cur.push(c);
                have = true;
            }
        }
    }
    if have {
        out.push(cur);
    }
    out
}

/// Read and apply one option file.
///
/// `apply` is called with each token in file order, so the caller controls how a
/// token becomes an option and can report `file:line:` itself. Returning early
/// on the first error is deliberate: half-appplied configuration with a
/// diagnostic is worse than a clean refusal.
pub fn apply_file<F>(path: &Path, label: &str, mut apply: F) -> Result<bool, String>
where
    F: FnMut(&str, usize, &str) -> Result<(), String>,
{
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("{label}: {e}")),
    };
    let tokens = tokenize(&text).map_err(|(line, msg)| format!("{label}:{line}: {msg}"))?;
    for t in tokens {
        apply(&t.value, t.line, label)?;
    }
    Ok(true)
}

/// The rc files to read, in precedence order, that exist.
///
/// `home` is taken from `$HOME` and `current` is the working directory. A
/// missing file is not an error -- upstream treats both as optional -- but a
/// file that exists and cannot be read is, since that is a real fault.
///
/// `~/.txt2htmlrc` and `./.txt2htmlrc` are listed separately rather than
/// deduplicated: on many systems they are the same file, and reading it twice
/// would apply array options twice. Upstream's `home=>1, current=>1` has the same
/// duplication, but a user who sets `HOME=$PWD` should not get their
/// `custom_heading_regexp` entries twice, so the paths are compared.
pub fn rc_files(home: Option<&Path>, current: &Path) -> Vec<(PathBuf, String)> {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    if let Some(h) = home {
        let p = h.join(".txt2htmlrc");
        if p.exists() {
            files.push((p.clone(), display_path(&p, home)));
        }
    }
    let cur = current.join(".txt2htmlrc");
    if cur.exists() && !files.iter().any(|(p, _)| *p == cur) {
        let label = display_path(&cur, home);
        files.push((cur, label));
    }
    files
}

/// Render a path the way a user would recognise it: `~/.txt2htmlrc` for the
/// home file, `./.txt2htmlrc` for the current-directory one, absolute otherwise.
fn display_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(h) = home {
        if let Ok(rest) = path.strip_prefix(h) {
            return format!("~/{}", rest.display());
        }
    }
    match path.strip_prefix(Path::new(".")) {
        Ok(rest) => format!("./{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<String> {
        tokenize(text)
            .unwrap()
            .into_iter()
            .map(|t| t.value)
            .collect()
    }

    #[test]
    fn splits_on_whitespace_and_quotes() {
        assert_eq!(
            toks("--title \"My Document\" --extract\n"),
            vec!["--title", "My Document", "--extract"]
        );
        assert_eq!(toks("--title 'a b'  --x\n"), vec!["--title", "a b", "--x"]);
        assert_eq!(
            toks("--bold_delimiter '#'\n"),
            vec!["--bold_delimiter", "#"]
        );
    }

    #[test]
    fn comments_are_stripped_except_inside_quotes() {
        assert_eq!(toks("# whole line\n"), Vec::<String>::new());
        assert_eq!(toks("--title x  # trailing\n"), vec!["--title", "x"]);
        assert_eq!(toks("--title \"a # b\"\n"), vec!["--title", "a # b"]);
    }

    #[test]
    fn double_dash_ends_options() {
        assert_eq!(
            toks("--extract\n--\n-dashed.txt\n# not a comment\n"),
            vec!["--extract", "-dashed.txt", "# not a comment"]
        );
    }

    #[test]
    fn equals_form_is_one_token() {
        assert_eq!(toks("--title=x\n"), vec!["--title=x"]);
    }

    #[test]
    fn backslash_escapes() {
        assert_eq!(toks("--title a\\ b\n"), vec!["--title", "a b"]);
    }

    #[test]
    fn blank_lines_are_skipped() {
        assert_eq!(toks("\n\n--extract\n\n"), vec!["--extract"]);
    }

    #[test]
    fn line_numbers_are_recorded() {
        let t = tokenize("\n\n--extract\n--title My\n  Doc\n").unwrap();
        assert_eq!(
            t.iter().map(|x| x.line).collect::<Vec<_>>(),
            vec![3, 4, 4, 5]
        );
        assert_eq!(
            t.iter().map(|x| x.value.as_str()).collect::<Vec<_>>(),
            vec!["--extract", "--title", "My", "Doc"]
        );
    }
}
