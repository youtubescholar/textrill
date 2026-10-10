//! Options for the txt2html converter, mirroring HTML::TextToHTML v3.0.
//!
//! [`Options::validate`] enforces the numeric and regexp contracts before any
//! output; covered by `tests/cliexit.rs`.

/// A single-byte encoding: the fallback guess and the escape hatch. These cannot
/// be *detected* -- mutually indistinguishable from the bytes -- so `Auto` picks
/// one and the user can name another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleByte {
    Latin1,
    Cp1252,
    Cp1251,
    Cp1253,
    Koi8R,
}

impl SingleByte {
    /// The [`Encoding`] that names this single-byte encoding.
    pub fn encoding(self) -> Encoding {
        match self {
            SingleByte::Latin1 => Encoding::Latin1,
            SingleByte::Cp1252 => Encoding::Cp1252,
            SingleByte::Cp1251 => Encoding::Cp1251,
            SingleByte::Cp1253 => Encoding::Cp1253,
            SingleByte::Koi8R => Encoding::Koi8R,
        }
    }

    /// The canonical name, as `--encoding` spells it.
    pub fn name(self) -> &'static str {
        match self {
            SingleByte::Latin1 => "iso-8859-1",
            SingleByte::Cp1252 => "cp1252",
            SingleByte::Cp1251 => "cp1251",
            SingleByte::Cp1253 => "cp1253",
            SingleByte::Koi8R => "koi8-r",
        }
    }
}

/// How to decode input bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// Detect. In order: a byte-order mark, then the NUL pattern that marks
    /// UTF-16 or UTF-32, then UTF-8 validity, then [`SingleByte::Cp1252`].
    ///
    /// The order matters: a BOM is a writer's declaration, the NUL pattern is a
    /// structural fact, and UTF-8 validity is weaker (ASCII-only UTF-16LE is
    /// also valid UTF-8). The single-byte fallback is last because it is the
    /// only guess.
    #[default]
    Auto,
    /// Decode as UTF-8 unconditionally; invalid bytes are replaced, so this is
    /// lossy and only right when the encoding is already known.
    Utf8,
    /// Decode as CP1252 unconditionally, one byte per code point, even for
    /// bytes that would have been valid UTF-8.
    Cp1252,
    /// Latin-1. A declared Latin-1 file is not second-guessed.
    Latin1,
    /// Windows Cyrillic; differs from CP1252 across most of `0x80`-`0xFF`.
    Cp1251,
    /// Windows Greek.
    Cp1253,
    /// KOI8-R, the other common Russian encoding. CP1251 and KOI8-R disagree
    /// about almost every high byte.
    Koi8R,
    /// UTF-16 little-endian, with or without a BOM. Name it when a BOM-less
    /// file would otherwise be guessed from `Auto`'s NUL heuristic.
    Utf16Le,
    /// UTF-16 big-endian, with or without a BOM.
    Utf16Be,
    /// UTF-32 little-endian. Detected from a BOM; `Auto` does not infer it from
    /// structure.
    Utf32Le,
    /// UTF-32 big-endian, BOM only.
    Utf32Be,
}

impl Encoding {
    /// The name accepted by `--encoding` and reported by `resolved_encoding`.
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Auto => "auto",
            Encoding::Utf8 => "utf-8",
            Encoding::Cp1252 => "cp1252",
            Encoding::Latin1 => SingleByte::Latin1.name(),
            Encoding::Cp1251 => SingleByte::Cp1251.name(),
            Encoding::Cp1253 => SingleByte::Cp1253.name(),
            Encoding::Koi8R => SingleByte::Koi8R.name(),
            Encoding::Utf16Le => "utf-16le",
            Encoding::Utf16Be => "utf-16be",
            Encoding::Utf32Le => "utf-32le",
            Encoding::Utf32Be => "utf-32be",
        }
    }

    /// The single-byte encoding this is, if it names one.
    pub fn single_byte(self) -> Option<SingleByte> {
        match self {
            Encoding::Cp1252 => Some(SingleByte::Cp1252),
            Encoding::Latin1 => Some(SingleByte::Latin1),
            Encoding::Cp1251 => Some(SingleByte::Cp1251),
            Encoding::Cp1253 => Some(SingleByte::Cp1253),
            Encoding::Koi8R => Some(SingleByte::Koi8R),
            _ => None,
        }
    }

    /// How many bytes one code unit occupies: 2 for UTF-16, 4 for UTF-32, 1
    /// otherwise. Used by [`crate::encode`] for surrogate pairs and byte layout.
    pub fn units_per_char(self) -> usize {
        match self {
            Encoding::Utf16Le | Encoding::Utf16Be => 2,
            Encoding::Utf32Le | Encoding::Utf32Be => 4,
            _ => 1,
        }
    }

    /// Parse a `--encoding` value, accepting several spellings of each.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "auto" | "detect" => Ok(Encoding::Auto),
            "utf8" | "utf-8" => Ok(Encoding::Utf8),
            "cp1252" | "windows-1252" | "win1252" | "1252" => Ok(Encoding::Cp1252),
            "latin-1" | "latin1" | "iso-8859-1" | "iso8859-1" | "iso88591" | "8859-1" => {
                Ok(Encoding::Latin1)
            }
            "cp1251" | "windows-1251" | "win1251" | "1251" => Ok(Encoding::Cp1251),
            "cp1253" | "windows-1253" | "win1253" | "1253" => Ok(Encoding::Cp1253),
            "koi8-r" | "koi8r" | "koi8" => Ok(Encoding::Koi8R),
            "utf-16le" | "utf16le" | "utf-16" | "utf16" => Ok(Encoding::Utf16Le),
            "utf-16be" | "utf16be" => Ok(Encoding::Utf16Be),
            "utf-32le" | "utf32le" | "utf-32" | "utf32" => Ok(Encoding::Utf32Le),
            "utf-32be" | "utf32be" => Ok(Encoding::Utf32Be),
            other => Err(format!(
                "Unknown encoding `{other}`; expected one of: \
                 auto, utf-8, utf-16le, utf-16be, utf-32le, utf-32be, \
                 iso-8859-1, cp1252, cp1251, cp1253, koi8-r"
            )),
        }
    }
}

/// Per-table-type enable flags.
#[derive(Debug, Clone, Copy)]
pub struct TableTypeFlags {
    pub align: bool,
    pub pgsql: bool,
    pub border: bool,
    pub delim: bool,
}

impl Default for TableTypeFlags {
    fn default() -> Self {
        TableTypeFlags {
            align: true,
            pgsql: true,
            border: true,
            delim: true,
        }
    }
}

/// The upper bound for numeric options used to build a regex quantifier, an
/// allocation, or a divisor. See [`Options::validate`].
pub const MAX_NUMERIC_OPTION: usize = 999;

/// The accepted range of each numeric option that cannot be arbitrary, as
/// `(option, low, high)`.
///
/// Single source of truth, enforced by [`Options::validate`] and handed to front
/// ends. Only options that reach a hazard appear; the rest are used solely in
/// comparisons and stay unbounded.
///
/// `tab_width` is the divisor in `tab % tw` and a space-repeat size;
/// `indent_width` a space-repeat size (0 is fine); the other three are
/// interpolated into a regex quantifier, with the ceiling checked in
/// `tests/cliexit.rs`.
pub const NUMERIC_RANGES: &[(&str, usize, usize)] = &[
    ("tab_width", 1, MAX_NUMERIC_OPTION),
    ("indent_width", 0, MAX_NUMERIC_OPTION),
    ("preformat_whitespace_min", 0, MAX_NUMERIC_OPTION),
    ("hrule_min", 0, MAX_NUMERIC_OPTION),
    ("min_caps_length", 0, MAX_NUMERIC_OPTION),
];

/// The accepted range of `name`, if it has one.
pub fn numeric_range(name: &str) -> Option<(usize, usize)> {
    NUMERIC_RANGES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, lo, hi)| (*lo, *hi))
}

/// All conversion options, with the same defaults as HTML::TextToHTML v3.0.
#[derive(Debug, Clone)]
pub struct Options {
    /// Schemes permitted in a generated `href`, comma separated; default
    /// [`urlscheme::DEFAULT_ALLOWED_SCHEMES`]. Setting this **replaces** the list,
    /// so it can only make the policy stricter. An anchor with any other scheme
    /// is unwrapped and reported once on standard error.
    ///
    /// Spelled out rather than "unset" so a front end can round trip it; see
    /// [`crate::urlscheme`] for why the check scans finished markup.
    pub allowed_url_schemes: Option<Vec<String>>,
    pub append_file: String,
    pub append_head: String,
    pub body_deco: String,
    pub bullets: String,
    pub bullets_ordered: String,
    pub bold_delimiter: String,
    /// Collect `{{textrill:cite:key}}` references into a numbered endnotes list.
    /// Default **off**: markers stay literal text. Refused with `--chunk` and
    /// `--stream`, which cannot afford the document-wide pass numbering needs.
    pub citations: bool,
    pub caps_tag: String,
    pub custom_heading_regexp: Vec<String>,
    pub default_link_dict: String,
    pub demoronize: bool,
    pub doctype: String,
    /// Path to a wrapper template inserted inside `<body>`. It must contain a
    /// `{{textrill:content}}` slot and may use the `toc`, `title`, `head` and
    /// `pager` slots; the engine still emits the doctype, `<head>` and `<body>`.
    /// Empty (the default) is byte-identical to the reference.
    pub template: String,
    /// Path to a whole-document template: it owns doctype, head and body, so no
    /// engine prolog is emitted. Mutually exclusive with [`Options::template`].
    /// Empty by default.
    pub document_template: String,
    /// A shipped template's name (`article`, `book`, `manpage`, `slide`, `bare`)
    /// from [`crate::library`]. Exactly one of [`Options::template`],
    /// [`Options::document_template`] and this may be set; its model decides body
    /// vs whole document. Uses only the fixed slots. Empty by default.
    pub template_library: String,
    /// Template parameters, in `--var` order, substituted verbatim for
    /// `{{textrill:var:name}}` in the active template. Empty by default.
    pub vars: Vec<(String, String)>,
    pub eight_bit_clean: bool,
    pub escape_html_chars: bool,
    pub explicit_headings: bool,
    pub extract: bool,
    /// Collect `{{textrill:gloss:term}}` references into a definition list,
    /// independent of [`Options::citations`]. Default **off**; refused with
    /// `--chunk` and `--stream` (see [`Options::citations`]).
    pub glossary: bool,
    /// Print what the conversion recovered -- bytes, headings, paragraphs,
    /// capitalised runs and line breaks -- as one `key=value` line on stderr
    /// once the output is written. Counts are of the produced document, not
    /// engine events ([`crate::report`]); default **off**, refused with `--stream`.
    pub report: bool,
    /// Wrap each heading-delimited section of the body in an
    /// `<article class="section" id="chunk-N">` element. Default **off**: the
    /// markup is HTML5-flavoured and would move the reference goldens.
    pub section: bool,
    /// Prepend a generated table of contents linking each section. Implies
    /// sectioning, because the links target the `chunk-N` ids it assigns.
    /// Default **off**.
    pub toc: bool,
    /// Write one HTML file per top-level section instead of a single document.
    /// Requires a file `--outfile`; not valid with `--extract` or stdout.
    /// Default **off**.
    pub chunk: bool,
    /// Prefix each heading with its hierarchical number (`1`, `1.1`, …).
    /// Composes with `--toc`, whose labels then carry the numbers. Default
    /// **off**.
    pub number_headings: bool,
    /// Read the input and write the output a paragraph at a time, so a very large
    /// UTF-8 file can be piped without holding the whole document in memory.
    /// Refused with `--instring` and the whole-body passes `--number_headings`,
    /// `--section`, `--toc` and `--chunk`. Default **off**.
    pub stream: bool,
    pub hrule_min: usize,
    /// Emit an HTML5 document: `<!DOCTYPE html>`, an `<html>` element without the
    /// XHTML namespace, and `<meta charset="utf-8">`. Default **on**;
    /// `--no-html5`/`--no-xhtml` select the reference's HTML 4.01, `--xhtml`
    /// selects XHTML. Tag case follows [`Options::lower_case_tags`].
    pub html5: bool,
    pub indent_width: usize,
    pub indent_par_break: bool,
    pub italic_delimiter: String,
    /// How `convert::read_any_file` decodes input that is not valid UTF-8, and
    /// what to assume for text handed over in memory. `Auto` is the default;
    /// `Utf8` and `Cp1252` force the fallback, needed when a CP1252 file is also
    /// valid UTF-8 and the probe cannot tell.
    pub encoding: Encoding,
    /// Emit `<meta charset="utf-8">` in the document head. Default **off**: the
    /// reference emits no charset declaration, so enabling it would move every
    /// golden. A GUI turns it on for its browser consumer.
    pub meta_charset: bool,
    pub links_dictionaries: Vec<String>,
    pub link_only: bool,
    pub lower_case_tags: bool,
    pub mailmode: bool,
    pub make_anchors: bool,
    pub make_links: bool,
    pub make_tables: bool,
    pub min_caps_length: usize,
    pub outfile: String,
    pub par_indent: usize,
    pub preformat_trigger_lines: i8,
    pub endpreformat_trigger_lines: i8,
    pub preformat_start_marker: String,
    pub preformat_end_marker: String,
    pub preformat_whitespace_min: usize,
    pub prepend_file: String,
    pub preserve_indent: bool,
    pub short_line_length: usize,
    pub style_url: String,
    pub tab_width: usize,
    pub table_type: TableTypeFlags,
    pub title: String,
    pub titlefirst: bool,
    pub underline_delimiter: String,
    pub underline_length_tolerance: usize,
    pub underline_offset_tolerance: usize,
    pub unhyphenation: bool,
    pub use_mosaic_header: bool,
    pub use_preformat_marker: bool,
    pub xhtml: bool,

    // Input / output sources
    pub infile: Vec<String>,
    pub instring: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        Options {
            // `None` is the default tier, not "allow everything"; an empty list
            // resolves to the same tier.
            allowed_url_schemes: None,
            append_file: String::new(),
            append_head: String::new(),
            body_deco: String::new(),
            bullets: "-=o*\u{00b7}".to_string(),
            bullets_ordered: String::new(),
            bold_delimiter: "#".to_string(),
            caps_tag: "STRONG".to_string(),
            custom_heading_regexp: Vec::new(),
            default_link_dict: default_link_dict(&home),
            demoronize: true,
            doctype: "-//W3C//DTD HTML 4.01//EN\"\n\"http://www.w3.org/TR/html4/strict.dtd"
                .to_string(),
            template: String::new(),
            document_template: String::new(),
            template_library: String::new(),
            vars: Vec::new(),
            eight_bit_clean: false,
            escape_html_chars: true,
            explicit_headings: false,
            extract: false,
            section: false,
            toc: false,
            citations: false,
            chunk: false,
            number_headings: false,
            stream: false,
            hrule_min: 4,
            glossary: false,
            html5: true,
            indent_width: 2,
            indent_par_break: false,
            italic_delimiter: "*".to_string(),
            encoding: Encoding::Auto,
            meta_charset: false,
            links_dictionaries: Vec::new(),
            link_only: false,
            lower_case_tags: true,
            mailmode: false,
            make_anchors: true,
            make_links: true,
            make_tables: false,
            min_caps_length: 3,
            outfile: "-".to_string(),
            par_indent: 2,
            preformat_trigger_lines: 2,
            endpreformat_trigger_lines: 2,
            preformat_start_marker: "^(:?(:?&lt;)|<)PRE(:?(:?&gt;)|>)$".to_string(),
            preformat_end_marker: "^(:?(:?&lt;)|<)/PRE(:?(:?&gt;)|>)$".to_string(),
            preformat_whitespace_min: 5,
            prepend_file: String::new(),
            preserve_indent: false,
            report: false,
            short_line_length: 40,
            style_url: String::new(),
            tab_width: 8,
            table_type: TableTypeFlags::default(),
            title: String::new(),
            titlefirst: false,
            underline_delimiter: "_".to_string(),
            underline_length_tolerance: 1,
            underline_offset_tolerance: 1,
            unhyphenation: true,
            use_mosaic_header: false,
            use_preformat_marker: false,
            xhtml: false,
            infile: Vec::new(),
            instring: Vec::new(),
        }
    }
}

/// The default link-dictionary location: textrill's own name if that file
/// exists, else the legacy `.txt2html.dict`, else the textrill name. With `HOME`
/// unset the name is relative, resolving against the working directory.
fn default_link_dict(home: &str) -> String {
    const NAMES: [&str; 2] = [".textrill.dict", ".txt2html.dict"];
    if home.is_empty() {
        for n in NAMES {
            if std::path::Path::new(n).exists() {
                return n.to_string();
            }
        }
        return NAMES[0].to_string();
    }
    for n in NAMES {
        let p = format!("{home}/{n}");
        if std::path::Path::new(&p).exists() {
            return p;
        }
    }
    format!("{home}/{}", NAMES[0])
}

/// Normalization / post-processing of options done once, mirroring
/// `deal_with_options` in HTML::TextToHTML.
impl Options {
    /// Reject option values that cannot be arbitrary.
    ///
    /// A bad value is a user error reported with the option, value and acceptable
    /// range -- not a panic or abort. Only options that size an allocation are
    /// bounded; the rest are comparison-only and stay unbounded to match the
    /// reference. 999 is the ceiling the GUI already assumed.
    ///
    /// It also compiles each caller-supplied regexp ([`Options::user_patterns`])
    /// so an invalid one is reported before any output; the two lists are kept
    /// honest by `every_regexp_option_is_validated` in `tests/cliexit.rs`.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in self.numeric_values() {
            if let Some((low, high)) = numeric_range(name) {
                if value < low || value > high {
                    return Err(format!(
                        "{name} must be between {low} and {high}, got {value}"
                    ));
                }
            }
        }
        for (name, pattern) in self.user_patterns() {
            crate::links::try_compile_pattern(pattern, false)
                .map_err(|e| format!("{name}: invalid regular expression {pattern:?}: {e}"))?;
        }
        self.validate_style_url()?;
        self.validate_template_options()?;
        self.validate_notes_options()?;
        Ok(())
    }

    /// The scheme policy a conversion runs under, read once and handed to both
    /// the dictionary loader and the scrub pass so the two cannot disagree.
    pub fn url_policy(&self) -> crate::urlscheme::UrlPolicy {
        match &self.allowed_url_schemes {
            // An empty or blank list is the default tier too, so a value that
            // has been persisted and read back unchanged cannot weaken the
            // policy.
            Some(list) => crate::urlscheme::UrlPolicy::strict(list),
            None => crate::urlscheme::UrlPolicy::default(),
        }
    }

    /// `--style_url` is pure operator input, so it is refused outright rather
    /// than scrubbed: there is no document text to preserve, and a silent drop
    /// would leave a stylesheet unexplained. A relative value has no scheme and
    /// passes.
    fn validate_style_url(&self) -> Result<(), String> {
        if self.style_url.is_empty() {
            return Ok(());
        }
        let policy = self.url_policy();
        if policy.allows(&self.style_url) {
            return Ok(());
        }
        Err(format!(
            "--style_url {:?} uses a scheme this conversion refuses ({}). \
             Name it in --allowed_url_schemes to keep the stylesheet.",
            self.style_url,
            policy.describe()
        ))
    }

    /// Refuse the note modes with `--chunk` and `--stream`.
    ///
    /// Numbering follows first reference, so it needs the whole document;
    /// refusing before any byte is written keeps a broken note set from
    /// producing output.
    fn validate_notes_options(&self) -> Result<(), String> {
        for (flag, on) in [
            ("--citations", self.citations),
            ("--glossary", self.glossary),
        ] {
            if !on {
                continue;
            }
            if self.chunk {
                return Err(format!("{flag} is not valid with --chunk"));
            }
            if self.stream {
                return Err(format!("{flag} is not valid with --stream"));
            }
        }
        Ok(())
    }

    /// Check the template options and, when one is set, load it and check its
    /// slots, so a bad template is reported before any output. The three sources
    /// are mutually exclusive and each is refused with `--extract`, `--chunk` and
    /// `--stream`; a whole-page template also with `--prepend_file`.
    fn validate_template_options(&self) -> Result<(), String> {
        let (whole_document, body, flag) = self.template_source()?;
        if whole_document == 2 {
            return Ok(());
        }
        if self.extract {
            return Err(format!("{flag} is not valid with --extract"));
        }
        if self.chunk {
            return Err(format!("{flag} is not valid with --chunk"));
        }
        if self.stream {
            return Err(format!("{flag} is not valid with --stream"));
        }
        if whole_document == 1 && !self.prepend_file.is_empty() {
            return Err(format!("{flag} is not valid with --prepend_file"));
        }
        let declared = self
            .vars
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>();
        crate::template::validate(&body, &declared)
    }

    /// Resolve which template is active, if any, and its text.
    ///
    /// Returns `(whole_document, template text, flag name)`, where
    /// `whole_document` is 0 for a body wrapper, 1 for a whole-page template, 2
    /// for none. An unknown library name is an error naming the library.
    pub fn template_source(&self) -> Result<(u8, String, String), String> {
        let wrapper = !self.template.is_empty();
        let whole = !self.document_template.is_empty();
        let library = !self.template_library.is_empty();
        let set = usize::from(wrapper) + usize::from(whole) + usize::from(library);
        if set == 0 {
            return Ok((2, String::new(), String::new()));
        }
        if set > 1 {
            return Err(
                "--body_template, --document_template and --template_library cannot be combined"
                    .to_string(),
            );
        }
        if wrapper {
            let body = std::fs::read_to_string(&self.template)
                .map_err(|e| format!("{}: {e}", self.template))?;
            return Ok((0, body, "--body_template".to_string()));
        }
        if whole {
            let body = std::fs::read_to_string(&self.document_template)
                .map_err(|e| format!("{}: {e}", self.document_template))?;
            return Ok((1, body, "--document_template".to_string()));
        }
        let shipped = crate::library::get(&self.template_library).ok_or_else(|| {
            let list = crate::library::names().collect::<Vec<_>>().join(", ");
            format!(
                "unknown shipped template {:?}; available: {list}",
                self.template_library
            )
        })?;
        let whole = matches!(shipped.model, crate::library::Model::Document);
        let flag = format!("--template_library {}", shipped.name);
        Ok((u8::from(whole), shipped.html.to_string(), flag))
    }

    /// The options whose value is a caller-supplied regular expression, as
    /// `(option, pattern)`. This explicit list is checked against
    /// [`Options::REGEXP_OPTIONS`] and the CLI table by `tests/cliexit.rs`; each
    /// pattern reaches [`compile_user_pattern`], so an uncompilable one cannot pass.
    pub fn user_patterns(&self) -> Vec<(&'static str, &str)> {
        let mut v: Vec<(&'static str, &str)> = Vec::new();
        for pattern in &self.custom_heading_regexp {
            v.push(("custom_heading_regexp", pattern.as_str()));
        }
        v.push((
            "preformat_start_marker",
            self.preformat_start_marker.as_str(),
        ));
        v.push(("preformat_end_marker", self.preformat_end_marker.as_str()));
        v
    }

    /// The canonical long name of every option that takes a regular expression,
    /// hand-maintained so `tests/cliexit.rs` can assert [`Options::user_patterns`]
    /// covers all of it. Aliases name the same field and are not listed.
    pub const REGEXP_OPTIONS: &[&str] = &[
        "custom_heading_regexp",
        "preformat_start_marker",
        "preformat_end_marker",
    ];

    /// The numeric options and their current values, for validation and for
    /// reporting.
    fn numeric_values(&self) -> Vec<(&'static str, usize)> {
        let v = [
            ("hrule_min", self.hrule_min),
            ("indent_width", self.indent_width),
            ("min_caps_length", self.min_caps_length),
            ("par_indent", self.par_indent),
            ("preformat_whitespace_min", self.preformat_whitespace_min),
            ("short_line_length", self.short_line_length),
            ("tab_width", self.tab_width),
            (
                "underline_length_tolerance",
                self.underline_length_tolerance,
            ),
            (
                "underline_offset_tolerance",
                self.underline_offset_tolerance,
            ),
        ];
        v.to_vec()
    }

    pub fn deal_with_options(&mut self) {
        if !self.make_links {
            self.links_dictionaries.clear();
        }
        self.preformat_trigger_lines = self.preformat_trigger_lines.clamp(0, 2);
        if self.preformat_trigger_lines == 0 {
            self.endpreformat_trigger_lines = 1;
        }
        self.endpreformat_trigger_lines = self.endpreformat_trigger_lines.clamp(0, 2);
        // XHTML implies lower case and disables HTML5: the two doctypes are
        // mutually exclusive, so a struct built directly with `xhtml: true`
        // must not keep the default `html5: true`.
        if self.xhtml {
            self.html5 = false;
            self.lower_case_tags = true;
        }
    }
}

#[cfg(test)]
mod dict_default_tests {
    use super::default_link_dict;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("textrill-dict-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn home_of(d: &std::path::Path) -> &str {
        d.to_str().expect("utf-8 temp path")
    }

    #[test]
    fn the_textrill_name_wins_where_both_dictionaries_exist() {
        let d = scratch("both");
        std::fs::write(d.join(".textrill.dict"), "").unwrap();
        std::fs::write(d.join(".txt2html.dict"), "").unwrap();
        let got = default_link_dict(home_of(&d));
        assert!(got.ends_with("/.textrill.dict"), "{got}");
    }

    #[test]
    fn the_legacy_dictionary_is_used_when_only_that_one_exists() {
        let d = scratch("legacy");
        std::fs::write(d.join(".txt2html.dict"), "").unwrap();
        let got = default_link_dict(home_of(&d));
        assert!(got.ends_with("/.txt2html.dict"), "{got}");
    }

    #[test]
    fn with_nothing_on_disk_the_default_is_the_textrill_name() {
        let d = scratch("empty");
        let got = default_link_dict(home_of(&d));
        assert!(got.ends_with("/.textrill.dict"), "{got}");
        assert!(!got.contains("txt2html"), "{got}");
    }

    #[test]
    fn without_a_home_the_name_is_relative_to_the_working_directory() {
        assert_eq!(default_link_dict(""), ".textrill.dict");
    }
}
