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
