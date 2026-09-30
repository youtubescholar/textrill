# txt2html — tool survey and feature-gap analysis

Status: draft, 2026-09-29. Companion to `/home/vicpu/build/REMEDIATION-PLAN.md`.
Question asked: are there features that other text-to-HTML tools have and this
port does not, and are any of them worth implementing?

Short answer: yes, but very few. The port is already a faithful and fairly
complete implementation of its own niche. The survey turned up **one high-value,
low-cost gap** (table of contents), **two medium-value gaps**, and **one genuine
regression against the shipped upstream script** that the plan missed
(rc/config files, P11 below).

## 1. Method, and why "competitors" is a misleading word

There is no real competitive set. Almost every tool people would call a
"plain text to HTML converter" requires the author to have *written markup into
the source file* first. That is a different product with a different goal:
`txt2html` exists to recover structure from layout in text that was never
marked up, and to do so with byte-level predictability.

So the tools fall into four groups, and only groups 1 and 2 are useful
comparators.

| Group | Tools | Useful as a comparator? |
|---|---|---|
| 1. Layout-inference, no markup | `HTML::TextToHTML` 3.0 (the reference), txt2tags, dmTxt2Html | **Yes — direct** |
| 2. Markup-required processors | pandoc (Markdown), Asciidoctor, docutils/rst2html | Partly — good for feature *breadth*, bad for design |
| 3. Category-different | ansi2html, man2html/groffer, Vim `TOhtml`, Emacs `htmlize` | No — different input, but see line anchors |
| 4. Not real tools | Syncfusion / Elementor "online TXT to HTML" pages, blog posts | No |

The group-4 results are worth calling out so nobody re-runs these searches:
those pages are WYSIWYG editors and marketing content that do not infer
structure at all. The Elementor page concedes it emits "basic structural HTML"
and cannot reliably autolink URLs. Nothing there is a reference point.

### Group 1 detail

**`HTML::TextToHTML` 3.0** — the reference implementation. See
https://txt2html.sourceforge.net/ and
https://txt2html.sourceforge.net/TextToHTML.html. The single most useful
sentence found, and the reason TOC is ranked first in section 4:

> txt2html is not a program for automatically generating a table-of-contents
> from a document. If you want to do that, then use txt2html to generate a HTML
> file, and then use htmltoc or hypertoc on the HTML file.

Two things follow. First, TOC is a *known and deliberate* omission, not an
oversight. Second, upstream names its intended workaround, which is a
post-processing pass over the generated HTML — and that is a much weaker design
than generating the TOC during conversion, because it has to re-parse the output
it just produced.

**txt2tags** — https://txt2tags.org/doc/english/manpage and
https://txt2tags.org/userguide/commandlineoptions. The closest thing to a
sibling project: pure text, infers some structure, deliberately small syntax.
It is also the only tool found that ships a *complete* option surface for the
same job. Relevant options:

- `--toc`, `--toc-level=N`, `--toc-only`, and a `%%toc` macro for placement
- `-n`, `--enum-title` — automatic heading numbering
- `--style=FILE`, `--css-sugar`, `--css-inside` — stylesheet control
- `--encoding=ENC` with `--no-encoding`
- `%!options`, `%!target`, `%!style`, `%!includeconf`, `%!include`, `%!csv`
  — in-document configuration, includes, CSV-as-table
- `--no-OPTION` to disable any boolean
- `-t/--target` across 18 output formats

**dmTxt2Html** (Marc-Andre Lemburg, gnosis.cx) — Python/CGI, effectively dead
but historically interesting. Has "proxy modes" that inject navigation and
paginate, and delegates syntax highlighting of preformatted blocks to
`py2html`/`PyFontify`. Notable as prior art for a proxy/wrapper mode; not
maintained.

I could not verify the existence of the C++ `Ensothom/txt2html` that turned up in
one search. Treat it as unconfirmed.

### Group 2 detail

Used only to establish what a mature document converter considers table stakes.
No layout inference in any of them, so none of their heuristics are transferable.

- **pandoc** — `--toc`, `--standalone`, `--template=FILE`, `--self-contained`
  (inlines CSS/JS/images as `data:` URIs), `--sanitize-html`,
  `--email-obfuscation=none|javascript|references`, `-S/--smart` typography,
  `--html-q-tags`. Manual: https://pandoc.org/MANUAL.html
- **Asciidoctor** — `toc`, `toclevels`, `toc-title`, `toc-class`, `sectnums`,
  `sectnumlevels`, `footnotes`/`nofootnotes`, `xrefstyle`. Attributes reference:
  https://docs.asciidoctor.org/asciidoc/latest/attributes/document-attributes-ref/
- **docutils / rst2html** — automatic section numbering (`sectnum` directive),
  numbered and auto-numbered footnotes, definition lists, field lists,
  `--link-stylesheet`. Currently defaults to `html4css1` and is documented to
  switch to `html5` in Docutils 2.0 — a good precedent for making the doctype
  a versioned opt-in rather than a silent change.
  https://docutils.sourceforge.io/docs/user/html.html

## 2. Feature matrix

Peer = group 1. Breadth = group 2, which is not a direct peer but shows what
mature converters ship. `yes`/`no` for the port column was read from the source,
not inferred.

| Feature | txt2html 3.0 | txt2html-rs | txt2tags | pandoc | asciidoctor | docutils |
|---|---|---|---|---|---|---|
| Infers structure from layout | yes | yes | partly | no | no | no |
| Underline-style headings | yes | yes | no | no | no | yes |
| Setext/custom-regexp headings | yes | yes | no | no | no | no |
| Bulleted / ordered lists | yes | yes | yes | yes | yes | yes |
| Definition lists | engine only | engine only | no | yes | yes | yes |
| Tables | yes (2 types) | yes (2 types) | yes (+CSV) | yes | yes | yes |
| Preformat detection | yes | yes | yes | no | yes | yes |
| Form feed / page break | yes | yes | no | no | no | no |
| Mail mode | yes | yes | no | no | no | no |
| Link dictionaries | yes | yes | no | no | no | no |
| Bold/italic/underline chars | yes | yes | yes | yes | yes | yes |
| Unhyphenation | yes | yes | no | no | no | no |
| Cap-lines as headings | yes | yes | no | no | no | no |
| Mosaix/legacy heading set | yes | yes | no | no | no | no |
| **Anchors on headings** | yes | yes | yes | yes | yes | yes |
| **Table of contents** | **no (by design)** | **no** | yes | yes | yes | yes |
| **Heading numbering** | no | no | yes | no | yes | yes |
| **Footnotes** | no | no | no | yes | yes | yes |
| HTML4/XHTML1 output | yes | yes | yes | yes | yes | yes |
| **HTML5 output** | no | **no** | yes | yes | yes | planned 2.0 |
| `charset` in output | no | **no** | yes | yes | yes | yes |
| Explicit `--encoding` | no (only `--utf8`) | `--utf8` only | yes | yes | yes | yes |
| External stylesheet link | yes | yes | yes | yes | yes | yes |
| **Built-in / inline CSS** | no | **no** | yes | yes | yes | yes |
| **Per-line anchors / line numbers** | no | **no** | no | no | no | no |
| **Config / rc file** | yes | **no** | yes | defaults file | attributes | config.py |
| `no-` boolean negation | yes | yes | yes | n/a | n/a | n/a |
| Option abbreviation | yes | no | yes | yes | n/a | n/a |
| Multiple output targets | no | no | 18 | many | many | 1 |

Two rows deserve emphasis because they are *not* gaps, and I checked them
specifically because they looked like they might be:

- **Definition lists** are implemented in the engine (`convert.rs:47,563,595,648`,
  `DL = 3`) and match the reference exactly (`TextToHTML.pm:684,2661,2719,2777`).
  Neither version exposes a public option for them. Faithful port, not a gap.
- **Form feed / page break** is handled in both (`convert.rs:275-278` splitting
  on `0x0C`; `TextToHTML.pm:2236-2241` doing the same). Faithful port.

## 3. Findings that came out of the matrix

### P11. Config/rc file support is a regression, not a gap — new, belongs in Phase 4

This is the one finding that is about *losing* functionality rather than failing
to gain it, and `REMEDIATION-PLAN.md` P10 missed it.

The shipped upstream script loads option files. `ref/txt2html-3.0/scripts/txt2html:838-845`:

```perl
if (eval("require Getopt::ArgvFile")) {
    my $bn = basename($0, '');
    my $rc_name = ".${bn}rc";
    Getopt::ArgvFile::argvFile(startupFilename=>$rc_name, home=>1, current=>1);
}
```

and the POD documents it as active behaviour, at `scripts/txt2html:509` and
`759-771`: `~/.txt2htmlrc` and `./.txt2htmlrc` are read, and `@filename` groups
further options.

Verified against the built binary:

```
$ txt2html @/tmp/opencode/rc.txt
Could not open @/tmp/opencode/rc.txt
```

The `@file` token is treated as an input filename. `grep` over `cli.rs` finds no
rc-file, defaults-file, or `@file` handling at all.

Recommendation: add it. `@file` and `TXT2HTML_RC`/`~/.txt2htmlrc` are cheap,
they are what existing users' muscle memory expects from a tool they have used
for twenty years, and silently treating `@x` as a filename is a confusing failure
rather than an honest "unknown option". Precedence should be
`@file` < `~/.txt2htmlrc` < `./.txt2htmlrc` < command line, matching upstream.
Add a test that a bad option inside an rc file reports the *file and line*, which
is the main ergonomic win of `Getopt::ArgvFile`.

### Option abbreviation: recommend NOT adding it

Upstream allows any unique prefix (`--titl` for `--title`) via
`Getopt::Long`'s auto-abbreviation, documented at `scripts/txt2html:59`. The port
rejects it:

```
$ txt2html --titl X
txt2html: Unknown option `titl`
```

This is a deliberate divergence and the right call. Unique-prefix matching breaks
silently the day an option is added or renamed — `--m` would change meaning with
no code change on the user's side. Make it explicit in the README so it reads as
a choice rather than an omission. Related: the port's `no-` / `no_` negation
(`cli.rs:138,249`) *is* implemented, so that part of upstream is preserved.

## 4. Ranked feature recommendations

### Worth implementing

**1. Table of contents. High value, low cost. (Plan P5.3 — now better justified.)**

Every group-1 and group-2 tool has it except the reference, which disclaims it
explicitly and points at a post-processor. The port already has everything
needed: `make_anchors` emits heading anchors, and the heading pass is the natural
place to collect them. Adding `--toc` with a `--toc-level=N` bound during
conversion is strictly better than the htmltoc/hypertoc route because the
heading list is in hand before the output is assembled.

Requirements for it to be safe:
- Must be **opt-in and default off**, or every golden changes.
- Anchor ids must match `make_anchors` exactly, including the numbering suffix
  behaviour for duplicate headings, or generated TOCs will contain dead links.
- Emit the TOC after the title, which is where all four other tools put it.
- A TOC makes the output non-idempotent in a new way: converting output that
  already contains a TOC must not nest one. Decide and test that case.

**2. Heading numbering. Medium value, low cost.**

txt2tags `-n`, asciidoctor `sectnums`, docutils `sectnum`. Cheap once the heading
pass exists, and it composes with the TOC above (numbered TOC entries). Same
constraint: opt-in, default off, goldens untouched.

**3. Config/rc files. High value for existing users, low cost. (P11.)**

See above. This is the only item here that is a compatibility requirement rather
than an enhancement.

**4. Built-in stylesheet. Medium value, low risk if opt-in.**

The port has `style_url`, which emits a `<link>`, but nothing for users who
cannot host a CSS file. txt2tags `--style` plus `--css-inside` and docutils'
`--link-stylesheet` cover both halves. A shipped default stylesheet is a
separate opt-in flag; do not make it implicit, because it would change output for
everyone and is exactly the kind of thing that breaks byte-identity.

### Deliberately out of scope

- **Footnotes/endnotes.** docutils, pandoc and asciidoctor all have them, and they
  are the most-requested missing feature in that class of tool. Rejected here
  because footnotes require unambiguous inline markers, and the one thing this
  tool must never do is guess. A `[^1]` in ordinary prose would become a link.
- **Multi-target output.** txt2tags ships 18 targets, pandoc more. This would
  turn a faithful port into a different product. The single-target HTML contract
  is the thing being preserved.
- **Syntax highlighting in preformatted blocks.** Precedent is dmTxt2Html and Vim
  `TOhtml`. It means a language-detection dependency, per-language definitions,
  and a large behavioural surface inside blocks the port currently treats as
  literal text. Poor value against the risk to byte-identity.
- **Built-in tables of contents via a post-processor.** This is what upstream
  tells users to do. Doing it in-conversion is strictly better, which is
  recommendation 1; the post-processor path is not worth building.

### Borderline, listed for the record

- **Per-line anchors / line numbers.** `ansi2html --markup-lines` and Vim
  `TOhtml` both do this; no text-layout tool does. It is genuinely useful for
  large documents and it would also make the "convert only the visible region"
  idea from plan P6 implementable. It is also a visible change to every
  `<pre>` block, so it is opt-in at best. Not recommended until P6 lands and
  there is a real user need.
- **HTML5 output mode.** Already in plan P5.1; the survey only confirms it. The
  docutils 1.x → 2.0 `html4css1` → `html5` transition is a good model: ship it as
  an opt-in mode, keep HTML4 as the default, and say in the changelog that the
  default will move at some named future version.

## 5. Corrections this survey forces on the plan

1. Add **P11** (rc files) to Phase 4, alongside the other missing script-level
   options in P10.
2. Add a short README line stating that option abbreviation is intentionally not
   supported, so it is not read as an omission.
3. Strengthen the P5.3 (TOC) rationale: the reference disclaims TOC and names
   `htmltoc`/`hypertoc` as the intended workaround, so in-conversion generation
   is a clear improvement rather than a speculative addition.
4. `REMEDIATION-PLAN.md` P10 lists `--debug`, `--verbose`, `--dict_debug` and
   `--manpage` as missing but does not mention `Getopt::ArgvFile`, which is
   present and active in the same file. That is why P11 was missed; the script
   should be diffed against `cli.rs` option-by-option rather than by eye.

## Sources

Group 1:
- https://txt2html.sourceforge.net/ (incl. the TOC disclaimer)
- https://txt2html.sourceforge.net/TextToHTML.html
- https://txt2tags.org/doc/english/manpage
- https://txt2tags.org/manpage.html
- https://txt2tags.org/userguide/commandlineoptions
- https://gnosis.cx/download/cgi-bin/txt2html.txt

Group 2:
- https://pandoc.org/MANUAL.html
- https://docs.asciidoctor.org/asciidoc/latest/toc/toc-ref/
- https://docs.asciidoctor.org/asciidoc/latest/attributes/document-attributes-ref/
- https://docutils.sourceforge.io/docs/user/html.html
- https://www.docutils.org/docs/user/tools.html
- https://www.docutils.org/docs/ref/rst/directives.html

Local references, read directly:
- `ref/txt2html-3.0/scripts/txt2html` (rc files, abbreviation, option list)
- `ref/txt2html-3.0/lib/HTML/TextToHTML.pm` (`$DL`, form-feed handling)
- `ref/ansi2html-main`, `ref/txt2html-master` (category check only)

All claims about the Rust port in this document were verified against the built
binary or by reading the named source lines. Nothing here is inferred from the
documentation alone.
