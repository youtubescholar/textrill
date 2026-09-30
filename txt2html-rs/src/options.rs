//! Options for the txt2html converter.
//!
//! Mirrors the option set of HTML::TextToHTML v3.0.

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
        TableTypeFlags { align: true, pgsql: true, border: true, delim: true }
    }
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
            preformat_start_marker:
                "^(:?(:?&lt;)|<)PRE(:?(:?&gt;)|>)$".to_string(),
            preformat_end_marker:
                "^(:?(:?&lt;)|<)/PRE(:?(:?&gt;)|>)$".to_string(),
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
    pub fn deal_with_options(&mut self) {
        if !self.make_links {
            self.links_dictionaries.clear();
        }
        if self.preformat_trigger_lines < 0 {
            self.preformat_trigger_lines = 0;
        }
        if self.preformat_trigger_lines > 2 {
            self.preformat_trigger_lines = 2;
        }
        if self.preformat_trigger_lines == 0 {
            self.endpreformat_trigger_lines = 1;
        }
        if self.endpreformat_trigger_lines < 0 {
            self.endpreformat_trigger_lines = 0;
        }
        if self.endpreformat_trigger_lines > 2 {
            self.endpreformat_trigger_lines = 2;
        }
        // XHTML implies lower case
        if self.xhtml {
            self.lower_case_tags = true;
        }
    }
}