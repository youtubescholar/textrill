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

// --- P11: option files -------------------------------------------------------

mod p11_rcexamples {
    use super::*;
    use std::path::Path;

    fn parse(args: &[&str], root: &Path) -> Result<Options, String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let mut o = Options::default();
        cli::parse_args_with_rc(&args, &mut o, None, root)?;
        o.deal_with_options();
        Ok(o)
    }

    /// Writes `name` into `root`, creating the directory if needed.
    fn write(root: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join(name), body).unwrap();
    }

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("textrill-p11-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn current_dir_rc_is_read() {
        let d = tmpdir("cur");
        write(&d, ".txt2htmlrc", "--extract\n--bold_delimiter \"@\"\n");
        let o = parse(&["in.txt"], &d).unwrap();
        assert!(o.extract, "--extract from ./.txt2htmlrc");
        assert_eq!(o.bold_delimiter, "@");
        assert_eq!(o.infile, vec!["in.txt"], "the input file still lands");
    }

    #[test]
    fn home_rc_is_read() {
        let home = tmpdir("home");
        write(&home, ".txt2htmlrc", "--title \"From Home\"\n");
        let cwd = tmpdir("home-cwd");
        std::fs::create_dir_all(&cwd).unwrap();
        let args = vec!["in.txt".to_string()];
        let mut o = Options::default();
        cli::parse_args_with_rc(&args, &mut o, Some(&home), &cwd).unwrap();
        assert_eq!(o.title, "From Home");
    }

    #[test]
    fn command_line_overrides_both_rc_files() {
        // The documented precedence: @file < ~/.txt2htmlrc < ./.txt2htmlrc < CLI.
        let home = tmpdir("prec-home");
        write(
            &home,
            ".txt2htmlrc",
            "--title \"Home\"\n--bold_delimiter \"#\"\n",
        );
        let cwd = tmpdir("prec-cwd");
        write(&cwd, ".txt2htmlrc", "--title \"Current\"\n");
        write(&cwd, "opts.txt", "--title \"Group\"\n");
        let args: Vec<String> = ["@opts.txt", "in.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut o = Options::default();
        cli::parse_args_with_rc(&args, &mut o, Some(&home), &cwd).unwrap();
        assert_eq!(
            o.title, "Current",
            "./.txt2htmlrc should beat ~/.txt2htmlrc and @file"
        );
        assert_eq!(o.bold_delimiter, "#", "home rc still supplies the rest");

        // And the command line beats all three.
        let args: Vec<String> = ["@opts.txt", "--title", "CLI", "in.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut o2 = Options::default();
        cli::parse_args_with_rc(&args, &mut o2, Some(&home), &cwd).unwrap();
        assert_eq!(o2.title, "CLI");
    }

    #[test]
    fn at_file_group_is_expanded() {
        let d = tmpdir("at");
        write(&d, "opts.txt", "--title \"From Group\"\n--extract\n");
        let o = parse(&["@opts.txt", "in.txt"], &d).unwrap();
        assert_eq!(o.title, "From Group");
        assert!(o.extract);
        assert_eq!(o.infile, vec!["in.txt"], "@opts.txt is not an input file");
    }

    #[test]
    fn at_file_that_does_not_exist_is_an_error_not_an_input_file() {
        let d = tmpdir("at-missing");
        let err = parse(&["@nope.txt"], &d).unwrap_err();
        assert!(
            err.contains("nope.txt"),
            "the error should name the file: {err}"
        );
    }

    #[test]
    fn a_bad_option_reports_file_and_line() {
        // The main ergonomic win over upstream, which says only
        // "Unmatched ( in regex"-style messages with no location.
        let d = tmpdir("bad");
        write(&d, ".txt2htmlrc", "# fine\n--extract\n--nonsense_option\n");
        let err = parse(&["in.txt"], &d).unwrap_err();
        assert!(err.contains(":3:"), "expected a line number: {err}");
        assert!(err.contains("nonsense_option"), "{err}");
        assert!(err.contains(".txt2htmlrc"), "{err}");
    }

    #[test]
    fn comments_and_quotes_work() {
        let d = tmpdir("comments");
        write(
            &d,
            ".txt2htmlrc",
            "# leading comment\n\
             --title \"A # inside quotes\"   # trailing comment\n\
             --bold_delimiter '#'\n",
        );
        let o = parse(&["in.txt"], &d).unwrap();
        assert_eq!(o.title, "A # inside quotes");
        assert_eq!(o.bold_delimiter, "#");
    }

    #[test]
    fn double_dash_makes_the_rest_input_files() {
        let d = tmpdir("ddash");
        write(&d, ".txt2htmlrc", "--extract\n--\n-not-an-option.txt\n");
        let o = parse(&[], &d).unwrap();
        assert!(o.extract);
        assert_eq!(o.infile, vec!["-not-an-option.txt"]);
    }

    #[test]
    fn array_options_accumulate_across_files() {
        let home = tmpdir("arr-home");
        write(&home, ".txt2htmlrc", "--custom_heading_regexp '^ *A'\n");
        let cwd = tmpdir("arr-cwd");
        write(&cwd, ".txt2htmlrc", "--custom_heading_regexp '^ *B'\n");
        let mut o = Options::default();
        cli::parse_args_with_rc(&[], &mut o, Some(&home), &cwd).unwrap();
        assert_eq!(o.custom_heading_regexp, vec!["^ *A", "^ *B"]);
    }

    #[test]
    fn a_missing_rc_file_is_not_an_error() {
        let d = tmpdir("none");
        std::fs::create_dir_all(&d).unwrap();
        let o = parse(&["in.txt"], &d).unwrap();
        assert_eq!(o.infile, vec!["in.txt"]);
    }

    #[test]
    fn the_same_rc_file_is_not_read_twice_when_home_is_cwd() {
        // `HOME=$PWD` is common in containers; reading the file twice would
        // duplicate every array option.
        let d = tmpdir("same");
        write(&d, ".txt2htmlrc", "--custom_heading_regexp '^ *A'\n");
        let mut o = Options::default();
        cli::parse_args_with_rc(&[], &mut o, Some(&d), &d).unwrap();
        assert_eq!(o.custom_heading_regexp, vec!["^ *A"]);
    }

    #[test]
    fn negation_prefix_works_in_an_rc_file() {
        let d = tmpdir("neg");
        write(&d, ".txt2htmlrc", "--no_extract\n");
        let o = parse(&["in.txt"], &d).unwrap();
        assert!(!o.extract, "--no_extract in an rc file");
    }
}
