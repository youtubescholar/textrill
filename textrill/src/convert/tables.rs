use super::*;

impl Converter {
    // ------------------------------------------------------- tables

    pub(super) fn table_spaces(rows: &[String], para_len: usize) -> String {
        let mut spaces: Vec<u8> = Vec::new();
        let mut min = para_len;
        for row in rows {
            if row.len() < min {
                min = row.len();
            }
            let bytes = row.as_bytes();
            if spaces.is_empty() {
                spaces = bytes.to_vec();
            } else {
                for (i, b) in bytes.iter().enumerate() {
                    if i >= spaces.len() {
                        break;
                    }
                    spaces[i] |= b;
                }
            }
        }
        for b in spaces.iter_mut() {
            if *b != b' ' {
                // 'X' keeps offsets byte==char (the reference uses \xff,
                // which would be multi-byte in a Rust &str).
                *b = b'X';
            }
        }
        spaces.truncate(min);
        spaces.iter().map(|&b| b as char).collect()
    }

    pub(super) fn is_aligned_table(rows: &[String], para_len: usize) -> bool {
        if rows.len() < 2 {
            return false;
        }
        let spaces = Self::table_spaces(rows, para_len);
        let mut starts: Vec<usize> = Vec::new();
        if !spaces.starts_with(' ') {
            starts.push(0);
        }
        let re = links::ascii_re_cached(r"(?:^| ) +(?=[^ ])");
        for m in re.find_iter(&spaces).flatten() {
            starts.push(m.end());
        }
        rows.len() >= 2 && starts.len() >= 2
    }

    pub(super) fn table_columns(rows: &[String], para_len: usize) -> (Vec<usize>, Vec<usize>) {
        let spaces = Self::table_spaces(rows, para_len);
        let max = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut starts: Vec<usize> = Vec::new();
        let mut ends: Vec<usize> = Vec::new();
        if !spaces.starts_with(' ') {
            starts.push(0);
        }
        let re = links::ascii_re_cached(r"((?:^| ) +)(?=[^ ])");
        for caps in re.captures_iter(&spaces).flatten() {
            let g = caps.get(1).unwrap();
            ends.push(g.start());
            starts.push(g.end());
        }
        if spaces.starts_with(' ') && !ends.is_empty() {
            ends.remove(0);
        }
        ends.push(max);
        (starts, ends)
    }

    pub(super) fn make_aligned_table(&mut self, rows: &mut Vec<String>, para_len: usize) -> bool {
        let (starts, ends) = Self::table_columns(rows, para_len);
        if rows.len() < 2 || starts.len() < 2 {
            return false;
        }
        self.mode |= TABLE;

        let align_idx: Vec<usize> = (0..starts.len())
            .map(|col| {
                let width = ends[col] - starts[col];
                let mut count = [0usize; 4];
                for row in rows.iter() {
                    let cell = byte_slice(row, starts[col], width);
                    let a = if cell.starts_with(' ') { 2 } else { 0 };
                    let b = if cell.ends_with(' ') || byte_len(cell) < width {
                        1
                    } else {
                        0
                    };
                    count[a + b] += 1;
                }
                let population = count[1] + count[2] + count[3];
                count
                    .iter()
                    .enumerate()
                    .skip(1)
                    .find(|(_, n)| **n * 2 > population)
                    .map_or(0, |(x, _)| x)
            })
            .collect();

        let mut new_rows: Vec<String> = Vec::new();
        for row in rows.iter() {
            let mut out = String::new();
            out.push_str(&self.get_tag("tr", TAG_START, ""));
            for (col, &sc) in starts.iter().enumerate() {
                let width = ends[col] - sc;
                let mut cell = byte_slice(row, sc, width).to_string();
                cell = cell.trim_matches(' ').to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                let inside = if self.opts.xhtml {
                    // XHTML parity: the reference emits inline CSS for a
                    // right/centre-aligned cell, and the corpus pins the mode.
                    match align_idx[col] {
                        2 => " style=\"text-align: right;\"".to_string(),
                        3 => " style=\"text-align: center;\"".to_string(),
                        _ => String::new(),
                    }
                } else if self.opts.html5 {
                    // `align` on `td`/`th` is obsolete and non-conforming in
                    // HTML5, so the default mode emits the documented styling
                    // hook instead (README, "Styling hooks").
                    match align_idx[col] {
                        2 => " class=\"align-right\"".to_string(),
                        3 => " class=\"align-center\"".to_string(),
                        _ => String::new(),
                    }
                } else if self.opts.lower_case_tags {
                    match align_idx[col] {
                        2 => " align=\"right\"".to_string(),
                        3 => " align=\"center\"".to_string(),
                        _ => String::new(),
                    }
                } else {
                    match align_idx[col] {
                        2 => " ALIGN=\"RIGHT\"".to_string(),
                        3 => " ALIGN=\"CENTER\"".to_string(),
                        _ => String::new(),
                    }
                };
                let tag = self.get_tag("td", TAG_START, &inside);
                let tag2 = self.close_tag("td");
                out.push_str(&format!("{tag}{cell}{tag2}"));
            }
            out.push_str(&self.close_tag("tr"));
            new_rows.push(out);
        }

        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, "")
        };
        new_rows[0] = format!("{tag}\n{}", new_rows[0]);
        let tag = self.close_tag("table");
        let last = new_rows.last_mut().unwrap();
        last.push_str(&format!("\n{tag}"));
        *rows = new_rows;
        true
    }

    pub(super) fn make_pgsql_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        // split the heading row on \s+|\s+
        let head_row = rows.remove(0);
        let hre = links::ascii_re_cached(r"\s+\|\s+");
        let headings: Vec<String> = hre
            .split(&head_row)
            .filter_map(|f| {
                let f = f.ok()?.trim().to_string();
                if f.is_empty() {
                    None
                } else {
                    Some(f)
                }
            })
            .collect();
        // skip the ----+--- line
        if !rows.is_empty() {
            rows.remove(0);
        }
        // grab the N rows line
        let n_rows = rows.pop().unwrap_or_default();
        // build the table
        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        let mut thead = String::new();
        thead.push_str(&self.get_tag("thead", TAG_START, ""));
        thead.push_str(&self.get_tag("tr", TAG_START, ""));
        for col in headings {
            let mut col = col.trim().to_string();
            let _ = &mut col;
            let tag1 = self.get_tag("th", TAG_START, "");
            let tag2 = self.close_tag("th");
            thead.push_str(&format!("{tag1}{col}{tag2}"));
        }
        thead.push_str(&self.close_tag("tr"));
        thead.push_str(&self.close_tag("thead"));
        tab_lines.push(format!("{thead}\n"));
        tab_lines.push(format!("{}\n", self.get_tag("tbody", TAG_START, "")));

        for row in &rows {
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split('|') {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.close_tag("tbody")));
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));

        // and add the N rows line
        let ptag = self.get_tag("p", TAG_START, "");
        tab_lines.push(format!("{ptag}{n_rows}\n"));
        if self.closes_optional_tags() {
            let ptag2 = self.get_tag("p", TAG_END, "");
            let last = tab_lines.last_mut().unwrap();
            if let Some(pos) = last.find('\n') {
                last.insert_str(pos, &ptag2);
            }
        }
        *out = tab_lines;
        true
    }

    pub(super) fn make_border_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        // skip the +----+---+ line
        rows.remove(0);
        // get the head row and cut off the start and end |
        let mut head_row = rows.remove(0);
        head_row = head_row.trim_start_matches([' ', '\t']).to_string();
        if head_row.starts_with('|') {
            head_row.remove(0);
        }
        if head_row.ends_with('|') {
            head_row.pop();
        }
        let hre = links::ascii_re_cached(r"\s+\|\s+");
        let headings: Vec<String> = hre
            .split(&head_row)
            .filter_map(|f| {
                let f = f.ok()?.trim().to_string();
                if f.is_empty() {
                    None
                } else {
                    Some(f)
                }
            })
            .collect();
        // skip the +----+---+ line
        rows.remove(0);
        // skip the last +----+---+ line
        rows.pop();

        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        let mut thead = String::new();
        thead.push_str(&self.get_tag("thead", TAG_START, ""));
        thead.push_str(&self.get_tag("tr", TAG_START, ""));
        for col in headings {
            let col = col.trim().to_string();
            let tag1 = self.get_tag("th", TAG_START, "");
            let tag2 = self.close_tag("th");
            thead.push_str(&format!("{tag1}{col}{tag2}"));
        }
        thead.push_str(&self.close_tag("tr"));
        thead.push_str(&self.close_tag("thead"));
        tab_lines.push(format!("{thead}\n"));
        tab_lines.push(format!("{}\n", self.get_tag("tbody", TAG_START, "")));
        for row in &rows {
            let mut row = row.trim_end_matches('\n').to_string();
            // cut off the start and end |
            row = row.trim_start_matches([' ', '\t']).to_string();
            if row.starts_with('|') {
                row.remove(0);
            }
            if row.ends_with('|') {
                row.pop();
            }
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split('|') {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.close_tag("tbody")));
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));
        *out = tab_lines;
        true
    }

    pub(super) fn make_delim_table(&mut self, out: &mut Vec<String>) -> bool {
        let mut rows = out.clone();
        let mut caption = String::new();
        if !rows[0].contains('|') {
            let re = links::ascii_re_cached(r"^\s*\w+");
            if re.is_match(&rows[0]).unwrap_or(false) {
                caption = rows.remove(0);
            }
        }
        let delim = {
            let re = links::ascii_re_cached(r"^\s*([^A-Za-z0-9])");
            re.captures(&rows[0])
                .ok()
                .flatten()
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().chars().next().unwrap())
        };
        let Some(delim) = delim else { return false };

        let mut tab_lines: Vec<String> = Vec::new();
        let tag = if self.opts.xhtml {
            self.get_tag("table", TAG_START, " border=\"1\" summary=\"\"")
        } else {
            self.get_tag("table", TAG_START, " border=\"1\"")
        };
        tab_lines.push(format!("{tag}\n"));
        if !caption.is_empty() {
            caption = caption.trim().to_string();
            let tag1 = self.get_tag("caption", TAG_START, "");
            let tag2 = self.close_tag("caption");
            tab_lines.push(format!("{tag1}{caption}{tag2}\n"));
        }
        for row in &rows {
            let row = row.trim_end_matches('\n');
            let mut row: Vec<char> = row.chars().collect();
            // cut off leading whitespace and one leading delimiter
            while let Some(&c) = row.first() {
                if c == ' ' || c == '\t' {
                    row.remove(0);
                } else {
                    break;
                }
            }
            if row.first() == Some(&delim) {
                row.remove(0);
            }
            if row.last() == Some(&delim) {
                row.pop();
            }
            let row: String = row.iter().collect();
            let mut this_row = self.get_tag("tr", TAG_START, "");
            for cell in row.split(delim) {
                let mut cell = cell.trim().to_string();
                if self.opts.escape_html_chars {
                    cell = chars::escape(&cell);
                }
                if cell == "0" || cell.is_empty() {
                    cell = "&nbsp;".to_string();
                }
                let tag1 = self.get_tag("td", TAG_START, "");
                let tag2 = self.close_tag("td");
                this_row.push_str(&format!("{tag1}{cell}{tag2}"));
            }
            this_row.push_str(&self.close_tag("tr"));
            tab_lines.push(format!("{this_row}\n"));
        }
        tab_lines.push(format!("{}\n", self.get_tag("table", TAG_END, "")));
        *out = tab_lines;
        true
    }

    pub(super) fn tablestuff(
        &mut self,
        table_type: u32,
        rows: &mut Vec<String>,
        para_len: usize,
    ) -> bool {
        match table_type {
            TAB_ALIGN => self.make_aligned_table(rows, para_len),
            TAB_PGSQL => self.make_pgsql_table(rows),
            TAB_BORDER => self.make_border_table(rows),
            TAB_DELIM => self.make_delim_table(rows),
            _ => false,
        }
    }

    pub(super) fn get_table_type(&self, rows: &[String], para_len: usize) -> u32 {
        if self.opts.table_type.delim && is_delim_table(rows) {
            TAB_DELIM
        } else if self.opts.table_type.align && Self::is_aligned_table(rows, para_len) {
            TAB_ALIGN
        } else if self.opts.table_type.pgsql && is_pgsql_table(rows) {
            TAB_PGSQL
        } else if self.opts.table_type.border && is_border_table(rows) {
            TAB_BORDER
        } else {
            0
        }
    }
}

/// Compile a (fragment) table regexp with ASCII semantics and dotall.
pub(super) fn table_re(pat: &str) -> Regex {
    let translated = links::translate_pattern(pat);
    Regex::new(&format!("(?s){translated}")).unwrap()
}

pub(super) fn is_pgsql_table(rows: &[String]) -> bool {
    // A PGSQL table must have at least 4 rows
    if rows.len() < 4 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    // possible caption
    if !r[0].contains('|') && table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false) {
        r.remove(0);
    }
    if r.len() < 4 {
        return false;
    }
    if !table_re(r"^\s*\w+\s+\|\s+").is_match(r[0]).unwrap_or(false) {
        return false; // Colname |
    }
    if !table_re(r"^\s*[-]+[+][-]+").is_match(r[1]).unwrap_or(false) {
        return false; // ----+----
    }
    if !table_re(r"^\s*[^|]*\s+\|\s+")
        .is_match(r[2])
        .unwrap_or(false)
    {
        return false; // value |
    }
    if !table_re(r"\(\d+\s+rows\)")
        .is_match(r[r.len() - 1])
        .unwrap_or(false)
    {
        return false; // (N rows)
    }
    true
}

pub(super) fn is_border_table(rows: &[String]) -> bool {
    // A BORDER table must have at least 5 rows
    if rows.len() < 5 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    if !r[0].contains('|') && table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false) {
        r.remove(0);
    }
    if r.len() < 5 {
        return false;
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$")
        .is_match(r[0])
        .unwrap_or(false)
    {
        return false; // +----+----+
    }
    if !table_re(r"^\s*\|\s*\w+\s+\|\s+.*\|$")
        .is_match(r[1])
        .unwrap_or(false)
    {
        return false; // | Colname |
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$")
        .is_match(r[2])
        .unwrap_or(false)
    {
        return false; // +----+----+
    }
    if !table_re(r"^\s*\|\s*[^|]*\s+\|\s+.*\|$")
        .is_match(r[3])
        .unwrap_or(false)
    {
        return false; // | value |
    }
    if !table_re(r"^\s*[+][-]+[+][-]+[+][-+]*$")
        .is_match(r[r.len() - 1])
        .unwrap_or(false)
    {
        return false; // +----+----+
    }
    true
}

pub(super) fn is_delim_table(rows: &[String]) -> bool {
    if rows.len() < 2 {
        return false;
    }
    let mut r: Vec<&String> = rows.iter().collect();
    // possible caption
    if !table_re(r"[^\w\s]").is_match(r[0]).unwrap_or(false)
        && table_re(r"^\s*\w+").is_match(r[0]).unwrap_or(false)
    {
        r.remove(0);
    }
    if r.len() < 2 {
        return false;
    }
    // find a possible delimiter
    let delim = if let Some(c) = table_re(r"^\s*([^A-Za-z0-9])")
        .captures(r[0])
        .ok()
        .flatten()
        .and_then(|c| c.get(1))
    {
        // get rid of ^ and [] and \
        let d = c.as_str().replace(['^', '[', ']', '\\'], "");
        if d.is_empty() {
            return false; // no delimiter after all
        }
        d
    } else {
        return false;
    };
    // There needs to be at least three delimiters in the row
    let dc = links::ascii_re(&format!("[{}]", delim));
    let total_num_delims = dc.find_iter(r[0]).flatten().count();
    if total_num_delims < 3 {
        return false;
    }
    // All rows must start and end with the delimiter
    // and have $total_num_delims number of them
    let re_start = table_re(&format!(r"^\s*[{delim}]"));
    let re_end = table_re(&format!(r"[{delim}]\s*$"));
    for row in &r {
        if !re_start.is_match(row).unwrap_or(false) {
            return false;
        }
        if !re_end.is_match(row).unwrap_or(false) {
            return false;
        }
        if dc.find_iter(row).flatten().count() != total_num_delims {
            return false;
        }
    }
    true
}

pub(super) fn byte_slice(s: &str, start: usize, len: usize) -> &str {
    let bytes = s.as_bytes();
    let end = (start + len).min(bytes.len());
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
}

pub(super) fn byte_len(s: &str) -> usize {
    s.len()
}
