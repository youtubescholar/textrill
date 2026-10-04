# textrill

Convert plain text to HTML.

`textrill` is a Rust reimplementation of
[`HTML::TextToHTML`](https://metacpan.org/pod/HTML::TextToHTML) 3.0 and its
`txt2html` script, originally written by Seth Golub. It reads plain text and
writes HTML, inferring structure from the conventions text authors already use:
indentation for lists, blank lines for paragraphs, underlines and setext rules
for headings, trailing punctuation for link labels, and so on.

This is a fork. It is not endorsed by, and carries no affiliation with, the
upstream `txt2html` project or its authors. See [Licence](#licence).

## Status

Early. `0.1.0` is the first release of the fork, and it is a rename plus a set
of correctness fixes on top of the existing port — see
[Relationship to upstream](#relationship-to-upstream).

What is **done**:

- The conversion engine is byte-verified against the Perl original across a
  differential corpus of 59 cases and 33 upstream golden files.
- 222 Rust tests, a fuzzer, and a 74-test native GUI suite.
- The CLI builds as a single static `x86_64-unknown-linux-musl` binary, and CI
  runs the differential corpus against that binary, so it runs on Alpine and
  other glibc-less distros with the same output as the reference.
- Encoding detection was reworked: BOM → UTF-16 evidence → UTF-8 → CP1252, with
  explicit overrides for the encodings that cannot be detected.

What is **not** done yet:

- No Flatpak, no distro packaging yet.

The GUI is the native `egui`/`eframe` front end in `textrill-gui-rs/`; the
earlier Python/PySide6 front end is retired and archived under `legacy-archive/`.
Performance is no longer a gap: the P6 link-pass fix puts the port ahead of the
Perl original on the link-dense benchmark (see `../REMEDIATION-PLAN.md`, P6).


## Option files

Options can come from the command line, from `@file` groups, or from an rc file.
Precedence, lowest first:

1. `@file` groups, in the order they appear on the command line
2. `~/.txt2htmlrc`
3. `./.txt2htmlrc`
4. the command line

One option per line, using the same spellings as the command line:

```sh
# .txt2htmlrc
--extract
--title "Release notes"
--bold_delimiter "@"
--custom_heading_regexp "^ *--[\\w\\s]+-- *$"
```

`#` starts a comment unless it is inside quotes, and a `--` line ends option
processing, so everything after it is an input filename. A bad option is
reported as `file:line: message`, naming both the file and the line.

```sh
textrill @opts.txt notes.txt
```

Both rc files are optional; a missing one is not an error. An `@file` that does
not exist *is* an error, since the name was given explicitly.

## Install

### The CLI

```sh
cargo install --git https://github.com/<you>/textrill
```

Or build from a checkout:

```sh
cd textrill
cargo build --release
./target/release/textrill --help
```

### The GUI

The GUI is a native `egui`/`eframe` binary that links the engine directly — no
Python runtime, no PySide6:

```sh
cd textrill-gui-rs
cargo run --release
```

Packaging as a Flatpak is planned; there is no wheel or installed entry point
yet.

## Usage

```sh
textrill [ options ] [ file ... ]
```

With no arguments it reads standard input and writes standard output. Options
use the GNU long form and can be abbreviated to any unambiguous prefix:

```sh
textrill --bold_delimiter='#' --italic_delimiter='*' --make_links README.md > README.html
```

There are **62 options** with **116 accepted spellings** including short
aliases; `textrill --help` lists them all with their defaults.

The options are the Perl originals, unchanged, so that documents converted by
either tool are directly comparable. A few are worth calling out:

| Option | Default | Effect |
| --- | --- | --- |
| `--bold_delimiter` | `#` | Delimiter that produces `<strong>` |
| `--italic_delimiter` | `*` | Delimiter that produces `<em>` |
| `--underline_delimiter` | `_` | Delimiter that produces `<u>` |
| `--make_tables` | off | Detect tab-separated or column-aligned text as tables |
| `--make_links` | off | Turn labelled URLs in the text into links |
| `--default_link_dict` | none | Load a link dictionary, as the original does |
| `--extract` | off | Output only the body, without the surrounding document |
| `--meta_charset` | off | Emit `<meta charset="utf-8">` |
| `--html5` | off | Emit HTML5: `<!DOCTYPE html>`, no namespace, charset meta |
| `--section` | off | Wrap each heading section in `<article id="chunk-N">` |
| `--toc` | off | Prepend a generated table of contents (implies `--section`) |
| `--chunk` | off | Write one HTML file per top-level section |
| `--number_headings` | off | Prefix headings with hierarchical numbers (`1`, `1.1`, …) |
| `--stream` | off | Read and write a paragraph at a time (UTF-8 input only) |
| `--template` | none | Wrap the body in a template file (slots, see below) |
| `--document_template` | none | Use a whole-document template file |
| `--encoding` | `auto` | How to decode the input (see below) |

The delimiter names are inherited from the Perl original and are not intuitive:
`--bold_delimiter` emits `<strong>`, not `<b>`, and `--italic_delimiter` emits
`<em>`. They were left as they are so that documents convert identically under
either tool. Note that the default delimiters are off unless enabled — with the
defaults above, only `#...#` and `*...*` are converted; a bare `_word_` is left
alone unless `--underline_delimiter` is matched by surrounding text.

### HTML5 output

By default the prolog matches the Perl original: an XHTML 1.0 Strict doctype
with an `xmlns` on `<html>`, and no encoding declaration. `--html5` switches the
prolog to HTML5 — the short `<!DOCTYPE html>`, an `<html>` element with no
namespace, and a forced `<meta charset="utf-8">` — and leaves the body markup
unchanged. It is **off by default** so output stays byte-identical to the
reference; tag case is still governed by the usual `--xhtml`/`--lower_case_tags`
settings, so a bare `--html5` is already lower-case.

### Sectioning, TOC and multi-file output

These three options are **new in textrill**, not part of the Perl original, and
all are **off by default** so the reference output never moves.

`--section` wraps each heading-delimited run of the body in
`<article class="section" id="chunk-N">`. The ids are assigned sequentially in
the order the sections appear (`chunk-1`, `chunk-2`, …), so they can never
collide and a link to one can never go stale. `--toc` additionally prepends a
`<nav class="toc" id="toc">` whose links target those ids; it implies
`--section`.

`--chunk` writes one file per heading at the shallowest level present — for a
document with `h1` chapters, one file per chapter, with any `h2` subsections
kept on their parent's page — instead of a single document. Files are named
`<outfile stem>-chunk-NN.html` next to `--outfile`, which is required. Each page
carries the same TOC (with cross-file links when `--toc` is set) and prev/next
pager links. `--chunk` cannot be combined with `--extract`, `--instring`, or
output to standard output.

`--number_headings` prefixes each heading with its hierarchical position
(`1`, `1.1`, `1.1.1`, …) before any sectioning runs, so the numbers also appear
in the `--toc` labels and in each `--chunk` page. It is independent of
`--section`/`--toc` and, like them, **off by default**.

The markup here is HTML5 (`<article>`, `<nav>`), so these options are intended
to be used together with `--html5`.

### Streaming large inputs

By default the engine reads each input whole, converts it, and holds the
document and its markup in memory at once. `--stream` converts one paragraph at
a time instead, so a very large file can be piped through without ever being
held whole, and output begins before the input has been fully read:

```sh
textrill --stream huge-book.txt > huge-book.html
```

The record boundary is the same blank-line paragraph mode the buffered path
uses, and every piece of cross-paragraph state (the open list, the preformatted
block, the section header, link rules) is carried in the converter, so for any
input the two paths accept the output is **byte-for-byte identical**. That is
the property the tests pin.

The restriction is encoding. A reader cannot run the full detection order that
`Auto` uses — that needs every byte — so `--stream` reads strictly UTF-8. A
UTF-16/UTF-32 byte-order mark or NUL structure is refused before anything is
written, and a byte that is not valid UTF-8 is an error rather than being
replaced, because the buffered `Auto` path would have decoded such a file
differently and quietly emitting something else would be the one failure mode
worth avoiding. For non-UTF-8 input, use the buffered path. `--stream` is also
refused with `--instring` and with the whole-body passes `--number_headings`,
`--section`, `--toc` and `--chunk`, all of which need the assembled body.

### Templates

A template lets you supply the page skeleton instead of accepting the one the
engine writes. The model is simple: **the engine renders named blocks, the
template decides where they go.** It is not a template language — there are no
loops, conditionals, expressions or includes, and no JavaScript.

There are two levels:

- `--template FILE` inserts a **fragment inside `<body>`**. The engine still
  emits the doctype, `<head>` and the `<body>` tags, so you only write the part
  you want to change.
- `--document_template FILE` takes over the **whole page**. The engine emits
  none of its own prolog, so you write the doctype, head and body yourself.

With neither option the output is byte-for-byte what it always was.

Slots are written `{{textrill:name}}`:

| Slot | Contents |
| --- | --- |
| `{{textrill:content}}` | The converted body (required) |
| `{{textrill:toc}}` | The generated TOC nav, empty unless `--toc` is on |
| `{{textrill:title}}` | The escaped document title |
| `{{textrill:head}}` | The engine's `<head>` contents (title, metas, stylesheet) |
| `{{textrill:pager}}` | Prev/next links (reserved for `--chunk`) |

A known slot is replaced; an **unknown** `{{textrill:...}}` slot is an error;
and every other `{{...}}` is passed through untouched, so a template can also
carry another engine's tokens:

```html
<main class="page">
  <aside class="sidebar">{{textrill:toc}}</aside>
  <article>{{textrill:content}}</article>
</main>
```

The `textrill` namespace exists exactly for that coexistence — Mustache,
Handlebars, Jinja2 and friends all claim `{{ }}`, so this pass only ever looks
at its own prefix. A template must contain `{{textrill:content}}`; the command
line reports a missing or malformed one before writing anything.

Because a template is an `Options` value, it can be set once in
`./.txt2htmlrc`, so a project commits its template and points at it there.

### Encodings

Input encoding is detected in a specific order, and it is worth understanding
which steps are evidence and which are guesses:

1. **Byte-order mark.** Decisive.
2. **UTF-16 NUL pattern.** Decisive for most real files.
3. **UTF-8 validity.** Decisive if the text is genuinely UTF-8.
4. **CP1252.** This is a guess, and it is wrong for Cyrillic, Greek and
   Turkish text.

For non-Western text, name the encoding explicitly:

```sh
textrill --encoding cp1251 russian.txt
textrill --encoding koi8-r old-cyrillic.txt
textrill --encoding cp1253 greek.txt
textrill --encoding utf-16le bomless.txt
```

Accepted values: `auto`, `utf-8`, `latin-1` (an alias for `iso-8859-1`),
`cp1251`, `cp1252`, `cp1253`, `koi8-r`, `utf-16le`, `utf-16be`, `utf-32le`,
`utf-32be`.

Output is always UTF-8.

Two limits worth stating, because they are not bugs but consequences of the
approach:

- A BOM-less UTF-16 file whose text contains almost no ASCII characters cannot
  be detected by NUL patterns. `"Привет"` in UTF-16LE looks like short words
  with high bytes interspersed. Pass `--encoding utf-16le`.
- BOM-less UTF-32 is never guessed. Always pass `--encoding utf-32le` or
  `utf-32be`.

The upstream Perl has no encoding handling at all, so this is an addition. For
Western European input the two agree, which is why the corpus treats them as
matching by default.

## Using it as a library

```rust
let mut conv = Converter::new(Options::default());
let html = conv.convert();
```

`convert()` swallows the unreadable-input error to match the reference's
behaviour. Use `try_convert()` if you want to know which files failed:

```rust
match conv.try_convert() {
    Ok(html) => println!("{html}"),
    Err(e) => eprintln!("could not read: {:?}", e.unreadable),
}
```

## Relationship to upstream

`txt2html` 3.0 is a Perl module plus a wrapper script by Seth Golub, later
maintained by Kathryn Andersen and Joao Eriberto Mota Filho. This project is a
Rust reimplementation of it, kept byte-compatible with the original wherever
that is achievable.

The compatibility work is the point. A plain-text-to-HTML converter is only
useful if it agrees with the tool people already have, so behaviour was pinned
by differential testing against the actual Perl implementation rather than
guessed from documentation:

- `tests/corpus/` runs 59 documents through both implementations and requires
  byte-identical output.
- The 33 upstream golden files are compared byte for byte.
- The fuzzer hunts for divergences in Unicode handling, delimiter recovery and
  encoding detection.

Where this fork deliberately differs from upstream, the divergence is declared
and explained in `tests/corpus/README.md` rather than left to be discovered.
Known differences:

- **Input encoding detection** does not exist upstream. See `--encoding` above.
- User-supplied regular expressions are validated before use. An invalid
  pattern produces a diagnostic naming the option, the pattern and the parser's
  complaint, instead of a panic. This is a strict improvement; the only
  behavioural change is that a bad pattern no longer crashes the process.
- A handful of pathological inputs that upstream handles by silently corrupting
  output are handled correctly here. These are marked in the corpus README.

The full audit trail of what was found and fixed is in
`../REMEDIATION-PLAN.md`.

## Licence

GPL-3.0-or-later.

Upstream `txt2html` licenses itself "under the same terms as Perl itself", which
is the Artistic License 1.0 or the GPL. This fork is a derivative work and is
distributed under the GPL branch of that grant. The reasoning, including why
GPLv3 was chosen over the available LGPL option, is in `LICENSE`.

Upstream copyright is preserved:

```
Copyright 1994-2000 Seth Golub
Copyright 2002-2013 Kathryn Andersen
Copyright 2018-2019 Joao Eriberto Mota Filho
```