//! Options for the txt2html converter.
//!
//! Mirrors the option set of HTML::TextToHTML v3.0.

/// P7.3. How to decode input bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// Decode as UTF-8 when the bytes are valid UTF-8, and as CP1252
    /// otherwise. This is the default and the pre-P7.1 behaviour, modulo the
    /// fallback being CP1252 rather than the Latin-1 it used to be.
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
}

impl Encoding {
    /// The name accepted by `--encoding` and reported by `--verbose`.
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Auto => "auto",
            Encoding::Utf8 => "utf-8",
            Encoding::Cp1252 => "cp1252",
        }
    }

    /// Parse a `--encoding` value. Accepts a few spellings of each, because a
    /// user typing an encoding name should not have to know which one the
    /// implementation happens to call it.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "detect" => Ok(Encoding::Auto),
            "utf8" | "utf-8" | "utf_8" => Ok(Encoding::Utf8),
            "cp1252" | "windows-1252" | "win1252" | "1252" | "latin-1" | "latin1" => {
                Ok(Encoding::Cp1252)
            }
            other => Err(format!(
                "Unknown encoding `{other}`; expected auto, utf-8 or cp1252"
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
    pub append_file: String,
    pub append_head: String,
    pub body_deco: String,
    pub bullets: String,
    pub bullets_ordered: String,
    pub bold_delimiter: String,
    pub caps_tag: String,
    pub custom_heading_regexp: Vec<String>,
    pub default_link_dict: String,
    pub demoronize: bool,
    pub doctype: String,
    pub eight_bit_clean: bool,
    pub escape_html_chars: bool,
    pub explicit_headings: bool,
    pub extract: bool,
    pub hrule_min: usize,
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
            append_file: String::new(),
            append_head: String::new(),
            body_deco: String::new(),
            bullets: "-=o*\u{00b7}".to_string(),
            bullets_ordered: String::new(),
            bold_delimiter: "#".to_string(),
            caps_tag: "STRONG".to_string(),
            custom_heading_regexp: Vec::new(),
            default_link_dict: if home.is_empty() {
                ".txt2html.dict".to_string()
            } else {
                format!("{home}/.txt2html.dict")
            },
            demoronize: true,
            doctype: "-//W3C//DTD HTML 4.01//EN\"\n\"http://www.w3.org/TR/html4/strict.dtd"
                .to_string(),
            eight_bit_clean: false,
            escape_html_chars: true,
            explicit_headings: false,
            extract: false,
            hrule_min: 4,
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
        Ok(())
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
