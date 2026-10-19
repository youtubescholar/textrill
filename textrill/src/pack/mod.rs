//! Template packs: the on-disk delivery format for user templates (roadmap §5).
//!
//! A pack is a directory holding `template.toml` plus its template files. This
//! module grows the pieces Phase 2 needs: [`manifest`] parses the manifest,
//! then validation, discovery and deterministic archives follow.

pub mod manifest;
