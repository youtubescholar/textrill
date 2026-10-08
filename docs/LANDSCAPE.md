# Landscape — where textrill sits

Written 2026-10-06. Research material is in `research/pandoc/`: the pandoc
3.12 source (full release tarball with docs and tests) and a stripped build
tarball of the same version. pandoc 3.1.3 is installed on this machine and is
what the comparisons below were run against.

The previous landscape work is `legacy-archive/TOOL-SURVEY.md`. Its feature
matrix is false now, and it is archived, but two of its conclusions still hold
and are cited below.

---

## 1. The category is small and mostly unmaintained

The tools that take **unmarked** text and infer structure from layout are a
genuinely small group. From the survey, re-checked against the current sources:

| Tool | Status | Note |
|---|---|---|
| `HTML::TextToHTML` 3.0 (Perl) | the origin | the archived reference; still shipped |
| **textrill** | active | this project; a native GUI and a static binary |
| txt2tags | active | closest sibling; requires `====`/`----` underlining |
| dmTxt2Html | effectively dead | Python/CGI, historically interesting |
| pandoc | very active | *requires* markup — see below |

Everything else that converts "text to HTML" requires the author to have written
markup first: pandoc, Asciidoctor, docutils, Markdown tools. Those are a
different product with a different goal, and they are not competitors.

The honest observation: **we may be the only project seriously studying
layout inference for plain text.** txt2tags does a little of it and requires
explicit underlining; the Perl tool does it and is essentially unmaintained.
That is a reason to be careful — there is no community to check assumptions
against — and not a reason to avoid the work.

## 2. pandoc cannot do this, and will not try

```
$ pandoc -f plain -t html examples/homer.txt
Unknown input format plain
```

pandoc 3.1.3 has no plain-text reader. Every reader it ships expects a markup
language. Its own documentation on the predecessor makes the reasoning explicit
— from `legacy-archive/TOOL-SURVEY.md`, quoting txt2html.sourceforge.net:

> txt2html is not a program for automatically generating a table-of-contents
> from a document. If you want to do that, then use txt2html to generate a HTML
> file, and then use htmltoc or hypertoc on the HTML file.

The same posture, stated plainly, applies to inference: pandoc converts
*documents*. It does not read *prose*.

### What happens when you force it

`pandoc -f markdown` on the Odyssey produces:

|  | textrill | pandoc (markdown) |
|---|---|---|
| `<strong>` | 39 | 0 |
| `<br>` | 34 | 0 |
| largest paragraph | 3 030 chars | 3 665 chars |

pandoc merges the title block, the contents list and the first chapter headings
into one 3 665-character paragraph, because markdown's soft-wrap joining rule
assumes the author never meant line breaks. For a document that was never
markdown, that rule destroys information.

## 3. The niche

> Something that takes raw text, works out what structure the layout implies,
> and drops the result into HTML through templates the user controls.

Three capabilities, and the competition on each:

| capability | textrill | nearest thing | gap |
|---|---|---|---|
| read unmarked text | yes, 68 options of layout inference | nothing active | **clear** |
| infer structure | setext, capitals, short lines, lists, rules, pre, tables, mail | txt2tags, needs underlining | real |
| user templates | 7 slots, `--var` parameters (4.1), 5 shipped templates incl. `--template_library` (4.2) | pandoc: 20+ writers, `-V` variables, `$if`, `$for`, partials | **medium** |

The third row is where textrill is behind, and it is the row the user named.
Measured directly:

```
$ pandoc --template=ptpl.html -V author="S. Butler" -V title="The Odyssey" p.md
<p class="by">By S. Butler</p>
```

pandoc injects arbitrary user variables into the template and can branch on
them. The founding measurement of textrill was seven fixed slots and no way to
pass a value in:

```
{{textrill:content}} {{textrill:toc}} {{textrill:title}} {{textrill:head}}
{{textrill:pager}} {{textrill:citations}} {{textrill:glossary}}
```

So the honest position is: textrill is **ahead** on the thing that is hard and
unusual (reading prose). On configurable output it started **behind**, and 4.1
(`--var name=value`, filled into `{{textrill:var:name}}`) and 4.2 (the five
shipped templates) closed the shipping gap — a user's first impression of any
converter is its templates, and there are now actual ones to start from. What
remains is the row's non-fixed part, the conditional (see §4 item 3).

## 4. What this implies

Not "add pandoc's whole template language." The namespaced 7-slot design is
deliberate and defensible: it passes other engines' tokens through untouched,
which is what lets a template carry Vue or Jinja syntax for a later pass.

What is missing is narrower and more valuable:

1. ~~**User variables.**~~ **Shipped (S3).** `--var name=value`, surfaced as
   `{{textrill:var:name}}`. That alone turns the template from a fixed frame
   into something a person can parameterise, at a fraction of the complexity
   of a template language.
2. ~~**A shipped template library.**~~ **Shipped (S4).** Not a language —
   actual templates: article, book, manpage, slide, bare. This is the
   "templates people could use" the niche is named for, and it is content,
   not machinery.
3. **Conditional blocks** — `{{textrill:if:toc}}…{{textrill:end}}` — so one
   template serves documents with and without a TOC.

## 5. Sources

- `research/pandoc/pandoc-3.12-full.tar.gz` — full release tarball, 3 091 files,
  including `doc/` and the test suite. `research/pandoc/pandoc-3.12-stripped.tar.gz`
  is a stripped build tarball of the same version: 262 files fewer, no docs, no
  `cabal.project`, no CI. Same version, different packaging; the full one is
  what to read.
- `/usr/bin/pandoc`, version 3.1.3 — every pandoc command above.
- `legacy-archive/TOOL-SURVEY.md` — the category survey, and the upstream TOC
  disclaimer quoted in §2. Matrix archived as false; conclusions re-checked.
- `legacy-archive/REMEDIATION-PLAN.md` § "Compatibility policy" — the
  three-tier rule that first recorded that the Perl reference is wrong on
  genuine UTF-8 input.
