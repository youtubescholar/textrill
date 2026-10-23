use super::*;

impl Converter {
    // ------------------------------------------------------- lists

    pub(super) fn listprefix(&mut self, line: &str) -> (String, String, String, String) {
        let bullets = class_body(&self.opts.bullets);
        let bullets_full = format!("[{}]", bullets);
        let number_match = self.number_match.clone();
        let term_match = self.term_match.clone();

        let pat_bullet = format!(r"^\s*{}\s+\S", bullets_full);
        let pat_ordered = format!(r"^\s*{number_match}[\.\)\]:]\s+\S");
        let pat_term = format!(r"^\s*{term_match}:$");

        let is_bullet = self.re(&pat_bullet).is_match(line).unwrap_or(false);
        let is_ordered = self.re(&pat_ordered).is_match(line).unwrap_or(false);
        let is_term = self.re(&pat_term).is_match(line).unwrap_or(false);
        if !is_bullet && !is_ordered && !is_term {
            return (String::new(), String::new(), String::new(), String::new());
        }

        let mut term = String::new();
        if let Some(caps) = self.re(&pat_term).captures(line).ok().flatten() {
            if let Some(g) = caps.get(1) {
                term = g.as_str().to_string();
            }
        }
        let mut number = String::new();
        let pat_num = format!(r"^\s*{number_match}\S\s+\S");
        if let Some(caps) = self.re(&pat_num).captures(line).ok().flatten() {
            if let Some(g) = caps.get(1) {
                number = g.as_str().to_string();
            }
        }
        if !self.opts.bullets_ordered.is_empty() {
            let bop = class_body(&self.opts.bullets_ordered);
            let bop_full = format!("[{}]", bop);
            if self.re(&bop_full).is_match(&number).unwrap_or(false) {
                number = "1".to_string();
            }
        }
        // slippery "o" bullet exception (Perl sets $number = 0, which is falsy)
        if self.opts.bullets.contains('o') {
            let re = links::ascii_re_cached(r"^\s*o\s");
            if re.is_match(line).unwrap_or(false) {
                number.clear();
            }
        }

        let rawprefix;
        let prefix = if !term.is_empty() {
            let pat = format!(r"^(\s*{term_match}.)$");
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            rawprefix.replace(&term, "")
        } else if !number.is_empty() {
            // the captured number string is truthy in Perl (even "0")
            let pat = format!(r"^(\s*{number_match}.)");
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            rawprefix.replace(&number, "")
        } else {
            let pat = format!(r"^(\s*{}.)", bullets_full);
            rawprefix = self
                .re(&pat)
                .captures(line)
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            rawprefix.clone()
        };
        (prefix, number, rawprefix, term)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn startlist(
        &mut self,
        prefix: &str,
        number: &str,
        _rawprefix: &str,
        term: &str,
        _lines: &mut [String],
        actions: &mut [u32],
        _indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        total_prefix: &str,
    ) -> bool {
        // The reference indexes these stacks by __listnum (assignment, not
        // push), so entries below listnum stay but are overwritten.
        while self.list_prefix.len() <= self.listnum {
            self.list_prefix.push(String::new());
        }
        self.list_prefix[self.listnum] = prefix.to_string();
        let num_truthy = !number.is_empty();
        let tag;
        if num_truthy {
            // A numeric marker starts an ordered list whatever number it carries;
            // the starting number is carried on the tag. The reference starts only
            // on 1/a/A and would drop a list structure starting at 3.
            let is_digit = number.bytes().all(|b| b.is_ascii_digit());
            if !is_digit && number != "a" && number != "A" {
                return false;
            }
            let inside = if is_digit && number != "1" {
                format!(" start=\"{}\"", number)
            } else {
                String::new()
            };
            tag = self.get_tag("ol", TAG_START, &inside);
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = OL;
        } else if !term.is_empty() {
            tag = self.get_tag("dl", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = DL;
        } else {
            tag = self.get_tag("ul", TAG_START, "");
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            while self.list.len() <= self.listnum {
                self.list.push(0);
            }
            self.list[self.listnum] = UL;
        }
        let _ = tag;
        while self.list_indent.len() <= self.listnum {
            self.list_indent.push(0);
        }
        self.list_indent[self.listnum] = total_prefix.chars().count();
        self.listnum += 1;
        self.list_nice_indent = " ".repeat(self.listnum * self.opts.indent_width);
        actions[ind] |= LIST;
        actions[ind] |= LIST_START;
        self.mode |= LIST;
        true
    }

    pub(super) fn endlist(&mut self, num_lists: usize, prev: &mut String, line_action: &mut u32) {
        let mut n = num_lists;
        while n > 0 {
            self.list_nice_indent = " ".repeat((self.listnum - 1) * self.opts.indent_width);
            let lt = self.list.get(self.listnum - 1).copied().unwrap_or(0);

            let tag = if lt == UL {
                self.get_tag("ul", TAG_END, "")
            } else if lt == OL {
                self.get_tag("ol", TAG_END, "")
            } else if lt == DL {
                self.get_tag("dl", TAG_END, "")
            } else {
                String::new()
            };
            prev.push_str(&format!("{}{tag}\n", self.list_nice_indent));
            self.list_indent.pop();
            self.listnum = self.listnum.saturating_sub(1);
            n -= 1;
        }
        *line_action |= END;
        if self.listnum == 0 {
            self.mode ^= LIST & self.mode;
        }
    }

    pub(super) fn continuelist(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        ind: usize,
        term: &str,
    ) {
        let list_indent = self.list_nice_indent.clone();
        let lt = self.list.get(self.listnum - 1).copied().unwrap_or(0);
        if lt == UL {
            let bullets_full = format!("[{}]", class_body(&self.opts.bullets));
            let pat = format!(r"^\s*{bullets_full} \s*");
            // compile once and reuse: this runs for every list item, and
            // building the pattern afresh each time dominated large documents
            let re = self.re(&pat);
            let matched = re.is_match(&lines[ind]).unwrap_or(false);
            if matched {
                let tag = self.get_tag("li", TAG_START, "");
                let re = self.re(&pat);
                let replaced = re
                    .replace(&lines[ind], format!("{list_indent}{tag}"))
                    .to_string();
                lines[ind] = replaced;
                actions[ind] |= LIST_ITEM;
            }
        }
        if lt == OL {
            let num_match = self.number_match.clone();
            let pat = format!(r"^\s*{num_match}.\s*");
            let tag = self.get_tag("li", TAG_START, "");
            let re = self.re(&pat);
            let replaced = re
                .replace(&lines[ind], format!("{list_indent}{tag}"))
                .to_string();
            lines[ind] = replaced;
            actions[ind] |= LIST_ITEM;
        }
        if lt == DL && !term.is_empty() {
            let term_match = self.term_match.clone();
            let tag = self.get_tag("dt", TAG_START, "");
            let tag2 = self.get_tag("dt", TAG_END, "");
            let term_clean = term.replace('_', " ");
            let pat = format!(r"^\s*{term_match}.$");
            let re = self.re(&pat);
            let replaced = re
                .replace(&lines[ind], format!("{list_indent}{tag}{term_clean}{tag2}"))
                .to_string();
            lines[ind] = replaced;
            let tag = self.get_tag("dd", TAG_START, "");
            lines[ind].push_str(&tag);
            actions[ind] |= LIST_ITEM;
        }
        actions[ind] |= LIST;
    }

    pub(super) fn total_prefix(&self, line: &str, term: bool) -> String {
        if term {
            // ^(\s*)term.$  -> leading whitespace only
            let ws: String = line.chars().take_while(|c| is_perl_space(*c)).collect();
            return format!("{ws}{}", " ".repeat(self.opts.indent_width));
        }
        let ch: Vec<char> = line.chars().collect();
        let n = ch.len();
        let mut i = 0;
        while i < n && is_perl_space(ch[i]) {
            i += 1;
        }
        while i < n
            && (is_word_char(ch[i])
                || self.opts.bullets.contains(ch[i])
                || self.opts.bullets_ordered.contains(ch[i]))
        {
            i += 1;
        }
        if i < n {
            i += 1; // the '.' any char
        }
        while i < n && is_perl_space(ch[i]) {
            i += 1;
        }
        ch[..i].iter().collect()
    }

    pub(super) fn liststuff(
        &mut self,
        lines: &mut [String],
        actions: &mut [u32],
        indents: &mut Vec<usize>,
        ind: usize,
        prev: &mut String,
        prev_action: &mut u32,
    ) {
        let (prefix, number, rawprefix, term) = self.listprefix(&lines[ind]);
        let _ = rawprefix;

        if prefix.is_empty() {
            if ind > 0 && !prev.trim().is_empty() {
                return;
            }
            if ind == 0 && self.listnum > 0 && indents[ind] == *self.list_indent.last().unwrap() {
                let tag = self.get_tag("p", TAG_START, "");
                prev.push_str(&tag);
                actions[ind] |= PAR;
                return;
            }
            if self.listnum > 0 {
                let nl = self.listnum;
                self.endlist(nl, prev, &mut actions[ind]);
            }
            return;
        }

        let mut prefix_alternate: Option<String> = None;
        if number.chars().count() > 1 {
            prefix_alternate = Some(format!(
                "{}{}",
                " ".repeat(number.chars().count() - 1),
                prefix
            ));
        }

        // walk back to a matching prefix
        let mut i = self.listnum as i64 - 1;
        while i >= 0 && prefix != self.list_prefix[i as usize] {
            if number.chars().count() > 1 {
                if let Some(pa) = &prefix_alternate {
                    if pa == &self.list_prefix[i as usize] {
                        break;
                    }
                }
            }
            i -= 1;
        }

        let total = self.total_prefix(&lines[ind], !term.is_empty());

        let mut islist = true;
        i += 1;
        if i > 0 && i as usize != self.listnum {
            let nl = self.listnum - i as usize;
            self.endlist(nl, prev, &mut actions[ind]);
            islist = false;
        } else if self.listnum == 0 || i as usize != self.listnum {
            // No open list matched this marker. A list indented no deeper than
            // the innermost open one is a sibling, not a child, so close the
            // open list(s) first; a deeper indent still nests.
            if i == 0
                && self.listnum > 0
                && leading_spaces(&lines[ind])
                    <= leading_spaces(&self.list_prefix[self.listnum - 1])
            {
                let nl = self.listnum;
                self.endlist(nl, prev, &mut actions[ind]);
            }
            if indents[ind] > 0
                || ind == 0
                || (ind > 0 && prev.trim().is_empty())
                || (ind > 0 && *prev_action & (BREAK | HEADER | CAPS) != 0)
            {
                islist = self.startlist(
                    &prefix, &number, "", &term, lines, actions, indents, ind, prev, &total,
                );
            } else {
                return;
            }
        }

        if self.mode & LIST != 0 {
            self.continuelist(lines, actions, ind, &term);
        }
        if islist {
            indents[ind] = total.chars().count();
        }
        let _ = prev_action;
    }
}
