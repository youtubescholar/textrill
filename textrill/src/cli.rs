//! Command line option handling, shared by the `textrill` binary and the
//! Python bindings.
//!
//! The table of options, the abbreviation rules and the `--no` prefix for
//! booleans follow `Getopt::Long` as used by the reference script
//! `scripts/txt2html`.

use crate::options::{Encoding, Options, TableTypeFlags};

/// One option specification.
pub struct Spec {
    /// Long names (aliases), each may be abbreviated.
    pub names: &'static [&'static str],
    pub kind: Kind,
    /// One-line description, used for `--help` and by the GUI.
    pub help: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Negatable boolean (`!` in Getopt::Long).
    Flag,
    /// String value.
    Str,
    /// Count value (integer).
    Int,
    /// Cumulative string array.
    StrArray,
    /// `table_type=ALIGN=1` hash.
    TableType,
}

macro_rules! specs {
    ($( $kind:ident $help:literal [ $($name:literal),* $(,)? ] ),* $(,)?) => {
        &[$(Spec { names: &[ $($name),* ], kind: Kind::$kind, help: $help }),*]
    }
}

pub const SPECS: &[Spec] = specs![
    Str "File whose contents are appended to the output." ["append_file", "append_body", "ab"],
    Str "File whose contents are inserted inside <head>." ["append_head", "ah"],
    Str "Text inserted between <body> and the first paragraph." ["body_deco"],
    Str "Delimiter that turns text into bold text." ["bold_delimiter"],
    Str "Characters that start an unordered list item." ["bullets"],
    Str "Characters that start an ordered list item." ["bullets_ordered"],
    Str "HTML tag wrapped around runs of capitals." ["caps_tag", "capstag", "ct"],
    StrArray "Regexp matching lines that become headings." ["custom_heading_regexp", "heading", "H"],
    Str "Link dictionary loaded at start-up." ["default_link_dict", "dict"],
    Flag "Convert Microsoft character codes into sensible HTML." ["demoronize"],
    Str "Document type declaration, without the surrounding quotes." ["doctype", "dt"],
    Flag "Assume the input is plain 7-bit ASCII." ["eight_bit_clean", "8"],
    Flag "Escape &, < and > in text." ["escape_HTML_chars", "escapechars", "ec"],
    Flag "Number custom headings by the order of their regexps." ["explicit_headings", "EH"],
    Flag "Output only the body, without the surrounding document." ["extract"],
    Int "Minimum length of a horizontal rule." ["hrule_min", "r"],
    Int "Spaces per indentation level." ["indent_width", "iw"],
    Flag "Indent a paragraph when a tag breaks it." ["indent_par_break", "ipb"],
    StrArray "Input file; repeat for several files." ["infile"],
    StrArray "Input string; repeat for several strings." ["instring"],
    Str "Delimiter that turns text into italic text." ["italic_delimiter"],
    Str "How to decode input: auto, utf-8 or cp1252 (P7.3)." ["encoding", "enc"],
    Flag "Emit <meta charset=\"utf-8\"> in the document head (P7.4)." ["meta_charset"],
    StrArray "Link dictionary to use; repeat for several." ["links_dictionaries", "link", "l"],
    Flag "Only convert URLs, leave the rest of the text alone." ["link_only", "linkonly", "LO"],
    Flag "Emit lower-case HTML tags." ["lower_case_tags", "lc_tags", "LC"],
    Flag "Recognise mail headers and quoted replies." ["mailmode", "m"],
    Flag "Add named anchors to headings and mail messages." ["make_anchors", "anchors"],
    Flag "Turn URLs and dictionary words into links." ["make_links"],
    Flag "Recognise ALIGN, PGSQL, BORDER and DELIM tables." ["make_tables", "tables"],
    Int "Minimum number of capitals that count as a run." ["min_caps_length", "caps", "c"],
    Str "Write the result here; \"-\" means standard output." ["outfile", "out", "o"],
    Int "Spaces to indent the first line of a paragraph." ["par_indent"],
    Int "Consecutive indented lines that start preformatted text." ["preformat_trigger_lines", "prebegin", "pb"],
    Int "Consecutive plain lines that end preformatted text." ["endpreformat_trigger_lines", "preend", "pe"],
    Str "Regexp marking the start of preformatted text." ["preformat_start_marker"],
    Str "Regexp marking the end of preformatted text." ["preformat_end_marker"],
    Int "Whitespace needed in a line to preformat it." ["preformat_whitespace_min", "prewhite", "p"],
    Str "File whose contents are prepended to the output." ["prepend_file", "prepend_body", "pp"],
    Flag "Keep the original indentation of list items." ["preserve_indent", "pi"],
    Int "Lines shorter than this are broken with <br/>." ["short_line_length", "shortline", "s"],
    Str "URL of a stylesheet linked into the output." ["style_url"],
    Int "Width of a tab character." ["tab_width", "tabwidth", "tw"],
    TableType "Enable one table type, e.g. ALIGN=0." ["table_type"],
    Str "Document title." ["title", "t"],
    Flag "Use the first line of the text as the title." ["titlefirst", "tf"],
    Str "Delimiter that turns text into underlined text." ["underline_delimiter"],
    Int "Allowed length difference when underlining." ["underline_length_tolerance", "ulength", "ul"],
    Int "Allowed offset difference when underlining." ["underline_offset_tolerance", "uoffset", "uo"],
    Flag "Join words split across lines by a hyphen." ["unhyphenation", "unhyphenate"],
    Flag "Accepted for compatibility; the input is decoded as UTF-8 when possible." ["utf8"],
    Flag "Recognise Mosaic-style headers." ["use_mosaic_header", "mosaic", "mh"],
    Flag "Honour the preformat start and end markers." ["use_preformat_marker", "preformat_marker", "pm"],
    Flag "Produce XHTML: lower-case tags, closed empty tags, XHTML doctype." ["xhtml"],
];

/// Canonical long name for an abbreviation (Getopt::Long prefix matching).
pub fn lookup(abbrev: &str) -> Option<&'static Spec> {
    // An exact name match wins over prefix matches (Getopt::Long), so
    // --bullets resolves even though --bullets_ordered also starts with it.
    for spec in SPECS {
        for name in spec.names.iter() {
            if *name == abbrev {
                return Some(spec);
            }
        }
    }
    let mut found: Option<&'static Spec> = None;
    for spec in SPECS {
        // short options like "8" and "H"/"l" need the whole name matched,
        // but Getopt::Long still allows partial matches of long names.
        for name in spec.names.iter() {
            if name.len() == 1 {
                if *name == abbrev {
                    return Some(spec);
                }
                continue;
            }
            if name.starts_with(abbrev) {
                if found.is_some() {
                    // ambiguous; keep looking for a tie-breaker
                    return None;
                }
                found = Some(spec);
            }
        }
    }
    found
}

/// Set one option from a textual value, as the command line would.
///
/// The name may be a full name or any unambiguous abbreviation, aliases
/// included. Booleans accept `0`/`1`, `no`/`yes`, `false`/`true`, `off`/`on`;
/// array options append. This is the entry point the Python bindings use, so
/// the binary and the GUI cannot drift apart.
pub fn set_value(opts: &mut Options, name: &str, value: &str) -> Result<(), String> {
    let spec = lookup(name).ok_or_else(|| format!("Unknown option `{name}`"))?;
    match spec.kind {
        Kind::Flag => {
            let (v, negated) = match value
                .strip_prefix("no-")
                .or_else(|| value.strip_prefix("no_"))
            {
                Some(rest) => (parse_bool(rest)?, true),
                None => (parse_bool(value)?, false),
            };
            set_bool(opts, spec, if negated { !v } else { v });
        }
        Kind::Str => set_str(opts, spec, value)?,
        Kind::Int => {
            let n: i64 = value
                .parse()
                .map_err(|_| format!("Option {name} requires a number, got `{value}`"))?;
            set_int(opts, spec, n);
        }
        Kind::StrArray => push_array(opts, spec, value),
        Kind::TableType => set_table_type(opts, value)?,
    }
    Ok(())
}

/// Read one option back as a textual value, the inverse of [`set_value`].
pub fn get_value(opts: &Options, name: &str) -> Result<String, String> {
    let spec = lookup(name).ok_or_else(|| format!("Unknown option `{name}`"))?;
    let v = match spec.names[0] {
        "append_file" => opts.append_file.clone(),
        "append_head" => opts.append_head.clone(),
        "body_deco" => opts.body_deco.clone(),
        "bold_delimiter" => opts.bold_delimiter.clone(),
        "bullets" => opts.bullets.clone(),
        "bullets_ordered" => opts.bullets_ordered.clone(),
        "caps_tag" => opts.caps_tag.clone(),
        "default_link_dict" => opts.default_link_dict.clone(),
        "demoronize" => opts.demoronize.to_string(),
        "doctype" => opts.doctype.clone(),
        "eight_bit_clean" => opts.eight_bit_clean.to_string(),
        "escape_HTML_chars" => opts.escape_html_chars.to_string(),
        "explicit_headings" => opts.explicit_headings.to_string(),
        "extract" => opts.extract.to_string(),
        "hrule_min" => opts.hrule_min.to_string(),
        "indent_width" => opts.indent_width.to_string(),
        "indent_par_break" => opts.indent_par_break.to_string(),
        "infile" => opts.infile.join("\n"),
        "instring" => opts.instring.join("\n"),
        "italic_delimiter" => opts.italic_delimiter.clone(),
        "encoding" => opts.encoding.name().to_string(),
        "meta_charset" => opts.meta_charset.to_string(),
        "links_dictionaries" => opts.links_dictionaries.join("\n"),
        "link_only" => opts.link_only.to_string(),
        "lower_case_tags" => opts.lower_case_tags.to_string(),
        "mailmode" => opts.mailmode.to_string(),
        "make_anchors" => opts.make_anchors.to_string(),
        "make_links" => opts.make_links.to_string(),
        "make_tables" => opts.make_tables.to_string(),
        "min_caps_length" => opts.min_caps_length.to_string(),
        "outfile" => opts.outfile.clone(),
        "par_indent" => opts.par_indent.to_string(),
        "preformat_trigger_lines" => opts.preformat_trigger_lines.to_string(),
        "endpreformat_trigger_lines" => opts.endpreformat_trigger_lines.to_string(),
        "preformat_start_marker" => opts.preformat_start_marker.clone(),
        "preformat_end_marker" => opts.preformat_end_marker.clone(),
        "preformat_whitespace_min" => opts.preformat_whitespace_min.to_string(),
        "prepend_file" => opts.prepend_file.clone(),
        "preserve_indent" => opts.preserve_indent.to_string(),
        "short_line_length" => opts.short_line_length.to_string(),
        "style_url" => opts.style_url.clone(),
        "tab_width" => opts.tab_width.to_string(),
        "table_type" => format!(
            "ALIGN={} PGSQL={} BORDER={} DELIM={}",
            opts.table_type.align as u8,
            opts.table_type.pgsql as u8,
            opts.table_type.border as u8,
            opts.table_type.delim as u8
        ),
        "title" => opts.title.clone(),
        "titlefirst" => opts.titlefirst.to_string(),
        "underline_delimiter" => opts.underline_delimiter.clone(),
        "underline_length_tolerance" => opts.underline_length_tolerance.to_string(),
        "underline_offset_tolerance" => opts.underline_offset_tolerance.to_string(),
        "unhyphenation" => opts.unhyphenation.to_string(),
        "utf8" => "1".to_string(),
        "use_mosaic_header" => opts.use_mosaic_header.to_string(),
        "use_preformat_marker" => opts.use_preformat_marker.to_string(),
        "xhtml" => opts.xhtml.to_string(),
        "custom_heading_regexp" => opts.custom_heading_regexp.join("\n"),
        other => return Err(format!("Option `{other}` cannot be read back")),
    };
    Ok(v)
}

/// Where an option came from, for diagnostics.
#[derive(Clone, Debug)]
pub enum Source {
    CommandLine,
    /// A file, with the label the user would recognise (`~/.txt2htmlrc`, ...).
    File(String, usize),
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::CommandLine => Ok(()),
            Source::File(label, line) => write!(f, "{label}:{line}"),
        }
    }
}

/// Parse one argument list, reporting errors as `file:line: message` when the
/// argument came from an option file.
pub fn parse_args_from(args: &[String], opts: &mut Options, label: &str) -> Result<(), String> {
    let mut it = args.iter().peekable();
    let mut table_type_seen = false;
    while let Some(arg) = it.next() {
        let argline = || -> String { label.to_string() };
        let at = |e: String| -> String { format!("{}: {e}", argline()) };
        let (name, mut inline) = if let Some(rest) = arg.strip_prefix("--") {
            match rest.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (rest, None),
            }
        } else if let Some(rest) = arg.strip_prefix('-') {
            if rest.is_empty() {
                opts.infile.push("-".to_string());
                continue;
            }
            match rest.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (rest, None),
            }
        } else {
            // a non-option argument is an input file
            opts.infile.push(arg.clone());
            continue;
        };

        let (spec, negated) = resolve(name).map_err(&at)?;
        if negated && !matches!(spec.kind, Kind::Flag) {
            return Err(at(format!("Unknown option `{name}`")));
        }

        match spec.kind {
            Kind::Flag => {
                let value = match inline.take() {
                    Some(v) => parse_bool(&v).map_err(&at)?,
                    None => true,
                };
                set_bool(opts, spec, if negated { !value } else { value });
            }
            Kind::Str => {
                let v = match inline.take() {
                    Some(v) => v,
                    None => take_value(&mut it, name).map_err(&at)?,
                };
                set_str(opts, spec, &v).map_err(&at)?;
            }
            Kind::Int => {
                let v = match inline.take() {
                    Some(v) => v,
                    None => take_value(&mut it, name).map_err(&at)?,
                };
                let n: i64 = v
                    .parse()
                    .map_err(|_| at(format!("Option {name} requires a number, got `{v}`")))?;
                set_int(opts, spec, n);
            }
            Kind::StrArray => {
                let v = match inline.take() {
                    Some(v) => v,
                    None => take_value(&mut it, name).map_err(&at)?,
                };
                push_array(opts, spec, &v);
            }
            Kind::TableType => {
                let v = match inline.take() {
                    Some(v) => v,
                    None => take_value(&mut it, name).map_err(&at)?,
                };
                // See reset_table_type: the command line names the whole set of
                // table types, so the defaults are dropped on the first
                // occurrence and later ones accumulate.
                if !table_type_seen {
                    reset_table_type(opts);
                    table_type_seen = true;
                }
                set_table_type(opts, &v).map_err(&at)?;
            }
        }
    }
    Ok(())
}

/// Look up an option name, handling the `no` negation prefix for booleans.
///
/// Split out of `parse_args` because the option-file path needs the same rules
/// and they must not drift: an rc file that accepted `--noextract` while the
/// command line did not would be a confusing asymmetry.
fn resolve(name: &str) -> Result<(&'static Spec, bool), String> {
    let stripped = name
        .strip_prefix("no-")
        .or_else(|| name.strip_prefix("no_"))
        .or_else(|| name.strip_prefix("no"));
    match stripped {
        Some(rest) => match lookup(rest) {
            Some(s) => Ok((s, true)),
            None => match lookup(name) {
                Some(s) => Ok((s, false)),
                None => Err(format!("Unknown option `{name}`")),
            },
        },
        None => match lookup(name) {
            Some(s) => Ok((s, false)),
            None => Err(format!("Unknown option `{name}`")),
        },
    }
}

/// Parse a command-line argument list.
///
/// Thin wrapper over [`parse_args_from`] so callers on the command line do not
/// have to name a source.
pub fn parse_args(args: &[String], opts: &mut Options) -> Result<(), String> {
    parse_args_from(args, opts, "")
}

/// P11: expand `@file` and the rc files, then parse everything in order.
///
/// The reference reads option files before its command line
/// (`Getopt::ArgvFile::argvFile` prepends its expansion to `@ARGV`), so a
/// command-line value always wins. Precedence is therefore
/// `@file` < `~/.txt2htmlrc` < `./.txt2htmlrc` < command line, and `@file` is
/// expanded where it appears so a later command-line option overrides it.
pub fn parse_args_with_rc(
    args: &[String],
    opts: &mut Options,
    home: Option<&std::path::Path>,
    current: &std::path::Path,
) -> Result<(), String> {
    let mut command_line: Vec<String> = Vec::new();

    for a in args {
        match a.strip_prefix('@') {
            Some(path) if !path.is_empty() => {
                let p = current.join(path);
                let label = p.display().to_string();
                read_option_file(&p, &label, opts, false)?;
            }
            // A bare `@` is not a group. Left for the input-file path, which
            // reports it as an unopenable file -- the same as upstream.
            _ => command_line.push(a.clone()),
        }
    }

    for (path, label) in crate::rcfile::rc_files(home, current) {
        read_option_file(&path, &label, opts, true)?;
    }

    parse_args(&command_line, opts)
}

/// Read one option file and apply it, one line at a time.
///
/// Lines rather than a flat token stream, because an option file is line
/// oriented: `--bold_delimiter #` is one option with one value, and flattening
/// it into tokens would make that indistinguishable from a bare
/// `--bold_delimiter` missing its argument.
fn read_option_file(
    path: &std::path::Path,
    label: &str,
    opts: &mut Options,
    optional_missing: bool,
) -> Result<(), String> {
    // The two rc files are optional, so a missing one is fine. An `@file` group
    // is not: the user named it explicitly, so a typo must be an error rather
    // than a silently ignored group.
    let optional = optional_missing;
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && optional => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("{label}: no such option file"))
        }
        Err(e) => return Err(format!("{label}: {e}")),
    };
    let mut table_type_seen = false;
    for (idx, raw) in text.lines().enumerate() {
        let where_ = format!("{label}:{}", idx + 1);
        let line = crate::rcfile::strip_comment(raw.trim());
        if line.is_empty() {
            continue;
        }
        if line == "--" {
            // Everything after this is an input filename. Option processing
            // ends, which is the only way to name a file beginning with a dash.
            for rest in text.lines().skip(idx + 1) {
                let t = rest.trim();
                if !t.is_empty() {
                    opts.infile.push(t.to_string());
                }
            }
            return Ok(());
        }
        let tokens: Vec<String> = crate::rcfile::split_tokens(line);
        if tokens.is_empty() {
            continue;
        }
        // `--table_type` names the whole set of table types, so the first
        // occurrence in a file drops the defaults just as the first occurrence
        // on the command line does. Per file, not per invocation, so a file's
        // own repeat accumulates.
        if !table_type_seen
            && tokens
                .first()
                .is_some_and(|t| t == "--table_type" || t.starts_with("--table_type="))
        {
            reset_table_type(opts);
            table_type_seen = true;
        }
        parse_args_from(&tokens, opts, &where_)?;
    }
    Ok(())
}

pub fn take_value<'a>(
    it: &mut std::iter::Peekable<std::slice::Iter<'a, String>>,
    name: &str,
) -> Result<String, String> {
    match it.next() {
        Some(v) => Ok(v.clone()),
        None => Err(format!("Option {name} requires an argument")),
    }
}

pub fn parse_bool(v: &str) -> Result<bool, String> {
    match v {
        "0" | "no" | "false" | "off" => Ok(false),
        "1" | "yes" | "true" | "on" => Ok(true),
        _ => Err(format!("Invalid boolean value `{v}`")),
    }
}

pub fn set_bool(opts: &mut Options, spec: &Spec, value: bool) -> bool {
    match spec.names[0] {
        "demoronize" => opts.demoronize = value,
        "eight_bit_clean" => opts.eight_bit_clean = value,
        "escape_HTML_chars" => opts.escape_html_chars = value,
        "explicit_headings" => opts.explicit_headings = value,
        "extract" => opts.extract = value,
        "indent_par_break" => opts.indent_par_break = value,
        "link_only" => opts.link_only = value,
        "lower_case_tags" => opts.lower_case_tags = value,
        "mailmode" => opts.mailmode = value,
        // P7.4.
        "meta_charset" => opts.meta_charset = value,
        "make_anchors" => opts.make_anchors = value,
        "make_links" => opts.make_links = value,
        "make_tables" => opts.make_tables = value,
        "preserve_indent" => opts.preserve_indent = value,
        "titlefirst" => opts.titlefirst = value,
        "unhyphenation" => opts.unhyphenation = value,
        "utf8" => {}
        "use_mosaic_header" => opts.use_mosaic_header = value,
        "use_preformat_marker" => opts.use_preformat_marker = value,
        "xhtml" => opts.xhtml = value,
        other => {
            let _ = other;
        }
    }
    value
}

pub fn set_str(opts: &mut Options, spec: &Spec, v: &str) -> Result<(), String> {
    match spec.names[0] {
        "append_file" => opts.append_file = v.to_string(),
        "append_head" => opts.append_head = v.to_string(),
        "body_deco" => opts.body_deco = v.to_string(),
        "bold_delimiter" => opts.bold_delimiter = v.to_string(),
        "bullets" => opts.bullets = v.to_string(),
        "bullets_ordered" => opts.bullets_ordered = v.to_string(),
        "caps_tag" => opts.caps_tag = v.to_string(),
        "default_link_dict" => opts.default_link_dict = v.to_string(),
        "doctype" => opts.doctype = v.to_string(),
        "italic_delimiter" => opts.italic_delimiter = v.to_string(),
        // P7.3. The only string option whose value is not stored verbatim: an
        // unrecognised encoding is a user error worth reporting, not a string
        // to be discovered three files later.
        "encoding" => opts.encoding = Encoding::parse(v).map_err(|e| e.to_string())?,
        "outfile" => opts.outfile = v.to_string(),
        "preformat_start_marker" => opts.preformat_start_marker = v.to_string(),
        "preformat_end_marker" => opts.preformat_end_marker = v.to_string(),
        "prepend_file" => opts.prepend_file = v.to_string(),
        "style_url" => opts.style_url = v.to_string(),
        "title" => opts.title = v.to_string(),
        "underline_delimiter" => opts.underline_delimiter = v.to_string(),
        other => {
            let _ = other;
        }
    }
    Ok(())
}

pub fn set_int(opts: &mut Options, spec: &Spec, v: i64) {
    match spec.names[0] {
        "preformat_trigger_lines" | "endpreformat_trigger_lines" => {
            let n = v.clamp(-128, 127) as i8;
            if spec.names[0] == "preformat_trigger_lines" {
                opts.preformat_trigger_lines = n;
            } else {
                opts.endpreformat_trigger_lines = n;
            }
        }
        _ => {
            let n = v.max(0) as usize;
            match spec.names[0] {
                "hrule_min" => opts.hrule_min = n,
                "indent_width" => opts.indent_width = n,
                "min_caps_length" => opts.min_caps_length = n,
                "par_indent" => opts.par_indent = n,
                "preformat_whitespace_min" => opts.preformat_whitespace_min = n,
                "short_line_length" => opts.short_line_length = n,
                "tab_width" => opts.tab_width = n,
                "underline_length_tolerance" => opts.underline_length_tolerance = n,
                "underline_offset_tolerance" => opts.underline_offset_tolerance = n,
                other => {
                    let _ = other;
                }
            }
        }
    }
}

pub fn push_array(opts: &mut Options, spec: &Spec, v: &str) {
    match spec.names[0] {
        "custom_heading_regexp" => opts.custom_heading_regexp.push(v.to_string()),
        "infile" => opts.infile.push(v.to_string()),
        "instring" => opts.instring.push(v.to_string()),
        "links_dictionaries" => opts.links_dictionaries.push(v.to_string()),
        other => {
            let _ = other;
        }
    }
}

/// Set one or more table types from their `TYPE=0/1` spelling.
///
/// The command line repeats the option for each type; a whitespace separated
/// list is accepted as well, which is what [`get_value`] hands back.
pub fn set_table_type(opts: &mut Options, v: &str) -> Result<(), String> {
    for word in v.split_whitespace() {
        set_one_table_type(opts, word)?;
    }
    Ok(())
}

/// Turn every table type off, ready for a command line `--table_type` to fill
/// in just the keys it names.
///
/// This is what makes `--table_type DELIM=0` disable *every* table type rather
/// than just DELIM.  The reference declares the option as `table_type=n%`
/// (scripts/txt2html:895), and Getopt::Long's `n%` builds a brand new hashref
/// from the options actually present.  `init_our_data` seeds the four flags to
/// 1, but that seed is thrown away the moment the option is named, so
/// `--table_type DELIM=0` leaves only `{DELIM => 0}` and ALIGN, PGSQL and
/// BORDER are simply not there.  Measured on a `+----+----+` table:
///
/// ```text
/// txt2html --make_tables --table_type DELIM=0
/// reference  <p>+----+----+<br/>| ab | cd |<br/>...</p>
/// port       <table border="1" summary="">...</table>   (before this fix)
/// ```
///
/// The reference's own --table_type documentation (scripts/txt2html:431) says
/// to repeat the option once per key, e.g. `--table_type ALIGN=1
/// --table_type BORDER=0`, and repeated occurrences accumulate into the one
/// hash.  So the flags are reset once, on the first --table_type, and each
/// later occurrence adds to that same set.
fn reset_table_type(opts: &mut Options) {
    opts.table_type = TableTypeFlags {
        align: false,
        pgsql: false,
        border: false,
        delim: false,
    };
}

fn set_one_table_type(opts: &mut Options, v: &str) -> Result<(), String> {
    let (key, val) = v
        .split_once('=')
        .ok_or_else(|| format!("Option table_type requires a TYPE=0/1 argument, got `{v}`"))?;
    let on = match val {
        "0" => false,
        "1" => true,
        _ => {
            return Err(format!(
                "table_type value for {key} must be 0 or 1, got `{val}`"
            ))
        }
    };
    match key {
        "ALIGN" | "align" | "Align" => opts.table_type.align = on,
        "PGSQL" | "pgsql" | "Pgsql" => opts.table_type.pgsql = on,
        "BORDER" | "border" | "Border" => opts.table_type.border = on,
        "DELIM" | "delim" | "Delim" => opts.table_type.delim = on,
        other => {
            return Err(format!(
                "Unknown table_type `{other}` (expected ALIGN, PGSQL, BORDER or DELIM)"
            ))
        }
    }
    Ok(())
}

pub fn usage() -> String {
    let mut s = String::new();
    s.push_str("Usage: textrill [ options ] [ file ... ]\n");
    s.push_str(
        "Convert plain text to HTML.  A reimplementation of txt2html 3.0; \
         see the textrill README for details.\n\n",
    );
    for spec in SPECS {
        let mut names = String::new();
        for (i, n) in spec.names.iter().enumerate() {
            if i > 0 {
                names.push('|');
            }
            if spec.kind == Kind::Flag {
                names.push_str("--")
            }
            names.push_str(n);
        }
        let kind = match spec.kind {
            Kind::Flag => "",
            Kind::Str | Kind::StrArray => " <value>",
            Kind::Int => " <n>",
            Kind::TableType => " <TYPE=0/1>",
        };
        s.push_str(&format!("    {names}{kind}\n"));
        s.push_str(&format!("        {}\n", spec.help));
    }
    s.push_str("\nOptions can be abbreviated.  Boolean options take a `no` prefix to disable.\n");
    // P11. Documented here rather than only in the README because this is the
    // only place a user learns the precedence order without opening a second file.
    s.push_str(
        "\nOptions may also be read from @file groups, and from ~/.txt2htmlrc or\n\
         ./.txt2htmlrc -- one option per line, `#` comments, and a `--` line to end\n\
         option processing.  Lowest precedence first:\n\
         \n\
         \x20   @file < ~/.txt2htmlrc < ./.txt2htmlrc < command line\n",
    );
    // P7.4. --help spells the detection order out, because --encoding is the
    // one option whose behaviour cannot be guessed from its name, and the
    // difference between "detected" and "guessed" decides whether a user
    // bothers to read this at all.
    s.push_str(
        "\n--encoding values:\n\
         \n  auto        probe in order: byte-order mark, then the NUL pattern\n\
         \x20             that marks UTF-16, then UTF-8 validity, then cp1252.\n\
         \x20             The first two are evidence; cp1252 is a guess, and it\n\
         \x20             is wrong for Cyrillic, Greek and Turkish.\n\
         \x20 utf-8 / cp1252\n\
         \x20 latin-1 (= iso-8859-1), cp1251, cp1253, koi8-r\n\
         \x20             Single-byte encodings, which cannot be detected from\n\
         \x20             the bytes -- name the right one for non-Western text.\n\
         \x20 utf-16le / utf-16be, utf-32le / utf-32be\n\
         \x20             For a BOM-less file whose text has too little ASCII\n\
         \x20             for the probe to see it as UTF-16.\n\
         \nOutput is always UTF-8.\n",
    );
    s
}
