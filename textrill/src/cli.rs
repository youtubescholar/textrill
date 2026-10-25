//! Command line option handling, shared by the `textrill` binary and the
//! Python bindings.
//!
//! Option table with abbreviation rules and `--no` prefix for booleans.

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

/// Define the option table once: `Opt`, its metadata, and [`SPECS`] are all
/// generated from this one list, so the setters, getters, `--help`, and the GUI
/// cannot drift from it. The setters and getters ([`set_opt`], [`get_opt`])
/// match on [`Opt`] exhaustively, so adding a variant without handling it is a
/// compile error rather than a runtime `unhandled` arm.
macro_rules! option_table {
    ($( $variant:ident, $kind:ident, $help:literal, [ $($name:literal),+ $(,)? ] );* $(;)?) => {
        /// One option, as a type: the single source of truth for the table.
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Opt {
            $($variant),*
        }

        impl Opt {
            /// Every option, in `--help` order.
            pub const ALL: &'static [Opt] = &[$(Opt::$variant),*];

            /// The accepted spellings, canonical long name first.
            pub fn names(self) -> &'static [&'static str] {
                match self {
                    $(Opt::$variant => &[$($name),+]),*
                }
            }

            /// The kind of value the option takes.
            pub fn kind(self) -> Kind {
                match self {
                    $(Opt::$variant => Kind::$kind),*
                }
            }

            /// One-line description, used by `--help` and the GUI.
            pub fn help(self) -> &'static str {
                match self {
                    $(Opt::$variant => $help),*
                }
            }

            /// The canonical long name (the first spelling).
            pub fn canonical(self) -> &'static str {
                self.names()[0]
            }
        }

        /// The option table, in `--help` order, generated from [`Opt`].
        pub const SPECS: &[Spec] = &[
            $(Spec { names: &[$($name),+], kind: Kind::$kind, help: $help }),*
        ];
    };
}

option_table![
    AllowedUrlSchemes, Str, "Allow only these URL schemes in href values; a stricter policy than the default, which refuses script-bearing schemes.", ["allowed_url_schemes", "url_schemes"];
    AppendFile, Str, "File whose contents are appended to the output.", ["append_file", "append_body", "ab"];
    AppendHead, Str, "File whose contents are inserted inside <head>.", ["append_head", "ah"];
    BodyDeco, Str, "Text inserted between <body> and the first paragraph.", ["body_deco"];
    BoldDelimiter, Str, "Delimiter that turns text into bold text.", ["bold_delimiter"];
    Bullets, Str, "Characters that start an unordered list item.", ["bullets"];
    BulletsOrdered, Str, "Characters that start an ordered list item.", ["bullets_ordered"];
    CapsTag, Str, "HTML tag wrapped around runs of capitals.", ["caps_tag", "capstag", "ct"];
    Chunk, Flag, "Write one HTML file per top-level section.", ["chunk"];
    ChunkChars, Int, "Split into chunks of at most this many characters (raw plain text), rounded down to nearest sentence boundary (.,;:!? and CJK punctuation).", ["chunk_chars"];
    Citations, Flag, "Collect {{textrill:cite:key}} references into a numbered endnotes list.", ["citations", "notes"];
    CloseOptionalTags, Flag, "Emit the end tags HTML allows to be omitted (</p>, </li>, </dt>, </dd>).", ["close_optional_tags"];
    CustomHeadingRegexp, StrArray, "Regexp matching lines that become headings.", ["custom_heading_regexp", "heading", "H"];
    DefaultLinkDict, Str, "Link dictionary loaded at start-up.", ["default_link_dict", "dict"];
    Demoronize, Flag, "Convert Microsoft character codes into sensible HTML.", ["demoronize"];
    Doctype, Str, "Document type declaration, without the surrounding quotes.", ["doctype", "dt"];
    DocumentTemplate, Str, "Use a whole-document template file.", ["document_template"];
    EightBitClean, Flag, "Assume the input is plain 7-bit ASCII.", ["eight_bit_clean", "8"];
    EscapeHtmlChars, Flag, "Escape &, < and > in text.", ["escape_HTML_chars", "escapechars", "ec"];
    ExplicitHeadings, Flag, "Number custom headings by the order of their regexps.", ["explicit_headings", "EH"];
    Extract, Flag, "Output only the body, without the surrounding document.", ["extract"];
    HruleMin, Int, "Minimum length of a horizontal rule.", ["hrule_min", "r"];
    Html5, Flag, "Emit HTML5: <!DOCTYPE html> and a charset meta.", ["html5"];
    Glossary, Flag, "Collect {{textrill:gloss:term}} references into a definition list.", ["glossary"];
    IndentWidth, Int, "Spaces per indentation level.", ["indent_width", "iw"];
    IndentParBreak, Flag, "Indent a paragraph when a tag breaks it.", ["indent_par_break", "ipb"];
    Infile, StrArray, "Input file; repeat for several files.", ["infile"];
    Instring, StrArray, "Input string; repeat for several strings.", ["instring"];
    Lang, Str, "Declared language of the document, a BCP 47 tag such as en, zh-Hans or ja; empty disables it.", ["lang"];
    LangRuns, Flag, "Wrap runs of a script other than the document language in <span lang> tags.", ["lang_runs", "wrap_langs"];
    ItalicDelimiter, Str, "Delimiter that turns text into italic text.", ["italic_delimiter"];
    Encoding, Str, "How to decode input: auto, utf-8 or cp1252.", ["encoding", "enc"];
    MetaCharset, Flag, "Emit <meta charset=\"utf-8\"> in the document head.", ["meta_charset"];
    LinksDictionaries, StrArray, "Link dictionary to use; repeat for several.", ["links_dictionaries", "link", "l"];
    LinkOnly, Flag, "Only convert URLs, leave the rest of the text alone.", ["link_only", "linkonly", "LO"];
    ListTemplates, Flag, "List the discovered templates on standard output and exit.", ["list_templates"];
    LowerCaseTags, Flag, "Emit lower-case HTML tags.", ["lower_case_tags", "lc_tags", "LC"];
    Mailmode, Flag, "Recognise mail headers and quoted replies.", ["mailmode", "m"];
    MakeAnchors, Flag, "Add named anchors to headings and mail messages.", ["make_anchors", "anchors"];
    MakeLinks, Flag, "Turn URLs and dictionary words into links.", ["make_links"];
    MakeTables, Flag, "Recognise ALIGN, PGSQL, BORDER and DELIM tables.", ["make_tables", "tables"];
    MinCapsLength, Int, "Minimum number of capitals that count as a run.", ["min_caps_length", "caps", "c"];
    NoReferrer, Flag, "Private reading: emit a no-referrer meta and put rel=\"nofollow noreferrer\" on generated external links.", ["no_referrer"];
    NumberHeadings, Flag, "Prefix headings with hierarchical numbers.", ["number_headings"];
    Outfile, Str, "Write the result here; \"-\" means standard output.", ["outfile", "out", "o"];
    ParIndent, Int, "Spaces to indent the first line of a paragraph.", ["par_indent"];
    PreformatTriggerLines, Int, "Consecutive indented lines that start preformatted text.", ["preformat_trigger_lines", "prebegin", "pb"];
    EndpreformatTriggerLines, Int, "Consecutive plain lines that end preformatted text.", ["endpreformat_trigger_lines", "preend", "pe"];
    PreformatStartMarker, Str, "Regexp marking the start of preformatted text.", ["preformat_start_marker"];
    PreformatEndMarker, Str, "Regexp marking the end of preformatted text.", ["preformat_end_marker"];
    PreformatWhitespaceMin, Int, "Whitespace needed in a line to preformat it.", ["preformat_whitespace_min", "prewhite", "p"];
    PrependFile, Str, "File whose contents are prepended to the output.", ["prepend_file", "prepend_body", "pp"];
    PreserveIndent, Flag, "Keep the original indentation of list items.", ["preserve_indent", "pi"];
    Preset, Str, "Apply the named preset from <config>/textrill/presets.toml. Expanded above the rc files and below the other command-line options.", ["preset"];
    Report, Flag, "Print what the conversion recovered (bytes, headings, paragraphs, capitals, breaks) on standard error.", ["report"];
    Section, Flag, "Wrap each heading section in <article>.", ["section"];
    ShortLineLength, Int, "Lines shorter than this are broken with <br/>.", ["short_line_length", "shortline", "s"];
    Stream, Flag, "Read and write a paragraph at a time.", ["stream"];
    StyleUrl, Str, "URL of a stylesheet linked into the output.", ["style_url"];
    TabWidth, Int, "Width of a tab character.", ["tab_width", "tabwidth", "tw"];
    TableType, TableType, "Enable one table type, e.g. ALIGN=0.", ["table_type"];
    BodyTemplate, Str, "Wrap the body in a template file; the whole-document counterpart is --document_template. The old name --template still works but is deprecated.", ["body_template", "template"];
    TemplateDir, StrArray, "Directory to search for template packs; repeat for several.", ["template_dir"];
    CatalogDir, StrArray, "Directory containing a catalog index (catalog.toml) to search/install from; repeat for several.", ["catalog_dir"];
    TemplateLibrary, Str, "Use a template by name: article, book, manpage, slide, bare, or pack-id:name from a discovered pack.", ["template_library"];
    Title, Str, "Document title.", ["title", "t"];
    Titlefirst, Flag, "Use the first line of the text as the title.", ["titlefirst", "tf"];
    Toc, Flag, "Prepend a generated table of contents.", ["toc"];
    UnderlineDelimiter, Str, "Delimiter that turns text into underlined text.", ["underline_delimiter"];
    UnderlineLengthTolerance, Int, "Allowed length difference when underlining.", ["underline_length_tolerance", "ulength", "ul"];
    UnderlineOffsetTolerance, Int, "Allowed offset difference when underlining.", ["underline_offset_tolerance", "uoffset", "uo"];
    Unhyphenation, Flag, "Join words split across lines by a hyphen.", ["unhyphenation", "unhyphenate"];
    Utf8, Flag, "Accepted for compatibility; the input is decoded as UTF-8 when possible.", ["utf8"];
    UseMosaicHeader, Flag, "Recognise Mosaic-style headers.", ["use_mosaic_header", "mosaic", "mh"];
    UsePreformatMarker, Flag, "Honour the preformat start and end markers.", ["use_preformat_marker", "preformat_marker", "pm"];
    Var, StrArray, "Template parameter `name=value`, substituted for {{textrill:var:name}} in the active template; repeat for several.", ["var"];
    Xhtml, Flag, "Produce XHTML: lower-case tags, closed empty tags, XHTML doctype.", ["xhtml"];
];

/// The README quotes the option count; binding it here makes an addition a
/// deliberate, compile-time act rather than a silent prose drift.
const _: () = assert!(
    Opt::ALL.len() == 77,
    "option count changed; update this assertion and README.md"
);

/// Resolve an abbreviation to its option (Getopt::Long prefix matching).
pub fn lookup(abbrev: &str) -> Option<Opt> {
    // Exact match wins over prefix match.
    for &opt in Opt::ALL {
        for name in opt.names() {
            if *name == abbrev {
                return Some(opt);
            }
        }
    }
    let mut found: Option<Opt> = None;
    for &opt in Opt::ALL {
        // Short single-char options must match exactly; long names allow partial matches.
        for name in opt.names() {
            if name.len() == 1 {
                if *name == abbrev {
                    return Some(opt);
                }
                continue;
            }
            if name.starts_with(abbrev) {
                if found.is_some() {
                    // ambiguous; look for tie-breaker
                    return None;
                }
                found = Some(opt);
            }
        }
    }
    found
}

/// Set one option from a textual value, as the command line would.
///
/// The name may be a full name or any unambiguous abbreviation. Booleans
/// accept `0`/`1`, `no`/`yes`, `false`/`true`, `off`/`on`; array options append.
pub fn set_value(opts: &mut Options, name: &str, value: &str) -> Result<(), String> {
    let opt = lookup(name).ok_or_else(|| format!("Unknown option `{name}`"))?;
    set_opt(opts, opt, value)
}

/// Read one option back as a textual value, the inverse of [`set_value`].
pub fn get_value(opts: &Options, name: &str) -> Result<String, String> {
    let opt = lookup(name).ok_or_else(|| format!("Unknown option `{name}`"))?;
    Ok(get_opt(opts, opt))
}

/// Parse a flag value, honouring the `no-`/`no_` prefix as a negation.
fn parse_flag(value: &str) -> Result<bool, String> {
    match value
        .strip_prefix("no-")
        .or_else(|| value.strip_prefix("no_"))
    {
        Some(rest) => Ok(!parse_bool(rest)?),
        None => parse_bool(value),
    }
}

/// Parse an integer option value, naming the option in the error.
fn parse_int(opt: Opt, value: &str) -> Result<i64, String> {
    value.parse().map_err(|_| {
        format!(
            "Option {} requires a number, got `{value}`",
            opt.canonical()
        )
    })
}

/// Set one option. Exhaustive over [`Opt`], so an option added to the table
/// without a setter arm is a compile error, not a runtime `unhandled`.
pub fn set_opt(opts: &mut Options, opt: Opt, value: &str) -> Result<(), String> {
    match opt {
        // --- flags ---
        Opt::Chunk => opts.chunk = parse_flag(value)?,
        Opt::ChunkChars => opts.chunk_chars = parse_int(opt, value)?.max(0) as usize,
        Opt::Citations => opts.citations = parse_flag(value)?,
        Opt::CloseOptionalTags => opts.close_optional_tags = parse_flag(value)?,
        Opt::Demoronize => opts.demoronize = parse_flag(value)?,
        Opt::EightBitClean => opts.eight_bit_clean = parse_flag(value)?,
        Opt::EscapeHtmlChars => opts.escape_html_chars = parse_flag(value)?,
        Opt::ExplicitHeadings => opts.explicit_headings = parse_flag(value)?,
        Opt::Extract => opts.extract = parse_flag(value)?,
        Opt::Glossary => opts.glossary = parse_flag(value)?,
        // html5/xhtml set tag case semantics (mutually exclusive doctypes).
        Opt::Html5 => {
            let on = parse_flag(value)?;
            opts.html5 = on;
            if on {
                opts.xhtml = false;
            }
            opts.lower_case_tags = on;
        }
        Opt::IndentParBreak => opts.indent_par_break = parse_flag(value)?,
        Opt::LangRuns => opts.lang_runs = parse_flag(value)?,
        Opt::LinkOnly => opts.link_only = parse_flag(value)?,
        // Three states rather than a boolean: off, "text" (the default
        // listing) and "json"; the bare flag and `=true` both mean "text".
        Opt::ListTemplates => {
            opts.list_templates = match value {
                "text" | "json" => value.to_string(),
                other => match parse_bool(other) {
                    Ok(true) => "text".to_string(),
                    Ok(false) => String::new(),
                    Err(_) => {
                        return Err(format!(
                            "--list_templates=json or --list_templates=text, got {other:?}"
                        ))
                    }
                },
            };
        }
        Opt::LowerCaseTags => opts.lower_case_tags = parse_flag(value)?,
        Opt::Mailmode => opts.mailmode = parse_flag(value)?,
        Opt::MakeAnchors => opts.make_anchors = parse_flag(value)?,
        Opt::MakeLinks => opts.make_links = parse_flag(value)?,
        Opt::MakeTables => opts.make_tables = parse_flag(value)?,
        Opt::MetaCharset => opts.meta_charset = parse_flag(value)?,
        Opt::NoReferrer => opts.no_referrer = parse_flag(value)?,
        Opt::NumberHeadings => opts.number_headings = parse_flag(value)?,
        Opt::PreserveIndent => opts.preserve_indent = parse_flag(value)?,
        Opt::Preset => opts.preset = value.to_string(),
        Opt::Report => opts.report = parse_flag(value)?,
        Opt::Section => opts.section = parse_flag(value)?,
        Opt::Stream => opts.stream = parse_flag(value)?,
        Opt::Titlefirst => opts.titlefirst = parse_flag(value)?,
        Opt::Toc => opts.toc = parse_flag(value)?,
        Opt::Unhyphenation => opts.unhyphenation = parse_flag(value)?,
        Opt::Utf8 => opts.utf8 = parse_flag(value)?,
        Opt::UseMosaicHeader => opts.use_mosaic_header = parse_flag(value)?,
        Opt::UsePreformatMarker => opts.use_preformat_marker = parse_flag(value)?,
        Opt::Xhtml => {
            let on = parse_flag(value)?;
            opts.xhtml = on;
            opts.html5 = false;
            opts.lower_case_tags = on;
        }

        // --- strings ---
        // Replaces (not accumulates); comma-separated single value.
        Opt::AllowedUrlSchemes => {
            opts.allowed_url_schemes =
                Some(value.split(',').map(|s| s.trim().to_string()).collect());
        }
        Opt::AppendFile => opts.append_file = value.to_string(),
        Opt::AppendHead => opts.append_head = value.to_string(),
        Opt::BodyDeco => opts.body_deco = value.to_string(),
        Opt::BoldDelimiter => opts.bold_delimiter = value.to_string(),
        Opt::Bullets => opts.bullets = value.to_string(),
        Opt::BulletsOrdered => opts.bullets_ordered = value.to_string(),
        Opt::CapsTag => opts.caps_tag = value.to_string(),
        Opt::DefaultLinkDict => opts.default_link_dict = value.to_string(),
        Opt::Doctype => opts.doctype = value.to_string(),
        Opt::DocumentTemplate => opts.document_template = value.to_string(),
        Opt::ItalicDelimiter => opts.italic_delimiter = value.to_string(),
        Opt::Lang => opts.lang = value.to_string(),
        // Unrecognized encoding is a user error.
        Opt::Encoding => opts.encoding = Encoding::parse(value).map_err(|e| e.to_string())?,
        Opt::Outfile => opts.outfile = value.to_string(),
        Opt::PreformatStartMarker => opts.preformat_start_marker = value.to_string(),
        Opt::PreformatEndMarker => opts.preformat_end_marker = value.to_string(),
        Opt::PrependFile => opts.prepend_file = value.to_string(),
        Opt::StyleUrl => opts.style_url = value.to_string(),
        Opt::BodyTemplate => opts.template = value.to_string(),
        Opt::TemplateDir => opts.template_dir.push(value.to_string()),
        Opt::CatalogDir => opts.catalog_dir.push(value.to_string()),
        Opt::TemplateLibrary => opts.template_library = value.to_string(),
        Opt::Title => opts.title = value.to_string(),
        Opt::UnderlineDelimiter => opts.underline_delimiter = value.to_string(),

        // --- integers ---
        Opt::PreformatTriggerLines => {
            opts.preformat_trigger_lines = parse_int(opt, value)?.clamp(-128, 127) as i8;
        }
        Opt::EndpreformatTriggerLines => {
            opts.endpreformat_trigger_lines = parse_int(opt, value)?.clamp(-128, 127) as i8;
        }
        Opt::HruleMin => opts.hrule_min = parse_int(opt, value)?.max(0) as usize,
        Opt::IndentWidth => opts.indent_width = parse_int(opt, value)?.max(0) as usize,
        Opt::MinCapsLength => opts.min_caps_length = parse_int(opt, value)?.max(0) as usize,
        Opt::ParIndent => opts.par_indent = parse_int(opt, value)?.max(0) as usize,
        Opt::PreformatWhitespaceMin => {
            opts.preformat_whitespace_min = parse_int(opt, value)?.max(0) as usize;
        }
        Opt::ShortLineLength => opts.short_line_length = parse_int(opt, value)?.max(0) as usize,
        Opt::TabWidth => opts.tab_width = parse_int(opt, value)?.max(0) as usize,
        Opt::UnderlineLengthTolerance => {
            opts.underline_length_tolerance = parse_int(opt, value)?.max(0) as usize;
        }
        Opt::UnderlineOffsetTolerance => {
            opts.underline_offset_tolerance = parse_int(opt, value)?.max(0) as usize;
        }

        // --- string arrays ---
        Opt::CustomHeadingRegexp => opts.custom_heading_regexp.push(value.to_string()),
        Opt::Infile => opts.infile.push(value.to_string()),
        Opt::Instring => opts.instring.push(value.to_string()),
        Opt::LinksDictionaries => opts.links_dictionaries.push(value.to_string()),
        // Parse name=value eagerly; values may include `=` and newlines. One per occurrence.
        Opt::Var => {
            if value.is_empty() {
                return Ok(());
            }
            let (name, var_value) = value
                .split_once('=')
                .ok_or_else(|| format!("Option var requires name=value, got {value:?}"))?;
            if name.is_empty() {
                return Err("Option var requires a non-empty name before `=`".to_string());
            }
            if !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(format!(
                    "invalid name {name:?} for --var; use letters, digits, _ and -"
                ));
            }
            opts.vars.push((name.to_string(), var_value.to_string()));
        }

        // --- table type ---
        Opt::TableType => set_table_type(opts, value)?,
    }
    Ok(())
}

/// Empty the `StrArray` `opt` names, so a caller that replaces an array can
/// re-add its values. Every [`Kind::StrArray`] option needs an arm: a miss
/// would leave the old list in place and a re-add would double it; the
/// `every_string_array_option_is_cleared` test walks every one.
pub fn clear_array(opts: &mut Options, opt: Opt) {
    match opt {
        Opt::CustomHeadingRegexp => opts.custom_heading_regexp.clear(),
        Opt::Infile => opts.infile.clear(),
        Opt::Instring => opts.instring.clear(),
        Opt::LinksDictionaries => opts.links_dictionaries.clear(),
        Opt::TemplateDir => opts.template_dir.clear(),
        Opt::CatalogDir => opts.catalog_dir.clear(),
        Opt::Var => opts.vars.clear(),
        _ => {}
    }
}

/// Read one option back as text. Exhaustive over [`Opt`], the inverse of
/// [`set_opt`] and so unable to fall out of step with it.
pub fn get_opt(opts: &Options, opt: Opt) -> String {
    match opt {
        Opt::Chunk => opts.chunk.to_string(),
        Opt::ChunkChars => opts.chunk_chars.to_string(),
        Opt::Citations => opts.citations.to_string(),
        Opt::CloseOptionalTags => opts.close_optional_tags.to_string(),
        Opt::Demoronize => opts.demoronize.to_string(),
        Opt::EightBitClean => opts.eight_bit_clean.to_string(),
        Opt::EscapeHtmlChars => opts.escape_html_chars.to_string(),
        Opt::ExplicitHeadings => opts.explicit_headings.to_string(),
        Opt::Extract => opts.extract.to_string(),
        Opt::Glossary => opts.glossary.to_string(),
        Opt::Html5 => opts.html5.to_string(),
        Opt::IndentParBreak => opts.indent_par_break.to_string(),
        Opt::LangRuns => opts.lang_runs.to_string(),
        Opt::LinkOnly => opts.link_only.to_string(),
        // Off reads as "false" like every other flag; a format reads as itself.
        Opt::ListTemplates => {
            if opts.list_templates.is_empty() {
                "false".to_string()
            } else {
                opts.list_templates.clone()
            }
        }
        Opt::LowerCaseTags => opts.lower_case_tags.to_string(),
        Opt::Mailmode => opts.mailmode.to_string(),
        Opt::MakeAnchors => opts.make_anchors.to_string(),
        Opt::MakeLinks => opts.make_links.to_string(),
        Opt::MakeTables => opts.make_tables.to_string(),
        Opt::MetaCharset => opts.meta_charset.to_string(),
        Opt::NoReferrer => opts.no_referrer.to_string(),
        Opt::NumberHeadings => opts.number_headings.to_string(),
        Opt::PreserveIndent => opts.preserve_indent.to_string(),
        Opt::Report => opts.report.to_string(),
        Opt::Section => opts.section.to_string(),
        Opt::Stream => opts.stream.to_string(),
        Opt::Titlefirst => opts.titlefirst.to_string(),
        Opt::Toc => opts.toc.to_string(),
        Opt::Unhyphenation => opts.unhyphenation.to_string(),
        Opt::Utf8 => opts.utf8.to_string(),
        Opt::UseMosaicHeader => opts.use_mosaic_header.to_string(),
        Opt::UsePreformatMarker => opts.use_preformat_marker.to_string(),
        Opt::Xhtml => opts.xhtml.to_string(),

        // Comma-separated list; round-trips to preserve unset/default semantics.
        Opt::AllowedUrlSchemes => opts.allowed_url_schemes.as_deref().unwrap_or(&[]).join(","),
        Opt::AppendFile => opts.append_file.clone(),
        Opt::AppendHead => opts.append_head.clone(),
        Opt::BodyDeco => opts.body_deco.clone(),
        Opt::BoldDelimiter => opts.bold_delimiter.clone(),
        Opt::Bullets => opts.bullets.clone(),
        Opt::BulletsOrdered => opts.bullets_ordered.clone(),
        Opt::CapsTag => opts.caps_tag.clone(),
        Opt::DefaultLinkDict => opts.default_link_dict.clone(),
        Opt::Doctype => opts.doctype.clone(),
        Opt::DocumentTemplate => opts.document_template.clone(),
        Opt::ItalicDelimiter => opts.italic_delimiter.clone(),
        Opt::Lang => opts.lang.clone(),
        Opt::Encoding => opts.encoding.name().to_string(),
        Opt::Outfile => opts.outfile.clone(),
        Opt::PreformatStartMarker => opts.preformat_start_marker.clone(),
        Opt::PreformatEndMarker => opts.preformat_end_marker.clone(),
        Opt::PrependFile => opts.prepend_file.clone(),
        Opt::Preset => opts.preset.clone(),
        Opt::StyleUrl => opts.style_url.clone(),
        Opt::BodyTemplate => opts.template.clone(),
        Opt::TemplateLibrary => opts.template_library.clone(),
        Opt::Title => opts.title.clone(),
        Opt::UnderlineDelimiter => opts.underline_delimiter.clone(),

        Opt::PreformatTriggerLines => opts.preformat_trigger_lines.to_string(),
        Opt::EndpreformatTriggerLines => opts.endpreformat_trigger_lines.to_string(),
        Opt::HruleMin => opts.hrule_min.to_string(),
        Opt::IndentWidth => opts.indent_width.to_string(),
        Opt::MinCapsLength => opts.min_caps_length.to_string(),
        Opt::ParIndent => opts.par_indent.to_string(),
        Opt::PreformatWhitespaceMin => opts.preformat_whitespace_min.to_string(),
        Opt::ShortLineLength => opts.short_line_length.to_string(),
        Opt::TabWidth => opts.tab_width.to_string(),
        Opt::UnderlineLengthTolerance => opts.underline_length_tolerance.to_string(),
        Opt::UnderlineOffsetTolerance => opts.underline_offset_tolerance.to_string(),

        Opt::CustomHeadingRegexp => opts.custom_heading_regexp.join("\n"),
        Opt::Infile => opts.infile.join("\n"),
        Opt::Instring => opts.instring.join("\n"),
        Opt::LinksDictionaries => opts.links_dictionaries.join("\n"),
        Opt::TemplateDir => opts.template_dir.join("\n"),
        Opt::CatalogDir => opts.catalog_dir.join("\n"),
        Opt::Var => opts
            .vars
            .iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("\n"),

        Opt::TableType => format!(
            "ALIGN={} PGSQL={} BORDER={} DELIM={}",
            opts.table_type.align as u8,
            opts.table_type.pgsql as u8,
            opts.table_type.border as u8,
            opts.table_type.delim as u8
        ),
    }
}

/// Where an option came from, for diagnostics.
#[derive(Clone, Debug)]
pub enum Source {
    CommandLine,
    /// A file, with the label the user would recognise (`~/.textrillrc`, ...).
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

/// Parse argument list; errors report file/line when from option file.
pub fn parse_args_from(args: &[String], opts: &mut Options, label: &str) -> Result<(), String> {
    let mut it = args.iter().peekable();
    let mut table_type_seen = false;
    while let Some(arg) = it.next() {
        let at = |e: String| -> String {
            if label.is_empty() {
                e
            } else {
                format!("{label}: {e}")
            }
        };
        let (name, mut inline) = if let Some(rest) = arg.strip_prefix("--") {
            if rest.is_empty() {
                // End of options: everything after -- is input filenames
                for r in it {
                    opts.infile.push(r.clone());
                }
                break;
            }
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

        let (opt, negated) = resolve(name).map_err(at)?;
        if negated && !matches!(opt.kind(), Kind::Flag) {
            return Err(at(format!("Unknown option `{name}`")));
        }

        // --template is deprecated; prefer --body_template or --document_template.
        if opt == Opt::BodyTemplate && !name.is_empty() && "template".starts_with(name) {
            eprintln!(
                "textrill: warning: `--{name}` is deprecated; use --body_template \
                 (body wrap) or --document_template (whole page)"
            );
        }

        match opt.kind() {
            Kind::Flag => {
                // --list_templates[=text|json] names a listing format rather
                // than a boolean: the bare form and `--no-` reduce to on/off,
                // a value goes to the setter, which knows the three states.
                if opt == Opt::ListTemplates {
                    let value = match inline.take() {
                        Some(v) => {
                            if negated {
                                return Err(at(
                                    "--list_templates cannot be combined with --no-".to_string()
                                ));
                            }
                            v
                        }
                        None => (if negated { "false" } else { "true" }).to_string(),
                    };
                    set_opt(opts, opt, &value).map_err(at)?;
                } else {
                    let on = match inline.take() {
                        Some(v) => parse_bool(&v).map_err(at)?,
                        None => true,
                    };
                    let on = if negated { !on } else { on };
                    set_opt(opts, opt, if on { "true" } else { "false" }).map_err(at)?;
                }
            }
            // The remaining kinds take a value: an inline `=value`, or the next token.
            Kind::Str | Kind::Int | Kind::StrArray | Kind::TableType => {
                let v = match inline.take() {
                    Some(v) => v,
                    None => take_value(&mut it, name).map_err(at)?,
                };
                // First --table_type resets defaults; subsequent ones accumulate.
                if opt == Opt::TableType && !table_type_seen {
                    reset_table_type(opts);
                    table_type_seen = true;
                }
                set_opt(opts, opt, &v).map_err(at)?;
            }
        }
    }
    Ok(())
}

/// Look up an option name, handling `no` prefix for boolean flags. Public
/// within the crate because the `--preset` scan reuses the same token rules.
pub(crate) fn resolve(name: &str) -> Result<(Opt, bool), String> {
    let stripped = name
        .strip_prefix("no-")
        .or_else(|| name.strip_prefix("no_"))
        .or_else(|| name.strip_prefix("no"));
    match stripped {
        Some(rest) => match lookup(rest) {
            Some(opt) => Ok((opt, true)),
            None => match lookup(name) {
                Some(opt) => Ok((opt, false)),
                None => Err(format!("Unknown option `{name}`")),
            },
        },
        None => match lookup(name) {
            Some(opt) => Ok((opt, false)),
            None => Err(format!("Unknown option `{name}`")),
        },
    }
}

/// Parse command-line argument list.
pub fn parse_args(args: &[String], opts: &mut Options) -> Result<(), String> {
    parse_args_from(args, opts, "")
}

/// The immediate action a command line requests, if any.
///
/// `--help`, `-h` and `--version` are answered before rc files are read, so a
/// broken rc file cannot get in the way of asking for help. The scan applies
/// the same token rules as [`parse_args_from`]: a bare `--` ends option
/// processing (so a file genuinely named `--help` or `--version` stays a
/// file), and a token that is the value of a value-taking option is skipped
/// (`--title --help` sets the title; it does not ask for help).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EarlyAction {
    None,
    Help,
    Version,
}

pub fn early_action(args: &[String]) -> EarlyAction {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let (name, inline) = if let Some(rest) = arg.strip_prefix("--") {
            if rest.is_empty() {
                // End of options: everything after `--` is a filename.
                return EarlyAction::None;
            }
            match rest.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (rest, None),
            }
        } else if let Some(rest) = arg.strip_prefix('-') {
            if rest.is_empty() {
                continue; // "-" is a filename
            }
            match rest.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (rest, None),
            }
        } else {
            continue; // a filename
        };

        // An option that takes a value swallows the next token, which may
        // itself begin with a dash (`--title --help`), so it is never seen as
        // a request for help here. `--name=value` form carries no next token.
        if inline.is_none() {
            if name == "help" {
                return EarlyAction::Help;
            }
            if name == "version" {
                return EarlyAction::Version;
            }
            if arg == "-h" {
                return EarlyAction::Help;
            }
            if let Some(opt) = lookup(name) {
                if matches!(
                    opt.kind(),
                    Kind::Str | Kind::Int | Kind::StrArray | Kind::TableType
                ) {
                    it.next();
                }
            }
        }
    }
    EarlyAction::None
}

/// Expand `@file` and rc files, then parse in order.
/// Precedence: `@file` < `~/.textrillrc` < `./.textrillrc` < command line.
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
            // Bare `@` is not a group; treated as input file.
            _ => command_line.push(a.clone()),
        }
    }

    for (path, label) in crate::rcfile::rc_files(home, current) {
        read_option_file(&path, &label, opts, true)?;
    }

    // --preset on the command line sits above the rc files and below the rest
    // of the command line: expand it now, then parse every command-line
    // option on top, so precedence holds no matter where the option stands in
    // the line.
    if let Some(name) = crate::presets::scan_preset(&command_line)? {
        crate::presets::apply(opts, &name)?;
    }

    parse_args(&command_line, opts)
}

/// Read and apply an option file (line-oriented).
fn read_option_file(
    path: &std::path::Path,
    label: &str,
    opts: &mut Options,
    optional_missing: bool,
) -> Result<(), String> {
    // rc files are optional; explicit `@file` groups are not.
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
            // Everything after -- is input filenames; option processing ends.
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
        if crate::presets::scan_preset(&tokens)?.is_some() {
            return Err(format!(
                "{where_}: `--preset` belongs on the command line: a preset is \
                 a whole option state, expanded after the rc files are read and \
                 before the command line is parsed"
            ));
        }
        // First --table_type in a file resets defaults; repeats accumulate.
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

/// Set one or more table types from `TYPE=0/1`; whitespace-separated values allowed.
pub fn set_table_type(opts: &mut Options, v: &str) -> Result<(), String> {
    for word in v.split_whitespace() {
        set_one_table_type(opts, word)?;
    }
    Ok(())
}

/// Reset all table type flags; used when first --table_type appears. Invariant:
/// naming the option resets prior defaults; later occurrences accumulate.
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
        "Convert plain text to HTML, inferring structure from layout.  See the\n\
         textrill README for details.\n\n",
    );
    for spec in SPECS {
        let mut names = String::new();
        for (i, n) in spec.names.iter().enumerate() {
            if i > 0 {
                names.push('|');
            }
            names.push_str("--");
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
    // Precedence documented here for visibility.
    s.push_str(
        "\nOptions may also be read from @file groups, and from ~/.textrillrc or\n\
         ./.textrillrc -- one option per line, `#` comments, and a `--` line to end\n\
         option processing.  The legacy names ~/.txt2htmlrc and ./.txt2htmlrc are\n\
         still read when no .textrillrc is present.  Lowest precedence first:\n\
         \n\
         \x20   @file < ~/.textrillrc < ./.textrillrc < command line\n",
    );
    // Encoding detection order: byte-order mark, UTF-16 pattern, UTF-8 validity, then CP1252 guess.
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
