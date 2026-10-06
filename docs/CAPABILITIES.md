# textrill — capability investigation

Written 2026-10-06, after the remediation plan was retired. The plan answered
"what does Perl do that we do not?". This document answers a different question,
which is the one that matters now: **what does textrill do, on its own merits,
that is worth doing?**

Every claim here was produced by running the release binary. Where a claim is
about a defect, the reproduction is given. Where it is about a limitation
inherited from the Perl original, that is said plainly, because "we did not
write this" and "this is wrong" are different statements and the previous
framing collapsed them into one.

Reproduce any of it with `make examples` and the commands inline below.

---

## 1. The premise

textrill converts **plain text** to HTML. The input is not required to carry any
markup language — no `#`, no `*`, no indentation contract, no YAML front matter.
Structure is *inferred from layout*.

That is the whole product, and it is a narrower thing than pandoc. It is also a
thing pandoc does not do at all, which is measured in §5.

The tool is 65 options (`textrill --help`), a dependency-free engine, a static
`x86_64-unknown-linux-musl` binary, and a native GUI crate.

---

## 2. Layout inference, measured

This is the capability. Each row is a real conversion, not a reading of the
source.

| Layout cue in the input | Output | Verified |
|---|---|---|
| `TITLE` + `=======` underline | `<h1><a name="section_1">` | yes |
| `TITLE` + `-------` underline | `<h2><a name="section_1_1">` | yes |
| Run of capitals, ≥ `min_caps_length` | `<strong>` | yes — 39 on `examples/homer.txt` |
| Line shorter than `short_line_length` (40) | `<br/>` | yes — 34 on `examples/homer.txt` |
| Leading `- ` | `<ul><li>` | yes |
| Line of dashes alone | `<hr/>` | yes |
| Line indented ≥ `preformat_whitespace_min` (5) | `<pre>` | yes |
| URL in body text | `<a href>` | yes |
| `term : definition` | *no* — stays a `<p>` | see §4 |

Encoding is detected, not assumed: BOM → UTF-16 evidence → UTF-8 → CP1252.
`examples/homer.txt` is UTF-8 with no BOM, and textrill reads it correctly. The
Perl original reads the same file as CP1252 and emits `&acirc;` for every
multi-byte character. That is the clearest single result in this document:

```
$ textrill --infile examples/homer.txt --outfile -
  … a work entitled "The Authoress of the Odyssey" …
$ perl scripts/txt2html --infile examples/homer.txt --outfile -
  … a work entitled &acirc;The Authoress of the Odyssey&acirc; …
```

The port is right and the reference is wrong, on the author's own expected
output.

---

## 3. The output modes, and what they are for

| Mode | What it produces |
|---|---|
| default | one XHTML 1.0 Strict document |
| `--html5` | HTML5 doctype and charset meta |
| `--extract` | body fragment only, no document wrapper |
| `--section` | each heading section wrapped in `<article class="section" id="chunk-N">` |
| `--toc` | generated `<nav class="toc">` with links to those ids |
| `--chunk` | one file per top-level section, plus pager links |
| `--stream` | constant memory: one paragraph in, one out |
| `--template` | wraps the **body** in a fragment |
| `--document_template` | wraps the **whole document** |
| `--link_only` | convert URLs and nothing else |
| `--mailmode` | mail headers and quoted replies |

`--template` and `--document_template` are a trap. The names are nearly
identical and the semantics are opposites; picking the wrong one produces a
complete HTML document nested inside a `<body>`, with two doctypes and two
titles, and no error. This is §4.4.

### Templates

Seven slots, namespaced `{{textrill:name}}`:

```
content   toc   title   head   pager   citations   glossary
```

Deliberately not a template language — no loops, conditionals, or includes —
because each slot is one pre-rendered block and there is nothing to iterate.
The namespace exists so a template can also carry another engine's tokens:

```
<script type="text/x-template"><div>{{ items.map(i => i.name) }}</div></script>
```

survives byte for byte, verified. An unknown `textrill` slot is a hard error;
every other `{{…}}` is passed through.

**This is the part that matches the niche** — see §5.

---

## 4. What is weak

All five are things the tool does badly or claims falsely about itself. None is
a regression: the first two are byte-for-byte what the Perl original does, which
is why the parity framing never surfaced them.

### 4.1 A numbered list that does not start at 1 is not a list

```
3. third item
7. seventh item
```
```
<p>3. third item<br/>7. seventh item</p>
```

No `<ol>`. The text survives, so nothing is lost, but the structure is gone —
and non-1-based numbering is common in technical and legal prose. **Inherited:
the Perl original produces the identical bytes.** No test caught it, because
every corpus case that exercises ordered lists numbers from 1.

### 4.2 An ordered list after a bullet list nests inside it

```
- bullet a
- bullet b

1. num one
2. num two
```
```html
<ul>
  <li>bullet a
  </li><li>bullet b
  <ol><li>num one</li><li>num two</li></ol>
</li></ul>
```

Two sibling lists separated by a blank line come out as one nested list. **Also
inherited, byte-for-byte.** This one costs real readability: the bullets now
render as if they belonged to the ordered list.

### 4.3 Every document claimed Perl made it — fixed

This one is resolved. It was:

```rust
// textrill/src/convert.rs
const PROG: &str = "HTML::TextToHTML";
const VERSION: &str = "3.0";
```

```html
<meta name="generator" content="HTML::TextToHTML v3.0"/>
```

Every file textrill wrote stated in its own metadata that the Perl module
produced it. That is a false provenance claim, distinct from licensing — the
GPLv3 credit to Seth Golub, Kathryn Andersen and Joao Eriberto Mota Filho in
`textrill/LICENSE` is correct and stays exactly as it is. The problem is the
`generator` field, which is a statement about what ran.

Now `<meta name="generator" content="textrill v0.1.0"/>`, with the version taken
from `CARGO_PKG_VERSION` so a bump cannot leave it stale. Cost: 18 differential
and 13 golden comparisons, resolved by `tests/corpus/normalize.py` plus
`tests/provenance.rs`. Rationale and the sabotage runs that verified both halves
are in `docs/PLAN.md` § "Item 1.1, resolved".

### 4.4 `--template` vs `--document_template`

Two near-identically named options with opposite behaviour and no diagnostic.
Worth a distinct name, or a warning when a document template lacks `<html>`.

### 4.5 The tool has no name of its own

Outside the binary path, textrill is still txt2html in every user-visible place:

- `~/.txt2htmlrc`, `./.txt2htmlrc`, `@file` groups
- `~/.txt2html.dict`, `.txt2html.dict`
- `--help` line 1: *"A reimplementation of txt2html 3.0"*

Every one of these needs a textrill identity, with the old names kept working
for people who have muscle memory — which is a compatibility decision, and the
one place where matching the old behaviour is genuinely right.

---

## 5. Against pandoc

pandoc 3.1.3 is installed here; `competition-files/` has the 3.12 source. Same
input, `examples/homer.txt`.

|  | textrill | pandoc, forced to markdown |
|---|---|---|
| bytes out | 38 888 | 38 132 |
| `<h1>`–`<h6>` | 0 | 0 |
| `<strong>` | **39** | **0** |
| `<br/>` | **34** | **0** |
| paragraphs | 64 | 62 |
| largest paragraph | 3 029 chars | **3 665 chars** |

And the finding that matters more than any number in that table:

```
$ pandoc -f plain -t html examples/homer.txt
Unknown input format plain
```

**pandoc has no plain-text reader.** Forcing it into markdown mode produces zero
structure and merges 3 665 characters — the entire title block and contents list
— into a single paragraph. Markdown's rule that soft-wrapped lines join into one
paragraph actively destroys the line structure of a document that was never
written in markdown.

textrill recovers 39 capitalised runs and 34 line breaks that pandoc does not
see at all. Neither tool finds headings, because the document contains none.

The niche: **something that takes raw text, works out what structure the layout
implies, and drops the result into HTML through templates the user controls.**
pandoc does not compete here because it requires the author to have marked the
document up first.

---

## 6. What textrill should promise

A contract, in the tool's own name. Every clause is testable, and none of them
mention Perl:

1. Input is plain text. No markup required, and no markup language assumed.
2. Structure is inferred from layout, and every inference is reported — the
   counts in `make examples` are part of the output contract.
3. Text is never silently dropped. Where inference declines, the characters stay.
4. Encoding is detected, and an explicit override always wins.
5. Malformed input produces an error and a non-zero exit, never a partial file.
6. Output is valid HTML, and `--html5` is the default going forward (§7).
7. Templates are the user's; the tool does not impose a page.
8. Generated `href`s are scheme-checked and escaped.
9. No network, no telemetry, no phone-home.
10. GPLv3, with the Perl original credited. Unchanged.

Clause 3 is the one that justifies the §4.1 fix: declining to infer is fine,
but it must be visible.

---

## 7. Open questions this raised

- Should XHTML 1.0 Strict remain the default? It is the Perl default. In 2026 the
  answer for a new tool is HTML5.
- Is `--chunk` on a document with no headings a useful feature or a silent
  no-op? Today it writes one file named after the input.
- The 13 options with no Perl equivalent (§ appendix) are not gaps. They are
  features Perl does not have, and under the new framing they are assets.
  They need documenting on their own terms rather than as deviations.

---

## Appendix — commands

```sh
make examples                                  # the smoke run over examples/
textrill --help                                # the 65-option surface
cargo test --manifest-path textrill/Cargo.toml # 306 tests
bash textrill/tests/corpus/run.sh              # 60 differential cases, 33 goldens
```
