//! Template packs: the on-disk delivery format for user templates (roadmap §5).
//!
//! A pack is a directory holding `template.toml` plus its template files.
//! [`manifest`] parses the manifest, [`validate`] answers whether a pack
//! directory is sound, and [`catalog`] discovers packs across the search
//! path and resolves `pack-id:template` names; deterministic archives
//! follow.

pub mod catalog;
pub mod manifest;
pub mod validate;

pub use catalog::{discover, Catalog, Entry, SearchPath, Source};
pub use validate::{load, ValidTemplate, Validated, Var};
