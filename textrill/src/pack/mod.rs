//! Template packs: the on-disk delivery format for user templates (roadmap §5).
//!
//! A pack is a directory holding `template.toml` plus its template files.
//! [`manifest`] parses the manifest, [`validate`] answers whether a pack
//! directory is sound, and discovery (roots, `pack-id:name` addressing) and
//! deterministic archives follow.

pub mod manifest;
pub mod validate;

pub use validate::{load, ValidTemplate, Validated, Var};
