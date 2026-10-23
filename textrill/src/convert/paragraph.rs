use super::*;

impl Converter {
    // ------------------------------------------------------- simple blocks

    pub(super) fn hrule(&mut self, lines: &mut [String], actions: &mut [u32], ind: usize) {
        let hrmin = self.opts.hrule_min;
        let pat = format!(r"^\s*([\-_~=*]\s*){{{hrmin},}}$");
        if self.re(&pat).is_match(&lines[ind]).unwrap_or(false) {
            let tag = self.get_tag("hr", TAG_EMPTY, "");
            lines[ind] = format!("{tag}\n");
            actions[ind] |= HRULE;
        } else if lines[ind].contains('\x0c') {
            actions[ind] |= HRULE;
            let tag = self.get_tag("hr", TAG_EMPTY, "");
            lines[ind] = lines[ind].replace('\x0c', &format!("\n{tag}\n"));
        }
    }

    pub(super) fn shortline(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        i: usize,
        prev: &mut String,
        prev_action: &mut u32,
        prev_line_len: usize,
    ) {
        let tag = self.get_tag("br", TAG_EMPTY, "");
        if !lines[i].trim().is_empty()
            && !prev.trim().is_empty()
            && prev_line_len < self.opts.short_line_length
            && actions[i] & (END | HEADER | HRULE | LIST | IND_BREAK | PAR) == 0
            && *prev_action & (HEADER | HRULE | BREAK | IND_BREAK) == 0
        {
            let c = prev.pop().unwrap_or('\0');
            prev.push_str(&tag);
            prev.push(c);
            *prev_action |= BREAK;
        }
    }

    pub(super) fn is_mailheader(&self, rows: &[String]) -> bool {
        let re = links::ascii_re_cached(r"^(?:From:?)|Newsgroups: ");
        if rows.is_empty() {
            return false;
        }
        re.is_match(&rows[0]).unwrap_or(false)
    }

    pub(super) fn mailheader(&mut self, rows_ref: &mut Vec<String>) {
        let mut rows = rows_ref.clone();
        if self.is_mailheader(rows_ref) {
            self.mode |= MAILHEADER;
            if self.opts.escape_html_chars {
                rows[0] = chars::escape(&rows[0]);
            }
            self.anchor_mail(&mut rows[0]);
            if rows[0].ends_with('\n') {
                rows[0].pop();
            }
            let tag = self.get_tag("p", TAG_START, " class='mail_header'");
            let tag2 = self.get_tag("br", TAG_EMPTY, "");
            rows[0] = format!("<!-- New Message -->\n{tag}{}{tag2}\n", rows[0]);
            // Every row but the last is terminated with a `<br/>`; the last is
            // left for the paragraph that follows it.
            if rows.len() > 1 {
                let (final_row, br_rows) = rows[1..].split_last_mut().unwrap();
                for row in br_rows {
                    if self.opts.escape_html_chars {
                        *row = chars::escape(row);
                    }
                    if row.ends_with('\n') {
                        row.pop();
                    }
                    let tag3 = self.get_tag("br", TAG_EMPTY, "");
                    *row = format!("{row}{tag3}\n");
                }
                if self.opts.escape_html_chars {
                    *final_row = chars::escape(final_row);
                }
            }
        }
        *rows_ref = rows;
    }

    pub(super) fn mailquote(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        next: Option<&str>,
    ) {
        let starts_quote = {
            let l = &lines[ind];
            let re1 = links::ascii_re_cached(r"^\w*&gt");
            let re2 = links::ascii_re_cached(r"^[\|:]");
            (re1.is_match(l).unwrap_or(false) || re2.is_match(l).unwrap_or(false))
                && next.is_some()
                && !next.unwrap().trim().is_empty()
        };
        if starts_quote {
            let tag = self.get_tag("br", TAG_EMPTY, "");
            // Perl: s/$/<tag>/ with `$` matching before the trailing newline
            if let Some(stripped) = lines[ind].strip_suffix('\n') {
                lines[ind] = format!("{stripped}{tag}\n");
            } else {
                lines[ind].push_str(&tag);
            }
            actions[ind] |= BREAK | MAILQUOTE;
            if *prev_action & (BREAK | MAILQUOTE) == 0 {
                let tag = self.get_tag("p", TAG_START, " class='quote_mail'");
                prev.push_str(&tag);
                actions[ind] |= PAR;
            }
        }
    }

    // The parameters of the three state-machine dispatchers below match the
    // reference's argument list for the same routine. Threading this state
    // through a struct would hide which state each routine reads and writes,
    // which is what the differential corpus tests.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paragraph(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        _indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
        line_indent: usize,
        prev_indent: usize,
        is_fragment: bool,
        line_no: usize,
    ) {
        let par_indent = self.opts.par_indent;
        let cond = !lines[ind].trim().is_empty()
            && subtract_modes(actions[ind], END | MAILQUOTE | CAPS | BREAK) == 0
            && (prev.trim().is_empty()
                || actions[ind] & END != 0
                || line_indent > prev_indent + par_indent)
            && !(is_fragment && line_no == 0);
        if cond {
            if self.opts.indent_par_break
                && !prev.trim().is_empty()
                && actions[ind] & END == 0
                && line_indent > prev_indent + par_indent
            {
                let tag = self.get_tag("br", TAG_EMPTY, "");
                prev.push_str(&tag);
                prev.push_str(&"&nbsp;".repeat(line_indent));
                lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
                *prev_action |= BREAK;
                actions[ind] |= IND_BREAK;
            } else if self.opts.preserve_indent {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                prev.push_str(&"&nbsp;".repeat(line_indent));
                lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
                actions[ind] |= PAR;
            } else {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                actions[ind] |= PAR;
            }
        } else if self.opts.indent_par_break
            && self.mode & (PRE | TABLE | LIST) == 0
            && !prev.trim().is_empty()
            && actions[ind] & END == 0
            && *prev_action & (IND_BREAK | PAR) != 0
            && subtract_modes(actions[ind], END | MAILQUOTE | CAPS) == 0
            && line_indent > par_indent
            && line_indent == prev_indent
        {
            let tag = self.get_tag("br", TAG_EMPTY, "");
            prev.push_str(&tag);
            prev.push_str(&"&nbsp;".repeat(line_indent));
            lines[ind] = strip_leading_spaces(&lines[ind], line_indent);
            *prev_action |= BREAK;
            actions[ind] |= IND_BREAK;
        }
    }
}

pub(super) fn strip_leading_spaces(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut removed = 0;
    let mut idx = 0;
    while idx < chars.len() && removed < n {
        if chars[idx] == ' ' {
            removed += 1;
            idx += 1;
        } else {
            break;
        }
    }
    chars[idx..].iter().collect()
}
