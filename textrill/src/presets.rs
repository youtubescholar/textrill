// textrill — convert plain text to HTML.
//
// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! User presets (roadmap §8.1): a named bundle of options in
//! `$XDG_CONFIG_HOME/textrill/presets.toml`, expanded by `--preset NAME` on
//! the command line and offered in the GUI's preset strip beside the shipped
//! starting points.
//!
//! A preset is *only options*: no modes, no hidden state. The file is the
//! strict TOML subset the packs and catalogs already use (line-oriented
//! `key = value`, `[[presets]]` sections, `line N:` diagnostics), each preset's
//! keys are applied onto the engine defaults, and applying a preset writes
//! that whole option state — so the GUI and the command line produce the
//! *same* `Options` from the same file, which is what makes the "Modified"
//! marker and the acceptance check mean one thing.
//!
//! Precedence (§8.1): `--preset` on the command line is expanded **above the
//! rc files and below the remaining command-line options**, so it is scanned
//! before the command line is parsed and the whole command line then applies
//! on top. `--preset` inside an rc file or `@file` group is refused with a
//! message saying where it belongs, rather than half-working with file-order
//! semantics.
//!
//! Covered by the tests below and by `tests/presets.rs`.

use std::path::{Path, PathBuf};

use crate::cli::{self, Kind, Opt};
use crate::options::Options;
use crate::pack::tomlsub::{check_key, err, section_name, Value};

/// The relative config path the file lives at, under `$XDG_CONFIG_HOME` or
/// `$HOME/.config`. Exposed so messages and tests name the same file.
pub const CONFIG_REL: &str = "textrill/presets.toml";

/// One named preset as the file describes it.
#[derive(Debug, Clone)]
pub struct Preset {
    /// The stable key `--preset` and the GUI's settings store look up.
    pub key: String,
    /// The name shown on the button; defaults to the key.
    pub name: String,
    /// One-line explanation shown when the button is hovered.
    pub description: String,
    /// The whole option state applying this preset produces: engine defaults
    /// with the bundle's keys applied, exactly what `--preset` writes.
    pub options: Options,
}

/// Where [`load`] reads: `$XDG_CONFIG_HOME/textrill/presets.toml`, else
/// `$HOME/.config/textrill/presets.toml`, else the config path with an empty
/// base (so a message still names a file).
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    base.unwrap_or_default().join(CONFIG_REL)
}

/// Read the user presets file. A missing file is an empty list, not an error:
/// most installations have no presets yet.
pub fn load() -> Result<Vec<Preset>, String> {
    load_from(&config_path())
}

/// [`load`] from an explicit path, for tests and for a caller that already
/// knows the file.
pub fn load_from(path: &Path) -> Result<Vec<Preset>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// The preset called `name`, if the list carries it.
pub fn find<'a>(presets: &'a [Preset], name: &str) -> Option<&'a Preset> {
    presets.iter().find(|p| p.key == name)
}

/// Replace `opts` with the state of the preset called `name`, or fail loud
/// naming the file and the keys it does carry.
pub fn apply(opts: &mut Options, name: &str) -> Result<(), String> {
    let presets = load()?;
    match find(&presets, name) {
        Some(preset) => {
            *opts = preset.options.clone();
            Ok(())
        }
        None => {
            let path = config_path();
            let keys: Vec<&str> = presets.iter().map(|p| p.key.as_str()).collect();
            Err(if keys.is_empty() {
                format!("no preset `{name}` ({} has no presets)", path.display())
            } else {
                format!(
                    "no preset `{name}`; {} has: {}",
                    path.display(),
                    keys.join(", ")
                )
            })
        }
    }
}

/// The keys `--preset` accepts, in file order, for a diagnostic.
pub fn keys(presets: &[Preset]) -> Vec<&str> {
    presets.iter().map(|p| p.key.as_str()).collect()
}

/// Which section a `key = value` line belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Top,
    Preset,
}

/// Parse a `presets.toml` document.
///
/// Errors read `line N: message`. A preset's identity keys (`key`, `name`,
/// `description`) are reserved; every other key is a textrill option applied
/// onto the defaults, so a typo is refused rather than silently ignored.
pub fn parse(input: &str) -> Result<Vec<Preset>, String> {
    let mut presets: Vec<Preset> = Vec::new();
    let mut schema: Option<i64> = None;
    let mut section = Section::Top;
    // The option keys already set on the current preset, so a repeated key is
    // refused the way TOML refuses one rather than silently taking the last.
    let mut seen_options: Vec<String> = Vec::new();
    for (idx, line) in input.lines().enumerate() {
        let n = idx + 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(rest) = t.strip_prefix("[[") {
            let name = section_name(rest, n)?;
            if name != "presets" {
                return err(
                    n,
                    format!("unsupported section `[[{name}]]`; the file has [[presets]] only"),
                );
            }
            presets.push(Preset {
                key: String::new(),
                name: String::new(),
                description: String::new(),
                options: Options::default(),
            });
            seen_options.clear();
            section = Section::Preset;
            continue;
        }
        if t.starts_with('[') {
            return err(
                n,
                format!(
                    "`{t}` is not in the presets subset: only the [[presets]] \
                     array-of-table form is allowed"
                ),
            );
        }
        let Some(eq) = t.find('=') else {
            return err(n, format!("expected `key = value`, got `{t}`"));
        };
        let key = t[..eq].trim();
        check_key(key, n)?;
        let value = Value::parse(&t[eq + 1..], n)?;
        match section {
            Section::Top => {
                if key != "schema" {
                    return err(
                        n,
                        format!(
                            "`{key}` belongs in a [[presets]] entry; the file top \
                             has `schema` only"
                        ),
                    );
                }
                if schema.is_some() {
                    return err(n, "duplicate key `schema`");
                }
                schema = Some(match value {
                    Value::Int(i) => i,
                    _ => return err(n, "`schema` expects an integer"),
                });
            }
            Section::Preset => {
                let last = presets
                    .last_mut()
                    .expect("the Preset section implies a pushed entry");
                set_preset_key(last, key, &value, n)?;
                if matches!(key, "key" | "name" | "description") {
                    continue;
                }
                if seen_options.iter().any(|k| k == key) {
                    return err(n, format!("duplicate key `{key}`"));
                }
                let Some(opt) = cli::lookup(key) else {
                    return err(
                        n,
                        format!("`{key}` is not a textrill option; see `textrill --help`"),
                    );
                };
                apply_value(&mut last.options, opt, &value, n)?;
                seen_options.push(opt.canonical().to_string());
            }
        }
    }
    match schema {
        None => return Err("`schema` is required (schema = 1)".to_string()),
        Some(1) => {}
        Some(other) => return Err(format!("unknown schema {other}; this build understands 1")),
    }
    for preset in &presets {
        if preset.key.is_empty() {
            return Err("`[[presets]]` entries need a `key`".to_string());
        }
    }
    // A key may appear once: two presets answering to one name would make
    // `--preset` order-dependent. The key names are known by now, so a
    // duplicate is reported against the name rather than a line number.
    for (i, preset) in presets.iter().enumerate() {
        if presets[..i].iter().any(|p| p.key == preset.key) {
            return Err(format!("duplicate preset key `{}`", preset.key));
        }
    }
    Ok(presets)
}

/// Place one `key = value` line on the current preset: `key`, `name` and
/// `description` are the identity; anything else is an option (the caller
/// decides).
fn set_preset_key(preset: &mut Preset, key: &str, value: &Value, n: usize) -> Result<(), String> {
    match key {
        "key" | "name" | "description" => {
            let slot = match key {
                "key" => &mut preset.key,
                "name" => &mut preset.name,
                _ => &mut preset.description,
            };
            if !slot.is_empty() {
                return err(n, format!("duplicate key `{key}`"));
            }
            match value {
                Value::Str(s) => {
                    *slot = s.clone();
                    Ok(())
                }
                _ => err(n, format!("`{key}` expects a string value")),
            }
        }
        _ => Ok(()),
    }
}

/// Apply one preset entry value onto the options, with the option's own type
/// rules and the line number in any message.
fn apply_value(opts: &mut Options, opt: Opt, value: &Value, n: usize) -> Result<(), String> {
    let at = |e: String| format!("`{}`: {e}", opt.canonical());
    if opt.kind() == Kind::StrArray {
        let list = match value {
            Value::Strs(list) => list,
            Value::Str(s) => std::slice::from_ref(s),
            other => {
                return err(
                    n,
                    format!(
                        "`{}` expects an array of strings, like [\"a\", \"b\"] \
                         (got {other:?})",
                        opt.canonical()
                    ),
                )
            }
        };
        cli::clear_array(opts, opt);
        for item in list {
            cli::set_opt(opts, opt, item).map_err(|e| format!("line {n}: {}", at(e)))?;
        }
        return Ok(());
    }
    let text = match value {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Strs(_) => {
            return err(
                n,
                format!("`{}` expects a single value, not an array", opt.canonical()),
            )
        }
    };
    cli::set_opt(opts, opt, &text).map_err(|e| format!("line {n}: {}", at(e)))
}

/// The token-aware `--preset` scan for a command line: the same token rules
/// [`cli::parse_args_from`] applies (values consume the next token, `--` ends
/// options), so `--title --preset` cannot be mistaken for a preset request.
/// The **last** `--preset` on the line wins; the parse that follows then
/// applies every command-line option on top of the expansion.
pub fn scan_preset(args: &[String]) -> Result<Option<String>, String> {
    let mut it = args.iter().peekable();
    let mut found: Option<String> = None;
    while let Some(arg) = it.next() {
        let (name, inline) = if let Some(rest) = arg.strip_prefix("--") {
            if rest.is_empty() {
                break;
            }
            match rest.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (rest.to_string(), None),
            }
        } else if let Some(rest) = arg.strip_prefix('-') {
            if rest.is_empty() {
                continue;
            }
            match rest.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (rest.to_string(), None),
            }
        } else {
            continue;
        };
        let Ok((opt, negated)) = cli::resolve(&name) else {
            // Unknown options are the real parse's to report; the scan only
            // needs enough shape to keep values out of its way.
            continue;
        };
        if negated && opt.kind() != Kind::Flag {
            continue;
        }
        if opt == Opt::Preset {
            if negated {
                continue;
            }
            let value = match inline {
                Some(v) => v,
                None => cli::take_value(&mut it, &name)?,
            };
            found = Some(value);
        } else if inline.is_none() && opt.kind() != Kind::Flag {
            // Consume the value token so it cannot read as an option.
            let _ = cli::take_value(&mut it, &name);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "schema = 1\n\n[[presets]]\nkey = \"study\"\n\
                        name = \"Study notes\"\ndescription = \"Notes with a TOC.\"\n\
                        section = true\ntemplate_library = \"article\"\n";

    #[test]
    fn a_preset_is_read_with_its_options_and_identity() {
        let presets = parse(GOOD).unwrap();
        assert_eq!(presets.len(), 1);
        let p = &presets[0];
        assert_eq!(p.key, "study");
        assert_eq!(p.name, "Study notes");
        assert_eq!(p.description, "Notes with a TOC.");
        assert!(p.options.section);
        assert_eq!(p.options.template_library, "article");
        assert_eq!(p.options.tab_width, Options::default().tab_width);
    }

    #[test]
    fn a_preset_is_a_whole_state_not_a_patch() {
        let presets = parse(GOOD).unwrap();
        // What `apply` writes: the bundle state built on defaults, replacing
        // whatever was, so a leftover option cannot leak through from an
        // earlier rc line or a previous preset.
        let opts = presets[0].options.clone();
        assert_eq!(opts.title, String::new(), "defaults, not leftovers");
        assert!(opts.section);
        assert_eq!(
            opts.tab_width,
            Options::default().tab_width,
            "keys the bundle omits stay at their defaults"
        );
    }

    #[test]
    fn unknown_option_and_missing_schema_are_loud() {
        let err = parse("schema = 1\n[[presets]]\nkey = \"x\"\nsectoin = true\n").unwrap_err();
        assert!(err.contains("line 4:"), "{err}");
        assert!(err.contains("`sectoin` is not a textrill option"), "{err}");

        let err = parse("[[presets]]\nkey = \"x\"\n").unwrap_err();
        assert!(err.contains("`schema` is required"), "{err}");

        let err = parse("schema = 2\n").unwrap_err();
        assert!(err.contains("unknown schema 2"), "{err}");
    }

    #[test]
    fn duplicate_preset_keys_and_options_are_refused() {
        let err =
            parse("schema = 1\n[[presets]]\nkey = \"x\"\n[[presets]]\nkey = \"x\"\n").unwrap_err();
        assert!(err.contains("duplicate preset key `x`"), "{err}");

        let err = parse(
            "schema = 1\n[[presets]]\nkey = \"x\"\n\
             template_library = \"article\"\ntemplate_library = \"book\"\n",
        )
        .unwrap_err();
        assert!(err.contains("duplicate key `template_library`"), "{err}");
    }

    #[test]
    fn identity_keys_need_strings_and_a_key_is_required() {
        let err = parse("schema = 1\n[[presets]]\nkey = 7\n").unwrap_err();
        assert!(err.contains("`key` expects a string"), "{err}");

        let err = parse("schema = 1\n[[presets]]\nname = \"x\"\n").unwrap_err();
        assert!(err.contains("need a `key`"), "{err}");

        let err = parse("schema = 1\ntitle = \"x\"\n").unwrap_err();
        assert!(err.contains("belongs in a [[presets]]"), "{err}");
    }

    #[test]
    fn wrong_typed_option_values_name_the_option_and_line() {
        let err =
            parse("schema = 1\n[[presets]]\nkey = \"x\"\ntab_width = \"wide\"\n").unwrap_err();
        assert!(err.contains("line 4:"), "{err}");
        assert!(err.contains("`tab_width`"), "{err}");
    }

    #[test]
    fn array_options_take_arrays_and_scalars_take_scalars() {
        let presets = parse(
            "schema = 1\n[[presets]]\nkey = \"x\"\ntemplate_dir = [\"a\", \"b\"]\n\
             var = [\"by=Ada\"]\n",
        )
        .unwrap();
        assert_eq!(presets[0].options.template_dir, ["a", "b"]);
        assert_eq!(
            presets[0].options.vars,
            vec![("by".to_string(), "Ada".to_string())]
        );
    }

    #[test]
    fn scan_preset_follows_the_command_line_token_rules() {
        let args: Vec<String> = ["textrill", "--preset", "book", "--tab_width", "2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(scan_preset(&args).unwrap().as_deref(), Some("book"));

        // A value that happens to be the word preset is not a request.
        let args: Vec<String> = ["--title", "--preset", "in", "--preset=book2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(scan_preset(&args).unwrap().as_deref(), Some("book2"));

        // Options end at `--`.
        let args: Vec<String> = ["--", "--preset"].iter().map(|s| s.to_string()).collect();
        assert_eq!(scan_preset(&args).unwrap(), None);

        let args: Vec<String> = ["--toc"].iter().map(|s| s.to_string()).collect();
        assert_eq!(scan_preset(&args).unwrap(), None);
    }
}
