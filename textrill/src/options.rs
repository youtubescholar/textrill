//! Options for the txt2html converter.
//!
//! Mirrors the option set of HTML::TextToHTML v3.0.

/// A single-byte encoding: the guess, and the escape hatch.
///
/// None of these can be *detected*. They are mutually indistinguishable from
/// the bytes alone — a CP1251 file is a valid CP1252 file, with different
/// meanings for about 60 of its 128 high bytes — so `Auto` picks one and the
/// user can name another.
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

/// P7.3. How to decode input bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// Detect. In order: a byte-order mark, then the NUL pattern that marks
    /// UTF-16 or UTF-32, then UTF-8 validity, then [`SingleByte::Cp1252`].
    ///
    /// The ordering is the whole point and it is not arbitrary. A BOM is a
    /// declaration by the writer, so it outranks every heuristic. The NUL
    /// pattern is a structural fact about the bytes. UTF-8 validity is a weaker
    /// signal than either, because UTF-16LE containing only ASCII is *also* valid
    /// UTF-8 — so it cannot be checked first, which is exactly how a BOM-less
    /// UTF-16 file used to decode as UTF-8 and reach the output with a NUL
    /// between every character.
    ///
    /// The single-byte fallback is last because it is the only guess in the
    /// list. It is right for Western European text and wrong for Cyrillic,
    /// Greek or Turkish, which is why those are selectable instead.
    #[default]
    Auto,
    /// Decode as UTF-8 unconditionally. Bytes that are not valid UTF-8 are
    /// replaced rather than interpreted, so this is lossy by construction and
    /// is only right when the caller already knows the encoding.
    Utf8,
    /// Decode as CP1252 unconditionally, one byte to one code point, including
    /// for bytes that would have been valid UTF-8. This is how to read a
    /// CP1252 file that the UTF-8 probe would have misjudged.
    Cp1252,
    /// Latin-1. Kept as a real option rather than a historical curiosity: a file
    /// that declares it should not be second-guessed, and it is what CP1252 was
    /// mistaken for until P7.1.
    Latin1,
    /// Windows Cyrillic. Differs from CP1252 across most of `0x80`-`0xFF`, so a
    /// bare Russian file is unreadable without naming it.
    Cp1251,
    /// Windows Greek.
    Cp1253,
    /// KOI8-R, the other common Russian encoding and the one Russian *Linux*
    /// uses. CP1251 and KOI8-R disagree about almost every high byte, so naming
    /// the wrong one is not a small error.
    Koi8R,
    /// UTF-16 little-endian, with or without a BOM. Named explicitly when the
    /// file has no BOM and `Auto`'s NUL heuristic would otherwise be the only
    /// evidence.
    Utf16Le,
    /// UTF-16 big-endian, with or without a BOM.
    Utf16Be,
    /// UTF-32 little-endian. Detected from a BOM; near-impossible to infer
    /// reliably from structure alone, so `Auto` does not try.
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

    /// Parse a `--encoding` value. Accepts several spellings of each, because a
    /// user naming an encoding should not have to know which one the
    /// implementation happens to prefer — and because "latin-1" is what most
    /// people type for what this port has been calling CP1252.
    /// How many bytes one code unit occupies, for the wide encodings.
    ///
    /// 2 for UTF-16 and 4 for UTF-32. Used by [`crate::encode`] to decide
    /// whether a character needs a surrogate pair, and to lay out the bytes.
    pub fn units_per_char(self) -> usize {
        match self {
            Encoding::Utf16Le | Encoding::Utf16Be => 2,
            Encoding::Utf32Le | Encoding::Utf32Be => 4,
            _ => 1,
        }
    }

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

/// The upper bound for every numeric option that is used to build a regex
/// quantifier, an allocation, or a divisor. See [`Options::validate`].
pub const MAX_NUMERIC_OPTION: usize = 999;

/// The accepted range of each numeric option that cannot be arbitrary, as
/// `(option, low, high)`.
///
/// This is the single source of truth: the engine enforces it in
/// [`Options::validate`], and the extension module hands it to a front end so a
/// GUI builds its spin boxes from the same numbers. It used to be hardcoded in
/// the GUI at 0..=999, which is how `tab_width=0` reached the engine at all.
///
/// Only options that reach a hazard appear here. The other numeric options are
/// used solely in comparisons and are harmless at any value, so they stay
/// unbounded and the port keeps accepting what the reference accepts.
///
/// * `tab_width` is the divisor in `tab % tw`, and the size of a space repeat.
/// * `indent_width` is the size of a space repeat; 0 is genuinely fine.
/// * The three below are interpolated into a regex quantifier, which the regex
///   engine refuses above some size. That limit is pattern-dependent rather than
///   a single number -- `\s{200000,}` is rejected while `[A-Z]{200000,}` is
///   accepted -- so this ceiling is chosen with margin and checked in
///   `tests/cliexit.rs`, not derived from the engine's exact threshold.
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
    /// A11. Schemes permitted in a generated `href`, comma separated.
    ///
    /// The default is [`urlscheme::DEFAULT_ALLOWED_SCHEMES`] spelled out rather
    /// than "unset", and that is deliberate. A front end reads the defaults with
    /// `cli::get_value` and writes them back, and it persists them to its own
    /// settings file. With an "unset means the default" encoding, the empty
    /// string that `get_value` returns for unset would be written back as an
    /// *explicitly empty* list, and an empty list means "refuse nothing" -- so
    /// saving the settings once would quietly turn the policy off. Carrying the
    /// list makes the round trip lossless in meaning as well as in text.
    ///
    /// An anchor using any other scheme is unwrapped -- the tags go, the text
    /// stays -- and the scheme is reported once on standard error.
    ///
    /// Setting this **replaces** the list rather than adding to it, so
    /// `https` alone also stops `http`, `ftp` and `mailto` from being linked.
    /// That is the useful direction: the option's job is to be able to make the
    /// policy stricter, and an additive option could not. The diagnostics say so
    /// on every rule they drop, because forgetting the other schemes is the
    /// obvious mistake.
    ///
    /// This exists because two of the engine's inputs are not the operator's
    /// text: the document being converted, and a link dictionary. `<URL:…>`
    /// in ordinary prose became a live `javascript:` link in the reference, so
    /// the default refuses every script-bearing scheme. See
    /// [`crate::urlscheme`] for why the check is a scan over the finished markup.
    pub allowed_url_schemes: Option<Vec<String>>,
    pub append_file: String,
    pub append_head: String,
    pub body_deco: String,
    pub bullets: String,
    pub bullets_ordered: String,
    pub bold_delimiter: String,
    /// Collect `{{textrill:cite:key}}` references into a numbered endnotes list.
    ///
    /// The source syntax, the rendering and the reasons each mistake is refused
    /// rather than guessed at are in [`crate::notes`]. Default **off**: with the
    /// mode off every marker is left in the output as literal text, so a document
    /// that does not use notes converts byte for byte as before.
    ///
    /// Refused with `--chunk` and `--stream`. `--chunk` would have to collect
    /// across the whole input to number references before splitting it, and
    /// `--stream` has already written bytes by the time the list is complete.
    /// Both would need a document-wide pass, which is the one thing those modes
    /// exist to avoid.
    pub citations: bool,
    pub caps_tag: String,
    pub custom_heading_regexp: Vec<String>,
    pub default_link_dict: String,
    pub demoronize: bool,
    pub doctype: String,
    /// P5.5. Path to a wrapper template inserted inside `<body>`.
    ///
    /// The template owns the arrangement of the body: it must contain a
    /// `{{textrill:content}}` slot, and may use `{{textrill:toc}}`,
    /// `{{textrill:title}}`, `{{textrill:head}}` and `{{textrill:pager}}`. The
    /// engine still emits the doctype, `<head>` and the `<body>` tags. Empty
    /// (the default) means output is byte-identical to the reference.
    pub template: String,
    /// P5.5. Path to a whole-document template.
    ///
    /// Like [`Options::template`], but the template owns the entire page
    /// (doctype, head and body included), so none of the engine prolog is
    /// emitted. Mutually exclusive with [`Options::template`]. Empty by default.
    pub document_template: String,
    /// P5.5. Template parameters, in `--var` order, substituted verbatim for
    /// `{{textrill:var:name}}` in the active template. Empty by default.
    pub vars: Vec<(String, String)>,
    pub eight_bit_clean: bool,
    pub escape_html_chars: bool,
    pub explicit_headings: bool,
    pub extract: bool,
    /// Collect `{{textrill:gloss:term}}` references into a definition list.
    ///
    /// Independent of [`Options::citations`]: either mode, both or neither may
    /// be on. Markers of a mode that is off stay literal text, so enabling
    /// `--citations` never draws the glossary markers into it. Default **off**.
    /// Refused with `--chunk` and `--stream`, for the reasons given on
    /// [`Options::citations`].
    pub glossary: bool,
    /// Phase 5. Wrap each heading-delimited section of the body in an
    /// `<article class="section" id="chunk-N">` element.
    ///
    /// Default **off**: the section markup is HTML5-flavoured and is intended
    /// to be used with `--html5`. Off by default so the reference goldens do
    /// not move.
    pub section: bool,
    /// Phase 5. Prepend a generated table of contents linking each section.
    ///
    /// Implies sectioning, because the links target the `chunk-N` ids that
    /// sectioning assigns. Default **off**.
    pub toc: bool,
    /// Phase 5. Write one HTML file per top-level section instead of a single
    /// document. Requires a file `--outfile`; not valid with `--extract` or
    /// with output to standard output. Default **off**.
    pub chunk: bool,
    /// P5.3. Prefix each heading with its hierarchical number (`1`, `1.1`, …).
    ///
    /// Composes with `--toc`, whose labels then carry the numbers. Default
    /// **off** so the reference goldens do not move.
    pub number_headings: bool,
    /// P5.4. Read the input and write the output a paragraph at a time, so a
    /// very large UTF-8 file can be piped without holding the whole document
    /// and its markup in memory.
    ///
    /// Only meaningful for file/stdin input that is UTF-8 (decoded lossily, as
    /// `--encoding utf-8` is); it is refused with `--instring` and with the
    /// whole-body passes `--number_headings`, `--section`, `--toc` and
    /// `--chunk`, which need the body in hand. Default **off**: the buffered
    /// path is unchanged and the output is byte-identical either way.
    pub stream: bool,
    pub hrule_min: usize,
    /// P5.1. Emit an HTML5 document: `<!DOCTYPE html>`, an `<html>` element
    /// without the XHTML namespace, and `<meta charset="utf-8">`.
    ///
    /// Default **off**. Byte-identical output from the reference is the port's
    /// contract, so HTML5 is strictly opt-in; turning it on by default would
    /// move every golden. The tag and attribute case still follows
    /// `lower_case_tags`, so `--html5 --lower_case_tags` gives idiomatic
    /// lower-case HTML5 and `--html5` alone gives valid but upper-case tags.
    pub html5: bool,
    pub indent_width: usize,
    pub indent_par_break: bool,
    pub italic_delimiter: String,
    /// P7.3. How `convert::read_any_file` should decode input that is not valid
    /// UTF-8, and what to assume for text handed over in memory.
    ///
    /// `Auto` is the historical behaviour and the default. `Utf8` and `Cp1252`
    /// force the fallback rather than probing, which matters when the probe is
    /// wrong — a CP1252 file whose bytes happen to be valid UTF-8 is
    /// indistinguishable from a UTF-8 file by inspection, and no amount of
    /// probing settles it.
    pub encoding: Encoding,
    /// P7.4. Emit `<meta charset="utf-8">` in the document head.
    ///
    /// Default **off**, and that is a compatibility decision rather than a
    /// preference: byte-identical output from the reference is a stated goal of
    /// this port, and the reference emits no charset declaration, so turning
    /// this on by default would move every golden. A GUI turns it on, because
    /// there the consumer is a browser that is about to guess.
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
            // A11. `None` is the default tier, not "allow everything" -- see
            // the field's doc comment. An empty list resolves to the same tier,
            // which is what makes a front end that persists this value safe.
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
            html5: false,
            indent_width: 2,
            indent_par_break: false,
            italic_delimiter: "*".to_string(),
            encoding: Encoding::Auto,
            meta_charset: false,
            links_dictionaries: Vec::new(),
            link_only: false,
            lower_case_tags: false,
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
            xhtml: true,
            infile: Vec::new(),
            instring: Vec::new(),
        }
    }
}

/// The default link-dictionary location.
///
/// The option points at a single file, so the default has to choose: textrill's
/// own name if that file exists, the legacy `.txt2html.dict` if only that one
/// does, and the textrill name when neither does. A user who has never heard of
/// textrill still has `~/.txt2html.dict` from the Perl tool and it keeps being
/// read; a user who has written both gets theirs read once, for the reason
/// given at [`crate::rcfile`'s `RC_NAMES`].
///
/// With `HOME` unset the name is relative, so it resolves against the working
/// directory -- which is what the legacy default did.
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
    /// A bad value here is a user error, and it is reported as a message naming
    /// the option, its value and the acceptable range -- not as a panic and not
    /// as an abort. Both of those were reachable from the command line:
    /// `tab_width=0` reached `tab % tw` and divided by zero (exit 101), and a
    /// large value reached `" ".repeat(tw)` and asked the allocator for an
    /// impossible size, which aborts with SIGABRT (exit 134) and cannot be
    /// caught. The GUI could produce `tab_width=0` directly, because its
    /// spin box minimum was 0 for every numeric option.
    ///
    /// Only the two options that size an allocation are bounded. The other
    /// seven numeric options are used solely in comparisons -- thresholds and
    /// lengths, never a size -- so they are harmless at any value, and bounding
    /// them would be stricter than the reference for no safety gain. The
    /// reference accepts them, so the port does too.
    ///
    /// 999 is the ceiling the GUI already assumed, so the two agree without
    /// inventing a second number.
    ///
    /// The second loop compiles each caller-supplied regular expression
    /// ([`Options::user_patterns`]). An uncompilable pattern was the last way a
    /// caller could take the process down: the engine compiled it lazily, so a
    /// bad `custom_heading_regexp` panicked part-way through a conversion rather
    /// than being rejected, and a GUI user could type one in. Compiling it here
    /// reports it before any output exists, which is the same contract as the
    /// bounds above -- and it diverges from the reference on purpose, since Perl
    /// interpolates the pattern into a match, warns, and carries on with the
    /// pattern silently inactive.
    ///
    /// The list is kept honest by `every_regexp_option_is_validated` in
    /// `tests/cliexit.rs`, which compares it against the CLI option table.
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

    /// The scheme policy a conversion runs under.
    ///
    /// Read once per conversion and handed to both the dictionary loader and the
    /// scrub pass, so the two cannot disagree about what is allowed.
    pub fn url_policy(&self) -> crate::urlscheme::UrlPolicy {
        match &self.allowed_url_schemes {
            // An empty or blank list is the default tier too, so a value that
            // has been persisted and read back unchanged cannot turn a
            // front end's "unset" into something weaker.
            Some(list) => crate::urlscheme::UrlPolicy::strict(list),
            None => crate::urlscheme::UrlPolicy::default(),
        }
    }

    /// A11. `--style_url` is the one `href` that is pure operator input, so it
    /// is refused outright rather than scrubbed: there is no document text to
    /// preserve and a silent drop would leave the operator wondering why their
    /// stylesheet is not applied.
    ///
    /// A relative `--style_url` (what most callers want, and what the templates
    /// in `tests/` use) has no scheme and always passes.
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

    /// Refuse the note modes with the two that stream.
    ///
    /// Both write output as they go, and a note list is only known once the
    /// whole document has been read: numbering follows first reference, so a
    /// citation in the last paragraph can insert `[1]` in the first. Refusing
    /// here, before any byte is written, is the only way to keep the promise
    /// that a broken note set produces no output at all.
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

    /// P5.5. Check the template options and, when one is set, load it and check
    /// its slots. Doing this before conversion means a bad template is reported
    /// as a message and a non-zero exit rather than part-way through output.
    ///
    /// The two template options are mutually exclusive and each is refused with
    /// the modes that own the surrounding document (`--extract`, `--chunk`,
    /// `--stream`); `--document_template` is additionally refused with
    /// `--prepend_file`, whose content has no slot in a template and would
    /// otherwise be dropped silently.
    fn validate_template_options(&self) -> Result<(), String> {
        let wrapper = !self.template.is_empty();
        let whole = !self.document_template.is_empty();
        if !wrapper && !whole {
            return Ok(());
        }
        if wrapper && whole {
            return Err("--body_template and --document_template cannot be combined".to_string());
        }
        let flag = if wrapper {
            "--body_template"
        } else {
            "--document_template"
        };
        if self.extract {
            return Err(format!("{flag} is not valid with --extract"));
        }
        if self.chunk {
            return Err(format!("{flag} is not valid with --chunk"));
        }
        if self.stream {
            return Err(format!("{flag} is not valid with --stream"));
        }
        if whole && !self.prepend_file.is_empty() {
            return Err("--document_template is not valid with --prepend_file".to_string());
        }
        let path = if wrapper {
            &self.template
        } else {
            &self.document_template
        };
        let body = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let declared = self
            .vars
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>();
        crate::template::validate(&body, &declared)
    }

    /// The options whose value is a regular expression supplied by the caller,
    /// as `(option, pattern)`.
    ///
    /// These three, and no others: every other pattern the engine compiles is
    /// either a literal in the source or derived from a delimiter option, and
    /// validating those here would mean duplicating the construction rules in a
    /// second place. The list is explicit rather than derived because a list
    /// that silently falls behind the code is worse than no list -- it reads as
    /// coverage. [`Options::REGEXP_OPTIONS`] exists so a test can check the two
    /// against the CLI option table, and it does.
    ///
    /// A pattern reaches [`compile_user_pattern`] on its way to the engine, so a
    /// caller cannot smuggle an uncompilable one past validation. See
    /// `Convert::re`, which used to panic on a bad pattern instead.
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

    /// The canonical long name of every option that takes a regular expression.
    ///
    /// Hand-maintained so `tests/cliexit.rs` can assert that
    /// [`Options::user_patterns`] covers all of it. That assertion is the point:
    /// a new regexp option added to the CLI table and wired into `Options`, but
    /// forgotten in `user_patterns`, is a pattern that panics the process again --
    /// the bug P22 fixed, reintroduced by the next person to add an option.
    /// Aliases such as `heading` and `H` are not listed; they name the same
    /// field, and only the canonical name has a field to validate.
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
        // XHTML implies lower case
        if self.xhtml {
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
