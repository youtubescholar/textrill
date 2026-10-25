use super::stream::paragraph_records;
use super::*;

impl Converter {
    // ------------------------------------------------------- file start / top-level

    pub fn do_file_start(&mut self, para: &str) -> String {
        let mut out = String::new();
        if !self.opts.extract {
            // split(/\n/, $para), not str::lines(): lines() also strips the \r
            // of a CRLF pair, so a CRLF file would lose it from --titlefirst
            // where the reference keeps it (TextToHTML.pm:5143).
            let first_line = para.split('\n').next().unwrap_or("").to_string();

            if !self.opts.doctype.is_empty() {
                // XHTML is checked first even though HTML5 is the default: tests
                // and front ends flip `opts.xhtml` on a converter carrying the
                // default `html5: true`, and the explicit doctype must win.
                if self.opts.xhtml {
                    out.push_str("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\"\n");
                    out.push_str("\"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\">\n");
                    out.push_str(&self.get_tag(
                        "html",
                        TAG_START,
                        " xmlns=\"http://www.w3.org/1999/xhtml\"",
                    ));
                    out.push('\n');
                } else if self.opts.html5 {
                    // HTML5 serialisation: the short doctype and an <html>
                    // element with no namespace. The charset meta is forced on
                    // with the doctype, because an HTML5 document with no declared
                    // encoding is the thing the mode exists to fix. The declared
                    // language rides on the root element, where it is useful to
                    // a screen reader and a validator. The parity modes keep the
                    // reference's bare <html> and ignore --lang.
                    out.push_str("<!DOCTYPE html>\n");
                    let lang_attr = if self.opts.lang.is_empty() {
                        String::new()
                    } else if self.opts.lower_case_tags {
                        format!(" lang=\"{}\"", chars::escape_attr(&self.opts.lang))
                    } else {
                        format!(" LANG=\"{}\"", chars::escape_attr(&self.opts.lang))
                    };
                    out.push_str(&self.get_tag("html", TAG_START, &lang_attr));
                    out.push('\n');
                } else {
                    out.push_str("<!DOCTYPE HTML PUBLIC \"");
                    out.push_str(&self.opts.doctype);
                    out.push_str("\">\n");
                    out.push_str(&self.get_tag("html", TAG_START, ""));
                    out.push('\n');
                }
            }
            self.head_inner = self.build_head(&first_line);
            out.push_str(&self.get_tag("head", TAG_START, ""));
            out.push('\n');
            out.push_str(&self.head_inner);
            out.push_str(&self.close_tag("head"));
            out.push('\n');
            let body_deco = self.opts.body_deco.clone();
            if !body_deco.is_empty() {
                out.push_str(&self.get_tag("body", TAG_START, &body_deco));
            } else {
                out.push_str(&self.get_tag("body", TAG_START, ""));
            }
            out.push('\n');
        }

        if !self.opts.prepend_file.is_empty() {
            if let Some(contents) = read_file_with(&self.opts.prepend_file, self.opts.encoding) {
                out.push_str(&contents);
            }
        }
        out
    }

    /// The contents of the document `<head>`: the `<title>`, any `--append_head`
    /// file, the generator and charset metas, and the stylesheet link.
    ///
    /// Records the final escaped title in `self.title_text` for
    /// `{{textrill:title}}`.
    pub(super) fn build_head(&mut self, first_line: &str) -> String {
        let mut out = String::new();

        // An explicit `--title` can close the element, so it is always escaped
        // (including `"`); a `--titlefirst` title comes from the document's own
        // first line, so it follows `escape_html_chars`: only `<`, `>` and `&`.
        let title_escaped = if self.opts.titlefirst && self.opts.title.is_empty() {
            // ($tit) = $first_line =~ /^ *(.*)/
            let tit = first_line.trim_start_matches(' ').to_string();
            let tit = tit.trim_end_matches(' ').to_string();
            if self.opts.escape_html_chars {
                self.opts.title = chars::escape(&tit);
            } else {
                self.opts.title = tit;
            }
            // Already escaped above; emitting it again would double escape.
            true
        } else {
            false
        };
        out.push_str(&self.get_tag("title", TAG_START, ""));
        if title_escaped {
            self.title_text = self.opts.title.clone();
        } else {
            self.title_text = chars::escape_attr(&self.opts.title);
        }
        out.push_str(&self.title_text);
        out.push_str(&self.close_tag("title"));
        out.push('\n');

        if self.opts.append_head_available() {
            if let Some(contents) = read_file_with(&self.opts.append_head, self.opts.encoding) {
                out.push_str(&contents);
            }
        }

        if self.opts.lower_case_tags {
            out.push_str(&self.get_tag(
                "meta",
                TAG_EMPTY,
                &format!(" name=\"generator\" content=\"{PROG} v{VERSION}\""),
            ));
        } else {
            out.push_str(&self.get_tag(
                "meta",
                TAG_EMPTY,
                &format!(" NAME=\"generator\" CONTENT=\"{PROG} v{VERSION}\""),
            ));
        }
        // Off by default because the reference emits no charset declaration and
        // byte-identical output is a goal; a GUI turns it on. The line break
        // belongs to the element, so the generator keeps its single newline.
        if self.opts.meta_charset || self.opts.html5 {
            out.push('\n');
            out.push_str(&self.get_tag(
                "meta",
                TAG_EMPTY,
                if self.opts.lower_case_tags {
                    " charset=\"utf-8\""
                } else {
                    " CHARSET=\"utf-8\""
                },
            ));
        }
        // Private reading: one document-wide policy is enough to stop the source
        // URL leaking on any link, including ones a template or the operator's
        // raw HTML supplies. Off by default, so no golden moves.
        if self.opts.no_referrer {
            out.push('\n');
            out.push_str(&self.get_tag(
                "meta",
                TAG_EMPTY,
                if self.opts.lower_case_tags {
                    " name=\"referrer\" content=\"no-referrer\""
                } else {
                    " NAME=\"referrer\" CONTENT=\"no-referrer\""
                },
            ));
        }
        out.push('\n');
        if !self.opts.style_url.is_empty() {
            let style_url = chars::escape_attr(&self.opts.style_url);
            if self.opts.lower_case_tags {
                out.push_str(&self.get_tag(
                    "link",
                    TAG_EMPTY,
                    &format!(" rel=\"stylesheet\" type=\"text/css\" href=\"{style_url}\""),
                ));
            } else {
                out.push_str(&self.get_tag(
                    "link",
                    TAG_EMPTY,
                    &format!(" REL=\"stylesheet\" TYPE=\"text/css\" HREF=\"{style_url}\""),
                ));
            }
            out.push('\n');
        }
        out
    }
}

impl Converter {
    /// The titled landmark the default (untemplated) body opens with, or
    /// nothing at all.
    ///
    /// This is the mirror of the `header` slot [`Self::apply_template`] fills;
    /// the exact string is kept in step with it, including the rule that an
    /// empty title vanishes the whole block rather than rendering an empty
    /// `<h1>`. It is emitted only in HTML5 mode: the parity modes
    /// (`--xhtml`, `--no-html5`) stay byte-identical to the reference, which
    /// prints no such landmark. The GUI's dedicated Title field feeds
    /// `--title`, so the preview of a titled document leads with this heading.
    pub(super) fn default_document_header(&self) -> String {
        if !self.opts.html5 {
            return String::new();
        }
        // `build_head` already recorded the escaped title in `title_text` in a
        // document run. `--extract` skips it, so fall back to the raw option
        // escaped for the same text context.
        let title = if self.title_text.is_empty() {
            chars::escape_attr(&self.opts.title)
        } else {
            self.title_text.clone()
        };
        if title.is_empty() {
            String::new()
        } else {
            format!("<header class=\"document-header\">\n<h1>{title}</h1>\n</header>\n")
        }
    }

    /// Collect and validate the note sets, returning `(stripped body, citations
    /// HTML, glossary HTML)`. All three are empty when both modes are off, which
    /// must leave the body untouched.
    pub(super) fn collect_notes(&self, body: &str) -> Result<(String, String, String), String> {
        if !self.opts.citations && !self.opts.glossary {
            return Ok((String::new(), String::new(), String::new()));
        }
        let (stripped, notes) =
            crate::notes::collect(body, (self.opts.citations, self.opts.glossary));
        crate::notes::validate(&notes)?;
        Ok((
            stripped,
            notes.render_one(crate::notes::Kind::Citation),
            notes.render_one(crate::notes::Kind::Glossary),
        ))
    }

    /// Fill the active template's slots and assemble the page:
    /// `{{textrill:content}}` (the sectioned body), `toc`, `title`, `head`,
    /// `pager` (always empty; pagers belong to `--chunk`), `citations` and
    /// `glossary`.
    pub(super) fn apply_template(
        &self,
        start: &str,
        body: &str,
        toc: &str,
        tail: &str,
        citations: &str,
        glossary: &str,
    ) -> String {
        // A template that names neither slot would silently drop the notes, so
        // fall back to the same end-of-body placement the untemplated path uses.
        let has_slot = |name: &str| {
            self.template_text
                .contains(&format!("{{{{textrill:{name}}}}}"))
        };
        let mut body = body.to_string();
        if !citations.is_empty() && !has_slot("citations") {
            body.push_str(citations);
        }
        if !glossary.is_empty() && !has_slot("glossary") {
            body.push_str(glossary);
        }
        // `--lang_runs` marks passages in another script in the body slot (the
        // templated sibling of the untemplated wrap above). The notes render
        // into their own slots and are left alone in either case.
        if self.opts.lang_runs {
            body = crate::langdetect::wrap_runs(&body, &self.opts.lang);
        }
        // `header` is the titled landmark or nothing at all: an empty `<h1>`
        // (or an empty wrapper around it) is exactly the silent-empty frame a
        // shipped template must never leave, so the whole block vanishes when
        // there is no title rather than rendering an empty one.
        let header = if self.title_text.is_empty() {
            String::new()
        } else {
            format!(
                "<header class=\"document-header\">\n<h1>{}</h1>\n</header>\n",
                self.title_text
            )
        };
        // `lang` is the declared language as an attribute value, so a shipped
        // template writes `<html lang="{{textrill:lang}}">` instead of hard-
        // coding `en`. It is escaped like any other attribute; an empty
        // `--lang` therefore renders an empty attribute in a template.
        let lang = chars::escape_attr(&self.opts.lang);
        let mut slots = vec![
            ("content", body.as_str()),
            ("toc", toc),
            ("title", self.title_text.as_str()),
            ("lang", lang.as_str()),
            ("header", header.as_str()),
            ("head", self.head_inner.as_str()),
            ("pager", ""),
            ("citations", citations),
            ("glossary", glossary),
        ];
        // User parameters extend the fixed set with `var:name` keys, owned by
        // this frame so the borrows stay live through `apply`. Values go in
        // verbatim, exactly as `--var` bound them.
        let var_keys: Vec<String> = self
            .opts
            .vars
            .iter()
            .map(|(name, _)| format!("var:{name}"))
            .collect();
        for (i, (_, value)) in self.opts.vars.iter().enumerate() {
            slots.push((var_keys[i].as_str(), value.as_str()));
        }
        let filled = crate::template::apply(&self.template_text, &slots);
        if self.document_template {
            return filled;
        }
        let mut out = String::with_capacity(start.len() + filled.len() + tail.len());
        out.push_str(start);
        out.push_str(&filled);
        out.push_str(tail);
        out
    }

    /// One top-level page produced by `--chunk`: a full document and the file
    /// name it should be written to, derived from the caller's `--outfile`.
    pub(super) fn convert_to_parts(
        &mut self,
        sources: Vec<String>,
        string_mode: bool,
    ) -> (String, String, String) {
        let mut start = String::new();
        let mut body = String::new();
        let mut count = 0;
        for source in &sources {
            if string_mode {
                let mut para = source.clone();
                if let Some(stripped) = para.strip_suffix('\n') {
                    para = stripped.to_string();
                }
                if count == 0 {
                    start.push_str(&self.do_file_start(&para));
                }
                self.links.sect_once_done = vec![false; self.links.rules.len()];
                let p = self.process_chunk(&para, false, false);
                body.push_str(&p);
                body.push('\n');
                self.print_count += 1;
                count += 1;
                continue;
            }
            for rec in paragraph_records(source) {
                let mut para = rec;
                if let Some(stripped) = para.strip_suffix('\n') {
                    para = stripped.to_string();
                }
                if count == 0 {
                    start.push_str(&self.do_file_start(&para));
                }
                self.links.sect_once_done = vec![false; self.links.rules.len()];
                let p = self.process_chunk(&para, false, false);
                body.push_str(&p);
                body.push('\n');
                self.print_count += 1;
                count += 1;
            }
        }

        self.finish_body(&mut body);
        let tail = self.finish_tail();
        (start, body, tail)
    }

    /// The state-dependent close-out appended to the body after the last
    /// paragraph: end an open list or preformatted block, close any remaining
    /// XHTML tags, then append `--append_file`. Shared by the buffered path and
    /// [`Converter::convert_stream`].
    pub(super) fn finish_body(&mut self, body: &mut String) {
        if self.mode & LIST != 0 {
            let nl = self.listnum;
            self.endlist(nl, body, &mut 0);
        }
        if self.mode & PRE != 0 {
            let tag = self.close_tag("pre");
            body.push_str(&tag);
        }
        if self.closes_optional_tags() && !self.opts.extract && !self.tags.is_empty() {
            let mut open_tag = self.tags.last().cloned().unwrap_or_default();
            while !self.tags.is_empty() && open_tag != "body" && open_tag != "html" {
                body.push_str(&self.close_tag(""));
                open_tag = self.tags.last().cloned().unwrap_or_default();
            }
            body.push('\n');
        }
        if !self.opts.append_file.is_empty() {
            if let Some(contents) = read_file_with(&self.opts.append_file, self.opts.encoding) {
                body.push_str(&contents);
            }
        }
    }

    /// The document footer: the `body` and `html` close tags, unless the output
    /// was extracted or nothing was printed.
    pub(super) fn finish_tail(&mut self) -> String {
        let mut tail = String::new();
        if self.print_count > 0 && !self.opts.extract {
            tail.push_str(&self.close_tag("body"));
            tail.push('\n');
            tail.push_str(&self.close_tag("html"));
            tail.push('\n');
        }
        tail
    }
}


/// Find the best split point near or before `max_chars` (counting raw chars).
/// Prefer sentence/phrase boundaries: punctuation like . , ; : ! ? and CJK
/// punctuation like 。、；：！？．， etc.
fn find_split_point(text: &str, max_chars: usize) -> usize {
    if max_chars == 0 {
        return 0;
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return chars.len();
    }
    // Look backwards from max_chars for punctuation boundaries
    let mut best = None;
    let search_start = if max_chars > 200 { max_chars - 200 } else { 0 };
    for i in (search_start..max_chars).rev() {
        let c = chars[i];
        // Check for punctuation that typically ends sentences/clauses
        if c == '.' || c == '!' || c == '?' || c == ';' || c == ':' || c == '。' || c == '！' || c == '？' || c == '；' || c == '：' || c == '．' {
            // Include the punctuation
            best = Some(i + 1);
            break;
        }
        if c == ',' || c == '、' || c == '，' {
            // Also acceptable as clause boundary
            if best.is_none() {
                best = Some(i + 1);
            }
        }
    }
    if let Some(b) = best {
        return b;
    }
    // Fall back to whitespace
    for i in (search_start..max_chars).rev() {
        if chars[i].is_whitespace() {
            return i + 1;
        }
    }
    max_chars
}

/// Split raw plain text into chunks by character count with smart boundaries.
fn split_text_by_chars(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = text.to_string();
    while !remaining.is_empty() {
        let chars: Vec<char> = remaining.chars().collect();
        if chars.len() <= max_chars {
            chunks.push(remaining);
            break;
        }
        let split_at_chars = find_split_point(&remaining, max_chars);
        // Convert char count back to byte index
        let byte_pos: usize = remaining.char_indices().nth(split_at_chars).map(|(i, _)| i).unwrap_or(remaining.len());
        let chunk = remaining[..byte_pos].to_string();
        remaining = remaining[byte_pos..].to_string();
        // Trim leading whitespace from next chunk
        remaining = remaining.trim_start_matches(|c: char| c.is_whitespace() && c != '\n').to_string();
        chunks.push(chunk);
    }
    chunks
}

impl Converter {

    /// Convert and split the body into character-limited chunks.
    pub fn try_convert_chunk_chars(&mut self) -> (Vec<(String, String)>, Vec<String>) {
        let mut sources: Vec<String> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        let mut worst = Resolved::Utf8;
        for f in &self.opts.infile {
            if f == "-" {
                let mut buf = String::new();
                use std::io::Read;
                let _ = std::io::stdin().read_to_string(&mut buf);
                sources.push(buf);
            } else {
                match read_with(f, self.opts.encoding) {
                    Some((c, resolved)) => {
                        if resolved.notice_rank() > worst.notice_rank() {
                            worst = resolved;
                        }
                        sources.push(c);
                    }
                    None => {
                        eprintln!("Could not open {f}\n");
                        unreadable.push(f.clone());
                    }
                }
            }
        }
        self.resolved = worst;
        // Concatenate all sources
        let combined = sources.join("\n\n");
        let max_chars = self.opts.chunk_chars;
        let text_chunks = split_text_by_chars(&combined, max_chars);
        
        let mut files = Vec::new();
        for (i, chunk_text) in text_chunks.iter().enumerate() {
            let chunk_num = i + 1;
            let mut conv = Converter::new(self.opts.clone());
            let out = conv.convert_text(chunk_text);
            let name = self.chunk_filename_chars(chunk_num);
            files.push((name, out));
        }
        (files, unreadable)
    }

    /// Base file name for char-chunked page `n`.
    pub(super) fn chunk_filename_chars(&self, n: usize) -> String {
        let path = std::path::Path::new(&self.opts.outfile);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".to_string());
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("html");
        let name = format!("{stem}-chunk-{n:02}.{ext}");
        match std::path::Path::new(&self.opts.outfile).parent() {
            Some(dir) if !dir.as_os_str().is_empty() => {
                dir.join(name).to_string_lossy().into_owned()
            }
            _ => name,
        }
    }

    /// Convert and split the body into one full document per top-level section,
    /// for `--chunk`. "Top-level" is the shallowest heading level present; its
    /// subsections stay in the same page, and content before the first heading
    /// becomes page one. Only `infile` inputs are chunked.
    pub fn try_convert_chunked(&mut self) -> (Vec<(String, String)>, Vec<String>) {
        let mut sources: Vec<String> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        let mut worst = Resolved::Utf8;
        for f in &self.opts.infile {
            if f == "-" {
                let mut buf = String::new();
                use std::io::Read;
                let _ = std::io::stdin().read_to_string(&mut buf);
                sources.push(buf);
            } else {
                match read_with(f, self.opts.encoding) {
                    Some((c, resolved)) => {
                        if resolved.notice_rank() > worst.notice_rank() {
                            worst = resolved;
                        }
                        sources.push(c);
                    }
                    None => {
                        eprintln!("Could not open {f}\n");
                        unreadable.push(f.clone());
                    }
                }
            }
        }
        self.resolved = worst;
        let (start, body, tail) = self.convert_to_parts(sources, false);
        let files = self.assemble_chunked(&start, &body, &tail);
        (files, unreadable)
    }

    /// Split an already-built body into pages and assemble each into a full
    /// document. Shared by [`Converter::try_convert_chunked`].
    pub(super) fn assemble_chunked(
        &self,
        start: &str,
        body: &str,
        tail: &str,
    ) -> Vec<(String, String)> {
        let (preamble, sections) = crate::section::split_sections(body);
        if sections.is_empty() {
            let mut single = String::with_capacity(start.len() + body.len() + tail.len());
            single.push_str(start);
            single.push_str(body);
            single.push_str(tail);
            return vec![(self.chunk_filename(1), single)];
        }
        let top = sections.iter().map(|s| s.level).min().unwrap_or(1);
        let mut pages: Vec<String> = Vec::new();
        // The `chunk-N` id of the top-level section each page holds, in page
        // order, so a cross-file TOC can point *into* a page.
        let mut page_ids: Vec<String> = Vec::new();
        let mut leading = String::new();
        let mut current: Option<String> = None;
        let mut current_id = String::new();
        for s in &sections {
            if s.level == top {
                if let Some(page) = current.take() {
                    pages.push(page);
                    page_ids.push(std::mem::take(&mut current_id));
                }
                let mut page = std::mem::take(&mut leading);
                page.push_str(&s.html);
                current = Some(page);
                current_id = s.id.clone();
            } else if let Some(page) = current.as_mut() {
                page.push_str(&s.html);
            } else {
                leading.push_str(&s.html);
            }
        }
        if let Some(page) = current {
            pages.push(page);
            page_ids.push(current_id);
        } else if !leading.trim().is_empty() {
            // No top-level section at all, which cannot happen while `top` is
            // the minimum level present; a page still needs an id to be a
            // target.
            pages.push(leading);
            page_ids.push(String::new());
        }
        if !preamble.is_empty() {
            if let Some(first) = pages.first_mut() {
                first.insert_str(0, &preamble);
            } else {
                pages.push(preamble);
            }
        }
        let names: Vec<String> = (1..=pages.len()).map(|n| self.chunk_filename(n)).collect();
        // Links between sibling files are bare file names, not the (possibly
        // absolute) write path, so the pages stay portable if moved together.
        let links: Vec<String> = (1..=pages.len()).map(|n| self.chunk_basename(n)).collect();
        let toc = if self.opts.toc {
            Some(self.render_page_toc(&sections, top, &links, &page_ids))
        } else {
            None
        };
        pages
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let mut out = String::with_capacity(start.len() + page.len() + tail.len() + 256);
                out.push_str(start);
                if let Some(toc) = &toc {
                    out.push_str(toc);
                }
                // The wrapper is emitted for `--toc` as well as `--section`:
                // it is the anchor the TOC's `file#chunk-N` points at, and a
                // dangling link is worse than a redundant `<article>`.
                if (self.opts.section || self.opts.toc) && !page_ids[i].is_empty() {
                    out.push_str("<article class=\"section\" id=\"");
                    out.push_str(&page_ids[i]);
                    out.push_str("\">\n");
                    out.push_str(page);
                    if !page.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str("</article>\n");
                } else {
                    out.push_str(page);
                }
                out.push_str(&self.render_pager(&links, i));
                out.push_str(tail);
                (names[i].clone(), out)
            })
            .collect()
    }

    /// Base file name for page `n`: `<stem>-chunk-NN.html`.
    pub(super) fn chunk_basename(&self, n: usize) -> String {
        let path = std::path::Path::new(&self.opts.outfile);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".to_string());
        format!("{stem}-chunk-{n:02}.html")
    }

    /// Write path for page `n`: its base name next to `--outfile`.
    pub(super) fn chunk_filename(&self, n: usize) -> String {
        let name = self.chunk_basename(n);
        match std::path::Path::new(&self.opts.outfile).parent() {
            Some(dir) if !dir.as_os_str().is_empty() => {
                dir.join(name).to_string_lossy().into_owned()
            }
            _ => name,
        }
    }

    /// TOC across files: each top-level page links to its own file.
    pub(super) fn render_page_toc(
        &self,
        sections: &[crate::section::Section],
        top: usize,
        names: &[String],
        page_ids: &[String],
    ) -> String {
        let mut out = String::from("<nav class=\"toc\" id=\"toc\">\n<ol class=\"toc-list\">\n");
        let mut page = 0usize;
        for s in sections {
            if s.level == top {
                out.push_str("<li class=\"toc-h");
                out.push_str(&s.level.to_string());
                out.push_str("\"><a href=\"");
                out.push_str(&names[page]);
                // Harmless when the page has no anchor of its own, and the
                // fragment is skipped rather than emitted empty so the href
                // stays a clean relative path.
                if let Some(id) = page_ids.get(page).filter(|id| !id.is_empty()) {
                    out.push('#');
                    out.push_str(id);
                }
                out.push_str("\">");
                out.push_str(&s.label);
                out.push_str("</a></li>\n");
                page += 1;
            }
        }
        out.push_str("</ol>\n</nav>\n");
        out
    }

    /// Prev/next links between chunked pages.
    pub(super) fn render_pager(&self, names: &[String], current: usize) -> String {
        let mut out = String::from("<nav class=\"pager\" aria-label=\"Parts\">\n");
        if current > 0 {
            out.push_str("<a href=\"");
            out.push_str(&names[current - 1]);
            out.push_str("\" rel=\"prev\">&#8592; part ");
            out.push_str(&(current).to_string());
            out.push_str("</a>\n");
        }
        if current + 1 < names.len() {
            out.push_str("<a href=\"");
            out.push_str(&names[current + 1]);
            out.push_str("\" rel=\"next\">part ");
            out.push_str(&(current + 2).to_string());
            out.push_str(" &#8594;</a>\n");
        }
        out.push_str("</nav>\n");
        out
    }
}

impl Options {
    pub(super) fn append_head_available(&self) -> bool {
        !self.append_head.is_empty()
    }
}
