# Templates and Catalogs (Phase 3)

Textrill templates are distributed as packs. A pack is a directory containing `template.toml` and HTML/template files. Packs may be discovered from:

- `--template_dir DIR` (explicit, repeatable)
- `./.textrill/templates/` (project-local)
- `$XDG_DATA_HOME/textrill/templates` (user-installed; `install` puts packs here)
- `$XDG_DATA_DIRS/textrill/templates` (system)

All of these flags can live in `.textrillrc`.

## Catalogs (offline)

Catalogs are local directories with a `catalog.toml` index. They let you share libraries of packs without the tool ever touching the network. A canonical layout is a git repository such as `textrill-templates/`:

```
textrill-templates/
├── catalog.toml
└── packs/
    └── academic-book-1.2.0.tar
```

Register catalogs with `--catalog_dir DIR` (repeatable). This can live in `.textrillrc` like any other flag.

### catalog.toml

The index is a strict subset of TOML: `schema = 1` at the top, then one or more `[[packs]]` entries. Unknown sections or keys are refused with a message naming the line and listing the keys that are allowed, so a typo never silently drops a pack.

```toml
schema = 1

[[packs]]
id = "org.example.academic-book"
name = "Academic Book"
version = "1.2.0"
description = "A print-ready skeleton."
authors = ["A. Scholar <a@example.org>"]
license = "CC0-1.0"
tags = ["book", "print"]
sha256 = "<hex of archive>"
min_textrill = "0.1.0"
archive = "packs/academic-book-1.2.0.tar"
```

Per-entry keys: `id`, `name`, `version`, `description`, `authors`, `license`, `tags`, `sha256`, `min_textrill`, `archive`.

- **`sha256` is required.** An entry without it is refused at install time; there is no unverified install from a catalog.
- **`min_textrill`** is a semver range checked against the running version before installing; too-old releases refuse loudly, and `search` marks such entries.
- **`archive`** is relative to the catalog directory; a path that escapes the catalog (e.g. `../`) is refused.
- The installed pack's own manifest id must match the index `id`.

### Commands

- `textrill templates search <query> [--catalog_dir DIR]` — search installed packs *and* registered catalogs; entries needing a newer textrill are marked
- `textrill templates install <PATH>` — install a pack directory or `.tar` by path
- `textrill templates install <id>[@ver] [--catalog_dir DIR]` — install from a catalog; the archive must already be on disk and its SHA-256 must match the index
- `textrill templates verify <id> [--catalog_dir DIR]` — repack the installed pack and compare it against the catalog's recorded hash
- `textrill templates sources` — list the registered catalog directories
- `textrill templates update [--catalog_dir DIR]` — rescan catalogs and reinstall from their indexes (no fetching; there is nothing to fetch)

`id@1.2.0` pins an exact version; a bare `id` picks the highest listed version, and a miss names the versions that are listed.

### GUI

The Template Manager has a **Catalogs** tab beside **Packs**: every registered catalog with its index problems, entries filtered by a search box, a compatibility warning when `min_textrill` excludes the running version, and one-click **Install** through the same hash-verified staged install the CLI uses.

### Integrity

SHA-256 of archives is verified against `catalog.toml` on every catalog install; a mismatch is a hard error naming the file. The engine and the GUI perform no network I/O — "update" and "rescan" only re-read the filesystem.
