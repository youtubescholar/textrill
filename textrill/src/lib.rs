//! Core conversion engine and options model for converting plain text to HTML.
//!
//! Structure is inferred from layout. Key invariants: output is always UTF-8;
//! encoding detection order is BOM → UTF-16 NUL pattern → UTF-8 validity → CP1252
//! guess (single-byte non-Latin encodings cannot be auto-detected). Input
//! decoding is by evidence. Unsafe is forbidden. Verified via test suite.

#![forbid(unsafe_code)]

pub mod chars;
pub mod cli;
pub mod convert;
pub mod encode;
pub mod langdetect;
pub mod library;
pub mod links;
pub mod notes;
pub mod options;
pub mod prefilter;
pub mod rcfile;
pub mod report;
pub mod section;
pub mod template;
pub mod urlscheme;

pub use convert::Converter;
pub use options::Options;
