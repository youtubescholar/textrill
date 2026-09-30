//! txt2html — a Rust port of HTML::TextToHTML v3.0.
//!
//! Converts plain text to HTML. This crate provides the core conversion
//! engine (`Converter`) plus the options model (`Options`).
//!
//! # Compatibility
//!
//! The conversion output is byte-identical to the Perl module for every file
//! of the upstream `tfiles` corpus (see `tests/corpus`). Perl-isms that the
//! output depends on are reproduced deliberately: `$/ = ""` paragraph records,
//! `$` matching before one trailing newline, `"0"` being false, one-byte
//! placeholders in the table space maps, and option tables that keep their
//! state between calls.
//!
//! Three deliberate deviations, none of which the upstream test suite
//! exercises:
//!
//! * `Options::instring` holds literal input strings, as documented. The Perl
//!   module reads `$_` instead of the current source in its string branch, so
//!   `instring` there always converts an empty paragraph.
//! * `Options::inhandle` does not exist; pass a file instead, or feed
//!   [`Converter::process_chunk`] / [`Converter::process_para`] directly.
//! * [`convert::read_any_file`] decodes UTF-8 when the bytes are valid UTF-8
//!   and falls back to Latin-1 (one byte, one code point) otherwise. Perl
//!   reads input as raw bytes, so on a UTF-8 file its `demoronize` pass sees
//!   each byte separately: `U+201C` (`e2 80 9c`) is mangled to `&acirc;` plus
//!   two stray bytes, because `0x9c` falls in the `0x82`-`0x9F` range that
//!   `demoronize_char` rewrites. Decoding first keeps the text intact. The
//!   difference is visible only for UTF-8 input containing characters whose
//!   encoding has a byte in the `0x80`-`0x9F` range, and only while
//!   `demoronize` is on.

#![forbid(unsafe_code)]
#[cfg(feature = "extension-module")]
mod python;

pub mod chars;
pub mod cli;
pub mod convert;
pub mod links;
pub mod options;

pub use convert::Converter;
pub use options::Options;
