//! textrill — convert plain text to HTML.
//!
//! A Rust reimplementation of `HTML::TextToHTML` v3.0 and its `txt2html`
//! script, originally by Seth Golub.
//!
//! Converts plain text to HTML. This crate provides the core conversion
//! engine (`Converter`) plus the options model (`Options`).
//!
//! # Compatibility
//!
//! The conversion output is byte-identical to the Perl module for 29 of the 31
//! golden files in the upstream `tfiles` corpus (see `tests/corpus`). The other
//! two are not divergences: upstream's own comparison is looser than
//! byte-comparison there, so the port's output is the correct one and the
//! goldens are not. Perl-isms that the output depends on are reproduced
//! deliberately: `$/ = ""` paragraph records, `$` matching before one trailing
//! newline, `"0"` being false, one-byte placeholders in the table space maps, and
//! option tables that keep their state between calls.
//!
//! Six deliberate deviations, none of which the upstream test suite
//! exercises:
//!
//! * An explicit `--title` and `--style_url` are escaped on their way into the
//!   document — `<`, `>`, `&` and `"` — because they are option values
//!   interpolated into a tag, where nothing stops them from closing it. Perl
//!   emits both verbatim: `--title '</title><script>alert(3)</script>'` came out
//!   of the reference as that literal script element, and `--style_url 'x.css"
//!   onload="alert(4)'` closed the `href` attribute the same way. A title derived
//!   with `--titlefirst` is *not* covered by this: it comes from the document's
//!   own first line, so it is document text and `escape_html_chars` governs it
//!   exactly as the reference does.
//! * A `href` whose URL names a script-bearing scheme — `javascript`, `data`,
//!   `vbscript`, `file` — is unwrapped rather than emitted; the visible words
//!   stay. See [`urlscheme`] for why the check is a scan over finished markup and
//!   why the default refuses a small named set rather than allowlisting one.
//!
//!   The document chooses these URLs. Perl never looked at them, so
//!   `<URL:javascript:alert(document.domain)>` in an otherwise ordinary file
//!   came out of the reference as a live link, and a link dictionary can name
//!   any URL at all. `--allowed_url_schemes` sets a strict allowlist instead,
//!   which is the stronger property, and is the right choice for a site that
//!   does not use custom schemes. **The two implementations therefore cannot
//!   agree** on a document containing one of these, so the `url_scheme` corpus
//!   case is declared with `differential must fail:` — the divergence is the
//!   fix, and a byte comparison is not the oracle.
//!
//!   Two related things are *not* deviations, because the reference cannot be
//!   compared to them: the refused scheme's diagnostic, and the diagnostic for
//!   a dictionary rule dropped at load. Both are new output on stderr.
//! * `Options::instring` holds literal input strings, as documented. The Perl
//!   module reads `$_` instead of the current source in its string branch, so
//!   `instring` there always converts an empty paragraph.
//! * `Options::inhandle` does not exist; pass a file instead, or feed
//!   [`Converter::process_chunk`] / [`Converter::process_para`] directly.
//! * [`convert::read_any_file`] decodes input by *evidence*, in the order a
//!   byte-order mark, then the NUL alignment of UTF-16, then UTF-8 validity,
//!   then a **CP1252** guess — see [`options::Encoding::Auto`]. Output is
//!   always UTF-8.
//!
//!   Three consequences are worth stating separately, because they have
//!   different characters:
//!
//!   1. **A BOM is honoured**, so a UTF-16 file is read as UTF-16. Perl has no
//!      UTF-16 concept at all and passes the bytes through.
//!   2. **BOM-less UTF-16 of ASCII prose is decoded**, which is the case that
//!      needed the order above: every byte is below `0x80`, so the file is *also
//!      valid UTF-8*, and a UTF-8 validity check cannot distinguish it. Both
//!      implementations used to accept it and emit a NUL between every letter.
//!      The `utf16le_ascii`, `utf16be_ascii` and `utf16le_bom` corpus cases
//!      record this.
//!   3. **The CP1252 fallback remains a guess**, and it is wrong for CP1251,
//!      KOI8-R, CP1253 and Turkish. Those encodings cannot be detected from the
//!      bytes — they are mutually indistinguishable — so they are selectable
//!      with [`options::Encoding`] and `--encoding` rather than guessed at. With
//!      the default, the port and the reference mangle such a file identically;
//!      the `cp1251_cyrillic`, `koi8r_cyrillic` and `cp1253_greek` corpus cases
//!      pin that equality, so a later change that starts guessing is visible.
//!
//!   Perl reads input as raw bytes, so its
//!   `demoronize` pass sees every byte separately: `U+201C` (`e2 80 9c`) is
//!   mangled to `&acirc;` plus two stray bytes, because `0x9c` falls in the
//!   `0x82`-`0x9F` range that `demoronize_char` rewrites. Decoding first keeps
//!   the text intact.
//!
//!   This divergence is wider than "UTF-8 input". An earlier version of this
//!   note said the difference shows up only for UTF-8 input containing a
//!   character whose encoding has a byte in `0x80`-`0x9F`, which understated it
//!   in both directions. It is also visible for **CP1252 input**, which is the
//!   more common case: a file written on Windows holds raw `0x93 0x94 0x96
//!   0x97`, the reference passes those bytes through for the browser to guess
//!   at, and the port decodes them to U+201C/U+201D/U+2013/U+2014 and
//!   demoronizes them to ASCII `"`, `"`, `-`, `--`. The rendered text agrees
//!   and the bytes do not, so a byte comparison against the reference must fail
//!   on such a file. The `cp1252_smart` corpus case records this.
//!
//!   The fallback was Latin-1 until P7.1, which made this divergence much
//!   worse than a byte difference: `demoronize_char` is keyed on the *CP1252*
//!   code points, and a Latin-1 decode of `0x93` produces U+0093, which is not
//!   in the table. Every substitution silently did nothing on exactly the
//!   files `demoronize` exists for, and the C1 control character was re-emitted
//!   as UTF-8 `c2 93` into the HTML, where it renders as nothing at all. The
//!   two encodings differ only on `0x80`-`0x9F` — the rest of Latin-1 is
//!   identical to CP1252 — so no existing fixture noticed.
//! * An input file that cannot be read is a failure, not a shrug. Perl prints
//!   `Could not open …` and exits 0 having written a 0-byte output file, which
//!   `make` and CI read as a successful build; the port exits 1. **The output
//!   is unchanged** — an unreadable file contributed nothing to it either way,
//!   and with several inputs the readable ones are still converted — so this
//!   moves no golden. The reference's own message is still printed. An *empty*
//!   file is not a failure: it is readable, and it exits 0. Use
//!   [`Converter::try_convert`] to see which files could not be read;
//!   [`Converter::txt2html`] keeps the reference's forgiving behaviour for
//!   in-process callers that want a `String` regardless.

#![forbid(unsafe_code)]

pub mod chars;
pub mod cli;
pub mod convert;
pub mod encode;
pub mod links;
pub mod notes;
pub mod options;
pub mod prefilter;
pub mod rcfile;
pub mod section;
pub mod template;
pub mod urlscheme;

pub use convert::Converter;
pub use options::Options;
