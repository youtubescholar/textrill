use super::stream::ReplAll;
use super::*;

impl Converter {
    // ------------------------------------------------------- headings/anchors

    pub(super) fn make_new_anchor(&mut self, heading_level: usize) -> String {
        if heading_level == 0 {
            let a = format!("{}", self.non_header_anchor);
            self.non_header_anchor += 1;
            return a;
        }
        let mut anchor = String::from("section");
        if self.heading_count.len() < heading_level {
            self.heading_count.resize(heading_level, 0);
        }
        self.heading_count[heading_level - 1] += 1;
        for i in (heading_level..self.heading_count.len()).rev() {
            self.heading_count[i] = 0;
        }
        for i in 0..heading_level {
            if self.heading_count[i] == 0 {
                self.heading_count[i] = 1;
            }
            anchor.push_str(&format!("_{}", self.heading_count[i]));
        }
        anchor
    }

    pub(super) fn anchor_mail(&mut self, line_ref: &mut String) {
        if self.opts.make_anchors {
            let anchor = self.make_new_anchor(0);
            // s/([^ ]*)/<a name="$anchor">$1<\/a>/   (first non-space run)
            let re = links::ascii_re_cached(r"[^ ]*");
            if let Some(m) = re.find(&*line_ref).ok().flatten() {
                let inner = m.as_str().to_string();
                let rep = if self.opts.lower_case_tags {
                    format!("<a name=\"{anchor}\">{inner}</a>")
                } else {
                    format!("<A NAME=\"{anchor}\">{inner}</A>")
                };
                line_ref.replace_range(m.start()..m.end(), &rep);
            }
        }
    }

    pub(super) fn anchor_heading(&mut self, level: usize, line_ref: &mut String) {
        if self.opts.make_anchors {
            let anchor = self.make_new_anchor(level);
            if self.opts.lower_case_tags {
                let re = links::ascii_re_cached(r"(<h\d>)(.*)(</h\d>)");
                *line_ref = re
                    .replace(&*line_ref, format!("$1<a name=\"{anchor}\">$2</a>$3"))
                    .to_string();
            } else {
                let re = links::ascii_re_cached(r"(<H\d>)(.*)(</H\d>)");
                *line_ref = re
                    .replace(&*line_ref, format!("$1<A NAME=\"{anchor}\">$2</A>$3"))
                    .to_string();
            }
        }
    }

    pub(super) fn is_ul_list_line(&mut self, line: &str) -> bool {
        let (prefix, number, _, _) = self.listprefix(line);
        !prefix.is_empty() && number.is_empty()
    }

    pub(super) fn heading_level(&mut self, style: &str) -> usize {
        if !self.heading_styles.contains_key(style) {
            self.num_heading_styles += 1;
            self.heading_styles
                .insert(style.to_string(), self.num_heading_styles);
        }
        self.heading_styles[style]
    }

    pub(super) fn is_heading(&mut self, line: &str, next: Option<&str>) -> bool {
        if line.trim().is_empty() {
            return false;
        }
        if self.is_ul_list_line(line) {
            return false;
        }
        let Some(next) = next else { return false };
        let re = links::ascii_re_cached(r"^\s*[-=*.~+]+\s*$");
        if !re.is_match(next).unwrap_or(false) {
            return false;
        }
        let (hoffset, heading) = match links::ascii_re_cached(r"^(\s*)(.+)$")
            .captures(line)
            .ok()
            .flatten()
        {
            Some(c) => (
                c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            ),
            None => return false,
        };
        let heading_c = links::ascii_re_cached(r"&[^;]+;")
            .replace_all(&heading, "X")
            .to_string();
        let (uoffset, underline) = match links::ascii_re_cached(r"^(\s*)(\S+)\s*$")
            .captures(next)
            .ok()
            .flatten()
        {
            Some(c) => (
                c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            ),
            None => return false,
        };
        let lendiff = (heading_c.chars().count() as i64 - underline.chars().count() as i64).abs();
        let offsetdiff = (hoffset.chars().count() as i64 - uoffset.chars().count() as i64).abs();
        let _ = (heading, offsetdiff);
        if (lendiff as usize <= self.opts.underline_length_tolerance)
            || (offsetdiff as usize <= self.opts.underline_offset_tolerance)
        {
            return true;
        }
        false
    }

    pub(super) fn heading(&mut self, line_ref: &mut String, next_ref: &mut String) {
        // ($uoffset, $underline) = ${$next_ref} =~ /^(\s*)(\S+)\s*$/
        let underline = {
            let re = links::ascii_re_cached(r"^(\s*)(\S+)\s*$");
            let mut u = String::new();
            if let Some(c) = re.captures(next_ref).ok().flatten() {
                if let Some(g) = c.get(2) {
                    u = g.as_str().to_string();
                }
            }
            u
        };
        let mut style = underline.chars().next().unwrap_or(' ').to_string();
        if self.iscaps(line_ref) {
            style.push('C');
        }
        *next_ref = " ".to_string();
        let level = self.heading_level(&style);
        if self.opts.escape_html_chars {
            *line_ref = chars::escape(line_ref);
        }
        self.tagline(&format!("H{level}"), line_ref);
        self.anchor_heading(level, line_ref);
    }

    pub(super) fn is_custom_heading(&mut self, line: &str) -> bool {
        for reg in self.opts.custom_heading_regexp.clone() {
            let re = self.re(&reg);
            if re.is_match(line).unwrap_or(false) {
                return true;
            }
        }
        false
    }

    pub(super) fn custom_heading(&mut self, line_ref: &mut String) {
        for (i, reg) in self
            .opts
            .custom_heading_regexp
            .clone()
            .into_iter()
            .enumerate()
        {
            let re = self.re(&reg);
            if re.is_match(&*line_ref).unwrap_or(false) {
                let level = if self.opts.explicit_headings {
                    i + 1
                } else {
                    self.heading_level(&format!("Cust{i}"))
                };
                if self.opts.escape_html_chars {
                    *line_ref = chars::escape(line_ref);
                }
                let tag_level = format!("H{level}");
                self.tagline(&tag_level, line_ref);
                self.anchor_heading(level, line_ref);
                return;
            }
        }
    }

    pub(super) fn unhyphenate_para(&mut self, para_ref: &mut String) {
        // s/(\s*)([^\W\d_]*)\-\n(\s*)([^\W\d_]+[\)\}\]\.,:;\'\"\>]*\s*)/$1$2$4\n$3/gs
        let re = self.re(r#"(\s*)([A-Za-z]*)-\n(\s*)([A-Za-z]+['").,:;}>]*\s*)"#);
        *para_ref = re.replace_all_captures(para_ref, |c| {
            let g = |i: usize| -> String {
                c.get(i).map(|m| m.as_str().to_string()).unwrap_or_default()
            };
            format!("{}{}{}\n{}", g(1), g(2), g(4), g(3))
        });
    }
}
