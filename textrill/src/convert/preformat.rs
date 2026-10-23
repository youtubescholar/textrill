use super::*;

impl Converter {
    // ------------------------------------------------------- preformat

    pub(super) fn is_preformatted(&mut self, line: &str) -> bool {
        let n = self.opts.preformat_whitespace_min;
        let re1 = format!(r"\s{{{n},}}\S+");
        let re2 = format!(r"\.{{{n},}}\S+");
        self.re(&re1).is_match(line).unwrap_or(false)
            || self.re(&re2).is_match(line).unwrap_or(false)
    }

    pub(super) fn split_end_explicit_preformat(&mut self, para: &mut String) -> String {
        let mut pre_str = String::new();
        if self.mode & PRE_EXPLICIT != 0 {
            // The reference's test here is *always false*: TextToHTML.pm:3868
            // reads `if (${para_ref} =~ /$pe_mark/io)`, and in Perl `para_ref`
            // without `$` is a symbolic reference to an unassigned global, so the
            // body is dead code and every call falls through to "no end -- the
            // whole thing is preformatted".
            //
            // The port reproduces the consequences: the whole paragraph is emitted
            // as preformatted text (a literal `</pre>` is escaped), PRE_EXPLICIT is
            // not cleared (the block does not end at the marker), and `para` is
            // emptied so the marker is not reprocessed.
            pre_str = if self.opts.escape_html_chars {
                chars::escape(para)
            } else {
                para.clone()
            };
            *para = String::new();
        }
        pre_str
    }

    pub(super) fn endpreformat(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        ind: usize,
        prev: &mut String,
    ) {
        if self.mode & PRE_EXPLICIT != 0 {
            let pe_mark = self.opts.preformat_end_marker.clone();
            let re = self.re_i(&pe_mark);
            if re.is_match(&lines[ind]).unwrap_or(false) {
                if ind == 0 {
                    let tag = self.close_tag("pre");
                    lines[ind] = format!("{tag}\n");
                } else {
                    let tag = self.close_tag("pre");
                    prev.push_str(&format!("{tag}\n"));
                    lines[ind] = String::new();
                }
                self.mode ^= (PRE | PRE_EXPLICIT) & self.mode;
                actions[ind] |= END;
            }
            return;
        }

        let cond = !self.is_preformatted(&lines[ind])
            && (self.opts.endpreformat_trigger_lines == 1
                || (ind + 1 < lines.len() && !self.is_preformatted(&lines[ind + 1]))
                || ind + 1 >= lines.len());
        if cond {
            if ind == 0 {
                let tag = self.close_tag("pre");
                *prev = format!("{tag}\n");
            } else {
                let tag = self.close_tag("pre");
                prev.push_str(&format!("{tag}\n"));
            }
            self.mode ^= PRE & self.mode;
            actions[ind] |= END;
        }
    }

    pub(super) fn preformat(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        next: Option<&str>,
    ) {
        if self.opts.use_preformat_marker {
            let pstart = self.opts.preformat_start_marker.clone();
            let re = self.re_i(&pstart);
            if re.is_match(&lines[ind]).unwrap_or(false) {
                if prev.ends_with("<p>") {
                    prev.truncate(prev.len() - 3);
                    self.tags.pop();
                }
                let tag = self.get_tag("pre", TAG_START, " class='quote_explicit'");
                lines[ind] = format!("{tag}\n");
                self.mode |= PRE | PRE_EXPLICIT;
                actions[ind] |= PRE;
                return;
            }
        }

        if *prev_action & MAILQUOTE == 0
            && actions[ind] & MAILQUOTE == 0
            && (self.opts.preformat_trigger_lines == 0
                || (self.is_preformatted(&lines[ind])
                    && (self.opts.preformat_trigger_lines == 1
                        || (next.is_some() && self.is_preformatted(next.unwrap())))))
        {
            if prev.ends_with("<p>") {
                prev.truncate(prev.len() - 3);
                self.tags.pop();
            }
            let tag = self.get_tag("pre", TAG_START, "");
            lines[ind] = format!("{tag}\n{}", lines[ind]);
            self.mode |= PRE;
            actions[ind] |= PRE;
        }
    }
}
