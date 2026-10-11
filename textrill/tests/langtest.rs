//! The `--lang` / `--lang_runs` feature end to end: the options surface, the
//! root-element declaration, and the span-wrapping pass over the body. The
//! per-script tagging policy itself is unit-tested in `langdetect`; these tests
//! pin the wiring between the CLI and the converter.

use textrill::cli;
use textrill::convert::Converter;
use textrill::options::Options;

fn render(opts: Options) -> String {
    let mut conv = Converter::new(opts);
    conv.opts.instring = vec!["Hello 世界 World".to_string()];
    conv.convert()
}

#[test]
fn lang_and_lang_runs_are_settable_and_reportable() {
    let mut opts = Options::default();
    cli::set_value(&mut opts, "lang", "fr").expect("set lang");
    assert_eq!(cli::get_value(&opts, "lang").unwrap(), "fr");
    cli::set_value(&mut opts, "lang_runs", "1").expect("set lang_runs");
    assert_eq!(cli::get_value(&opts, "lang_runs").unwrap(), "true");
    assert!(opts.lang_runs);
}

#[test]
fn lang_runs_wraps_a_foreign_passage_in_the_body() {
    let out = render(Options {
        html5: true,
        lang_runs: true,
        ..Options::default()
    });
    assert!(
        out.contains("<p>Hello <span lang=\"zh\">世界</span> World\n"),
        "{out}"
    );
}

#[test]
fn the_declared_language_is_never_rewrapped() {
    // A CJK document declaring ja leaves its own kana/kanji alone; the root
    // lang is ja. The default en would wrap the whole passage instead.
    let opts = Options {
        html5: true,
        lang_runs: true,
        lang: "ja".to_string(),
        ..Options::default()
    };
    let mut conv = Converter::new(opts);
    conv.opts.instring = vec!["日本語のテキストです。世界は広い。".to_string()];
    let out = conv.convert();
    assert!(
        out.starts_with("<!DOCTYPE html>\n<html lang=\"ja\">"),
        "{out}"
    );
    assert!(!out.contains("<span lang="), "{out}");
}

#[test]
fn lang_runs_is_refused_with_stream() {
    let mut opts = Options::default();
    cli::set_value(&mut opts, "lang_runs", "1").expect("set lang_runs");
    cli::set_value(&mut opts, "stream", "1").expect("set stream");
    let received = &opts.validate();
    assert!(received.is_err(), "stream must refuse lang_runs");
}

#[test]
fn extract_does_not_lose_the_wrap() {
    // --extract keeps the body-only promise, but the wrap must still apply.
    let out = render(Options {
        html5: true,
        extract: true,
        lang_runs: true,
        ..Options::default()
    });
    assert!(
        out.contains("<p>Hello <span lang=\"zh\">世界</span> World\n"),
        "{out}"
    );
    assert!(!out.contains("DOCTYPE"), "{out}");
}
