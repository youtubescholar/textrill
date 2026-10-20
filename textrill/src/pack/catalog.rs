//! Discovery (roadmap §5.1): where packs come from and how they are named.
//!
//! A precedence-ordered set of roots is searched; each root's subdirectories
//! are packs. The first root that provides a pack id wins, and every
//! collision is recorded rather than silently dropped. A template is
//! addressed as `pack-id:template-name`; a bare name resolves only when it
//! is unambiguous across every source, and a bare-name collision is refused
//! with the full addresses to choose from — never guessed at by a priority
//! the user cannot see. The compiled-in shipped set is the last root, always
//! present. Covered by the tests below.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::tomlsub::{check_key, err, section_name, Value};
use super::validate::{self, Validated};
use crate::library::{self, Model};

/// The reserved pack id of the compiled-in set. No pack can claim it: a pack
/// id needs at least one dot (see `validate::check_id`) and this has none.
pub const SHIPPED_ID: &str = "shipped";

/// Where a template was found, for the `--list_templates` source column and
/// the GUI badge. [`Source::label`] folds the five origins into the three
/// words the roadmap names: shipped / local / installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `--template_dir`, the highest-priority explicit root.
    Explicit,
    /// `./.textrill/templates`, committable with the project.
    Project,
    /// `$XDG_DATA_HOME/textrill/templates`, where `install` puts packs.
    User,
    /// `$XDG_DATA_DIRS/textrill/templates`, system-wide installs.
    System,
    /// The compiled-in set: always present, never installed or removed.
    Shipped,
}

impl Source {
    /// The badge word for listings and the GUI.
    pub fn label(self) -> &'static str {
        match self {
            Source::Shipped => "shipped",
            Source::Explicit | Source::Project => "local",
            Source::User | Source::System => "installed",
        }
    }
}

/// The roots packs may live under, in precedence order. Built from the
/// environment by [`SearchPath::from_env`], or from explicit values by
/// [`SearchPath::build`] so the rules are testable without touching the
/// process environment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchPath {
    /// `--catalog_dir` directories containing catalog indexes.
    pub catalog_dirs: Vec<PathBuf>,
    /// `--template_dir` roots, highest priority, in flag order.
    pub explicit: Vec<PathBuf>,
    /// `<cwd>/.textrill/templates`.
    pub project: PathBuf,
    /// `$XDG_DATA_HOME/textrill/templates`, when a data home exists.
    pub data_home: Option<PathBuf>,
    /// `$XDG_DATA_DIRS/textrill/templates`, one per data dir.
    pub data_dirs: Vec<PathBuf>,
}

impl SearchPath {
    /// The search path from the live environment.
    pub fn from_env() -> SearchPath {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let data_dirs = std::env::var_os("XDG_DATA_DIRS").map(|s| s.to_string_lossy().into_owned());
        Self::build(&cwd, home, data_home, data_dirs.as_deref())
    }

    /// Build a search path from its inputs, applying the XDG defaults: no
    /// usable `XDG_DATA_HOME` falls back to `HOME/.local/share`, and no
    /// `XDG_DATA_DIRS` means `/usr/local/share:/usr/share`. Per the XDG
    /// spec, relative entries in the environment are ignored rather than
    /// resolved against the working directory.
    pub fn build(
        cwd: &Path,
        home: Option<PathBuf>,
        data_home: Option<PathBuf>,
        data_dirs: Option<&str>,
    ) -> SearchPath {
        let data_home = data_home
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local/share")))
            .map(|h| h.join("textrill").join("templates"));
        let dirs = data_dirs.unwrap_or("/usr/local/share:/usr/share");
        let data_dirs = dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .filter(|d| d.is_absolute())
            .map(|d| d.join("textrill").join("templates"))
            .collect();
        SearchPath {
            catalog_dirs: Vec::new(),
            explicit: Vec::new(),
            project: cwd.join(".textrill").join("templates"),
            data_home,
            data_dirs,
        }
    }

    /// Add a `--template_dir` root, in flag order.
    pub fn add_explicit(&mut self, dir: impl Into<PathBuf>) {
        self.explicit.push(dir.into());
    }

    /// Add a `--catalog_dir` directory containing a catalog index.
    pub fn add_catalog_dir(&mut self, dir: impl Into<PathBuf>) {
        self.catalog_dirs.push(dir.into());
    }

    /// Every root in precedence order, each paired with the source it
    /// confers. A root that does not exist is still listed; discovery skips
    /// it, because an absent system directory is the normal case.
    pub fn roots(&self) -> Vec<(PathBuf, Source)> {
        let mut v: Vec<(PathBuf, Source)> = self
            .explicit
            .iter()
            .map(|d| (d.clone(), Source::Explicit))
            .collect();
        v.push((self.project.clone(), Source::Project));
        if let Some(d) = &self.data_home {
            v.push((d.clone(), Source::User));
        }
        for d in &self.data_dirs {
            v.push((d.clone(), Source::System));
        }
        v
    }
}

/// One template in the catalog: discovered or shipped, listed and resolved
/// the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The pack it belongs to. The shipped set uses [`SHIPPED_ID`].
    pub pack_id: String,
    pub pack_name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    /// The template's name inside its pack (`book.html` → `book`).
    pub name: String,
    pub model: Model,
    /// The `slots` the manifest declares; empty for the shipped set.
    pub slots: Vec<String>,
    pub vars: Vec<validate::Var>,
    pub preview: Option<PathBuf>,
    pub source: Source,
    /// The pack directory. `None` for the shipped set, which has no
    /// directory on disk.
    pub root: Option<PathBuf>,
    /// The template file inside [`Entry::root`]. `None` for shipped.
    pub file: Option<PathBuf>,
}

impl Entry {
    /// The address of this template: `pack-id:name`, which resolves back to
    /// it through [`Catalog::resolve`] in every case.
    pub fn address(&self) -> String {
        format!("{}:{}", self.pack_id, self.name)
    }

    /// The template's markup: embedded for the shipped set, read from the
    /// pack for a discovered one. A pack file that vanished since discovery
    /// is an error naming the path.
    pub fn html(&self) -> Result<String, String> {
        match self.source {
            Source::Shipped => Ok(library::get(&self.name)
                .expect("a shipped entry always comes from the library")
                .html
                .to_string()),
            _ => {
                let file = self
                    .file
                    .as_ref()
                    .expect("a discovered entry always names a file");
                fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))
            }
        }
    }
}

/// Every discoverable template, plus the problems seen on the way. Discovery
/// never fails: a broken pack is skipped and recorded so a listing can still
/// show everything else and say what is wrong with the rest.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    /// Packs that were found but could not be used, one per pack, as
    /// `path: problem`.
    pub problems: Vec<String>,
}

impl Catalog {
    /// Resolve a template reference. `pack-id:template` selects inside a
    /// pack; a bare `template` name must be unambiguous across every source.
    /// A bare name shared by several templates is refused with the full
    /// addresses to choose from.
    pub fn resolve(&self, spec: &str) -> Result<&Entry, String> {
        if let Some((pack, name)) = spec.split_once(':') {
            return self
                .entries
                .iter()
                .find(|e| e.pack_id == pack && e.name == name)
                .ok_or_else(|| {
                    let mut have: Vec<&str> = self
                        .entries
                        .iter()
                        .filter(|e| e.pack_id == pack)
                        .map(|e| e.name.as_str())
                        .collect();
                    have.sort_unstable();
                    if have.is_empty() {
                        format!("no pack with id {pack:?} was discovered")
                    } else {
                        format!(
                            "pack {pack:?} has no template {name:?}; it has: {}",
                            have.join(", ")
                        )
                    }
                });
        }
        let mut hits: Vec<&Entry> = self.entries.iter().filter(|e| e.name == spec).collect();
        match hits.len() {
            0 => {
                // A small catalog (the shipped default is five) names what it
                // has, as the engine always did; a large one points at the
                // listing rather than flooding the message.
                if self.entries.len() <= 10 {
                    let have: Vec<&str> = self.entries.iter().map(|e| e.name.as_str()).collect();
                    Err(format!(
                        "no template named {spec:?} was discovered; available: {}",
                        have.join(", ")
                    ))
                } else {
                    Err(format!(
                        "no template named {spec:?} was discovered; {} templates are \
                         available (see --list_templates)",
                        self.entries.len()
                    ))
                }
            }
            1 => Ok(hits.pop().expect("one match")),
            n => {
                hits.sort_by_key(|e| e.address());
                let all = hits
                    .iter()
                    .map(|e| e.address())
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(format!(
                    "{spec:?} is ambiguous: {n} templates share that name ({all}); \
                     address one as pack-id:name"
                ))
            }
        }
    }
}

/// Discover every pack under the search path, validating each, then append
/// the shipped set as the lowest-priority root. Within a root, subdirectories
/// are visited in sorted order so a listing is deterministic; a root that
/// does not exist is skipped, because most of the search path usually does
/// not exist.
pub fn discover(search: &SearchPath) -> Catalog {
    let mut catalog = Catalog::default();
    let mut claimed: HashMap<String, PathBuf> = HashMap::new();
    for (root, source) in search.roots() {
        let Ok(read) = fs::read_dir(&root) else {
            continue;
        };
        let mut packs: Vec<PathBuf> = read
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                // Dot directories are never packs: `install` stages into
                // `.install-*` beside the real destinations, and OS metadata
                // (.DS_Store and friends) must not surface as broken packs.
                !p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            })
            .filter(|p| p.is_dir())
            .collect();
        packs.sort();
        for dir in packs {
            match validate::load(&dir) {
                Ok(pack) => match claimed.get(&pack.id) {
                    Some(winner) => catalog.problems.push(format!(
                        "{}: pack id {:?} is already provided by {}; the \
                         higher-precedence copy wins",
                        dir.display(),
                        pack.id,
                        winner.display()
                    )),
                    None => {
                        claimed.insert(pack.id.clone(), dir.clone());
                        add_pack(&mut catalog.entries, pack, &dir, source);
                    }
                },
                Err(problem) => catalog
                    .problems
                    .push(format!("{}: {problem}", dir.display())),
            }
        }
    }
    for shipped in library::LIBRARY {
        catalog.entries.push(Entry {
            pack_id: SHIPPED_ID.to_string(),
            pack_name: "Shipped".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: String::new(),
            authors: Vec::new(),
            tags: Vec::new(),
            name: shipped.name.to_string(),
            model: shipped.model,
            slots: Vec::new(),
            vars: Vec::new(),
            preview: None,
            source: Source::Shipped,
            root: None,
            file: None,
        });
    }
    catalog
}

/// Expand one validated pack into one entry per template it declares.
fn add_pack(entries: &mut Vec<Entry>, pack: Validated, dir: &Path, source: Source) {
    let Validated {
        id,
        name,
        version,
        description,
        authors,
        tags,
        templates,
        ..
    } = pack;
    for t in templates {
        entries.push(Entry {
            pack_id: id.clone(),
            pack_name: name.clone(),
            version: version.clone(),
            description: description.clone(),
            authors: authors.clone(),
            tags: tags.clone(),
            name: t.name,
            model: t.model,
            slots: t.slots,
            vars: t.vars,
            preview: t.preview,
            source,
            root: Some(dir.to_path_buf()),
            file: Some(t.file),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("textrill-cat-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    /// A valid pack `id` under `root`, with one template named `stem`.
    fn pack(root: &Path, id: &str, stem: &str) {
        write(
            &root.join(id),
            "template.toml",
            &format!(
                "schema = 1\nid = \"{id}\"\nname = \"N\"\nversion = \"1.0.0\"\n\
                 license = \"GPL-3.0-or-later\"\n\n[[templates]]\nfile = \
                 \"{stem}.html\"\nmodel = \"document\"\n"
            ),
        );
        write(
            &root.join(id),
            &format!("{stem}.html"),
            "{{textrill:content}}",
        );
    }

    /// A search path whose only roots are the ones under `base`.
    fn only(roots: &[(&Path, Source)]) -> SearchPath {
        let mut s = SearchPath {
            catalog_dirs: Vec::new(),
            project: PathBuf::from("/nonexistent-project-root"),
            data_home: None,
            data_dirs: Vec::new(),
            explicit: Vec::new(),
        };
        for (dir, source) in roots {
            match source {
                Source::Explicit => s.explicit.push((*dir).to_path_buf()),
                Source::Project => s.project = (*dir).to_path_buf(),
                Source::User => s.data_home = Some((*dir).to_path_buf()),
                Source::System => s.data_dirs.push((*dir).to_path_buf()),
                Source::Shipped => unreachable!("shipped is not a directory root"),
            }
        }
        s
    }

    #[test]
    fn the_xdg_defaults_apply_when_the_environment_is_empty() {
        let s = SearchPath::build(
            Path::new("/work"),
            Some(PathBuf::from("/home/u")),
            None,
            None,
        );
        assert_eq!(s.project, Path::new("/work/.textrill/templates"));
        assert_eq!(
            s.data_home.as_deref(),
            Some(Path::new("/home/u/.local/share/textrill/templates"))
        );
        assert_eq!(
            s.data_dirs,
            vec![
                PathBuf::from("/usr/local/share/textrill/templates"),
                PathBuf::from("/usr/share/textrill/templates"),
            ]
        );
    }

    #[test]
    fn an_absolute_xdg_data_home_wins_over_home() {
        let s = SearchPath::build(
            Path::new("/work"),
            Some(PathBuf::from("/home/u")),
            Some(PathBuf::from("/custom/share")),
            None,
        );
        assert_eq!(
            s.data_home.as_deref(),
            Some(Path::new("/custom/share/textrill/templates"))
        );
    }

    #[test]
    fn a_relative_xdg_entry_is_ignored_not_resolved() {
        // The XDG spec: relative values are invalid and must be disregarded.
        let s = SearchPath::build(
            Path::new("/work"),
            Some(PathBuf::from("/home/u")),
            Some(PathBuf::from("relative/share")),
            Some(":/opt/share::relative"),
        );
        assert_eq!(
            s.data_home.as_deref(),
            Some(Path::new("/home/u/.local/share/textrill/templates"))
        );
        assert_eq!(
            s.data_dirs,
            vec![PathBuf::from("/opt/share/textrill/templates")]
        );
    }

    #[test]
    fn no_home_leaves_no_data_home() {
        let s = SearchPath::build(Path::new("/work"), None, None, Some(""));
        assert_eq!(s.data_home, None);
        assert!(s.data_dirs.is_empty());
    }

    #[test]
    fn roots_come_out_in_precedence_order() {
        let s = SearchPath {
            catalog_dirs: Vec::new(),
            explicit: vec![PathBuf::from("/a"), PathBuf::from("/b")],
            project: PathBuf::from("/p"),
            data_home: Some(PathBuf::from("/u")),
            data_dirs: vec![PathBuf::from("/s1"), PathBuf::from("/s2")],
        };
        let got: Vec<Source> = s.roots().iter().map(|(_, src)| *src).collect();
        assert_eq!(
            got,
            vec![
                Source::Explicit,
                Source::Explicit,
                Source::Project,
                Source::User,
                Source::System,
                Source::System,
            ]
        );
        assert_eq!(s.roots()[0].0, Path::new("/a"));
        assert_eq!(s.roots()[5].0, Path::new("/s2"));
    }

    #[test]
    fn source_labels_fold_to_the_three_badge_words() {
        assert_eq!(Source::Shipped.label(), "shipped");
        assert_eq!(Source::Explicit.label(), "local");
        assert_eq!(Source::Project.label(), "local");
        assert_eq!(Source::User.label(), "installed");
        assert_eq!(Source::System.label(), "installed");
    }

    #[test]
    fn a_pack_in_the_project_root_is_discovered() {
        let base = scratch("project");
        let root = base.join("templates");
        pack(&root, "org.example.book", "book");
        let c = discover(&only(&[(&root, Source::Project)]));
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        let e = c.resolve("org.example.book:book").unwrap();
        assert_eq!(e.source, Source::Project);
        assert_eq!(e.model, Model::Document);
        assert_eq!(e.address(), "org.example.book:book");
        assert_eq!(e.html().unwrap(), "{{textrill:content}}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_shipped_five_are_always_present_last() {
        let c = discover(&only(&[]));
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        assert_eq!(c.entries.len(), library::LIBRARY.len());
        let e = c.resolve("article").unwrap();
        assert_eq!(e.source, Source::Shipped);
        assert_eq!(e.pack_id, SHIPPED_ID);
        assert_eq!(e.model, Model::Document);
        assert!(e.root.is_none());
        assert!(e.file.is_none());
        assert!(c.resolve("shipped:article").is_ok());
        assert!(c.resolve("shipped:bare").unwrap().model == Model::Body);
    }

    #[test]
    fn a_higher_precedence_root_wins_and_the_loser_is_named() {
        let base = scratch("precedence");
        let high = base.join("high");
        let low = base.join("low");
        pack(&high, "org.example.dup", "a");
        pack(&low, "org.example.dup", "b");
        let c = discover(&only(&[(&high, Source::Explicit), (&low, Source::Project)]));
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(c.problems.len(), 1);
        assert!(
            c.problems[0].contains("org.example.dup"),
            "{:?}",
            c.problems
        );
        assert!(
            c.problems[0].contains("already provided"),
            "{:?}",
            c.problems
        );
        let e = c.resolve("org.example.dup:a").unwrap();
        assert_eq!(e.source, Source::Explicit);
        assert!(c.resolve("org.example.dup:b").is_err());
    }

    #[test]
    fn a_bare_name_shared_by_two_packs_is_refused_with_addresses() {
        let base = scratch("ambiguous");
        let root = base.join("templates");
        pack(&root, "org.example.a", "theme");
        pack(&root, "org.example.b", "theme");
        let c = discover(&only(&[(&root, Source::Project)]));
        let _ = std::fs::remove_dir_all(&base);
        let err = c.resolve("theme").unwrap_err();
        assert!(err.contains("ambiguous"), "{err}");
        assert!(err.contains("org.example.a:theme"), "{err}");
        assert!(err.contains("org.example.b:theme"), "{err}");
        assert!(c.resolve("org.example.a:theme").is_ok());
        assert!(c.resolve("org.example.b:theme").is_ok());
    }

    #[test]
    fn a_bare_name_that_equals_a_pack_template_is_also_ambiguous() {
        let base = scratch("ship-collide");
        let root = base.join("templates");
        pack(&root, "org.example.art", "article");
        let c = discover(&only(&[(&root, Source::Project)]));
        let _ = std::fs::remove_dir_all(&base);
        let err = c.resolve("article").unwrap_err();
        assert!(err.contains("ambiguous"), "{err}");
        assert!(err.contains("shipped:article"), "{err}");
        assert!(err.contains("org.example.art:article"), "{err}");
        assert_eq!(
            c.resolve("shipped:article").unwrap().source,
            Source::Shipped
        );
    }

    #[test]
    fn an_unknown_pack_or_template_is_reported_named() {
        let c = discover(&only(&[]));
        let err = c.resolve("nope:nope").unwrap_err();
        assert!(err.contains("no pack with id \"nope\""), "{err}");
        let err = c.resolve("shipped:nope").unwrap_err();
        assert!(err.contains("has no template \"nope\""), "{err}");
        assert!(
            err.contains("article"),
            "the message lists what it has: {err}"
        );
        let err = c.resolve("nope").unwrap_err();
        assert!(err.contains("no template named \"nope\""), "{err}");
        assert!(err.contains("available"), "{err}");
    }

    #[test]
    fn a_broken_pack_is_skipped_and_recorded_not_fatal() {
        let base = scratch("broken");
        let root = base.join("templates");
        pack(&root, "org.example.good", "ok");
        write(
            &root.join("org.example.bad"),
            "template.toml",
            "schema = 1\n",
        );
        let c = discover(&only(&[(&root, Source::Project)]));
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(
            c.entries
                .iter()
                .filter(|e| e.source != Source::Shipped)
                .count(),
            1
        );
        assert_eq!(c.problems.len(), 1);
        assert!(
            c.problems[0].contains("org.example.bad"),
            "{:?}",
            c.problems
        );
        assert!(
            c.problems[0].contains("id"),
            "the reason is included: {:?}",
            c.problems
        );
    }

    #[test]
    fn a_missing_root_is_skipped_silently() {
        let c = discover(&only(&[(
            Path::new("/nonexistent-textrill-root"),
            Source::Explicit,
        )]));
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        assert_eq!(c.entries.len(), library::LIBRARY.len());
    }

    #[test]
    fn a_dot_directory_under_a_root_is_skipped_not_reported() {
        // `install` stages into `.install-*` under the same root it installs
        // into; discovery must pass it by rather than fail to load it.
        let base = scratch("dotdir");
        let root = base.join("templates");
        pack(&root, "org.example.real", "ok");
        write(
            &root.join(".install-123-4"),
            "template.toml",
            "schema = 1\n",
        );
        write(&root.join(".DS_Store"), "junk", "not a directory");
        let c = discover(&only(&[(&root, Source::Project)]));
        let _ = std::fs::remove_dir_all(&base);
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        assert!(c.resolve("org.example.real:ok").is_ok());
        assert_eq!(c.entries.len(), 1 + library::LIBRARY.len());
    }

    #[test]
    fn pack_subdirectories_are_visited_in_sorted_order() {
        let base = scratch("sorted");
        let root = base.join("templates");
        pack(&root, "org.example.zulu", "z");
        pack(&root, "org.example.alpha", "a");
        let c = discover(&only(&[(&root, Source::Project)]));
        let _ = std::fs::remove_dir_all(&base);
        let ids: Vec<&str> = c
            .entries
            .iter()
            .filter(|e| e.source != Source::Shipped)
            .map(|e| e.pack_id.as_str())
            .collect();
        assert_eq!(ids, vec!["org.example.alpha", "org.example.zulu"]);
    }

    #[test]
    fn one_pack_with_several_templates_yields_one_entry_each() {
        let base = scratch("multi");
        let root = base.join("templates");
        let p = root.join("org.example.multi");
        write(
            &p,
            "template.toml",
            "schema = 1\nid = \"org.example.multi\"\nname = \"M\"\n\
             version = \"1.0.0\"\nlicense = \"GPL-3.0-or-later\"\n\
             tags = [\"x\"]\n\n[[templates]]\nfile = \"one.html\"\n\
             model = \"body\"\n\n[[templates]]\nfile = \"two.html\"\n\
             model = \"document\"\n",
        );
        write(&p, "one.html", "{{textrill:content}}");
        write(&p, "two.html", "{{textrill:content}}");
        let c = discover(&only(&[(&root, Source::Project)]));
        let _ = std::fs::remove_dir_all(&base);
        let mine: Vec<(&str, Model)> = c
            .entries
            .iter()
            .filter(|e| e.source != Source::Shipped)
            .map(|e| (e.name.as_str(), e.model))
            .collect();
        assert_eq!(mine, vec![("one", Model::Body), ("two", Model::Document)]);
        let e = c.resolve("org.example.multi:two").unwrap();
        assert_eq!(e.tags, vec!["x".to_string()]);
        assert_eq!(e.pack_name, "M");
    }

    #[test]
    fn a_shipped_entry_reads_out_of_the_library() {
        let c = discover(&only(&[]));
        let html = c.resolve("book").unwrap().html().unwrap();
        assert_eq!(html, library::get("book").unwrap().html);
    }
}

// ------------------------------------------------------- catalog indexes --
//
// Roadmap §6: people share *libraries* of packs by moving files around — git,
// a shared drive, email — and the tool only ever reads what is already on
// disk. A catalog is a directory with a `catalog.toml` index listing the
// packs it carries (id, versions, sha256 of each archive, min_textrill,
// licence, tags, description). The index is read with the same strict TOML
// subset as a pack manifest, so an index written as full TOML fails loudly
// with its line number rather than being half-understood.

/// The index file a catalog directory must contain.
pub const CATALOG_INDEX: &str = "catalog.toml";

/// The engine's own version, the thing `min_textrill` is compared against.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One pack listed by a catalog index. The archive is a deterministic `.tar`
/// already on disk; nothing here can name a URL, because the tool never
/// fetches. `archive` is the resolved path, checked at load time to live
/// inside the catalog directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CatalogIndexEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub tags: Vec<String>,
    /// Lowercase hex SHA-256 of the archive, verified on install.
    pub sha256: Option<String>,
    /// The oldest engine version this pack supports, `major[.minor[.patch]]`.
    pub min_textrill: Option<String>,
    /// The archive's path, already confined to the catalog directory.
    pub archive: PathBuf,
    /// The catalog directory this entry came from.
    pub catalog: PathBuf,
}

/// One registered catalog directory and everything its index said — good
/// entries and problems alike. Loading never fails: a broken index is a
/// problem recorded against its catalog, so one bad file cannot hide the
/// rest, matching how pack discovery treats a broken pack.
#[derive(Debug, Clone, Default)]
pub struct CatalogIndex {
    pub path: PathBuf,
    pub entries: Vec<CatalogIndexEntry>,
    pub problems: Vec<String>,
}

impl CatalogIndex {
    /// Resolve one entry by `id` or `id@version`, in index order. A bare id
    /// with several versions listed takes the first; an exact version pins it,
    /// and a miss names the versions that *are* listed.
    pub fn find(&self, spec: &str) -> Result<&CatalogIndexEntry, String> {
        let (id, want) = match spec.split_once('@') {
            Some((id, ver)) => (id, Some(ver)),
            None => (spec, None),
        };
        let mut versions = Vec::new();
        for e in &self.entries {
            if e.id != id {
                continue;
            }
            match want {
                Some(v) if v != e.version => versions.push(e.version.as_str()),
                _ => return Ok(e),
            }
        }
        let listed = if versions.is_empty() {
            String::new()
        } else {
            format!("; it lists {}", versions.join(", "))
        };
        match want {
            Some(v) => Err(format!(
                "no version {v} of {id} in {}{listed}",
                self.path.display()
            )),
            None => Err(format!("no pack {id} in {}", self.path.display())),
        }
    }
}

/// Load every `--catalog_dir` in the search path. A missing directory or a
/// missing/unreadable/malformed index becomes a problem on that catalog, and
/// every other catalog still loads.
pub fn load_catalogs(sp: &SearchPath) -> Vec<CatalogIndex> {
    let mut out = Vec::new();
    for dir in &sp.catalog_dirs {
        let mut problems = Vec::new();
        let mut entries = Vec::new();
        let index_path = dir.join(CATALOG_INDEX);
        if !dir.is_dir() {
            problems.push(format!("{}: not a directory", dir.display()));
        } else if !index_path.is_file() {
            problems.push(format!("{}: no {CATALOG_INDEX} in it", dir.display()));
        } else {
            match fs::read_to_string(&index_path) {
                Ok(text) => match parse_index(&text, dir) {
                    Ok(parsed) => entries = parsed,
                    Err(msg) => problems.push(format!("{}: {msg}", index_path.display())),
                },
                Err(e) => problems.push(format!("{}: {e}", index_path.display())),
            }
        }
        out.push(CatalogIndex {
            path: dir.clone(),
            entries,
            problems,
        });
    }
    out
}

/// The top-level keys the index subset knows.
const INDEX_TOP_KEYS: &str = "schema";

/// The keys one `[[packs]]` entry knows.
const PACK_KEYS: &str = "id, name, version, description, authors, license, \
                         tags, sha256, min_textrill, archive";

/// Parse a `catalog.toml` document against the directory that holds it.
/// Errors read `line N: message`, exactly like a pack manifest.
fn parse_index(text: &str, base: &Path) -> Result<Vec<CatalogIndexEntry>, String> {
    let mut entries: Vec<CatalogIndexEntry> = Vec::new();
    let mut schema: Option<i64> = None;
    let mut current: Option<(usize, CatalogIndexEntry)> = None;
    for (idx, line) in text.lines().enumerate() {
        let n = idx + 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(rest) = t.strip_prefix("[[") {
            if let Some((header, open)) = current.take() {
                entries.push(finish_entry(open, header)?);
            }
            let name = section_name(rest, n)?;
            if name != "packs" {
                return err(
                    n,
                    format!(
                        "unsupported section `[[{name}]]`; a catalog index has \
                         [[packs]] only"
                    ),
                );
            }
            current = Some((n, CatalogIndexEntry::default()));
            continue;
        }
        if t.starts_with('[') {
            return err(
                n,
                format!(
                    "`{t}` is not in the TOML subset: only the [[packs]] \
                     array-of-table form is allowed in a catalog index"
                ),
            );
        }
        let Some(eq) = t.find('=') else {
            return err(n, format!("expected `key = value`, got `{t}`"));
        };
        let key = t[..eq].trim();
        check_key(key, n)?;
        let value = Value::parse(&t[eq + 1..], n)?;
        match current.as_mut() {
            None => set_index_top(&mut schema, key, value, n)?,
            Some((_, entry)) => set_pack_key(entry, key, value, n, base)?,
        }
    }
    if let Some((header, open)) = current.take() {
        entries.push(finish_entry(open, header)?);
    }
    Ok(entries)
}

/// Place one `key = value` line above every `[[packs]]` section.
fn set_index_top(schema: &mut Option<i64>, key: &str, v: Value, n: usize) -> Result<(), String> {
    match key {
        "schema" => {
            if schema.is_some() {
                return err(n, "duplicate key `schema`");
            }
            let Value::Int(i) = v else {
                return err(n, "`schema` expects an integer value");
            };
            if i != 1 {
                return err(n, format!("unknown schema {i}; this reader knows 1"));
            }
            *schema = Some(i);
        }
        "packs" => {
            return err(
                n,
                "pack entries belong in a `[[packs]]` section, not a `packs = …` value",
            )
        }
        other => {
            return err(
                n,
                format!("unknown key `{other}`; a catalog index has: {INDEX_TOP_KEYS}"),
            )
        }
    }
    Ok(())
}

/// Place one `key = value` line in the current `[[packs]]` entry.
fn set_pack_key(
    entry: &mut CatalogIndexEntry,
    key: &str,
    v: Value,
    n: usize,
    base: &Path,
) -> Result<(), String> {
    let want_str = |v: Value| match v {
        Value::Str(s) => Ok(s),
        _ => err(n, format!("`{key}` expects a string value")),
    };
    let want_strs = |v: Value| match v {
        Value::Strs(s) => Ok(s),
        _ => err(
            n,
            format!("`{key}` expects an array of strings, like [\"a\", \"b\"]"),
        ),
    };
    match key {
        "id" | "name" | "version" | "description" => {
            let slot = match key {
                "id" => &mut entry.id,
                "name" => &mut entry.name,
                "version" => &mut entry.version,
                _ => &mut entry.description,
            };
            if !slot.is_empty() {
                return err(n, format!("duplicate key `{key}`"));
            }
            *slot = want_str(v)?;
        }
        "authors" | "tags" => {
            let slot = if key == "authors" {
                &mut entry.authors
            } else {
                &mut entry.tags
            };
            if !slot.is_empty() {
                return err(n, format!("duplicate key `{key}`"));
            }
            *slot = want_strs(v)?;
        }
        "license" | "sha256" | "min_textrill" => {
            let slot = match key {
                "license" => &mut entry.license,
                "sha256" => &mut entry.sha256,
                _ => &mut entry.min_textrill,
            };
            if slot.is_some() {
                return err(n, format!("duplicate key `{key}`"));
            }
            *slot = Some(want_str(v)?);
        }
        "archive" => {
            if !entry.archive.as_os_str().is_empty() {
                return err(n, "duplicate key `archive`");
            }
            // Confine the path now, while the line number is still in hand.
            entry.archive = safe_archive(&want_str(v)?, n, base)?;
        }
        other => {
            return err(
                n,
                format!("unknown key `{other}`; a [[packs]] entry has: {PACK_KEYS}"),
            )
        }
    }
    Ok(())
}

/// Close a `[[packs]]` entry: every field the schema requires must be there,
/// and the values that have a shape (sha256, min_textrill) must have it. The
/// error names the entry's own `[[packs]]` header line.
fn finish_entry(entry: CatalogIndexEntry, header: usize) -> Result<CatalogIndexEntry, String> {
    for (field, value) in [
        ("id", &entry.id),
        ("name", &entry.name),
        ("version", &entry.version),
    ] {
        if value.is_empty() {
            return err(header, format!("this [[packs]] entry is missing `{field}`"));
        }
    }
    if entry.archive.as_os_str().is_empty() {
        return err(
            header,
            "this [[packs]] entry is missing `archive` (a catalog always \
             points at a file it carries; the tool never fetches)",
        );
    }
    if let Some(hash) = &entry.sha256 {
        if !is_sha256_hex(hash) {
            return err(
                header,
                format!("`sha256` must be 64 hexadecimal digits, got {hash:?}"),
            );
        }
    }
    if let Some(min) = &entry.min_textrill {
        if let Err(e) = parse_version(min) {
            return err(header, format!("`min_textrill`: {e}"));
        }
    }
    Ok(entry)
}

/// Confine an `archive` value to the catalog directory: a plain relative path
/// with no `..`, no absolute root, no Windows-style separators. The resolved
/// path is stored, so later use cannot re-widen it.
fn safe_archive(rel: &str, n: usize, base: &Path) -> Result<PathBuf, String> {
    use std::path::Component;
    if rel.contains('\\') {
        return err(n, "`archive` must use `/` separators");
    }
    let p = Path::new(rel);
    if p.is_absolute() {
        return err(n, "`archive` must be relative to the catalog directory");
    }
    for c in p.components() {
        if !matches!(c, Component::Normal(_)) {
            return err(
                n,
                format!("`archive` {rel:?} escapes the catalog directory"),
            );
        }
    }
    Ok(base.join(p))
}

/// A 64-digit hex string: a SHA-256 as written by catalog tooling.
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parse the leading `major[.minor[.patch]]` of a version for comparison.
/// A component that is not a number is refused, loudly: a `min_textrill`
/// that cannot be compared must not be compared silently.
pub fn parse_version(v: &str) -> Result<[u64; 3], String> {
    let bad = || format!("{v:?} is not a major[.minor[.patch]] version");
    let mut out = [0u64; 3];
    let mut parts = v.trim().split('.');
    for slot in out.iter_mut() {
        let Some(p) = parts.next() else { break };
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        *slot = p.parse().map_err(|_| bad())?;
    }
    if let Some(rest) = parts.next() {
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
    }
    Ok(out)
}

/// Whether the engine `have` satisfies a `min_textrill` requirement.
pub fn version_satisfies(have: &str, min: &str) -> Result<bool, String> {
    Ok(parse_version(have)? >= parse_version(min)?)
}

#[cfg(test)]
mod index_tests {
    use super::*;

    fn base() -> PathBuf {
        PathBuf::from("/cat")
    }

    fn parse(text: &str) -> Result<Vec<CatalogIndexEntry>, String> {
        parse_index(text, &base())
    }

    fn rejects(text: &str, line: usize, fragments: &[&str]) {
        let got = parse(text).expect_err("expected a hard error");
        assert!(
            got.contains(&format!("line {line}:")),
            "expected `line {line}:`, got: {got}"
        );
        for f in fragments {
            assert!(got.contains(f), "expected `{f}` in: {got}");
        }
    }

    const GOOD: &str = "\
schema = 1

[[packs]]
id = \"org.example.book\"
name = \"Academic Book\"
version = \"1.2.0\"
description = \"A print-ready skeleton.\"
authors = [\"A. Scholar <a@example.org>\"]
license = \"CC0-1.0\"
tags = [\"book\", \"print\"]
sha256 = \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\"
min_textrill = \"0.1.0\"
archive = \"packs/book-1.2.0.tar\"
";

    #[test]
    fn reads_the_documented_index() {
        let list = parse(GOOD).unwrap();
        assert_eq!(list.len(), 1);
        let e = &list[0];
        assert_eq!(e.id, "org.example.book");
        assert_eq!(e.name, "Academic Book");
        assert_eq!(e.version, "1.2.0");
        assert_eq!(e.description, "A print-ready skeleton.");
        assert_eq!(e.authors, vec!["A. Scholar <a@example.org>"]);
        assert_eq!(e.license.as_deref(), Some("CC0-1.0"));
        assert_eq!(e.tags, vec!["book", "print"]);
        assert_eq!(
            e.sha256.as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert_eq!(e.min_textrill.as_deref(), Some("0.1.0"));
        assert_eq!(e.archive, PathBuf::from("/cat/packs/book-1.2.0.tar"));
    }

    #[test]
    fn the_empty_index_lists_nothing() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("# nothing\n").unwrap().is_empty());
    }

    #[test]
    fn a_hash_inside_a_string_is_content() {
        let list = parse(
            "[[packs]]\nid = \"x.y\"\nname = \"X\"\ndescription = \"a # b\"\n\
             version = \"1.0.0\"\narchive = \"a.tar\"\n",
        )
        .unwrap();
        assert_eq!(list[0].description, "a # b");
    }

    #[test]
    fn rejects_an_unknown_top_level_key() {
        rejects(
            "colour = \"red\"\n",
            1,
            &["colour", "unknown key", INDEX_TOP_KEYS],
        );
    }

    #[test]
    fn rejects_an_unknown_pack_key() {
        rejects(
            "[[packs]]\nid = \"x.y\"\ncolour = \"red\"\n",
            3,
            &["colour", "unknown key", PACK_KEYS],
        );
    }

    #[test]
    fn rejects_a_table_and_an_unsupported_section() {
        rejects("[packs]\n", 1, &["[packs]", "subset"]);
        rejects("[[notes]]\n", 1, &["[[notes]]", "unsupported section"]);
    }

    #[test]
    fn rejects_a_duplicate_key() {
        rejects(
            "[[packs]]\nid = \"x.y\"\nid = \"a.b\"\n",
            3,
            &["duplicate key `id`"],
        );
    }

    #[test]
    fn a_missing_required_field_names_the_header_line() {
        rejects(
            "[[packs]]\nid = \"x.y\"\nname = \"X\"\nversion = \"1.0.0\"\n",
            1,
            &["line 1:", "missing `archive`"],
        );
        rejects(
            "[[packs]]\nname = \"X\"\narchive = \"a.tar\"\n",
            1,
            &["missing `id`"],
        );
    }

    #[test]
    fn an_absolute_or_escaping_archive_is_refused() {
        rejects(
            "[[packs]]\nid = \"x.y\"\nname = \"X\"\nversion = \"1.0.0\"\n\
             archive = \"/etc/passwd\"\n",
            5,
            &["relative to the catalog"],
        );
        rejects(
            "[[packs]]\nid = \"x.y\"\nname = \"X\"\nversion = \"1.0.0\"\n\
             archive = \"../outside.tar\"\n",
            5,
            &["escapes the catalog"],
        );
    }

    #[test]
    fn a_bad_sha256_or_min_textrill_is_refused() {
        rejects(
            "[[packs]]\nid = \"x.y\"\nname = \"X\"\nversion = \"1.0.0\"\n\
             sha256 = \"beef\"\narchive = \"a.tar\"\n",
            1,
            &["64 hexadecimal"],
        );
        rejects(
            "[[packs]]\nid = \"x.y\"\nname = \"X\"\nversion = \"1.0.0\"\n\
             min_textrill = \"one\"\narchive = \"a.tar\"\n",
            1,
            &["min_textrill"],
        );
    }

    #[test]
    fn an_unknown_schema_is_refused() {
        rejects("schema = 2\n", 1, &["schema 2"]);
        rejects("schema = \"1\"\n", 1, &["`schema` expects an integer"]);
    }

    #[test]
    fn find_pins_a_version_and_names_what_is_listed() {
        let two = GOOD.to_string()
            + "\n[[packs]]\nid = \"org.example.book\"\nname = \"Academic Book\"\n\
               version = \"2.0.0\"\narchive = \"packs/book-2.0.0.tar\"\n";
        let idx = CatalogIndex {
            path: base(),
            entries: parse(&two).unwrap(),
            problems: Vec::new(),
        };
        assert_eq!(idx.find("org.example.book").unwrap().version, "1.2.0");
        assert_eq!(
            idx.find("org.example.book@2.0.0").unwrap().archive,
            PathBuf::from("/cat/packs/book-2.0.0.tar")
        );
        let e = idx.find("org.example.book@3.0.0").unwrap_err();
        assert!(e.contains("no version 3.0.0"), "{e}");
        assert!(e.contains("1.2.0") && e.contains("2.0.0"), "{e}");
        assert!(idx
            .find("org.example.missing")
            .unwrap_err()
            .contains("no pack"));
    }

    #[test]
    fn versions_compare_by_their_leading_triple() {
        assert!(version_satisfies("0.1.0", "0.1.0").unwrap());
        assert!(version_satisfies("0.1.1", "0.1").unwrap());
        assert!(version_satisfies("1.0.0", "0.9.9").unwrap());
        assert!(!version_satisfies("0.1.0", "99.0.0").unwrap());
        assert!(parse_version("1.2").is_ok());
        assert!(parse_version("1.2.3.4").is_ok());
        assert!(parse_version("").is_err());
        assert!(parse_version("1.x").is_err());
        assert!(parse_version("v1.0").is_err());
    }

    #[test]
    fn sha256_shapes_are_recognised() {
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(is_sha256_hex(&"AB01".repeat(16)));
        assert!(!is_sha256_hex("beef"));
        assert!(!is_sha256_hex(&"g".repeat(64)));
    }
}
