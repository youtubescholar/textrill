//! Citations and glossary: the promises made by `--citations` and `--glossary`.
//!
//! `src/notes.rs` unit-tests the parser over hand-written body HTML, because
//! that is what the parser actually receives. This file drives the whole
//! pipeline with *source text* and checks the things only the pipeline can
//! show: that the feature is off by default, that it refuses before writing,
//! that it composes with the other phases, and that the fragment links it emits
//! resolve.
//!
//! # The properties
//!
//! 1. Both modes off: output is byte-identical, and note markers survive as
//!    literal text rather than being interpreted or deleted.
//! 2. A broken note set produces an error and no output -- never a document
//!    with a `[1]` that points at nothing.
//! 3. Every generated fragment link resolves, and every generated id is unique.
//! 4. The definition block leaves no trace in the body: no leftover markup, no
//!    empty paragraph, no stray newlines.
//! 5. The two modes are independent, and neither disturbs the other phases
//!    (`--number_headings`, `--section`, `--toc`, `--extract`, `--html5`,
//!    `--body_template`).
//! 6. The refused combinations (`--chunk`, `--stream`) are refused up front.

use textrill::convert::Converter;
use textrill::options::Options;

/// An option-setting function, so a table of configurations reads as one list.
type Configure = fn(&mut Options);

/// Convert `src` with `configure` applied, returning `(html, notes error)`.
fn convert(src: &str, configure: impl FnOnce(&mut Options)) -> (String, Option<String>) {
    let mut opts = Options::default();
    configure(&mut opts);
    let mut conv = Converter::new(opts);
    let out = conv.convert_text(src);
    (out, conv.notes_error)
}

/// Convert with the flags a test cares about, panicking on a note error.
fn ok(src: &str, configure: impl FnOnce(&mut Options)) -> String {
    let (out, err) = convert(src, configure);
    assert!(err.is_none(), "unexpected note error: {err:?}\n{out}");
    out
}

/// Convert with both note modes on.
fn notes(src: &str) -> (String, Option<String>) {
    convert(src, |o| {
        o.citations = true;
        o.glossary = true;
    })
}

fn ids_of(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("id=\"") {
        rest = &rest[i + 4..];
        match rest.find('"') {
            Some(e) => {
                out.push(rest[..e].to_string());
                rest = &rest[e + 1..];
            }
            None => break,
        }
    }
    out
}

/// Every `href="#…"` target in `html`.
fn fragments_of(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("href=\"#") {
        // Skip past the `#` too, so the value matches an id.
        rest = &rest[i + 7..];
        match rest.find('"') {
            Some(e) => {
                out.push(rest[..e].to_string());
                rest = &rest[e + 1..];
            }
            None => break,
        }
    }
    out
}

/// Assert every generated fragment link resolves and no id is repeated.
fn check_ids_resolve(label: &str, html: &str) {
    let ids = ids_of(html);
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        ids.len(),
        sorted.len(),
        "{label}: duplicate id in output\n{html}"
    );
    for f in fragments_of(html) {
        assert!(
            ids.contains(&f),
            "{label}: link to #{f} has no id\nids: {ids:?}\n{html}"
        );
    }
}

const DOC: &str = "\
Prose citing {{textrill:cite:knuth}} and repeating {{textrill:cite:knuth}}, \
with a term {{textrill:gloss:monad}}.

{{textrill:def:cite:knuth}}
Knuth, *Literate Programming*.
{{/textrill:def:cite:knuth}}

{{textrill:def:gloss:monad}}
A monad.
{{/textrill:def:gloss:monad}}
";

// ---------------------------------------------------------------- property 1

#[test]
fn both_modes_off_is_byte_identical() {
    let baseline = ok(DOC, |o| o.extract = true);
    // The default must not even mention the markers.
    assert!(baseline.contains("{{textrill:cite:knuth}}"), "{baseline}");
    assert!(
        baseline.contains("{{textrill:def:gloss:monad}}"),
        "{baseline}"
    );
    assert!(!baseline.contains("note-ref"), "{baseline}");
    assert!(!baseline.contains("gloss-ref"), "{baseline}");
}

#[test]
fn a_document_with_no_markers_is_unaffected_by_the_modes() {
    let plain = "Just a paragraph.\n\nAnother one.\n";
    let off = ok(plain, |o| o.extract = true);
    let on = ok(plain, |o| {
        o.citations = true;
        o.glossary = true;
        o.extract = true;
    });
    assert_eq!(off, on);
}

// ---------------------------------------------------------------- property 2

#[test]
fn a_dangling_reference_fails_and_renders_nothing_useful() {
    let (out, err) = notes("Bad {{textrill:cite:missing}}\n");
    let e = err.expect("a dangling reference must be an error");
    assert!(e.contains("missing"), "{e}");
    assert!(e.contains("no definition"), "{e}");
    // The marker is left in place rather than half-substituted, so the author
    // sees what they wrote next to the message.
    assert!(out.contains("{{textrill:cite:missing}}"), "{out}");
}

#[test]
fn every_failure_mode_is_reported() {
    let cases: &[(&str, &str)] = &[
        ("{{textrill:cite:a}}\n", "no definition"),
        (
            "{{textrill:def:cite:a}}\nx\n{{/textrill:def:cite:a}}\n",
            "never referenced",
        ),
        (
            "{{textrill:cite:a}}\n{{textrill:def:cite:a}}\nx\n{{/textrill:def:cite:a}}\n\
             {{textrill:def:cite:a}}\ny\n{{/textrill:def:cite:a}}\n",
            "more than once",
        ),
        (
            "{{textrill:cite:a}}\n{{textrill:def:cite:a}}\n{{/textrill:def:cite:a}}\n",
            "empty",
        ),
        (
            "{{textrill:cite:a}}\n{{textrill:def:cite:a}}\nx\n",
            "never closed",
        ),
        ("{{textrill:cites:a}}\n", "unknown marker"),
        // The key lands in an id, so the charset is an injection boundary.
        ("{{textrill:cite:a\"b}}\n", "note key"),
    ];
    for (src, want) in cases {
        let (_, err) = notes(src);
        let e = err.unwrap_or_else(|| panic!("{src:?} should have been refused"));
        assert!(
            e.to_lowercase().contains(&want.to_lowercase()),
            "{src:?}: expected {want:?} in {e:?}"
        );
    }
}

#[test]
fn the_two_modes_do_not_satisfy_each_other() {
    // A glossary term is not a citation, so citing it must not resolve.
    let (_, err) = convert(
        "{{textrill:gloss:x}}\n{{textrill:def:cite:x}}\nT.\n{{/textrill:def:cite:x}}\n",
        |o| o.citations = true,
    );
    assert!(
        err.is_some(),
        "a glossary ref must not resolve a citation def"
    );
    let (_, err) = convert(
        "{{textrill:cite:x}}\n{{textrill:def:gloss:x}}\nT.\n{{/textrill:def:gloss:x}}\n",
        |o| o.glossary = true,
    );
    assert!(
        err.is_some(),
        "a citation ref must not resolve a glossary def"
    );
}

// ---------------------------------------------------------------- property 3

#[test]
fn generated_links_resolve_and_ids_are_unique() {
    let out = ok(DOC, |o| {
        o.citations = true;
        o.glossary = true;
    });
    check_ids_resolve("both modes", &out);
    assert!(out.contains("href=\"#note-knuth\""), "{out}");
    assert!(out.contains("href=\"#gloss-monad\""), "{out}");
    assert!(out.contains("href=\"#note-ref-knuth\""), "{out}");
}

#[test]
fn a_reference_repeated_many_times_stays_valid() {
    let mut src = String::new();
    for _ in 0..50 {
        src.push_str("x {{textrill:cite:a}} ");
    }
    src.push_str("\n{{textrill:def:cite:a}}\nA.\n{{/textrill:def:cite:a}}\n");
    let out = ok(&src, |o| o.citations = true);
    check_ids_resolve("repeated", &out);
    assert_eq!(out.matches("[1]").count(), 50, "every mention is numbered");
    assert_eq!(out.matches("id=\"note-ref-a\"").count(), 1, "{out}");
    assert_eq!(out.matches("id=\"note-a\"").count(), 1, "{out}");
}

#[test]
fn two_lists_and_two_kinds_keep_their_ids_apart() {
    let out = ok(
        "{{textrill:cite:x}}\n{{textrill:gloss:x}}\n\
         {{textrill:def:cite:x}}\nCite.\n{{/textrill:def:cite:x}}\n\
         {{textrill:def:gloss:x}}\nGloss.\n{{/textrill:def:gloss:x}}\n",
        |o| {
            o.citations = true;
            o.glossary = true;
        },
    );
    check_ids_resolve("same key, two kinds", &out);
}

// ---------------------------------------------------------------- property 4

#[test]
fn the_definition_block_leaves_nothing_behind() {
    // Pinned to XHTML mode (PLAN Phase 3 exempts rather than deletes): the
    // empty-paragraph shape this guards against is `<p></p>`, which only a
    // mode that closes paragraphs can emit -- in the HTML5 default the assert
    // would be vacuous and the gate gone.
    let out = ok(
        "Lead.\n\nSee {{textrill:cite:a}}.\n\n         {{textrill:def:cite:a}}\nA.\n{{/textrill:def:cite:a}}\n\nTail.\n",
        |o| {
            o.citations = true;
            o.extract = true;
            o.xhtml = true;
        },
    );
    assert!(!out.contains("{{textrill"), "no marker may survive: {out}");
    assert!(!out.contains("<p></p>"), "no empty paragraph: {out}");
    assert!(!out.contains(">A.<"), "no leftover content: {out}");
    assert!(out.contains("<p>Lead."), "{out}");
    assert!(out.contains("<p>Tail."), "{out}");
}

#[test]
fn the_paragraph_wrapper_goes_with_the_block() {
    // The engine wraps the block's lines in `<p>`, and puts the `</p>` on the
    // same line as the closing tag. Removing only the block leaves a stray
    // `<p>` with nothing after it, so the wrapper has to go too. XHTML mode is
    // pinned for the `<p></p>` assert, which the HTML5 default (which never
    // closes a paragraph) could not fail -- see the note above.
    let modes: Vec<(&str, Configure)> = vec![
        ("citations", |o| o.citations = true),
        ("glossary", |o| o.glossary = true),
    ];
    for (name, o) in modes {
        let out = ok(
            "Lead {{textrill:gloss:a}}.\n\n{{textrill:def:gloss:a}}\nA.\n{{/textrill:def:gloss:a}}\n",
            |opts| {
                o(opts);
                opts.extract = true;
                opts.xhtml = true;
            },
        );
        assert!(
            !out.contains("<p>\n"),
            "{name}: stray paragraph tag:\n{out}"
        );
        assert!(!out.contains("<p></p>"), "{name}: empty paragraph:\n{out}");
        assert!(out.contains("Lead "), "{name}: {out}");
        check_ids_resolve(name, &out);
    }
}

#[test]
fn a_definition_inline_in_prose_keeps_its_paragraph() {
    // The marker is part of the sentence, so the paragraph must survive; only
    // the marker and the block go. XHTML pinned: `<p></p>` cannot occur in
    // the HTML5 default (see the note in the property-4 tests above).
    let out = ok(
        "Lead.\n\nA sentence with {{textrill:cite:k}} inside it.\n\n\
         {{textrill:def:cite:k}}\nK.\n{{/textrill:def:cite:k}}\n",
        |o| {
            o.citations = true;
            o.extract = true;
            o.xhtml = true;
        },
    );
    assert!(out.contains("A sentence with"), "{out}");
    assert!(out.contains("inside it."), "{out}");
    assert!(!out.contains("<p></p>"), "{out}");
}

// ---------------------------------------------------------------- property 5

#[test]
fn the_modes_are_independent() {
    // Citations only: the glossary markers are not collected, not refused, and
    // not deleted.
    let (out, err) = convert(DOC, |o| o.citations = true);
    assert!(err.is_none(), "{err:?}");
    assert!(out.contains("{{textrill:gloss:monad}}"), "{out}");
    assert!(out.contains("Notes"), "{out}");
    assert!(!out.contains("Glossary"), "{out}");

    let (out, err) = convert(DOC, |o| o.glossary = true);
    assert!(err.is_none(), "{err:?}");
    assert!(out.contains("{{textrill:cite:knuth}}"), "{out}");
    assert!(out.contains("Glossary"), "{out}");
    assert!(!out.contains("Notes"), "{out}");
}

#[test]
fn notes_compose_with_the_other_phases() {
    // Each of these rewrites the body, so a note set has to survive all of them.
    let mut configs: Vec<(&str, Configure)> = vec![
        ("plain", |_| {}),
        ("html5", |o| o.html5 = true),
        ("lower_case_tags", |o| o.lower_case_tags = true),
        ("number_headings", |o| o.number_headings = true),
        ("section", |o| o.section = true),
        ("toc", |o| o.toc = true),
        ("section+toc", |o| {
            o.section = true;
            o.toc = true;
        }),
        ("extract", |o| o.extract = true),
    ];
    configs.push(("number+toc+html5", |o| {
        o.number_headings = true;
        o.toc = true;
        o.html5 = true;
    }));

    for (name, f) in configs {
        let out = ok(DOC, |o| {
            o.citations = true;
            o.glossary = true;
            f(o);
        });
        assert!(out.contains("id=\"note-knuth\""), "{name}: {out}");
        assert!(out.contains("id=\"gloss-monad\""), "{name}: {out}");
        assert!(!out.contains("{{textrill"), "{name}: {out}");
        check_ids_resolve(name, &out);
    }
}

#[test]
fn the_section_ids_and_the_note_ids_do_not_collide() {
    // Both are engine-invented, so this is the one collision the author cannot
    // avoid by choosing different keys.
    let out = ok(
        "One\n====\n\n{{textrill:cite:a}}\n\n{{textrill:def:cite:a}}\nA.\n{{/textrill:def:cite:a}}\n",
        |o| {
            o.citations = true;
            o.section = true;
            o.toc = true;
        },
    );
    check_ids_resolve("sections + notes", &out);
}

#[test]
fn a_template_slot_places_the_lists() {
    let dir = std::env::temp_dir().join(format!("textrill-notes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("t.html");
    std::fs::write(
        &path,
        "<html><body>{{textrill:content}}\
         <hr>{{textrill:citations}}{{textrill:glossary}}</body></html>",
    )
    .expect("write template");

    let out = ok(DOC, |o| {
        o.citations = true;
        o.glossary = true;
        o.template = path.to_string_lossy().into_owned();
    });
    // Both lists land in their slots, after the content and after the `<hr>`.
    let hr = out.find("<hr>").expect("hr");
    let notes_at = out.find("id=\"notes\"").expect("notes section");
    let gloss_at = out.find("id=\"glossary\"").expect("glossary section");
    assert!(hr < notes_at, "the citation slot must be used:\n{out}");
    assert!(notes_at < gloss_at, "{out}");
    check_ids_resolve("template", &out);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_template_without_the_slots_still_keeps_the_notes() {
    // A template written before this feature existed must not silently lose a
    // note list; the fallback is the same end-of-body placement.
    let dir = std::env::temp_dir().join(format!("textrill-notes-nb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("t.html");
    std::fs::write(&path, "<html><body>{{textrill:content}}</body></html>").expect("write");

    let out = ok(DOC, |o| {
        o.citations = true;
        o.glossary = true;
        o.template = path.to_string_lossy().into_owned();
    });
    assert!(out.contains("id=\"note-knuth\""), "{out}");
    assert!(out.contains("id=\"gloss-monad\""), "{out}");
    check_ids_resolve("template fallback", &out);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- property 6

#[test]
fn the_streaming_modes_are_refused() {
    let modes: Vec<(&str, Configure)> = vec![
        ("chunk", |o| o.chunk = true),
        ("stream", |o| o.stream = true),
    ];
    for (name, f) in modes {
        for flag in ["citations", "glossary"] {
            let mut o = Options::default();
            f(&mut o);
            if flag == "citations" {
                o.citations = true;
            } else {
                o.glossary = true;
            }
            let e = o
                .validate()
                .err()
                .unwrap_or_else(|| panic!("{name} + --{flag} should be refused"));
            assert!(e.contains(&format!("--{flag}")), "{e}");
        }
    }
}

#[test]
fn a_refusal_names_the_offending_flag() {
    let o = Options {
        chunk: true,
        glossary: true,
        ..Default::default()
    };
    assert!(o.validate().unwrap_err().contains("--glossary"));
}

// ------------------------------------------------------------ prose safety

#[test]
fn text_that_only_looks_like_a_note_is_never_interpreted() {
    // The no-inference rule. Every one of these must survive untouched, which
    // is the whole reason the markers are namespaced.
    let body = "\
A footnote[^1], a caret^2, (3), [4], @five, and {6}.

Also {{ and }} and {{textrill}} and {{ textrill:cite:x }}.

Cite as [7] and see note 8. ~x~ and 9. \"Quoted\" and 'single'.
";
    // Compared against the same conversion with the modes off, not against the
    // source: the engine's own paragraph wrapping is not what is under test.
    let off = ok(body, |o| o.extract = true);
    let (on, err) = convert(body, |o| {
        o.citations = true;
        o.glossary = true;
        o.extract = true;
    });
    assert!(err.is_none(), "{err:?}");
    assert_eq!(on, off, "nothing may be interpreted");
}

#[test]
fn a_marker_looking_like_a_url_is_still_a_url() {
    // The scrubber and the marker scanner share the body; neither may swallow
    // the other's territory.
    let out = ok(
        "See <URL:https://example.com/x> and {{textrill:cite:a}}.\n\n\
         {{textrill:def:cite:a}}\nA.\n{{/textrill:def:cite:a}}\n",
        |o| {
            o.citations = true;
            o.make_links = true;
        },
    );
    assert!(out.contains("https://example.com/x"), "{out}");
    assert!(out.contains("id=\"note-a\""), "{out}");
    check_ids_resolve("url + notes", &out);
}
