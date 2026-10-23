use super::*;

impl Converter {
    /// Whether to emit the end tags HTML allows to be omitted: `</p>`, `</li>`,
    /// `</dt>` and `</dd>`. XHTML always closes them; the HTML5 default closes
    /// them for well-formedness ([`Options::close_optional_tags`]); the
    /// reference's HTML 4.01 mode (`--no-html5`) omits them for byte parity.
    pub(super) fn closes_optional_tags(&self) -> bool {
        self.opts.xhtml || (self.opts.html5 && self.opts.close_optional_tags)
    }

    pub(super) fn get_tag(&mut self, in_tag: &str, tag_type: u8, inside_tag: &str) -> String {
        let open_tag = self.tags.last().cloned().unwrap_or_default();
        let mut tag_prefix = String::new();

        if self.closes_optional_tags() {
            // A `p` is closed by a nested `p` or by a block element that cannot
            // live inside one; an `li` by a nested `li` or the end of its list.
            // The reference states these as separate branches with identical
            // bodies, and the corpus pins the result.
            if open_tag == "p"
                && ((in_tag == "p" && tag_type != TAG_END)
                    || in_tag.starts_with("hr")
                    || in_tag == "ul"
                    || in_tag == "ol"
                    || in_tag == "dl"
                    || in_tag == "pre"
                    || in_tag == "table"
                    || in_tag.starts_with('h'))
            {
                tag_prefix = self.close_tag("p");
            } else if open_tag == "li"
                && ((in_tag == "li" && tag_type != TAG_END)
                    || ((in_tag == "ul" || in_tag == "ol") && tag_type == TAG_END))
            {
                tag_prefix = self.close_tag("li");
            } else if open_tag == "dt" && in_tag == "dd" && tag_type != TAG_END {
                tag_prefix = self.close_tag("dt");
            } else if open_tag == "dd"
                && ((in_tag == "dt" && tag_type != TAG_END)
                    || (in_tag == "dl" && tag_type == TAG_END))
            {
                tag_prefix = self.close_tag("dd");
            }
        }

        if tag_type == TAG_END {
            let out = self.close_tag(in_tag);
            return if tag_prefix.is_empty() {
                out
            } else {
                format!("{tag_prefix}{out}")
            };
        }

        let mut out_tag = in_tag.to_string();
        if self.opts.lower_case_tags {
            out_tag = out_tag.to_ascii_lowercase();
        } else {
            out_tag = out_tag.to_ascii_uppercase();
        }
        let out;
        if tag_type == TAG_EMPTY {
            if self.opts.xhtml {
                out = format!("<{out_tag}{inside_tag}/>");
            } else {
                out = format!("<{out_tag}{inside_tag}>");
            }
        } else {
            self.tags.push(in_tag.to_string());
            out = format!("<{out_tag}{inside_tag}>");
        }
        if tag_prefix.is_empty() {
            out
        } else {
            format!("{tag_prefix}{out}")
        }
    }

    pub(super) fn close_tag(&mut self, in_tag: &str) -> String {
        let open_tag = self.tags.pop().unwrap_or_default();
        let in_tag = if in_tag.is_empty() { &open_tag } else { in_tag };
        if !open_tag.is_empty() && open_tag != in_tag {
            self.tags.push(open_tag.clone());
        }
        let mut out_tag = in_tag.to_string();
        if self.opts.lower_case_tags {
            out_tag = out_tag.to_ascii_lowercase();
        } else {
            out_tag = out_tag.to_ascii_uppercase();
        }
        format!("</{out_tag}>")
    }
}

impl Converter {
    pub(super) fn tagline(&mut self, tag: &str, line_ref: &mut String) {
        // Perl chomp: drop a trailing newline (and CR), not any char
        if line_ref.ends_with('\n') {
            line_ref.pop();
        }
        if line_ref.ends_with('\r') {
            line_ref.pop();
        }
        let re = links::ascii_re_cached(r"^\s*(.*)$");
        // `^\s*(.*)$` matches every line, so the only way `captures` yields no
        // match is a runtime error: the `$` compiles to the look-ahead
        // `(?=\n?$)`, which puts the pattern on fancy-regex's backtracking VM,
        // and that returns `Err` (not a match) if the 1,000,000-step budget is
        // exhausted. A document must not be able to take the process down, so
        // fall back to the line itself rather than unwrapping.
        let body = match re.captures(&*line_ref).ok().flatten() {
            Some(caps) => caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
            None => line_ref.clone(),
        };
        let tag1 = self.get_tag(tag, TAG_START, "");
        let tag2 = self.close_tag(tag);
        *line_ref = format!("{tag1}{body}{tag2}\n");
    }
}
