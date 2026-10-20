//! Scaffolding a new template pack (roadmap §5.4, §5.5).
//!
//! `textrill templates new` and the GUI Template Manager both need to create a
//! fresh, already-valid pack: one `template.toml` plus one template file. The
//! rules for turning a user's word into a pack id and a file name, and the
//! minimal manifest and template that validate against §5.3 with no edits, live
//! here so the two front ends cannot drift apart.

use std::path::PathBuf;

use super::validate::MANIFEST;

/// A pack or file name from a user's word: lowercase, with runs of anything
/// else folded to a single `-`. `None` when nothing usable is left.
pub fn sanitize(word: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_dash = false;
    for c in word.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// A reverse-DNS id for a scaffolded pack, from `cwd`'s components (reversed,
/// sanitized, non-numeric) plus `leaf`. An empty prefix (say, a scratch
/// directory of digits) becomes the label `local`, which keeps the id valid and
/// readable. `cwd` is passed in rather than read from the environment so the
/// caller — and its tests — control the result.
pub fn scaffold_id(cwd: &std::path::Path, leaf: &str) -> String {
    let mut labels: Vec<String> = cwd
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => sanitize(&s.to_string_lossy()),
            _ => None,
        })
        .filter(|s| s.chars().any(|c| c.is_ascii_alphabetic()))
        .collect();
    labels.reverse();
    labels.push(leaf.to_string());
    if labels.len() < 2 {
        labels.insert(0, "local".to_string());
    }
    labels.join(".")
}

/// The minimal manifest a scaffolded pack needs, valid against §5.3 with no
/// edits: one document-model template using only the fixed `title` slot.
pub fn manifest(id: &str, leaf: &str) -> String {
    format!(
        "schema = 1\n\
         id = {id:?}\n\
         name = {name:?}\n\
         version = \"0.1.0\"\n\
         license = \"GPL-3.0-or-later\"\n\
         description = \"A scaffolded textrill template pack.\"\n\
         \n\
         [[templates]]\n\
         file = {file:?}\n\
         model = \"document\"\n",
        name = format!("{leaf} templates"),
        file = format!("{leaf}.html"),
    )
}

/// A minimal whole-document template: the required `content` slot plus a title,
/// so it validates and converts out of the box.
pub fn html(leaf: &str) -> String {
    format!(
        "<!DOCTYPE html>\n\
         <html>\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <title>{{{{textrill:title}}}}</title>\n\
         </head>\n\
         <body>\n\
         <main class=\"{leaf}\">\n\
         {{{{textrill:content}}}}\n\
         </main>\n\
         </body>\n\
         </html>\n"
    )
}

/// The directory a scaffold for `leaf` under `root` would occupy, and the pack
/// id it carries. Used by both front ends before anything is written.
pub struct Scaffold {
    pub id: String,
    pub leaf: String,
    pub dir: PathBuf,
}

/// Plan a scaffold: `id` from `root`'s parent context and `leaf`, the pack
/// directory `root/<id>`, and the template stem. `None` when the word has no
/// usable characters.
pub fn plan(root: &std::path::Path, cwd: &std::path::Path, word: &str) -> Option<Scaffold> {
    let leaf = sanitize(word)?;
    let id = scaffold_id(cwd, &leaf);
    let dir = root.join(&id);
    Some(Scaffold { id, leaf, dir })
}

/// Write a scaffold into `Scaffold::dir`, creating it. `Scaffold::dir` must not
/// already exist (the caller checks, to give a better message). Returns the
/// manifest and template file names for a caller to report.
pub fn write(scaffold: &Scaffold) -> Result<(), String> {
    let dir = &scaffold.dir;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let manifest_path = dir.join(MANIFEST);
    let html_path = dir.join(format!("{}.html", scaffold.leaf));
    std::fs::write(&manifest_path, manifest(&scaffold.id, &scaffold.leaf))
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    std::fs::write(&html_path, html(&scaffold.leaf))
        .map_err(|e| format!("{}: {e}", html_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn sanitize_folds_runs_and_trims() {
        assert_eq!(sanitize("My Book!!").as_deref(), Some("my-book"));
        assert_eq!(sanitize("--weird__name--").as_deref(), Some("weird-name"));
        assert_eq!(sanitize("!!!").as_deref(), None);
        assert_eq!(sanitize("").as_deref(), None);
    }

    #[test]
    fn scaffold_id_is_reverse_dns_from_the_cwd() {
        let id = scaffold_id(Path::new("/home/u/projects/book"), "layout");
        assert_eq!(id, "book.projects.u.home.layout");
    }

    #[test]
    fn scaffold_id_falls_back_to_local_for_a_bare_scratch_dir() {
        let id = scaffold_id(Path::new("/123/456"), "layout");
        assert_eq!(id, "local.layout");
    }

    /// The manifest and template a scaffold writes validate; the plan names the
    /// id and the directory under the root.
    #[test]
    fn a_scaffold_validates_and_plans_its_directory() {
        let base = std::env::temp_dir().join(format!("textrill-scaffold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let plan = plan(&base, Path::new("/home/u/book"), "My Theme").unwrap();
        assert_eq!(plan.leaf, "my-theme");
        assert_eq!(plan.id, "book.u.home.my-theme");
        assert_eq!(plan.dir, base.join("book.u.home.my-theme"));
        write(&plan).unwrap();
        let validated = super::super::validate::load(&plan.dir).unwrap();
        assert_eq!(validated.id, plan.id);
        assert_eq!(validated.templates.len(), 1);
        assert_eq!(validated.templates[0].name, "my-theme");
        let _ = std::fs::remove_dir_all(&base);
    }
}
