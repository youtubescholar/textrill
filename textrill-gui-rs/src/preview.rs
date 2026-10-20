// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The native rendered preview (roadmap §7, workstreams 1 and 2).
//!
//! Textrill's output is HTML, and egui cannot render HTML. Rather than embed
//! a browser engine (a large C dependency, and against the GUI's no-GTK
//! stance) or add a network path, this draws an **honest subset** of the
//! produced HTML with egui's own text and layout: headings, paragraphs,
//! lists (`ul`/`ol`/`dl`), `strong`/`em`/`u`/`code`, quotes, `pre`, tables,
//! links, rules and the `header`/`nav`/`article`/`main` landmarks.
//!
//! It is not pixel-identical to a browser and says so in the UI. What it
//! guarantees is structural fidelity to *our* output (it parses the same
//! bytes the HTML pane shows) and zero I/O: the only interaction it performs
//! is reporting a clicked link; the app decides whether to hand it to the
//! desktop.
//!
//! The parser is deliberately tiny and tolerant: unknown tags render their
//! children rather than disappearing, unclosed tags are closed at the end,
//! and comments/doctypes are skipped. It only needs to cope with the tag set
//! the engine emits (see `convert.rs`) plus whatever a template adds.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{FontFamily, RichText, Sense, TextFormat, TextStyle, Ui};

use crate::fonts;

// ------------------------------------------------------------------ parsing

/// One child of an element: nested markup, or a run of text (undecoded).
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Element(Element),
    Text(String),
}

/// A parsed element. Tag names are lowercased; attributes keep their spelling
/// and their (entity-decoded) values.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Item>,
}

impl Element {
    /// The first value for `name`, case-insensitively.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Whether the element's `class` list contains `class`.
    pub fn class_has(&self, class: &str) -> bool {
        self.attr("class")
            .is_some_and(|classes| classes.split_whitespace().any(|c| c == class))
    }

    /// The element's text content, entities decoded, whitespace collapsed
    /// (the HTML-rendering rule for inline text; `<pre>` is handled by the
    /// caller, which reads raw text instead). A `<br>` contributes a hard
    /// newline, which the whitespace pass must not flatten.
    pub fn text(&self) -> String {
        let mut out = String::new();
        collect_text(self, &mut out);
        hard_breaks(collapse_ws(&decode_entities(&out)))
    }
}

/// A marker for a hard line break (`<br>`), so the whitespace collapse pass
/// does not turn it into an ordinary space.
const HARD_BREAK: char = '\u{0}';

fn collect_text(element: &Element, out: &mut String) {
    for item in &element.children {
        match item {
            Item::Text(text) => out.push_str(text),
            Item::Element(child) => {
                if child.tag == "br" {
                    out.push(HARD_BREAK);
                }
                collect_text(child, out);
            }
        }
    }
}

/// Turn the [`HARD_BREAK`] markers into real newlines.
fn hard_breaks(src: String) -> String {
    src.replace(HARD_BREAK, "\n")
}

/// Tags that never have children, so `<br>` need not open an element.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// The bytes between `<body…>` and `</body>`, or the whole input when there is
/// no body wrapper (a template may be a fragment). Case-insensitive, and
/// tolerant of attributes on `<body>`.
pub fn extract_body(html: &str) -> &str {
    let lower = html.to_ascii_lowercase();
    let Some(start) = lower.find("<body") else {
        return html;
    };
    let Some(open_end) = lower[start..].find('>') else {
        return html;
    };
    let content_start = start + open_end + 1;
    match lower[content_start..].find("</body") {
        Some(len) => &html[content_start..content_start + len],
        None => &html[content_start..],
    }
}

/// Parse the body of an HTML document into a shallow tree.
pub fn parse(html: &str) -> Vec<Item> {
    let body = extract_body(html);
    let mut stack: Vec<Element> = Vec::new();
    let mut roots: Vec<Item> = Vec::new();
    let mut text = String::new();
    let bytes = body.as_bytes();
    let mut i = 0;

    fn push_text(stack: &mut [Element], roots: &mut Vec<Item>, text: &mut String) {
        if text.is_empty() {
            return;
        }
        let node = Item::Text(std::mem::take(text));
        match stack.last_mut() {
            Some(current) => current.children.push(node),
            None => roots.push(node),
        }
    }

    while i < bytes.len() {
        if bytes[i] != b'<' {
            let next = bytes[i..]
                .iter()
                .position(|b| *b == b'<')
                .map_or(bytes.len(), |n| i + n);
            text.push_str(&body[i..next]);
            i = next;
            continue;
        }
        // Comments and declarations are not rendered.
        if body[i..].starts_with("<!--") {
            match body[i + 4..].find("-->") {
                Some(len) => i += 4 + len + 3,
                None => break,
            }
            continue;
        }
        if body[i..].starts_with("<!") || body[i..].starts_with("<?") {
            match body[i..].find('>') {
                Some(len) => i += len + 1,
                None => break,
            }
            continue;
        }
        // Closing tag: close the nearest matching element, dropping any
        // unclosed children above it (tolerant recovery).
        if body[i..].starts_with("</") {
            let Some(end) = body[i..].find('>') else {
                break;
            };
            let name = body[i + 2..i + end].trim().to_ascii_lowercase();
            push_text(&mut stack, &mut roots, &mut text);
            if let Some(depth) = stack.iter().rposition(|e| e.tag == name) {
                while stack.len() > depth {
                    let done = stack.pop().expect("depth is within the stack");
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(Item::Element(done)),
                        None => roots.push(Item::Element(done)),
                    }
                }
            }
            i += end + 1;
            continue;
        }
        // Opening tag: read the name, then the attributes, respecting quotes.
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
            j += 1;
        }
        if j == i + 1 {
            // A bare `<` in text.
            text.push('<');
            i += 1;
            continue;
        }
        let tag = body[i + 1..j].to_ascii_lowercase();
        let mut quote: Option<u8> = None;
        let mut k = j;
        while k < bytes.len() {
            let b = bytes[k];
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if b == b'"' || b == b'\'' => quote = Some(b),
                None if b == b'>' => break,
                None => {}
            }
            k += 1;
        }
        let Some(close) = (k < bytes.len()).then_some(k) else {
            break;
        };
        let attr_src = &body[j..close];
        let self_closing = attr_src.trim_end().ends_with('/');
        push_text(&mut stack, &mut roots, &mut text);
        let element = Element {
            tag: tag.clone(),
            attrs: parse_attrs(attr_src),
            children: Vec::new(),
        };
        i = close + 1;
        if self_closing || VOID.contains(&tag.as_str()) {
            match stack.last_mut() {
                Some(parent) => parent.children.push(Item::Element(element)),
                None => roots.push(Item::Element(element)),
            }
        } else {
            stack.push(element);
        }
    }
    push_text(&mut stack, &mut roots, &mut text);
    // Close anything the document forgot to close.
    while let Some(done) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(Item::Element(done)),
            None => roots.push(Item::Element(done)),
        }
    }
    roots
}

/// Parse `name="value" name='value' name=value` and bare names.
fn parse_attrs(src: &str) -> Vec<(String, String)> {
    let bytes = src.as_bytes();
    let mut attrs = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            i += 1;
        }
        if start == i {
            i += 1;
            continue;
        }
        let name = src[start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let value_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                value = src[value_start..i].to_string();
                i += usize::from(i < bytes.len());
            } else {
                let value_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = src[value_start..i].to_string();
            }
        }
        attrs.push((name, decode_entities(&value)));
    }
    attrs
}

/// Decode the entities the engine can produce (it escapes `& < > " '`) plus
/// `&nbsp;` and numeric references.
pub fn decode_entities(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let Some(semi) = tail.find(';').filter(|semi| *semi <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..semi];
        let replacement = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => {
                if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    None
                }
            }
        };
        match replacement {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse each run of whitespace to one space, keeping a single space at
/// the start and end (for joining inline runs).
fn collapse_ws_edges(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut space = false;
    for c in src.chars() {
        if c.is_whitespace() {
            if !space {
                out.push(' ');
                space = true;
            }
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

/// Collapse HTML whitespace (spaces, tabs, newlines) to single spaces.
fn collapse_ws(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut space = false;
    for c in src.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// Raw text of an element, entities decoded, whitespace preserved (`pre`).
fn raw_text(element: &Element) -> String {
    let mut out = String::new();
    collect_text(element, &mut out);
    hard_breaks(decode_entities(&out))
}

// ----------------------------------------------------------------- outline

/// One heading in the produced document, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// `1` for `<h1>`, …, `6` for `<h6>`.
    pub level: u8,
    /// The `id` attribute, when the heading carries one.
    pub id: Option<String>,
    /// The heading's text, entities decoded and whitespace collapsed.
    pub text: String,
}

/// The document's headings, which is what the outline pane lists. The engine
/// does not expose its section tree structurally, so the outline is derived
/// from the same bytes the preview renders — by construction it cannot drift
/// from what is shown.
pub fn outline(html: &str) -> Vec<Heading> {
    fn walk(items: &[Item], out: &mut Vec<Heading>) {
        for item in items {
            if let Item::Element(element) = item {
                if let Some(level) = heading_level(&element.tag) {
                    out.push(Heading {
                        level,
                        id: anchor_of(element),
                        text: element.text(),
                    });
                }
                walk(&element.children, out);
            }
        }
    }
    let mut headings = Vec::new();
    walk(&parse(html), &mut headings);
    headings
}

/// The heading's anchor: its own `id`, else the `name` of a nested `<a>`
/// (the engine's section headings wrap `<a name="section_…">`), else the
/// fragment of a nested `href="#…"`.
fn anchor_of(element: &Element) -> Option<String> {
    if let Some(id) = element.attr("id") {
        return Some(id.to_string());
    }
    for item in &element.children {
        if let Item::Element(child) = item {
            if child.tag == "a" {
                if let Some(name) = child.attr("name") {
                    return Some(name.to_string());
                }
                if let Some(href) = child.attr("href") {
                    if let Some(fragment) = href.strip_prefix('#') {
                        return Some(fragment.to_string());
                    }
                }
            }
            if let Some(found) = anchor_of(child) {
                return Some(found);
            }
        }
    }
    None
}

fn heading_level(tag: &str) -> Option<u8> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

// ---------------------------------------------------------------- rendering

/// What the preview interacted with this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A link was clicked; the app decides how to hand it to the desktop.
    Open(String),
}

/// Draw `html` as the rendered preview and return any interaction.
pub fn show(ui: &mut Ui, html: &str) -> Option<Action> {
    let items = parsed(ui, html);
    if items.is_empty() {
        ui.weak("Nothing converted yet — type or load a document.");
        return None;
    }
    let mut action = None;
    render_blocks(ui, &items, &mut action);
    action
}

/// Parse once per distinct output. The preview redraws every frame; reparsing
/// a few hundred kilobytes sixty times a second would be wasteful, and the
/// produced HTML only changes on a conversion.
fn parsed(ui: &Ui, html: &str) -> Arc<Vec<Item>> {
    let key = egui::Id::new("textrill-preview-parsed");
    let mut hasher = DefaultHasher::new();
    html.len().hash(&mut hasher);
    html.hash(&mut hasher);
    let hash = hasher.finish();
    if let Some((cached, items)) = ui
        .ctx()
        .data_mut(|data| data.get_temp::<(u64, Arc<Vec<Item>>)>(key))
    {
        if cached == hash {
            return items;
        }
    }
    let items = Arc::new(parse(html));
    ui.ctx()
        .data_mut(|data| data.insert_temp(key, (hash, Arc::clone(&items))));
    items
}

/// Inline formatting stacked while walking a text run.
#[derive(Clone, Default)]
struct Inline {
    bold: bool,
    italic: bool,
    underline: bool,
    code: bool,
    link: Option<String>,
}

impl Inline {
    fn bold() -> Inline {
        Inline {
            bold: true,
            ..Inline::default()
        }
    }
}

/// A laid-out paragraph plus where its links live, in character offsets, so a
/// click can be mapped back to a URL.
struct Paragraph {
    job: LayoutJob,
    links: Vec<(Range<usize>, String)>,
}

fn inline_paragraph(ui: &Ui, items: &[Item], base: &Inline) -> Paragraph {
    let mut paragraph = Paragraph {
        job: LayoutJob::default(),
        links: Vec::new(),
    };
    let mut chars = 0usize;
    append_items(ui, &mut paragraph, &mut chars, items, base);
    // A trailing space is never visible at a line end; drop it (the produced
    // HTML wraps blocks with newlines, which the edge-preserving collapse
    // turns into one).
    while paragraph.job.text.ends_with(' ') {
        paragraph.job.text.pop();
        match paragraph.job.sections.last_mut() {
            Some(last) => {
                last.byte_range.end = last.byte_range.end.saturating_sub(1);
                if last.byte_range.is_empty() {
                    paragraph.job.sections.pop();
                }
            }
            None => break,
        }
    }
    paragraph
}

fn append_items(
    ui: &Ui,
    paragraph: &mut Paragraph,
    chars: &mut usize,
    items: &[Item],
    style: &Inline,
) {
    for item in items {
        match item {
            Item::Text(text) => {
                // Collapse whitespace but keep one space at each edge: the
                // spaces around `<em>`/`<a>` are content ("Some <em>x</em>
                // here"), not padding. Adjacent runs are joined below so no
                // double space survives.
                let text = collapse_ws_edges(&decode_entities(text));
                let mut text = text.as_str();
                if paragraph.job.text.is_empty() || paragraph.job.text.ends_with('\n') {
                    text = text.trim_start();
                } else if paragraph.job.text.ends_with(' ') && text.starts_with(' ') {
                    text = &text[1..];
                }
                if text.is_empty() {
                    continue;
                }
                append_run(ui, paragraph, chars, text, style);
            }
            Item::Element(element) => match element.tag.as_str() {
                "br" => append_run(ui, paragraph, chars, "\n", style),
                "strong" | "b" => {
                    let nested = Inline {
                        bold: true,
                        ..style.clone()
                    };
                    append_items(ui, paragraph, chars, &element.children, &nested);
                }
                "em" | "i" => {
                    let nested = Inline {
                        italic: true,
                        ..style.clone()
                    };
                    append_items(ui, paragraph, chars, &element.children, &nested);
                }
                "u" | "ins" => {
                    let nested = Inline {
                        underline: true,
                        ..style.clone()
                    };
                    append_items(ui, paragraph, chars, &element.children, &nested);
                }
                "code" | "kbd" | "samp" => {
                    let nested = Inline {
                        code: true,
                        ..style.clone()
                    };
                    append_items(ui, paragraph, chars, &element.children, &nested);
                }
                "a" => {
                    let nested = Inline {
                        link: element.attr("href").map(str::to_string),
                        ..style.clone()
                    };
                    append_items(ui, paragraph, chars, &element.children, &nested);
                }
                // A block element met inside a paragraph (a table cell with a
                // nested list, a quote paragraph) contributes its text with a
                // break between children rather than vanishing.
                "p" | "div" | "li" | "dt" | "dd" | "tr" | "td" | "th" | "blockquote" => {
                    append_run(ui, paragraph, chars, "\n", style);
                    append_items(ui, paragraph, chars, &element.children, style);
                }
                _ => append_items(ui, paragraph, chars, &element.children, style),
            },
        }
    }
}

fn append_run(ui: &Ui, paragraph: &mut Paragraph, chars: &mut usize, text: &str, style: &Inline) {
    let start = *chars;
    *chars += text.chars().count();
    if let Some(href) = &style.link {
        if !href.is_empty() && href != "#" {
            paragraph.links.push((start..*chars, href.clone()));
        }
    }
    paragraph.job.append(text, 0.0, text_format(ui, style));
}

fn text_format(ui: &Ui, style: &Inline) -> TextFormat {
    let mut font = if style.code {
        TextStyle::Monospace.resolve(ui.style())
    } else {
        TextStyle::Body.resolve(ui.style())
    };
    if style.bold {
        font.family = FontFamily::Name(fonts::BOLD_FAMILY.into());
    }
    let link_color = ui.visuals().hyperlink_color;
    let color = if style.link.is_some() {
        link_color
    } else {
        ui.visuals().text_color()
    };
    let underline = if style.link.is_some() {
        egui::Stroke::new(1.0, link_color)
    } else if style.underline {
        egui::Stroke::new(1.0, color)
    } else {
        egui::Stroke::NONE
    };
    TextFormat {
        font_id: font,
        color,
        background: if style.code {
            ui.visuals().code_bg_color
        } else {
            egui::Color32::TRANSPARENT
        },
        italics: style.italic,
        underline,
        ..Default::default()
    }
}

/// Draw a run of inline content as a wrapping paragraph, wiring link clicks.
fn render_paragraph(ui: &mut Ui, items: &[Item], style: &Inline, action: &mut Option<Action>) {
    let paragraph = inline_paragraph(ui, items, style);
    if paragraph.job.text.is_empty() {
        return;
    }
    let mut job = paragraph.job;
    job.wrap.max_width = ui.available_width().max(1.0);
    let plain = job.text.clone();
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
    let response = ui.add(egui::Label::new(job).wrap().sense(Sense::click()));
    // A `LayoutJob` label carries no text for screen readers or tests; expose
    // the plain text (the document content is the accessible name here).
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, plain.clone()));
    if response.clicked() && !paragraph.links.is_empty() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let local = pointer - response.rect.min;
            let index = galley.cursor_from_pos(local).index.0;
            if let Some((_, href)) = paragraph
                .links
                .iter()
                .find(|(range, _)| range.contains(&index))
            {
                *action = Some(Action::Open(href.clone()));
            }
        }
    }
}

fn render_blocks(ui: &mut Ui, items: &[Item], action: &mut Option<Action>) {
    for item in items {
        match item {
            Item::Text(text) => {
                let text = decode_entities(text);
                if text.trim().is_empty() {
                    continue;
                }
                let style = Inline::default();
                let build = inline_paragraph(ui, &[Item::Text(text)], &style);
                let mut job = build.job;
                job.wrap.max_width = ui.available_width().max(1.0);
                let plain = job.text.clone();
                let response = ui.add(egui::Label::new(job).wrap());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Label, true, plain.clone())
                });
            }
            Item::Element(element) => render_block(ui, element, action),
        }
    }
}

fn render_block(ui: &mut Ui, element: &Element, action: &mut Option<Action>) {
    match element.tag.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = heading_level(&element.tag).unwrap_or(1);
            ui.add_space(8.0);
            let mut style = Inline::bold();
            style.italic = false;
            let build = inline_paragraph(ui, &element.children, &style);
            let mut job = build.job;
            let size = heading_size(ui, level);
            for section in &mut job.sections {
                section.format.font_id.size = size;
            }
            job.wrap.max_width = ui.available_width().max(1.0);
            let plain = job.text.clone();
            let response = ui.add(egui::Label::new(job).wrap());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Label, true, plain.clone())
            });
            ui.add_space(2.0);
        }
        "p" => {
            if element.class_has("quote_mail") {
                ui.indent("preview-quote", |ui| {
                    let style = Inline {
                        underline: false,
                        ..Inline::default()
                    };
                    render_paragraph(ui, &element.children, &style, action);
                });
            } else if element.class_has("mail_header") {
                let build = inline_paragraph(
                    ui,
                    &element.children,
                    &Inline {
                        code: true,
                        ..Inline::default()
                    },
                );
                let mut job = build.job;
                job.wrap.max_width = ui.available_width().max(1.0);
                ui.add(egui::Label::new(job).wrap());
            } else {
                render_paragraph(ui, &element.children, &Inline::default(), action);
            }
            ui.add_space(4.0);
        }
        "pre" => {
            let text = raw_text(element);
            let font = TextStyle::Monospace.resolve(ui.style());
            egui::Frame::NONE
                .fill(ui.visuals().code_bg_color)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    let mut job = LayoutJob::default();
                    job.append(
                        &text,
                        0.0,
                        TextFormat {
                            font_id: font,
                            color: ui.visuals().text_color(),
                            ..Default::default()
                        },
                    );
                    job.wrap.max_width = ui.available_width().max(1.0);
                    ui.add(egui::Label::new(job).wrap());
                });
            ui.add_space(4.0);
        }
        "ul" | "ol" => render_list(ui, element, action),
        "dl" => render_definition_list(ui, element, action),
        "table" => render_table(ui, element, action),
        "hr" => {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
        }
        "header" | "nav" | "article" | "main" | "section" | "div" | "body" | "figure"
        | "figcaption" | "details" | "summary" => {
            render_blocks(ui, &element.children, action);
        }
        "blockquote" => {
            ui.indent("preview-blockquote", |ui| {
                render_blocks(ui, &element.children, action);
            });
        }
        "script" | "style" => {}
        "img" => {
            let alt = element.attr("alt").unwrap_or("image");
            ui.weak(format!("[{alt}]"));
        }
        // Inline elements met at block level: still show their text.
        _ => render_paragraph(ui, &element.children, &Inline::default(), action),
    }
}

fn heading_size(ui: &Ui, level: u8) -> f32 {
    let heading = TextStyle::Heading.resolve(ui.style()).size;
    let body = TextStyle::Body.resolve(ui.style()).size;
    let scale = match level {
        1 => 1.18,
        2 => 1.08,
        3 => 1.0,
        4 => 0.94,
        5 => 0.88,
        _ => 0.84,
    };
    (heading * scale).max(body)
}

fn render_list(ui: &mut Ui, element: &Element, action: &mut Option<Action>) {
    let ordered = element.tag == "ol";
    let mut number = element
        .attr("start")
        .and_then(|start| start.parse::<usize>().ok())
        .unwrap_or(1);
    for child in &element.children {
        let Item::Element(li) = child else {
            continue;
        };
        if li.tag != "li" {
            continue;
        }
        let marker = if ordered {
            let text = format!("{number}.");
            number += 1;
            text
        } else {
            "•".to_string()
        };
        ui.horizontal_top(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(marker).weak());
            ui.add_space(4.0);
            ui.vertical(|ui| render_blocks(ui, &li.children, action));
        });
    }
    ui.add_space(2.0);
}

fn render_definition_list(ui: &mut Ui, element: &Element, action: &mut Option<Action>) {
    for child in &element.children {
        let Item::Element(entry) = child else {
            continue;
        };
        match entry.tag.as_str() {
            "dt" => {
                render_paragraph(ui, &entry.children, &Inline::bold(), action);
            }
            "dd" => {
                ui.indent("preview-dd", |ui| {
                    render_blocks(ui, &entry.children, action);
                });
            }
            _ => render_block(ui, entry, action),
        }
    }
    ui.add_space(2.0);
}

/// The alignment HTML asks for on a table cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

fn cell_align(cell: &Element) -> Align {
    let from_style = cell.attr("style").and_then(|style| {
        style.split(';').find_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            (name.trim().eq_ignore_ascii_case("text-align")).then(|| value.trim())
        })
    });
    let value = from_style
        .or_else(|| cell.attr("align"))
        .unwrap_or(if cell.tag == "th" { "center" } else { "left" });
    match value.to_ascii_lowercase().as_str() {
        "right" | "end" => Align::Right,
        "center" => Align::Center,
        _ => Align::Left,
    }
}

fn render_table(ui: &mut Ui, element: &Element, action: &mut Option<Action>) {
    // Flatten rows (however they are wrapped in thead/tbody/tfoot) into cells.
    let mut rows: Vec<Vec<&Element>> = Vec::new();
    for child in &element.children {
        match child {
            Item::Element(section) if section.tag == "tr" => {
                rows.push(cells_of(section));
            }
            Item::Element(section)
                if matches!(section.tag.as_str(), "thead" | "tbody" | "tfoot") =>
            {
                for row in &section.children {
                    if let Item::Element(tr) = row {
                        if tr.tag == "tr" {
                            rows.push(cells_of(tr));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if rows.is_empty() {
        return;
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return;
    }
    // Measure each column at its natural width, then share the pane so a wide
    // table wraps instead of being clipped.
    let body_font = TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().text_color();
    let mut widths = vec![0.0_f32; columns];
    for row in &rows {
        for (column, cell) in row.iter().enumerate() {
            let text = cell.text();
            let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(text, body_font.clone(), color));
            widths[column] = widths[column].max(galley.size().x + 14.0);
        }
    }
    let available = ui.available_width().max(120.0);
    let natural: f32 = widths.iter().sum();
    if natural > available {
        let factor = available / natural;
        for width in &mut widths {
            *width = (*width * factor).max(28.0);
        }
    }
    let caption = element.children.iter().find_map(|child| match child {
        Item::Element(caption) if caption.tag == "caption" => Some(caption.text()),
        _ => None,
    });
    if let Some(caption) = caption {
        ui.label(RichText::new(caption).weak());
    }
    let mut first = true;
    for row in &rows {
        let header = row.iter().all(|cell| cell.tag == "th");
        // Compute the row height at the final column widths.
        let mut row_height = 0.0_f32;
        for (column, cell) in row.iter().enumerate() {
            let job = cell_job(ui, cell);
            let mut job = job;
            job.wrap.max_width = widths[column].max(1.0);
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            row_height = row_height.max(galley.size().y);
        }
        ui.horizontal_top(|ui| {
            for (column, cell) in row.iter().enumerate() {
                let width = widths.get(column).copied().unwrap_or(80.0);
                let align = match cell_align(cell) {
                    Align::Left => egui::Align::Min,
                    Align::Center => egui::Align::Center,
                    Align::Right => egui::Align::Max,
                };
                ui.allocate_ui_with_layout(
                    egui::vec2(width, row_height),
                    egui::Layout::top_down(align),
                    |ui| {
                        let mut job = cell_job(ui, cell);
                        job.wrap.max_width = width.max(1.0);
                        ui.add(egui::Label::new(job).wrap());
                    },
                );
            }
        });
        if (first && header) || rows.len() == 1 {
            ui.separator();
        } else {
            ui.add_space(2.0);
        }
        first = false;
    }
    ui.add_space(4.0);
    let _ = action;
}

fn cells_of(row: &Element) -> Vec<&Element> {
    row.children
        .iter()
        .filter_map(|child| match child {
            Item::Element(cell) if matches!(cell.tag.as_str(), "td" | "th") => Some(cell),
            _ => None,
        })
        .collect()
}

fn cell_job(ui: &Ui, cell: &Element) -> LayoutJob {
    let style = if cell.tag == "th" {
        Inline::bold()
    } else {
        Inline::default()
    };
    inline_paragraph(ui, &cell.children, &style).job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_extraction_handles_attributes_and_fragments() {
        assert_eq!(
            extract_body("<html><body class=\"x\">hi</body></html>"),
            "hi"
        );
        assert_eq!(extract_body("<p>no body</p>"), "<p>no body</p>");
        assert_eq!(extract_body("<BODY>\n<p>a</p>\n</BODY>"), "\n<p>a</p>\n");
    }

    #[test]
    fn parsing_builds_a_nested_tree() {
        let items = parse("<p>Hello <strong>world</strong></p>");
        let Item::Element(p) = &items[0] else {
            panic!("expected a paragraph");
        };
        assert_eq!(p.tag, "p");
        assert!(matches!(p.children[0], Item::Text(ref t) if t == "Hello "));
        let Item::Element(strong) = &p.children[1] else {
            panic!("expected strong");
        };
        assert_eq!(strong.text(), "world");
    }

    #[test]
    fn void_elements_do_not_swallow_following_text() {
        let items = parse("<p>a<br>b<hr>c</p>");
        let Item::Element(p) = &items[0] else {
            panic!("expected a paragraph");
        };
        assert_eq!(p.text(), "a\nbc");
    }

    #[test]
    fn unclosed_tags_are_closed_at_the_end_and_unknown_wrappers_recurse() {
        // `<p>` is not auto-closed here (the engine emits well-formed output;
        // the parser is tolerant, not a full HTML5 tree builder), so the
        // second paragraph nests. What matters is that nothing is lost.
        let items = parse("<div><p>one<p>two");
        assert_eq!(items.len(), 1);
        let Item::Element(div) = &items[0] else {
            panic!("expected div");
        };
        let text = div.text();
        assert!(text.contains("one") && text.contains("two"), "{text:?}");
    }

    #[test]
    fn attributes_keep_quoted_values_with_spaces_and_entities() {
        let items = parse("<td style=\"text-align: right;\" title='a &amp; b'>1</td>");
        let Item::Element(td) = &items[0] else {
            panic!("expected td");
        };
        assert_eq!(td.attr("style"), Some("text-align: right;"));
        assert_eq!(td.attr("title"), Some("a & b"));
    }

    #[test]
    fn entities_are_decoded_in_text_and_attributes() {
        assert_eq!(
            decode_entities("&lt;a&gt; &amp; &quot;b&quot; &#65; &#x42; &nbsp;"),
            "<a> & \"b\" A B \u{a0}"
        );
        // A bare ampersand and an unknown entity survive.
        assert_eq!(decode_entities("a & b &bogus; c"), "a & b &bogus; c");
    }

    #[test]
    fn text_collapses_whitespace_the_way_html_wraps_it() {
        assert_eq!(collapse_ws("  a \n\t b  "), "a b");
        assert_eq!(collapse_ws("   "), "");
    }

    #[test]
    fn outline_lists_headings_in_document_order_with_ids() {
        let html = "<body><header class=\"document-header\"><h1>Title</h1></header>\
                    <h2 id=\"one\">One &amp; only</h2><p>x</p><h3>Sub</h3></body>";
        let headings = outline(html);
        assert_eq!(
            headings,
            vec![
                Heading {
                    level: 1,
                    id: None,
                    text: "Title".to_string()
                },
                Heading {
                    level: 2,
                    id: Some("one".to_string()),
                    text: "One & only".to_string()
                },
                Heading {
                    level: 3,
                    id: None,
                    text: "Sub".to_string()
                },
            ]
        );
    }

    #[test]
    fn pre_text_is_preserved_verbatim_and_decoded() {
        let items = parse("<pre>  a\n  &lt;b&gt;\n</pre>");
        let Item::Element(pre) = &items[0] else {
            panic!("expected pre");
        };
        assert_eq!(raw_text(pre), "  a\n  <b>\n");
    }

    #[test]
    fn heading_levels_are_bounded() {
        for level in 1..=6u8 {
            let tag = format!("h{level}");
            assert_eq!(heading_level(&tag), Some(level));
        }
        assert_eq!(heading_level("h7"), None);
    }

    #[test]
    fn cell_alignment_follows_style_then_attribute_then_default() {
        let style = parse("<td style=\"color: red; text-align: right;\">x</td>");
        let Item::Element(cell) = &style[0] else {
            panic!("expected td");
        };
        assert_eq!(cell_align(cell), Align::Right);

        let attr = parse("<td align=\"center\">x</td>");
        let Item::Element(cell) = &attr[0] else {
            panic!("expected td");
        };
        assert_eq!(cell_align(cell), Align::Center);

        let th = parse("<th>x</th>");
        let Item::Element(cell) = &th[0] else {
            panic!("expected th");
        };
        assert_eq!(cell_align(cell), Align::Center);
    }

    #[test]
    fn comments_and_doctypes_are_skipped() {
        let items = parse("<!doctype html><!-- hidden --><p>x</p>");
        assert_eq!(items.len(), 1);
        let Item::Element(p) = &items[0] else {
            panic!("expected p");
        };
        assert_eq!(p.text(), "x");
    }
}
