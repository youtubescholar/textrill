# textrill

Convert plain text to HTML.

`textrill` converts plain text to HTML, inferring structure from the
conventions text authors already use: indentation for lists, blank lines for
paragraphs, underlines and setext rules for headings, trailing punctuation for
link labels, and so on. No markup language is required or assumed.

It began as a Rust port of
[`HTML::TextToHTML`](https://metacpan.org/pod/HTML::TextToHTML) 3.0 and its
`txt2html` script, originally written by Seth Golub. Byte-parity with that
reference across the upstream test corpus is preserved and frozen; see
[Relationship to upstream](#relationship-to-upstream).

This is a fork. It is not endorsed by, and carries no affiliation with, the
upstream `txt2html` project or its authors. See [Licence](#licence).

## Status

Early. `0.1.0` is the first release of the fork.

What is **done**:

- The conversion engine is pinned by reference-free acceptance: a reviewed,
  frozen output for every one of the 61 corpus cases, 33 golden comparisons
  against upstream's own `good_*.html` files, and 8 frozen real-document
  examples. The Perl differential remains available as `make diff`.
- 367 Rust tests and a 74-test native GUI suite.
- The CLI builds as a single static `x86_64-unknown-linux-musl` binary, and CI
  runs the reference-free acceptance against that binary, so it runs on Alpine
  and other glibc-less distros with the same output it produces everywhere.
- Encoding detection was reworked: BOM → UTF-16 evidence → UTF-8 → CP1252, with
  explicit overrides for the encodings that cannot be detected.
- Opt-in `--citations` and `--glossary` collect namespaced markers into an
  endnotes list and a definition list, with no JavaScript and no inference — see
  [Citations and glossary](#citations-and-glossary). Generated `href`s and their
  target ids are checked by a permanent test, as are the `href`s built by
  `--toc`, `--section` and `--chunk`.

What is **not** done yet:

- No Flatpak, no distro packaging yet.

The GUI is the native `egui`/`eframe` front end in `textrill-gui-rs/`. A
link-pass fix puts textrill ahead of the reference on the link-dense
benchmark.


## Option files

Options can come from the command line, from `@file` groups, or from an rc file.
Precedence, lowest first:

1. `@file` groups, in the order they appear on the command line
2. `~/.textrillrc`
3. `./.textrillrc`
4. the command line

One option per line, using the same spellings as the command line:

```sh
# .textrillrc
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

The legacy names `~/.txt2htmlrc` and `./.txt2htmlrc` are read when no
`textrillrc` is present in the same directory, so a configuration written for
the Perl tool keeps working. Where both exist, `.textrillrc` is the one read.
The link dictionary follows the same rule: `~/.textrill.dict` if it exists,
otherwise `~/.txt2html.dict` (`./.textrill.dict` / `./.txt2html.dict` when
`HOME` is unset).

## Install

### The CLI

```sh
cargo install --git https://github.com/youtubescholar/textrill
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

There are **69 options** with **126 accepted spellings** including short
aliases; `textrill --help` lists them all with their defaults.

The option names are the upstream ones, unchanged, so a document converts the
same way under either tool; the doctype default is the one deliberate
departure (HTML5, see "HTML5 output" above) and `--xhtml` restores the
upstream pair. A few are worth calling out:

| Option | Default | Effect |
| --- | --- | --- |
| `--bold_delimiter` | `#` | Delimiter that produces `<strong>` |
| `--italic_delimiter` | `*` | Delimiter that produces `<em>` |
| `--underline_delimiter` | `_` | Delimiter that produces `<u>` |
| `--make_tables` | off | Detect tab-separated or column-aligned text as tables |
| `--make_links` | off | Turn labelled URLs in the text into links |
| `--default_link_dict` | none | Load a link dictionary, as the original does |
| `--allowed_url_schemes` | none | Allow only these URL schemes in `href`s, instead of refusing the script-bearing ones |
| `--extract` | off | Output only the body, without the surrounding document |
| `--meta_charset` | off | Emit `<meta charset="utf-8">` |
| `--no_referrer` | off | Private reading: emit `<meta name="referrer" content="no-referrer">` and `rel="nofollow noreferrer"` on generated external links |
| `--html5` | on | Emit HTML5: `<!DOCTYPE html>`, no namespace, charset meta; `--no-html5` gives the Perl original's HTML 4.01 |
| `--xhtml` | off | Emit XHTML 1.0 Strict instead — the Perl original's default |
| `--section` | off | Wrap each heading section in `<article id="chunk-N">` |
| `--toc` | off | Prepend a generated table of contents (implies `--section`) |
| `--chunk` | off | Write one HTML file per top-level section |
| `--number_headings` | off | Prefix headings with hierarchical numbers (`1`, `1.1`, …) |
| `--stream` | off | Read and write a paragraph at a time (UTF-8 input only) |
| `--report` | off | Print the inference counts — headings, paragraphs, capitals, breaks, bytes — on standard error |
| `--body_template` | none | Wrap the body in a template file (slots, see below); the legacy `--template` name still works |
| `--document_template` | none | Use a whole-document template file |
| `--template_library` | none | Use a shipped template by name: `article`, `book`, `manpage`, `slide` or `bare` |
| `--var` | none | Template parameter `name=value`, filled into `{{textrill:var:name}}` (see below); repeat for several |
| `--encoding` | `auto` | How to decode the input (see below) |

The delimiter names are inherited from the Perl original and are not intuitive:
`--bold_delimiter` emits `<strong>`, not `<b>`, and `--italic_delimiter` emits
`<em>`. They were left as they are so that documents convert identically under
either tool. Note that the default delimiters are off unless enabled — with the
defaults above, only `#...#` and `*...*` are converted; a bare `_word_` is left
alone unless `--underline_delimiter` is matched by surrounding text.

### Lists

Three shapes are recognised from the layout of the text. `- ` at line start
opens an unordered list, a number or letter followed by `.`, `)`, `]` or `:`
opens an ordered list, and a line that is exactly `term:` (a two-or-more-letter
name, a colon, nothing after it) opens a **definition list**: the name becomes
the `<dt>` and the following indented block becomes the `<dd>`.

```
term:
    the definition of the term
```

```html
<dl>
  <dt>term</dt>
<dd>    the definition of the term
</dd></dl>
```

The one-line form `term: definition` on a single line stays a paragraph, on
purpose: a colon is ordinary prose, and the same delimiter already starts an
ordered list (`1: two`, `a: one`), so turning every `Word: rest` into a
definition would rewrite what a sentence means.

### HTML5 output

By default the prolog is HTML5: the short `<!DOCTYPE html>`, an `<html>`
element with no namespace, a forced `<meta charset="utf-8">`, and lower-case
tags. The Perl original's modes are all still one flag away: `--xhtml` gives
the XHTML 1.0 Strict doctype with the XHTML namespace, and `--no-html5` (or
`--no-xhtml`) gives the HTML 4.01 prolog with upper-case tags, byte for byte
what the reference produces. The frozen corpus cases, and the differential's
cases when `make diff` is run, pin those modes explicitly, so the default
change never touches the parity record. Tag case is governed by
`--lower_case_tags` as usual, and each mode flag sets it as part of entering
its mode — an explicit `--lower_case_tags` after the flag wins.

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
carries the same TOC and prev/next pager links. `--chunk` cannot be combined
with `--extract`, `--instring`, or output to standard output.

Each chunked page is itself wrapped in `<article class="section" id="chunk-N">`,
so a cross-file TOC entry deep-links to the heading rather than to the top of the
page: `out-chunk-02.html#chunk-3`. The wrapper is emitted for `--toc` as well as
`--section`, because it is the anchor the TOC points at — `--toc` implies its
own targets, so it can never emit a link to somewhere that is not there.

Every internal link the engine generates is checked to resolve, in
`tests/linkintegrity.rs`. Note the scope: a document can write its own URLs with
`<URL:...>` or a link dictionary, and those are passed through as the author
wrote them — the converter has no way to know whether `docs/readme` exists.

`--number_headings` prefixes each heading with its hierarchical position
(`1`, `1.1`, `1.1.1`, …) before any sectioning runs, so the numbers also appear
in the `--toc` labels and in each `--chunk` page. It is independent of
`--section`/`--toc` and, like them, **off by default**.

The markup here is HTML5 (`<article>`, `<nav>`), which is the default output
mode, so no extra flag is needed.

### Reporting what was recovered

An inference a user cannot see is indistinguishable from a feature that
silently does nothing, so `--report` prints what the engine recovered, on
standard error, once the output is written:

```sh
$ textrill --report examples/homer.txt >/dev/null
```

```text
textrill: report bytes=38477 headings=0 paragraphs=64 strong=39 br=34
```

Five numbers, in the same `key=value` form `make examples` prints (it reads
them from this line rather than re-computing them, so the two cannot drift):
bytes, headings (`<h1>`–`<h6>`), paragraphs, capitalised runs (`<strong>`) and
line breaks (`<br>`). The counts are of the tags in the **produced document**,
not of the events the engine believes it performed — a user with the file can
reproduce every one with `grep`, and the tests do. The output is byte-identical
with and without the flag; only stderr differs. It is refused with `--stream`,
which never assembles a whole document to count.

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

- `--body_template FILE` inserts a **fragment inside `<body>`**. The engine still
  emits the doctype, `<head>` and the `<body>` tags, so you only write the part
  you want to change.
- `--document_template FILE` takes over the **whole page**. The engine emits
  none of its own prolog, so you write the doctype, head and body yourself.

The two names are a body-wrap / whole-page pair; until textrill 0.1.*, the body
wrap was called `--template`. That spelling still works, but it now prints a
deprecation warning naming the pair, so nobody has to guess which one the
near-identical name meant.

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
`./.textrillrc`, so a project commits its template and points at it there.

#### Shipped templates (`--template_library`)

Five templates ship with the tool, embedded in the binary and present as plain
files under `templates/` in the source tree, so you can use one as-is or copy
it and make it your own:

| Name | Model | What it is |
| --- | --- | --- |
| `article` | whole document | A single-document page: header with title, TOC, body, notes |
| `book` | whole document | Front matter with title, TOC, then the body as chapters |
| `manpage` | whole document | A man-style page: title, body, glossaries |
| `slide` | whole document | A deck skeleton: title, TOC, body as slides (`--section` gives one slide per heading) |
| `bare` | body wrapper | Just `{{textrill:content}}`: converts byte-identically to no template at all |

```sh
textrill --template_library article --title "T. H. White" chapter.txt
```

Each shipped template uses **only the fixed slots** — never a
`{{textrill:var:...}}` slot — so it converts with zero required arguments and
produces no silent-empty frames. A template that needs a byline, date or the
like is meant to be copied and given `{{textrill:var:name}}` slots of its own
(that is what `--var` is for). `--template_library` is mutually exclusive with
`--body_template` and `--document_template`, and an unknown name is an error
that lists the library.

#### Parameters (`--var`)

`--var name=value` (repeatable) binds `{{textrill:var:name}}` in the active
template — the settings a page or a site build varies per run:

```sh
textrill --body_template wrap.html \
  --var date="2026-10-07" \
  --var author="A &amp; One" \
  --var strapline="<span class=\"strapline\">draft</span>" \
  chapter.txt
```

The rules are the strict end of the scale, on purpose:

- **Verbatim, not re-escaped.** The value is inserted exactly as given, `&`
  stays `&` and markup stays markup; you own any escaping. A value may carry
  whole blocks (`<aside>…</aside>`) and they survive byte for byte, without
  being reflowed or squished against the engine's own blocks.
- **Declared only.** An undeclared `{{textrill:var:name}}` is a hard error that
  names the missing variable and lists the declared ones, raised while the
  template is validated — before any output bytes exist. There is no silent
  empty, unlike Mustache's missing-var behaviour.
- **Optional by choice.** An empty value (`--var name=`) is legal where a
  parameter is genuinely blank: the slot contributes nothing and the
  surrounding markup stands. Nothing invisible is emitted behind your back —
  the token you wrote is where the value shows.
- **No re-scan.** Substitution is one pass: a value that happens to contain
  `{{textrill:toc}}` stays literal, so a parameter cannot smuggle the engine's
  own slots in or cascade into them; a real slot after it still fills.
- **Names** are letters, digits, `_` or `-`. The first `=` splits `name` from
  `value`, so `--var k=a=b` binds `k` to `a=b`. A malformed `--var` (missing
  `=`, empty name, bad name) is itself a command-line error.

This is single-insertion templating, not a template language: no loops,
conditionals or includes, and declared-variable errors surface at validation
time rather than as blank output.

### Citations and glossary

Two opt-in, default-off modes collect explicit references into a list at the end
of the body. Both are triggered only by a namespaced marker; ordinary prose is
never interpreted, so `[^1]`, `^2`, `(3)`, `[4]` and `@five` stay text.

| Marker | Meaning |
| --- | --- |
| `{{textrill:cite:key}}` | Cite `key` |
| `{{textrill:def:cite:key}}` … `{{/textrill:def:cite:key}}` | The text of that citation |
| `{{textrill:gloss:key}}` | Refer to the term `key` |
| `{{textrill:def:gloss:key}}` … `{{/textrill:def:gloss:key}}` | The definition of that term |

```sh
textrill --citations <<'EOF'
See the study {{textrill:cite:knuth}} and again {{textrill:cite:knuth}}.

{{textrill:def:cite:knuth}}
Knuth, *Literate Programming*.
{{/textrill:def:cite:knuth}}
EOF
```

Citations become `[1]`, `[2]`, … numbered by first reference, with the list
appended as an endnotes `<ol>`; terms become a `<dl>` whose `<dt>` is the key.
Both link to their entry and each entry links back to its first reference. Only
the first reference to a key carries an `id`, so fifty mentions of one citation
still produce one target.

A definition may use textrill's own delimiters (`*italic*`, `` `code` ``), since
it is collected from the converted document.

With both modes off the markers are left alone and the output is byte-identical
to a run without them. A marker belonging to a mode that is *off* is also left
alone, so `--citations` never turns a glossary marker into an error.

Anything ambiguous is refused, and the message goes to standard error with a
non-zero exit **before the output file is opened** — a document with a broken
note set produces no output rather than a `[1]` pointing at nothing:

- a reference with no definition, or a definition nothing references
- a definition given twice, or given empty
- an unbalanced block, or a closing tag that does not match the block it closes
- a key that is empty or uses anything but ASCII letters, digits, `-`, `_`, `.`
  (keys land in `id` attributes)
- a `{{textrill:…}}` token that is not one of the four above

`--chunk` and `--stream` are refused with either mode: numbering depends on the
whole document, and both write output as they go. `--extract` works, since the
lists simply append to the body.

With `--body_template`, `{{textrill:citations}}` and `{{textrill:glossary}}` place
the two lists wherever the template wants them. A template that names neither
slot still gets them appended, so an existing template never loses a list.

No JavaScript is involved, and no CSS either: the note body is emitted once, as a
real list, rather than once per reference.

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
Western European input the two agree, which is why the differential treats them
as matching by default.

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
maintained by Kathryn Andersen and Joao Eriberto Mota Filho. textrill began as
a Rust port of it. The port was restored to byte-parity with that reference by
differential testing against the real implementation rather than guessed from
documentation, which is why the option names are the upstream ones and a
document can be converted by either tool and compared:

- `tests/corpus/` runs 61 cases through both implementations and requires
  byte-identical output.
- The 33 upstream golden files are compared byte for byte.

That parity is now **recorded, not re-proved against Perl every build**.
`make accept` compares textrill against its own goldens for every one of those
61 cases plus the 33 upstream goldens and the 8 example documents, and it is
part of `make verify`. To change output deliberately, recapture the affected
goldens (`make accept-write`, `make examples-write`) and review the diff —
see the root `CONTRIBUTING.md`. The Perl differential itself became `make
diff`, a non-gating cross-check for when a fresh comparison against Perl is
worth having — the reference is a historical artifact, not the specification,
and it never sits on the path of `make verify` or CI. Where the two disagree,
textrill decides.

Where textrill deliberately differs from upstream, the divergence is declared
and explained in `tests/corpus/README.md` rather than left to be discovered.
Known differences:

- **The default doctype is HTML5, not XHTML.** The reference defaults to
  XHTML 1.0 Strict; textrill emits HTML5 with a charset meta and lower-case
  tags unless told otherwise. `--xhtml` selects the reference's XHTML mode and
  `--no-html5` its HTML 4.01 mode, both byte-identical to the reference — and
  every frozen corpus case pins one of the modes explicitly, so no acceptance
  output is ever compared against a default the run did not declare. See "The
  doctype is pinned on both sides" in the corpus README.

- **Input encoding detection** does not exist upstream. See `--encoding` above.
- User-supplied regular expressions are validated before use. An invalid
  pattern produces a diagnostic naming the option, the pattern and the parser's
  complaint, instead of a panic. This is a strict improvement; the only
  behavioural change is that a bad pattern no longer crashes the process.
- A handful of pathological inputs that upstream handles by silently corrupting
  output are handled correctly here. These are marked in the corpus README.

The audit trail for the harness itself — how the corpus and the goldens are
meant to be believed and changed — is in `tests/corpus/README.md`
and the root `../CONTRIBUTING.md`.

## Licence

GPL-3.0-or-later.

Upstream `txt2html` licenses itself "under the same terms as Perl itself", which
is the Artistic License 1.0 or the GPL. This fork is a derivative work and is
distributed under the GPL branch of that grant. The reasoning, including why
the GPL (not the available LGPL option) was chosen, is in
[`../LICENSE-NOTICE.md`](../LICENSE-NOTICE.md).

Upstream copyright is preserved:

```
Copyright 1994-2000 Seth Golub
Copyright 2002-2013 Kathryn Andersen
Copyright 2018-2019 Joao Eriberto Mota Filho
```