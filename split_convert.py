#!/usr/bin/env python3
"""Split src/convert.rs into src/convert/*.rs per ROADMAP §8 module map.

Behavior-preserving: every line of code is emitted exactly once; only file
placement changes. Visibility of moved items is widened to pub(super), and
name resolution is preserved by re-exports in the module root:

  * `pub(crate) use decode::*;` etc. brings every child item back into the
    `convert` namespace (so `crate::convert::NAME` still resolves), and
  * the root re-exports names that are redefined in sibling modules, so
    `use super::*;` in a child reaches those names too.
"""
import re, os

SRC = "textrill/src/convert.rs"
OUT = "textrill/src/convert"

lines = open(SRC).read().split("\n")
n = len(lines)


def deltas(line):
    """Brace delta of one line, ignoring braces in strings/chars/comments."""
    d = 0
    i = 0
    ln = len(line)
    while i < ln:
        c = line[i]
        if c == "/" and i + 1 < ln and line[i + 1] == "/":
            break
        if c == '"':
            h = i
            while h > 0 and line[h - 1] == "#":
                h -= 1
            if h > 0 and line[h - 1] == "r":          # raw string
                k = h - 1
                if k > 0 and line[k - 1] in ("b", "c"):
                    k -= 1
                close = '"' + line[h:i]
                end = line.find(close, i + 1)
                i = end + len(close) if end != -1 else ln
                continue
            j = i + 1
            while j < ln:
                if line[j] == "\\":
                    j += 2
                    continue
                if line[j] == '"':
                    break
                j += 1
            i = j + 1
            continue
        if c == "'":
            m = re.match(r"'(\\.|[^'\\])'", line[i:])
            if m:
                i += len(m.group(0)); continue
            m2 = re.match(r"'[A-Za-z_][A-Za-z0-9_]*'", line[i:])
            if m2:
                i += len(m2.group(0)); continue
            i += 1; continue
        if c == "{":
            d += 1
        elif c == "}":
            d -= 1
        i += 1
    return d


# ---------------------------------------------------------------------------
# module routing
# ---------------------------------------------------------------------------
FREE = {
    "subtract_modes": "paragraph", "strip_leading_spaces": "paragraph",
    "is_perl_space": "lists", "is_word_char": "lists", "leading_spaces": "lists",
    "split_lines": "mod", "class_body": "mod", "not_utf8_error": "mod",
    "table_re": "tables", "is_pgsql_table": "tables", "is_border_table": "tables",
    "is_delim_table": "tables", "byte_slice": "tables", "byte_len": "tables",
    "paragraph_records": "stream", "blank_sep_at": "stream",
    "split_blank_lines": "stream",
}
DELIM_INLINE = {"non_boundary", "is_word_byte", "delim_is_word", "not_preceded_by",
                "not_preceded_by_word_or_underscore", "not_preceded_by_str",
                "no_list_or_para_tag", "char_len", "none_of"}
DECODE_FREE = {"read_any_file_with_encoding", "read_with", "decode_bytes_with",
               "detect", "bom", "sniff_utf16_or_32", "units_to_string",
               "decode_utf16_or_32", "cp1252_char", "encoding_table",
               "single_byte_char", "read_any_file", "read_file_with",
               "chop_trailing_cr", "chop_leading_cr"}

METHOD_MOD = {
    "new": "mod", "cache_pattern": "mod", "re": "mod", "re_i": "mod",
    "process_chunk": "mod", "process_para": "mod", "process_line": "mod",
    "do_file_start": "mod", "try_convert": "mod", "resolved_encoding": "mod",
    "convert": "mod", "convert_text": "mod", "convert_sources": "mod",
    "convert_stream": "mod",
    "closes_optional_tags": "tags", "get_tag": "tags", "close_tag": "tags",
    "tagline": "tags",
    "hrule": "paragraph", "shortline": "paragraph", "is_mailheader": "paragraph",
    "mailheader": "paragraph", "mailquote": "paragraph", "paragraph": "paragraph",
    "listprefix": "lists", "startlist": "lists", "endlist": "lists",
    "continuelist": "lists", "total_prefix": "lists", "liststuff": "lists",
    "table_spaces": "tables", "is_aligned_table": "tables",
    "table_columns": "tables", "make_aligned_table": "tables",
    "make_pgsql_table": "tables", "make_border_table": "tables",
    "make_delim_table": "tables", "tablestuff": "tables", "get_table_type": "tables",
    "is_preformatted": "preformat", "split_end_explicit_preformat": "preformat",
    "endpreformat": "preformat", "preformat": "preformat",
    "make_new_anchor": "headings", "anchor_mail": "headings",
    "anchor_heading": "headings", "heading_level": "headings",
    "is_heading": "headings", "heading": "headings",
    "is_custom_heading": "headings", "custom_heading": "headings",
    "unhyphenate_para": "headings", "is_ul_list_line": "headings",
    "iscaps": "inline", "caps": "inline", "do_delim": "inline",
    "delim_replace": "inline", "delim_loop": "inline", "apply_links": "inline",
    "build_head": "pages", "default_document_header": "pages",
    "collect_notes": "pages", "apply_template": "pages",
    "convert_to_parts": "pages", "finish_body": "pages", "finish_tail": "pages",
    "try_convert_chunked": "pages", "assemble_chunked": "pages",
    "chunk_basename": "pages", "chunk_filename": "pages",
    "render_page_toc": "pages", "render_pager": "pages",
}
IMPL_MOD = {"impl Wide": "decode", "impl Resolved": "decode", "impl Options": "pages"}

assign = [None] * n
bind = {}   # name -> module, for the sibling re-export list


def route_fn(name):
    if name in DECODE_FREE:
        return "decode"
    if name in FREE:
        return FREE[name]
    if name in DELIM_INLINE:
        return "inline"
    return None


# ---------------------------------------------------------------------------
# walk top-level items (depth 0)
# ---------------------------------------------------------------------------
i = 0
while i < n:
    raw = lines[i]
    st = raw.strip()
    if st == "" or st.startswith("//") or (st.startswith("#[") and "cfg(test)" not in st):
        i += 1
        continue

    if st.startswith("#[cfg(test)]") and re.match(r"^(pub )?mod ", lines[i + 1].strip()):
        mod = "tests"
        # consume the whole test module to EOF
        d = 0; opened = False; k = i
        while k < n:
            d += deltas(lines[k])
            if "{" in lines[k]:
                opened = True
            assign[k] = mod
            if opened and d == 0:
                break
            k += 1
        i = k + 1
        continue

    # attached leading region
    lead = i
    while lead > 0 and (lines[lead - 1].strip().startswith("//")
                        or (lines[lead - 1].strip().startswith("#[")
                            and "cfg(test)" not in lines[lead - 1])):
        lead -= 1
    # decide the module
    om = re.match(r"^(pub(?:\([^)]*\))? )?(const )?fn ([A-Za-z0-9_]+)", st)
    mod = None
    name = None
    if om:
        name = om.group(3)
        mod = route_fn(name)
        if mod is None:
            if "impl" in st:
                mod = "mod"
            else:
                mod = "mod"
    elif st.startswith("impl Converter"):
        mod = "mod"
    elif st.startswith("#[allow") and "fn" in lines[i + 1] if i + 1 < n else False:
        pass
    if mod is None:
        for key, m in IMPL_MOD.items():
            if st.startswith(key):
                mod = m
                break
    if mod is None:
        if st.startswith("struct ParagraphReader") or st.startswith("impl<R: BufRead> ParagraphReader"):
            mod = "stream"; bind["ParagraphReader"] = mod
        elif st.startswith("trait ReplAll") or st.startswith("impl ReplAll for"):
            mod = "inline"; bind["ReplAll"] = mod
        elif st.startswith("impl ") or st.startswith("trait "):
            mod = "mod"
        elif st.startswith("static "):
            mod = "decode"
        elif st.startswith("const ") or st.startswith("pub const "):
            mod = "tags"
        else:
            mod = "mod"

    # free fn item bind
    if name and mod not in (None, "mod"):
        bind[name] = mod

    # find body end
    one_liner = deltas(raw) == 0 and "{" in raw and "}" in raw
    if one_liner:
        for x in range(lead, i + 1):
            assign[x] = mod
        i += 1
        continue
    d = 0; opened = False; k = i
    while k < n:
        d += deltas(lines[k])
        if "{" in lines[k]:
            opened = True
        assign[k] = mod
        if opened and d == 0:
            break
        k += 1
    # leading comments/attrs join this item
    for x in range(lead, i):
        assign[x] = mod
    i = k + 1


print("walked; unassigned lines:", sum(1 for a in assign if a is None))
for idx in range(n):
    if assign[idx] is None and lines[idx].strip():
        print("  UNASSIGNED", idx + 1, lines[idx][:70])
