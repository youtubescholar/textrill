//! Option files: `@file`, `~/.textrillrc` and `./.textrillrc`, with the legacy
//! `.txt2htmlrc` still accepted so an existing configuration keeps working.
//!
//! Precedence is `@file`, home, working directory, then the command line, so the
//! command line always wins; within a directory only the preferred name is read.
//! Syntax is one option per line (`#` comments, `--` ends options); a bad option
//! is reported as `file:line: message`.
//! Covered by tests/optionstest.rs.

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
/// Exposed separately from applying them so it can be tested without a
/// filesystem and before anything is mutated.
pub fn tokenize(text: &str) -> Result<Vec<Token>, (usize, String)> {
    let mut out = Vec::new();
    let mut options_done = false;
    for (idx, raw) in text.lines().enumerate() {
        let lineno = idx + 1;
        if options_done {
            // After a `--` line, filenames keep comment markers and are not split.
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

/// Remove a `#` comment, honouring quotes so a value may contain one: an
/// unquoted `#` starts a comment, a quoted one does not.
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
/// `apply` is called with each token in file order; it controls how a token
/// becomes an option and can report `file:line:` itself. Stops on the first
/// error, since half-applied configuration is worse than a clean refusal.
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

/// The option-file names, preferred first.
///
/// `.txt2htmlrc` is still read for compatibility. `.textrillrc` wins when both
/// exist in one directory: reading both would apply an array option twice.
const RC_NAMES: [&str; 2] = [".textrillrc", ".txt2htmlrc"];

/// The rc files to read, in precedence order, that exist.
///
/// `home` is `$HOME` and `current` the working directory. A missing file is
/// optional, but one that exists and cannot be read is a fault. The two
/// directories are deduplicated, so when they are the same each is read once
/// under whichever name exists.
pub fn rc_files(home: Option<&Path>, current: &Path) -> Vec<(PathBuf, String)> {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let mut handled: Vec<&Path> = Vec::new();
    for dir in [home, Some(current)].into_iter().flatten() {
        if handled.contains(&dir) {
            continue;
        }
        handled.push(dir);
        let found = RC_NAMES.iter().map(|n| dir.join(n)).find(|p| p.exists());
        if let Some(p) = found {
            let label = display_path(&p, home);
            files.push((p, label));
        }
    }
    files
}

/// Render a path as the user would recognise it: `~/`, `./` or absolute. The
/// name shown is the name that was read.
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

    // --- textrill and legacy rc names ----------------------------------------

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("textrill-rcfile-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_textrill_name_wins_where_both_names_exist() {
        let d = scratch("both");
        std::fs::write(d.join(".textrillrc"), "--extract\n").unwrap();
        std::fs::write(d.join(".txt2htmlrc"), "--toc\n").unwrap();

        let got = rc_files(None, &d);
        assert_eq!(got.len(), 1, "a directory yields one file: {got:?}");
        assert!(
            got[0].0.ends_with(".textrillrc"),
            "preferred name not chosen: {:?}",
            got[0].0
        );
        assert!(
            got[0].1.ends_with(".textrillrc"),
            "label should show the name that was read: {}",
            got[0].1
        );
    }

    #[test]
    fn the_legacy_name_is_read_when_it_is_the_one_that_exists() {
        let d = scratch("legacy");
        std::fs::write(d.join(".txt2htmlrc"), "--extract\n").unwrap();

        let got = rc_files(None, &d);
        assert_eq!(got.len(), 1);
        assert!(got[0].0.ends_with(".txt2htmlrc"), "{:?}", got[0].0);
    }

    #[test]
    fn home_and_cwd_are_one_directory_when_they_are_the_same_directory() {
        // `HOME=$PWD`: the directory is handled once under whichever name exists.
        for tag in ["same-both", "same-legacy"] {
            let d = scratch(tag);
            if tag == "same-both" {
                std::fs::write(d.join(".textrillrc"), "--extract\n").unwrap();
                std::fs::write(d.join(".txt2htmlrc"), "--toc\n").unwrap();
            } else {
                std::fs::write(d.join(".txt2htmlrc"), "--extract\n").unwrap();
            }
            let got = rc_files(Some(&d), &d);
            assert_eq!(got.len(), 1, "{tag}: {got:?}");
            let expected = if tag == "same-both" {
                ".textrillrc"
            } else {
                ".txt2htmlrc"
            };
            assert!(got[0].0.ends_with(expected), "{tag}: {:?}", got[0].0);
        }
    }

    #[test]
    fn home_is_read_before_cwd_and_each_keeps_its_own_name() {
        let home = scratch("prec-home");
        let cwd = scratch("prec-cwd");
        std::fs::write(home.join(".textrillrc"), "--extract\n").unwrap();
        std::fs::write(cwd.join(".txt2htmlrc"), "--toc\n").unwrap();

        let got = rc_files(Some(&home), &cwd);
        let names: Vec<String> = got
            .iter()
            .map(|(p, _)| p.to_string_lossy().into())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names[0].ends_with(".textrillrc"), "{names:?}");
        assert!(names[1].ends_with(".txt2htmlrc"), "{names:?}");
        assert!(got[0].1.starts_with("~/"), "home label: {}", got[0].1);
        assert!(
            got[1].1 == names[1],
            "a cwd outside home is shown as itself: {}",
            got[1].1
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
