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
  differential corpus of 48 cases and 33 upstream golden files.
- 95 Rust tests, a fuzzer, and a 58-test GUI suite.
- Encoding detection was reworked: BOM → UTF-16 evidence → UTF-8 → CP1252, with
  explicit overrides for the encodings that cannot be detected.

What is **not** done yet:

- The GUI is still the Python/PySide6 front end. A native Rust GUI is the next
  major step.
- No Flatpak, no distro packaging yet.
- Performance is roughly 2x slower than the Perl original. Not a correctness
  problem, but not good.
- `.txt2htmlrc` config-file support is not yet restored.

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

The GUI is Python-based for now and needs a Python 3.8+ and PySide6:

```sh
cd textrill-gui
pip install -e .
textrill-gui
```

## Usage

```sh
textrill [ options ] [ file ... ]
```

With no arguments it reads standard input and writes standard output. Options
use the GNU long form and can be abbreviated to any unambiguous prefix:

```sh
textrill --bold_delimiter='#' --italic_delimiter='*' --make_links README.md > README.html
```

There are **53 options** with **107 accepted spellings** including short
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
| `--encoding` | `auto` | How to decode the input (see below) |

The delimiter names are inherited from the Perl original and are not intuitive:
`--bold_delimiter` emits `<strong>`, not `<b>`, and `--italic_delimiter` emits
`<em>`. They were left as they are so that documents convert identically under
either tool. Note that the default delimiters are off unless enabled — with the
defaults above, only `#...#` and `*...*` are converted; a bare `_word_` is left
alone unless `--underline_delimiter` is matched by surrounding text.

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

- `tests/corpus/` runs 48 documents through both implementations and requires
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