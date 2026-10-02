//! Regressions for Perl semantics that are easy to get wrong, plus the
//! command line option lookup rules of `Getopt::Long`.

use textrill::cli;
use textrill::options::Options;

// ---------------------------------------------------------------- options --

#[test]
fn default_doctype_contains_a_newline() {
    // The Perl default is written across two source lines, so the emitted
    // doctype spans two output lines.
    let o = Options::default();
    assert_eq!(
        o.doctype,
        "-//W3C//DTD HTML 4.01//EN\"\n\"http://www.w3.org/TR/html4/strict.dtd"
    );
}

#[test]
fn default_delimiters() {
    let o = Options::default();
    assert_eq!(o.bold_delimiter, "#");
    assert_eq!(o.italic_delimiter, "*");
    assert_eq!(o.bullets, "-=o*\u{b7}");
    assert_eq!(o.bullets_ordered, "");
    // The module defaults to XHTML output, and turns lower_case_tags on with
    // it; the non-XHTML path has to be requested explicitly.
    assert!(o.xhtml);
    assert!(!o.lower_case_tags);
    assert!(!o.extract);
    assert!(!o.make_tables);
    assert!(o.make_anchors);
    assert!(o.make_links);
    assert_eq!(o.caps_tag, "STRONG");
    assert_eq!(o.min_caps_length, 3);
    assert_eq!(o.short_line_length, 40);
    assert_eq!(o.hrule_min, 4);
    assert_eq!(o.tab_width, 8);
    assert_eq!(o.outfile, "-");
}

// ------------------------------------------------------------ CLI lookups --

/// Getopt::Long accepts an unambiguous abbreviation, but an exact option name
/// always wins even when it is also the prefix of a longer one: `--bullets`
/// must mean `bullets`, not "ambiguous between bullets and bullets_ordered".
#[test]
fn exact_option_name_beats_prefix() {
    let name_of = |abbrev: &str| cli::lookup(abbrev).map(|spec| spec.names[0]);
    assert_eq!(name_of("bullets"), Some("bullets"));
    assert_eq!(name_of("bullets_ordered"), Some("bullets_ordered"));
    assert_eq!(name_of("bullet"), None, "still ambiguous");
    // aliases, including the case sensitive ones
    assert_eq!(name_of("ab"), Some("append_file"));
    assert_eq!(name_of("tw"), Some("tab_width"));
    assert_eq!(name_of("LO"), Some("link_only"));
    assert_eq!(name_of("l"), Some("links_dictionaries"));
    assert_eq!(name_of("nope"), None);
}

#[test]
fn every_alias_is_reachable() {
    for spec in cli::SPECS {
        for name in spec.names {
            let found = cli::lookup(name).unwrap_or_else(|| panic!("{name} not found"));
            assert_eq!(found.names[0], spec.names[0], "{name} points elsewhere");
        }
    }
}

#[test]
fn aliases_do_not_collide() {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for spec in cli::SPECS {
        for name in spec.names {
            if let Some((other, _)) = seen.iter().find(|(n, _)| *n == *name) {
                panic!("{name} is both {other} and {}", spec.names[0]);
            }
            seen.push((name, spec.names[0]));
        }
    }
}

#[test]
fn defaults_round_trip_through_get_value() {
    // A front end reads the defaults with `get_value` and writes them back
    // with `set_value`; that has to be lossless for every option.
    let defaults = Options::default();
    for spec in cli::SPECS {
        let name = spec.names[0];
        let text = cli::get_value(&defaults, name).unwrap();
        let mut copy = Options::default();
        cli::set_value(&mut copy, name, &text)
            .unwrap_or_else(|e| panic!("{name}: {text:?} could not be set: {e}"));
        assert_eq!(
            cli::get_value(&copy, name).unwrap(),
            text,
            "{name} did not survive the round trip"
        );
    }
}

#[test]
fn table_type_accepts_several_pairs() {
    // the command line repeats the option; a front end passes one string
    let mut o = Options::default();
    cli::set_table_type(&mut o, "BORDER=0 ALIGN=1").unwrap();
    assert!(o.table_type.align);
    assert!(!o.table_type.border);
    assert!(o.table_type.pgsql, "untouched types keep their default");
    assert!(cli::set_table_type(&mut o, "BORDER=2").is_err());
    assert!(cli::set_table_type(&mut o, "NOPE=1").is_err());
    assert!(cli::set_table_type(&mut o, "BORDER").is_err());
}
