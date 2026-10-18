//! The window's command line, mirroring `app.py`.
//!
//! Hand-parsed so the GUI crate gains no argument-parsing dependency; the
//! engine's own CLI stays the place for the full option set.

use std::ffi::OsString;
use std::path::PathBuf;

/// The `--help` text.
pub const USAGE: &str = "\
Usage: textrill-gui [options] [file]

Convert plain text to HTML, with a live preview.

Positional:
  file          text file to open (UTF-8 or Latin-1)

Options:
  --xhtml       produce XHTML 1.0 Strict (the Perl original's default)
  --no-xhtml    produce HTML 4 instead
  --tables      start with table recognition on
  --version     print the version and exit
  -h, --help    show this help and exit
";

/// What the command line asked for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    /// A text file to open at startup.
    pub file: Option<PathBuf>,
    /// `Some(true)` for `--xhtml`, `Some(false)` for `--no-xhtml`.
    pub xhtml: Option<bool>,
    /// `--tables` starts with table recognition on.
    pub tables: bool,
    /// `--version` prints the version and opens no window.
    pub version: bool,
    /// `-h`/`--help` prints [`USAGE`] and opens no window.
    pub help: bool,
}

/// Parse the arguments after the program name.
///
/// A second positional file and an unknown flag are errors, so a typo is
/// reported instead of being silently ignored.
pub fn parse<I>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut parsed = Args::default();
    for arg in args {
        match arg.to_str() {
            Some("--xhtml") => parsed.xhtml = Some(true),
            Some("--no-xhtml") => parsed.xhtml = Some(false),
            Some("--tables") => parsed.tables = true,
            Some("--version") => parsed.version = true,
            Some("-h") | Some("--help") => parsed.help = true,
            Some(text) if text.starts_with('-') && text != "-" => {
                return Err(format!("textrill-gui: unrecognized option `{text}`"));
            }
            _ => {
                if parsed.file.is_some() {
                    return Err("textrill-gui: only one file can be opened".to_string());
                }
                parsed.file = Some(PathBuf::from(arg));
            }
        }
    }
    Ok(parsed)
}
