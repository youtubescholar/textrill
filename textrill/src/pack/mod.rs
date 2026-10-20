//! Template packs: the on-disk delivery format for user templates (roadmap §5).
//!
//! A pack is a directory holding `template.toml` plus its template files.
//! [`manifest`] parses the manifest, [`validate`] answers whether a pack
//! directory is sound, [`catalog`] discovers packs across the search path
//! and resolves `pack-id:template` names, and [`archive`] packs a directory
//! into a deterministic uncompressed tar and safely unpacks one.

pub mod archive;
pub mod catalog;
pub mod install;
pub mod manifest;
pub mod scaffold;
pub mod sha256;
pub mod tomlsub;
pub mod validate;

pub use archive::{pack, unpack};
pub use catalog::{
    discover, load_catalogs, parse_version, version_satisfies, Catalog, CatalogIndex,
    CatalogIndexEntry, Entry, SearchPath, Source, CATALOG_INDEX, SHIPPED_ID, VERSION,
};
pub use install::{from_archive, from_catalog_entry, from_dir};
pub use scaffold::{plan as plan_scaffold, sanitize, Scaffold};
pub use validate::{load, ValidTemplate, Validated, Var, MANIFEST};
